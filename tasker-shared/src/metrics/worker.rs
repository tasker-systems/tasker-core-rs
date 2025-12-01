//! # Worker Metrics (TAS-29 Phase 3.3)
//!
//! OpenTelemetry metrics for worker layer operations including:
//! - Step execution counters (total, successes, failures)
//! - Step execution duration histograms
//! - Active step execution gauges
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::metrics::worker::*;
//! use opentelemetry::KeyValue;
//!
//! // Record step execution
//! let correlation_id = uuid::Uuid::new_v4();
//! let namespace = "payments";
//! let step_name = "process_payment";
//! step_executions_total().add(
//!     1,
//!     &[
//!         KeyValue::new("correlation_id", correlation_id.to_string()),
//!         KeyValue::new("namespace", namespace.to_string()),
//!         KeyValue::new("step_name", step_name.to_string()),
//!     ],
//! );
//!
//! // Record step execution duration
//! let start = std::time::Instant::now();
//! // ... execute step ...
//! let duration_ms = start.elapsed().as_millis() as f64;
//! step_execution_duration().record(
//!     duration_ms,
//!     &[
//!         KeyValue::new("namespace", namespace.to_string()),
//!         KeyValue::new("result", "success"),
//!     ],
//! );
//! ```

use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter};
use std::sync::OnceLock;

/// Lazy-initialized meter for worker metrics
static WORKER_METER: OnceLock<Meter> = OnceLock::new();

/// Get or initialize the worker meter
fn meter() -> &'static Meter {
    WORKER_METER.get_or_init(|| opentelemetry::global::meter_provider().meter("tasker-worker"))
}

// Counters

/// Total number of step executions attempted
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - namespace: Worker namespace
/// - step_name: Name of the step
/// - handler_type: rust, ruby
pub fn step_executions_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.steps.executions.total")
        .with_description("Total number of step executions attempted")
        .build()
}

/// Total number of step executions that completed successfully
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - namespace: Worker namespace
/// - step_name: Name of the step
/// - handler_type: rust, ruby
pub fn step_successes_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.steps.successes.total")
        .with_description("Total number of step executions that completed successfully")
        .build()
}

/// Total number of step executions that failed
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - namespace: Worker namespace
/// - step_name: Name of the step
/// - handler_type: rust, ruby
/// - error_type: Category of failure
/// - retryable: true, false
pub fn step_failures_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.steps.failures.total")
        .with_description("Total number of step executions that failed")
        .build()
}

/// Total number of steps claimed from queue
///
/// Labels:
/// - namespace: Worker namespace
/// - claim_method: event, poll
pub fn steps_claimed_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.steps.claimed.total")
        .with_description("Total number of steps claimed from queue")
        .build()
}

/// Total number of step results submitted to orchestration
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - namespace: Worker namespace
/// - result_type: success, error, cancelled
pub fn step_results_submitted_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.steps.results_submitted.total")
        .with_description("Total number of step results submitted to orchestration")
        .build()
}

// Histograms

/// Step execution duration in milliseconds
///
/// Tracks end-to-end step execution time including handler invocation.
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - namespace: Worker namespace
/// - step_name: Name of the step
/// - handler_type: rust, ruby
/// - result: success, error
pub fn step_execution_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.step.execution.duration")
        .with_description("Step execution duration in milliseconds")
        .with_unit("ms")
        .build()
}

/// Step claiming duration in milliseconds
///
/// Tracks time to claim step from queue.
///
/// Labels:
/// - namespace: Worker namespace
/// - claim_method: event, poll
pub fn step_claim_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.step.claim.duration")
        .with_description("Step claiming duration in milliseconds")
        .with_unit("ms")
        .build()
}

/// Step result submission duration in milliseconds
///
/// Tracks time to submit result back to orchestration queue.
///
/// Labels:
/// - namespace: Worker namespace
/// - result_type: success, error
pub fn step_result_submission_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.step_result.submission.duration")
        .with_description("Step result submission duration in milliseconds")
        .with_unit("ms")
        .build()
}

// Gauges

/// Number of steps currently being executed
///
/// Labels:
/// - namespace: Worker namespace
/// - handler_type: rust, ruby
pub fn active_step_executions() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.steps.active_executions")
        .with_description("Number of steps currently being executed")
        .build()
}

/// Current queue depth per namespace
///
/// Labels:
/// - namespace: Worker namespace
pub fn queue_depth() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.queue.depth")
        .with_description("Current queue depth per namespace")
        .build()
}

// Static instances for convenience

