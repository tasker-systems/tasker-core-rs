//! Worker Web API Routes
//!
//! Route definitions for all worker web endpoints organized by functionality.
//!
//! ## TAS-176: Resource-Based Authorization
//!
//! Protected routes use the `authorize()` wrapper for declarative permission checking.
//! Public routes (health, metrics, docs) remain unwrapped.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tasker_shared::types::resources::{Action, Resource};
use tasker_shared::web::authorize;

use crate::web::{handlers, state::WorkerWebState};

/// Health check routes for monitoring and Kubernetes probes
pub fn health_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route("/health", get(handlers::health::health_check))
        .route("/health/ready", get(handlers::health::readiness_check))
        .route("/health/live", get(handlers::health::liveness_check))
        .route(
            "/health/detailed",
            get(handlers::health::detailed_health_check),
        )
}

/// Metrics routes for Prometheus and monitoring systems
pub fn metrics_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route("/metrics", get(handlers::metrics::prometheus_metrics))
        .route("/metrics/worker", get(handlers::metrics::worker_metrics))
        .route(
            "/metrics/events",
            get(handlers::metrics::domain_event_stats),
        )
}

/// Task template management routes (TAS-169: API versioning)
///
/// Templates are loaded from YAML → DB on worker bootstrap and are effectively
/// immutable until reboot. Only essential endpoints are exposed:
/// - List templates
/// - Get specific template
/// - Validate template for execution
///
/// Cache operations moved to internal-only (restart worker to clear cache).
/// Distributed cache status available via /health/detailed.
///
/// ## Authorization (TAS-176)
///
/// - List/Read templates: `worker:templates_read`
/// - Validate template: `templates:validate`
pub fn template_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route(
            "/v1/templates",
            get(authorize(
                Resource::Worker,
                Action::List,
                handlers::templates::list_templates,
            )),
        )
        .route(
            "/v1/templates/{namespace}/{name}/{version}",
            get(authorize(
                Resource::Worker,
                Action::Read,
                handlers::templates::get_template,
            )),
        )
        .route(
            "/v1/templates/{namespace}/{name}/{version}/validate",
            post(authorize(
                Resource::Worker,
                Action::Validate,
                handlers::templates::validate_template,
            )),
        )
}

/// Configuration observability routes (whitelist-only, no secrets)
///
/// Requires `worker:config_read` permission.
pub fn config_routes() -> Router<Arc<WorkerWebState>> {
    Router::new().route(
        "/config",
        get(authorize(
            Resource::Worker,
            Action::ConfigRead,
            handlers::config::get_config,
        )),
    )
}

/// API documentation routes (OpenAPI spec + Swagger UI)
#[cfg(feature = "web-api")]
pub fn docs_routes() -> Router<Arc<WorkerWebState>> {
    use utoipa::OpenApi;
    use utoipa_swagger_ui::SwaggerUi;

    use crate::web::openapi::ApiDoc;

    SwaggerUi::new("/api-docs/ui")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
        .into()
}

/// No-op docs routes when web-api feature is disabled
#[cfg(not(feature = "web-api"))]
pub fn docs_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
}
