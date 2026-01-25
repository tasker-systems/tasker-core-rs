//! Worker Web API Routes
//!
//! Route definitions for all worker web endpoints organized by functionality.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

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
pub fn template_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route("/v1/templates", get(handlers::templates::list_templates))
        .route(
            "/v1/templates/{namespace}/{name}/{version}",
            get(handlers::templates::get_template),
        )
        .route(
            "/v1/templates/{namespace}/{name}/{version}/validate",
            post(handlers::templates::validate_template),
        )
}

/// Configuration observability routes (whitelist-only, no secrets)
pub fn config_routes() -> Router<Arc<WorkerWebState>> {
    Router::new().route("/config", get(handlers::config::get_config))
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
