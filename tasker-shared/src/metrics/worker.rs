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
        .init()
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
        .init()
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
        .init()
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
        .init()
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
        .init()
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
        .init()
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
        .init()
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
        .init()
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
        .init()
}

/// Current queue depth per namespace
///
/// Labels:
/// - namespace: Worker namespace
pub fn queue_depth() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.queue.depth")
        .with_description("Current queue depth per namespace")
        .init()
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
/// Tracks all state machine transitions for workflow steps.
///
/// Labels:
/// - task_uuid: Parent task UUID
/// - from_state: Source state (pending, enqueued, in_progress, etc.)
/// - to_state: Target state
/// - namespace: Worker namespace
pub fn step_state_transitions_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.step.state_transitions.total")
        .with_description("Total number of step state transitions")
        .init()
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
        .init()
}

// TAS-65: Step state metrics statics

/// Static counter: step_state_transitions_total
pub static STEP_STATE_TRANSITIONS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: step_attempts_total
pub static STEP_ATTEMPTS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

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
}
