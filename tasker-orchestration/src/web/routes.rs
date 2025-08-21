//! # Web API Route Definitions
//!
//! Defines the HTTP route structure for the Tasker web API.
//! Routes are organized into logical groups with proper versioning.

use crate::web::handlers;
use crate::web::openapi::ApiDoc;
use crate::web::state::AppState;
use axum::routing::{delete, get, patch, post};
use axum::Router;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Create API v1 routes
///
/// All v1 routes are prefixed with `/v1` and include:
/// - Tasks API - Task creation, status, and management
/// - Workflow Steps API - Step details and manual resolution
/// - Handlers API - Handler discovery and information
/// - Analytics API - Performance metrics and bottleneck analysis
pub fn api_v1_routes() -> Router<AppState> {
    Router::new()
        // Tasks API
        .route("/tasks", post(handlers::tasks::create_task))
        .route("/tasks", get(handlers::tasks::list_tasks))
        .route("/tasks/:uuid", get(handlers::tasks::get_task))
        .route("/tasks/:uuid", delete(handlers::tasks::cancel_task))
        // Workflow Steps API
        .route(
            "/tasks/:uuid/workflow_steps",
            get(handlers::steps::list_task_steps),
        )
        .route(
            "/tasks/:uuid/workflow_steps/:step_uuid",
            get(handlers::steps::get_step),
        )
        .route(
            "/tasks/:uuid/workflow_steps/:step_uuid",
            patch(handlers::steps::resolve_step_manually),
        )
        // Handlers API (read-only)
        .route("/handlers", get(handlers::registry::list_namespaces))
        .route(
            "/handlers/:namespace",
            get(handlers::registry::list_namespace_handlers),
        )
        .route(
            "/handlers/:namespace/:name",
            get(handlers::registry::get_handler_info),
        )
        // Analytics API (read-only)
        .route(
            "/analytics/performance",
            get(handlers::analytics::get_performance_metrics),
        )
        .route(
            "/analytics/bottlenecks",
            get(handlers::analytics::get_bottlenecks),
        )
}

/// Create health and metrics routes
///
/// These routes are at the root level following Kubernetes standards:
/// - `/health` - Basic health check
/// - `/ready` - Kubernetes readiness probe
/// - `/live` - Kubernetes liveness probe
/// - `/health/detailed` - Detailed health status
/// - `/metrics` - Prometheus metrics export
pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health::basic_health))
        .route("/ready", get(handlers::health::readiness_probe))
        .route("/live", get(handlers::health::liveness_probe))
        .route("/health/detailed", get(handlers::health::detailed_health))
        .route("/metrics", get(handlers::health::prometheus_metrics))
}

/// Create API documentation routes
///
/// These routes serve the OpenAPI specification and Swagger UI:
/// - `/api-docs/openapi.json` - OpenAPI JSON specification
/// - `/api-docs/ui` - Swagger UI interface for interactive API exploration
#[cfg(feature = "web-api")]
pub fn docs_routes() -> Router<AppState> {
    SwaggerUi::new("/api-docs/ui")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
        .into()
}
