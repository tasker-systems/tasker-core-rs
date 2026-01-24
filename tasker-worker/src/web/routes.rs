//! Worker Web API Routes
//!
//! Route definitions for all worker web endpoints organized by functionality.

use axum::{
    routing::{delete, get, post},
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

// TODO(TAS-169): Template routes should be nested under /v1 for API versioning consistency.
// All non-infrastructure endpoints (health, metrics, docs) should have version prefixes.
/// Task template management routes
pub fn template_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        // Template retrieval and listing
        .route("/templates", get(handlers::templates::list_templates))
        .route(
            "/templates/{namespace}/{name}/{version}",
            get(handlers::templates::get_template),
        )
        // Template validation
        .route(
            "/templates/{namespace}/{name}/{version}/validate",
            post(handlers::templates::validate_template),
        )
        // Cache management
        .route("/templates/cache", delete(handlers::templates::clear_cache))
        .route(
            "/templates/cache/stats",
            get(handlers::templates::get_cache_stats),
        )
        .route(
            "/templates/cache/maintain",
            post(handlers::templates::maintain_cache),
        )
        // Template refresh
        .route(
            "/templates/{namespace}/{name}/{version}/refresh",
            post(handlers::templates::refresh_template),
        )
}

// TODO(TAS-169): Review redact_secrets() coverage after auth config changes to ensure
// API keys and JWT key material are never leaked via this endpoint.
/// Configuration observability routes - unified endpoint
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
