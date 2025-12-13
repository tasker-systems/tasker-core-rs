//! # Worker Metrics Handlers
//!
//! Prometheus-compatible metrics endpoints for worker monitoring and observability.
//!
//! TAS-77: Handlers now delegate to MetricsService for actual metrics collection,
//! enabling the same functionality to be accessed via FFI.

use axum::extract::State;
use axum::http::StatusCode;
use axum::{response::Html, Json};
use std::sync::Arc;

use crate::web::state::WorkerWebState;
use tasker_shared::types::web::*;

/// Prometheus metrics endpoint: GET /metrics
///
/// Returns metrics in Prometheus format for scraping by monitoring systems.
pub async fn prometheus_metrics(State(state): State<Arc<WorkerWebState>>) -> Html<String> {
    Html(state.metrics_service().prometheus_format().await)
}

/// Worker-specific metrics endpoint: GET /metrics/worker
///
/// Returns worker metrics in JSON format for programmatic access.
pub async fn worker_metrics(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<MetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(state.metrics_service().worker_metrics().await))
}

/// Domain event statistics endpoint: GET /metrics/events
///
/// Returns statistics about domain event routing and delivery paths.
/// Used for monitoring event publishing and by E2E tests to verify
/// events were published through the expected delivery paths.
///
/// # Response
///
/// Returns statistics for:
/// - **Router stats**: durable_routed, fast_routed, broadcast_routed counts
/// - **In-process bus stats**: handler dispatches, FFI channel dispatches
///
/// # Example Response
///
/// ```json
/// {
///   "router": {
///     "total_routed": 42,
///     "durable_routed": 10,
///     "fast_routed": 30,
///     "broadcast_routed": 2
///   },
///   "in_process_bus": {
///     "total_events_dispatched": 32,
///     "rust_handler_dispatches": 20,
///     "ffi_channel_dispatches": 12
///   },
///   "captured_at": "2025-11-30T12:00:00Z",
///   "worker_id": "worker-01234567"
/// }
/// ```
pub async fn domain_event_stats(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<DomainEventStats> {
    Json(state.metrics_service().domain_event_stats().await)
}
