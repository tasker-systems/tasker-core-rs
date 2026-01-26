//! # Web API Route Definitions
//!
//! Defines the HTTP route structure for the Tasker web API.
//! Routes are organized into logical groups with proper versioning.
//!
//! ## TAS-176: Resource-Based Authorization
//!
//! Protected routes use the `authorize()` wrapper for declarative permission checking:
//! ```rust,ignore
//! .route("/tasks", post(authorize(Resource::Tasks, Action::Create, handlers::tasks::create_task)))
//! ```
//!
//! The authorization check happens BEFORE body deserialization, ensuring:
//! - Early rejection of unauthorized requests
//! - Consistent permission model across REST and future gRPC endpoints
//! - Clear declaration of route protection requirements

use crate::web::handlers;
use crate::web::openapi::ApiDoc;
use crate::web::state::AppState;
use axum::routing::{delete, get, patch, post};
use axum::Router;
use tasker_shared::types::resources::{Action, Resource};
use tasker_shared::web::authorize;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Create API v1 routes
///
/// All v1 routes are prefixed with `/v1` and include:
/// - Tasks API - Task creation, status, and management
/// - Workflow Steps API - Step details and manual resolution
/// - Templates API - Template discovery and information
/// - Analytics API - Performance metrics and bottleneck analysis
/// - DLQ API - Dead letter queue investigation
///
/// ## Authorization (TAS-176)
///
/// All API routes are wrapped with `authorize()` for declarative permission checking.
/// See `tasker_shared::types::resources` for the resource→permission mapping.
pub fn api_v1_routes() -> Router<AppState> {
    Router::new()
        // Tasks API
        .route(
            "/tasks",
            post(authorize(
                Resource::Tasks,
                Action::Create,
                handlers::tasks::create_task,
            )),
        )
        .route(
            "/tasks",
            get(authorize(
                Resource::Tasks,
                Action::List,
                handlers::tasks::list_tasks,
            )),
        )
        .route(
            "/tasks/{uuid}",
            get(authorize(
                Resource::Tasks,
                Action::Read,
                handlers::tasks::get_task,
            )),
        )
        .route(
            "/tasks/{uuid}",
            delete(authorize(
                Resource::Tasks,
                Action::Cancel,
                handlers::tasks::cancel_task,
            )),
        )
        // Workflow Steps API
        .route(
            "/tasks/{uuid}/workflow_steps",
            get(authorize(
                Resource::Steps,
                Action::List,
                handlers::steps::list_task_steps,
            )),
        )
        .route(
            "/tasks/{uuid}/workflow_steps/{step_uuid}",
            get(authorize(
                Resource::Steps,
                Action::Read,
                handlers::steps::get_step,
            )),
        )
        .route(
            "/tasks/{uuid}/workflow_steps/{step_uuid}",
            patch(authorize(
                Resource::Steps,
                Action::Resolve,
                handlers::steps::resolve_step_manually,
            )),
        )
        // TAS-62: Step audit history endpoint
        .route(
            "/tasks/{uuid}/workflow_steps/{step_uuid}/audit",
            get(authorize(
                Resource::Steps,
                Action::Read,
                handlers::steps::get_step_audit,
            )),
        )
        // TAS-76: Templates API (read-only) - replaces legacy /handlers
        .route(
            "/templates",
            get(authorize(
                Resource::Templates,
                Action::List,
                handlers::templates::list_templates,
            )),
        )
        .route(
            "/templates/{namespace}/{name}/{version}",
            get(authorize(
                Resource::Templates,
                Action::Read,
                handlers::templates::get_template,
            )),
        )
        // Analytics API (read-only)
        .route(
            "/analytics/performance",
            get(authorize(
                Resource::System,
                Action::AnalyticsRead,
                handlers::analytics::get_performance_metrics,
            )),
        )
        .route(
            "/analytics/bottlenecks",
            get(authorize(
                Resource::System,
                Action::AnalyticsRead,
                handlers::analytics::get_bottlenecks,
            )),
        )
        // DLQ API - Investigation tracking (TAS-49)
        .route(
            "/dlq",
            get(authorize(
                Resource::Dlq,
                Action::List,
                handlers::dlq::list_dlq_entries,
            )),
        )
        .route(
            "/dlq/task/{task_uuid}",
            get(authorize(
                Resource::Dlq,
                Action::Read,
                handlers::dlq::get_dlq_entry,
            )),
        )
        .route(
            "/dlq/entry/{dlq_entry_uuid}",
            patch(authorize(
                Resource::Dlq,
                Action::Update,
                handlers::dlq::update_dlq_investigation,
            )),
        )
        .route(
            "/dlq/stats",
            get(authorize(
                Resource::Dlq,
                Action::Stats,
                handlers::dlq::get_dlq_stats,
            )),
        )
        .route(
            "/dlq/investigation-queue",
            get(authorize(
                Resource::Dlq,
                Action::Read,
                handlers::dlq::get_investigation_queue,
            )),
        )
        .route(
            "/dlq/staleness",
            get(authorize(
                Resource::Dlq,
                Action::Read,
                handlers::dlq::get_staleness_monitoring,
            )),
        )
}

/// Create health and metrics routes
///
/// Health endpoints are grouped under `/health` for consistency:
/// - `/health` - Basic health check
/// - `/health/ready` - Kubernetes readiness probe
/// - `/health/live` - Kubernetes liveness probe
/// - `/health/detailed` - Detailed health status with subsystem checks
/// - `/metrics` - Prometheus metrics export
pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health::basic_health))
        .route("/health/ready", get(handlers::health::readiness_probe))
        .route("/health/live", get(handlers::health::liveness_probe))
        .route("/health/detailed", get(handlers::health::detailed_health))
        .route("/metrics", get(handlers::health::prometheus_metrics))
}

/// Create configuration routes
///
/// Configuration observability routes at root level (system endpoints, not REST API):
/// - `/config` - Operational configuration (whitelist-only, no secrets)
///
/// Requires `system:config_read` permission.
pub fn config_routes() -> Router<AppState> {
    Router::new().route(
        "/config",
        get(authorize(
            Resource::System,
            Action::ConfigRead,
            handlers::config::get_config,
        )),
    )
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
