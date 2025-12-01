//! # TAS-65: Debug Handlers for Domain Event Observability
//!
//! Endpoints for observing domain event system state for testing and debugging.
//! These endpoints expose statistics that E2E tests can use to verify events
//! were actually published through the expected delivery paths.
//!
//! ## Design Rationale
//!
//! E2E tests run workers in separate Docker containers, so they can't directly
//! observe in-memory event delivery. These debug endpoints expose statistics
//! that allow tests to verify:
//!
//! - **Durable events**: Count of events published to PGMQ
//! - **Fast events**: Count of events dispatched to in-process subscribers
//! - **Broadcast events**: Count of events sent to both paths
//!
//! ## Security Considerations
//!
//! These endpoints should be disabled or protected in production environments.
//! They are intended for development and testing only.

use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use tracing::debug;

use crate::web::state::WorkerWebState;
use tasker_shared::types::web::DomainEventStats;

/// Get domain event statistics: GET /debug/events
///
/// Returns statistics about domain event routing and delivery.
/// Used by E2E tests to verify events were published through the correct paths.
///
/// # Response
///
/// ```json
/// {
///   "router": {
///     "total_routed": 42,
///     "durable_routed": 10,
///     "fast_routed": 30,
///     "broadcast_routed": 2,
///     "fast_delivery_errors": 0,
///     "routing_errors": 0
///   },
///   "in_process_bus": {
///     "total_events_dispatched": 32,
///     "rust_handler_dispatches": 20,
///     "ffi_channel_dispatches": 12,
///     "rust_handler_errors": 0,
///     "ffi_channel_drops": 0,
///     "rust_subscriber_patterns": 2,
///     "rust_handler_count": 2,
///     "ffi_subscriber_count": 1
///   },
///   "captured_at": "2025-11-30T12:00:00Z",
///   "worker_id": "worker-01234567-89ab-cdef-0123-456789abcdef"
/// }
/// ```
///
/// # Test Usage Example
///
/// ```rust,ignore
/// // Before running task
/// let stats_before: DomainEventStats = client.get("/debug/events").await?;
///
/// // Run task that should publish events
/// client.post("/tasks", task_request).await?;
/// wait_for_task_completion(&task_uuid).await?;
///
/// // After task completion
/// let stats_after: DomainEventStats = client.get("/debug/events").await?;
///
/// // Verify events were published
/// let durable_published = stats_after.router.durable_routed - stats_before.router.durable_routed;
/// let fast_published = stats_after.router.fast_routed - stats_before.router.fast_routed;
///
/// assert!(durable_published > 0, "Expected durable events to be published");
/// assert!(fast_published > 0, "Expected fast events to be dispatched");
/// ```
pub async fn domain_event_stats(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<DomainEventStats> {
    debug!("Serving domain event statistics for debug/observability");

    // Lock the worker core to get stats
    let core = state.worker_core.lock().await;
    let stats = core.get_domain_event_stats().await;

    Json(stats)
}

/// Reset domain event statistics: POST /debug/events/reset
///
/// Resets all counters to zero. Useful for isolating test runs.
/// Currently a no-op placeholder - full implementation would require
/// mutable access to the stats structures.
///
/// # Note
///
/// For now, tests should capture stats before and after operations
/// and compare the difference, rather than relying on reset.
pub async fn reset_domain_event_stats(
    State(_state): State<Arc<WorkerWebState>>,
) -> Json<ResetResponse> {
    debug!("Reset domain event statistics requested (no-op for now)");

    // TODO: Implement actual reset if needed
    // For now, tests should use before/after comparison pattern

    Json(ResetResponse {
        success: true,
        message: "Stats reset not implemented - use before/after comparison pattern".to_string(),
    })
}

/// Response for reset operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResetResponse {
    pub success: bool,
    pub message: String,
}