/// Static counter: step_executions_total
pub static STEP_EXECUTIONS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: step_successes_total
pub static STEP_SUCCESSES_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: step_failures_total
pub static STEP_FAILURES_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: steps_claimed_total
pub static STEPS_CLAIMED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: step_results_submitted_total
pub static STEP_RESULTS_SUBMITTED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static histogram: step_execution_duration
pub static STEP_EXECUTION_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: step_claim_duration
pub static STEP_CLAIM_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: step_result_submission_duration
pub static STEP_RESULT_SUBMISSION_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static gauge: active_step_executions
pub static ACTIVE_STEP_EXECUTIONS: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static gauge: queue_depth
pub static QUEUE_DEPTH: OnceLock<Gauge<u64>> = OnceLock::new();

// ============================================================================
// TAS-65: Step State Machine Event Metrics
// ============================================================================

/// Total number of step state transitions
///
/// Tracks all state machine transitions for workflow steps with low-cardinality labels.
///
/// Labels (low-cardinality only):
/// - namespace: Worker namespace (payments, inventory, notifications, etc.)
/// - from_state: Source state (pending, enqueued, in_progress, etc.)
/// - to_state: Target state
///
/// Note: High-cardinality IDs (step_uuid, task_uuid) are in spans/logs, not metrics.
pub fn step_state_transitions_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.step.state_transitions.total")
        .with_description("Total number of step state transitions")
        .build()
}

/// Step attempt counts by outcome
///
/// Tracks retry behavior and success/failure patterns.
///
/// Labels:
/// - handler_name: Step handler name
/// - namespace: Worker namespace
/// - outcome: success, error, cancelled, resolved_manually
/// - attempt_number: 1, 2, 3, etc.
pub fn step_attempts_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.step.attempts.total")
        .with_description("Step attempt counts by outcome")
        .build()
}

// TAS-65: Step state metrics statics

/// Static counter: step_state_transitions_total
pub static STEP_STATE_TRANSITIONS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: step_attempts_total
pub static STEP_ATTEMPTS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

// ============================================================================
// TAS-65/TAS-69: Domain Event Publishing Metrics
// ============================================================================

/// Total number of domain events dispatched to the event system
///
/// Labels:
/// - namespace: Worker namespace
/// - event_name: Domain event name (e.g., "order.processed", "payment.completed")
/// - delivery_mode: durable, fast
pub fn domain_events_dispatched_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.domain_events.dispatched.total")
        .with_description("Total number of domain events dispatched")
        .build()
}

/// Total number of domain events successfully published
///
/// Labels:
/// - namespace: Worker namespace
/// - event_name: Domain event name
/// - delivery_mode: durable, fast
/// - publisher: Publisher name (default, custom)
pub fn domain_events_published_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.domain_events.published.total")
        .with_description("Total number of domain events successfully published")
        .build()
}

/// Total number of domain events that failed to publish
///
/// Labels:
/// - namespace: Worker namespace
/// - event_name: Domain event name
/// - delivery_mode: durable, fast
/// - error_type: channel_full, publish_failed, etc.
pub fn domain_events_failed_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.domain_events.failed.total")
        .with_description("Total number of domain events that failed to publish")
        .build()
}

/// Total number of domain events dropped due to backpressure
///
/// Labels:
/// - namespace: Worker namespace
/// - event_name: Domain event name
/// - delivery_mode: durable, fast
pub fn domain_events_dropped_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.domain_events.dropped.total")
        .with_description("Total number of domain events dropped due to backpressure")
        .build()
}

/// Domain event publishing duration in milliseconds
///
/// Labels:
/// - namespace: Worker namespace
/// - delivery_mode: durable, fast
/// - result: success, error
pub fn domain_event_publish_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.domain_event.publish.duration")
        .with_description("Domain event publishing duration in milliseconds")
        .with_unit("ms")
        .build()
}

/// Current domain event channel depth (approximate)
///
/// Labels:
/// - channel_type: command, notification
pub fn domain_event_channel_depth() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.domain_events.channel_depth")
        .with_description("Current domain event channel depth")
        .build()
}

// TAS-65/TAS-69: Domain event metrics statics

/// Static counter: domain_events_dispatched_total
pub static DOMAIN_EVENTS_DISPATCHED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: domain_events_published_total
pub static DOMAIN_EVENTS_PUBLISHED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: domain_events_failed_total
pub static DOMAIN_EVENTS_FAILED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: domain_events_dropped_total
pub static DOMAIN_EVENTS_DROPPED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static histogram: domain_event_publish_duration
pub static DOMAIN_EVENT_PUBLISH_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static gauge: domain_event_channel_depth
pub static DOMAIN_EVENT_CHANNEL_DEPTH: OnceLock<Gauge<u64>> = OnceLock::new();

// ============================================================================
// TAS-65: Domain Event System Statistics (Canonical Location)
// ============================================================================

