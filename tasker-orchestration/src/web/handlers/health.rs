//! # Health Check Handlers
//!
//! TAS-76: Kubernetes-compatible health check endpoints for monitoring and load balancing.
//! These handlers delegate business logic to HealthService, keeping handlers thin.

use axum::extract::State;
use axum::response::Html;
use axum::Json;

use crate::web::state::AppState;
use tasker_shared::types::api::orchestration::{
    DetailedHealthResponse, HealthResponse, ReadinessResponse,
};
use tasker_shared::types::web::ApiError;

/// Basic health check endpoint: GET /health
///
/// Simple health check that returns OK if the service is running.
/// This endpoint is always available, even during graceful shutdown.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is running", body = HealthResponse)
    ),
    tag = "health"
))]
pub async fn basic_health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(state.health_service.basic_health())
}

/// Kubernetes readiness probe: GET /ready
///
/// Indicates whether the service is ready to accept traffic.
/// Checks database connectivity and circuit breaker status.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, description = "Service is ready", body = ReadinessResponse),
        (status = 503, description = "Service is not ready", body = ApiError)
    ),
    tag = "health"
))]
pub async fn readiness_probe(
    State(state): State<AppState>,
) -> Result<Json<ReadinessResponse>, ApiError> {
    state
        .health_service
        .readiness()
        .await
        .map(Json)
        .map_err(|_| ApiError::ServiceUnavailable)
}

/// Kubernetes liveness probe: GET /live
///
/// Indicates whether the service is alive and should not be restarted.
/// This is a simpler check than readiness - mainly checks if the process is responsive.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/live",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse)
    ),
    tag = "health"
))]
pub async fn liveness_probe(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(state.health_service.liveness().await)
}

/// Detailed health status: GET /health/detailed
///
/// Comprehensive health check with detailed information about all subsystems.
/// May require authentication depending on configuration.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/health/detailed",
    responses(
        (status = 200, description = "Detailed health information", body = DetailedHealthResponse)
    ),
    tag = "health"
))]
pub async fn detailed_health(State(state): State<AppState>) -> Json<DetailedHealthResponse> {
    Json(state.health_service.detailed_health().await)
}

/// Prometheus metrics endpoint: GET /metrics
///
/// Returns metrics in Prometheus format for monitoring and alerting.
pub async fn prometheus_metrics(State(state): State<AppState>) -> Html<String> {
    Html(state.health_service.prometheus_metrics().await)
}
