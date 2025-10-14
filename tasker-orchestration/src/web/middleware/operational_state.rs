//! # Operational State Middleware
//!
//! Middleware that coordinates web API availability with orchestration system operational state.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use tracing::debug;

use crate::web::state::AppState;
use tasker_shared::types::web::SystemOperationalState;

/// Operational state middleware
///
/// Coordinates web API availability with orchestration system state:
/// - Normal: All endpoints available
/// - GracefulShutdown: Only health and metrics endpoints
/// - Emergency/Stopped: Only basic health endpoint
///
/// Note: This middleware is currently disabled in the main middleware stack
/// due to complications with State extractor in from_fn middleware.
/// Operational state checking is implemented at the handler level instead.
pub async fn operational_state_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let operational_state = state.operational_state().await;
    let path = request.uri().path();

    debug!(
        operational_state = ?operational_state,
        path = %path,
        "Checking operational state for request"
    );

    match operational_state {
        SystemOperationalState::Normal => {
            // All endpoints available during normal operation
            Ok(next.run(request).await)
        }
        SystemOperationalState::GracefulShutdown => {
            // During graceful shutdown, only health and metrics remain available
            if is_health_or_metrics_endpoint(path) {
                Ok(next.run(request).await)
            } else {
                debug!(path = %path, "Rejecting request during graceful shutdown");
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
        SystemOperationalState::Emergency | SystemOperationalState::Stopped => {
            // During emergency or stopped state, only basic health check available
            if path == "/health" {
                Ok(next.run(request).await)
            } else {
                debug!(path = %path, "Rejecting request during emergency/stopped state");
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
        SystemOperationalState::Startup => {
            // During startup, only health endpoints available
            if is_health_or_metrics_endpoint(path) {
                Ok(next.run(request).await)
            } else {
                debug!(path = %path, "Rejecting request during startup");
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
    }
}

/// Check if the path is a health or metrics endpoint
fn is_health_or_metrics_endpoint(path: &str) -> bool {
    path.starts_with("/health") || path == "/metrics" || path == "/ready" || path == "/live"
}