/// Statistics for the domain event system
///
/// Used for:
/// - Internal tracking via atomic counters
/// - API responses via /debug/events endpoint
/// - GetStats command responses
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DomainEventSystemStats {
    /// Total events dispatched (sent to command channel)
    pub events_dispatched: u64,
    /// Events successfully published
    pub events_published: u64,
    /// Events that failed to publish
    pub events_failed: u64,
    /// Events dropped due to channel backpressure
    pub events_dropped: u64,
    /// Events routed via durable path (PGMQ)
    pub durable_events: u64,
    /// Events routed via fast path (in-process)
    pub fast_events: u64,
    /// Events routed via broadcast path (both PGMQ and in-process)
    pub broadcast_events: u64,
    /// Last event published timestamp
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Current channel depth (approximate)
    pub channel_depth: usize,
    /// Channel capacity
    pub channel_capacity: usize,
}

/// Result of domain event system shutdown
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DomainEventShutdownResult {
    /// Whether shutdown completed successfully
    pub success: bool,
    /// Number of events drained during shutdown
    pub events_drained: u64,
    /// Time taken to shutdown in milliseconds
    pub duration_ms: u64,
}

// ============================================================================
// TAS-65: Event Router Statistics (Canonical Location)
// ============================================================================

/// Statistics for the event router
///
/// Tracks how events are routed based on delivery mode.
/// Used by both the event router implementation and the /debug/events API.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EventRouterStats {
    /// Total events routed through the router
    pub total_routed: u64,
    /// Events sent via durable path (PGMQ)
    pub durable_routed: u64,
    /// Events sent via fast path (in-process)
    pub fast_routed: u64,
    /// Events broadcast to both paths
    pub broadcast_routed: u64,
    /// Fast delivery errors in broadcast mode (non-fatal, logged for monitoring)
    pub fast_delivery_errors: u64,
    /// Failed routing attempts (durable failures only)
    pub routing_errors: u64,
}

// ============================================================================
// TAS-65: In-Process Event Bus Statistics (Canonical Location)
// ============================================================================

/// Statistics for the in-process event bus
///
/// Tracks event delivery to in-memory subscribers.
/// Used by both the event bus implementation and the /debug/events API.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct InProcessEventBusStats {
    /// Total events dispatched through the bus
    pub total_events_dispatched: u64,
    /// Total events dispatched to Rust handlers
    pub rust_handler_dispatches: u64,
    /// Total events dispatched to FFI channel
    pub ffi_channel_dispatches: u64,
    /// Total Rust handler errors (logged, not propagated)
    pub rust_handler_errors: u64,
    /// Total FFI channel drops (no subscribers)
    pub ffi_channel_drops: u64,
    /// Number of registered Rust subscriber patterns
    pub rust_subscriber_patterns: usize,
    /// Number of registered Rust handlers
    pub rust_handler_count: usize,
    /// Current FFI channel subscriber count
    pub ffi_subscriber_count: usize,
}

/// Initialize all worker metrics
///
/// This should be called during application startup after init_metrics().
pub fn init() {
    STEP_EXECUTIONS_TOTAL.get_or_init(step_executions_total);
    STEP_SUCCESSES_TOTAL.get_or_init(step_successes_total);
    STEP_FAILURES_TOTAL.get_or_init(step_failures_total);
    STEPS_CLAIMED_TOTAL.get_or_init(steps_claimed_total);
    STEP_RESULTS_SUBMITTED_TOTAL.get_or_init(step_results_submitted_total);
    STEP_EXECUTION_DURATION.get_or_init(step_execution_duration);
    STEP_CLAIM_DURATION.get_or_init(step_claim_duration);
    STEP_RESULT_SUBMISSION_DURATION.get_or_init(step_result_submission_duration);
    ACTIVE_STEP_EXECUTIONS.get_or_init(active_step_executions);
    QUEUE_DEPTH.get_or_init(queue_depth);

    // TAS-65: Step state machine metrics
    STEP_STATE_TRANSITIONS_TOTAL.get_or_init(step_state_transitions_total);
    STEP_ATTEMPTS_TOTAL.get_or_init(step_attempts_total);

    // TAS-65/TAS-69: Domain event publishing metrics
    DOMAIN_EVENTS_DISPATCHED_TOTAL.get_or_init(domain_events_dispatched_total);
    DOMAIN_EVENTS_PUBLISHED_TOTAL.get_or_init(domain_events_published_total);
    DOMAIN_EVENTS_FAILED_TOTAL.get_or_init(domain_events_failed_total);
    DOMAIN_EVENTS_DROPPED_TOTAL.get_or_init(domain_events_dropped_total);
    DOMAIN_EVENT_PUBLISH_DURATION.get_or_init(domain_event_publish_duration);
    DOMAIN_EVENT_CHANNEL_DEPTH.get_or_init(domain_event_channel_depth);
}
