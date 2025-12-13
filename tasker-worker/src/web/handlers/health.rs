//! # Worker Health Check Handlers
//!
//! Kubernetes-compatible health check endpoints for worker monitoring and load balancing.
//! Focuses on worker-specific health concerns like queue processing and step execution.
//!
//! TAS-77: Handlers now delegate to HealthService for actual health check logic,
//! enabling the same functionality to be accessed via FFI.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use std::sync::Arc;

use crate::web::state::WorkerWebState;
use tasker_shared::types::api::worker::{BasicHealthResponse, DetailedHealthResponse};
use tasker_shared::types::web::*;

/// Basic health check endpoint: GET /health
///
/// Simple health check that returns OK if the worker service is running.
/// Always available, even during graceful shutdown.
pub async fn health_check(State(state): State<Arc<WorkerWebState>>) -> Json<BasicHealthResponse> {
    Json(state.health_service().basic_health())
}

/// Kubernetes readiness probe: GET /health/ready
///
/// Indicates whether the worker is ready to process steps.
/// Checks database connectivity, command processor status, and queue health.
pub async fn readiness_check(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<DetailedHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.health_service().readiness().await {
        Ok(response) => Ok(Json(response)),
        Err(_response) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "service_unavailable".to_string(),
                message: "Worker is not ready to process steps".to_string(),
                timestamp: Utc::now(),
                request_id: None,
            }),
        )),
    }
}

/// Kubernetes liveness probe: GET /health/live
///
/// Indicates whether the worker is alive and should not be restarted.
/// Simple check focusing on basic process responsiveness.
pub async fn liveness_check(State(state): State<Arc<WorkerWebState>>) -> Json<BasicHealthResponse> {
    Json(state.health_service().liveness())
}

/// Comprehensive health check: GET /health/detailed
///
/// Detailed health information about all worker subsystems.
/// Includes performance metrics and diagnostic information.
pub async fn detailed_health_check(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<DetailedHealthResponse> {
    Json(state.health_service().detailed_health().await)
}
