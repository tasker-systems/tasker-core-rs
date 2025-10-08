//! # Orchestration Metrics (TAS-29 Phase 3.3)
//!
//! OpenTelemetry metrics for orchestration layer operations including:
//! - Task lifecycle counters (requests, completions, failures)
//! - Task initialization duration histograms
//! - Active task gauges
//! - Step enqueueing counters
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::metrics::orchestration::*;
//!
//! // Record task request
//! TASK_REQUESTS_TOTAL.add(
//!     1,
//!     &[
//!         KeyValue::new("correlation_id", correlation_id.to_string()),
//!         KeyValue::new("task_type", task_type.clone()),
//!     ],
//! );
//!
//! // Record task initialization duration
//! let start = std::time::Instant::now();
//! // ... initialization logic ...
//! let duration_ms = start.elapsed().as_millis() as f64;
//! TASK_INITIALIZATION_DURATION.record(duration_ms, &[]);
//! ```

use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter};
use std::sync::OnceLock;

/// Lazy-initialized meter for orchestration metrics
static ORCHESTRATION_METER: OnceLock<Meter> = OnceLock::new();

/// Get or initialize the orchestration meter
fn meter() -> &'static Meter {
    ORCHESTRATION_METER
        .get_or_init(|| opentelemetry::global::meter_provider().meter("tasker-orchestration"))
}

// Counters

/// Total number of task requests received
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - task_type: Task handler type
/// - namespace: Task namespace
pub fn task_requests_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.tasks.requests.total")
        .with_description("Total number of task requests received")
        .init()
}

/// Total number of tasks completed successfully
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - task_type: Task handler type
/// - namespace: Task namespace
pub fn task_completions_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.tasks.completions.total")
        .with_description("Total number of tasks completed successfully")
        .init()
}

/// Total number of tasks that failed
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - task_type: Task handler type
/// - namespace: Task namespace
/// - error_type: Category of failure
pub fn task_failures_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.tasks.failures.total")
        .with_description("Total number of tasks that failed")
        .init()
}

/// Total number of steps enqueued for execution
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - namespace: Worker namespace
/// - step_name: Name of the step
pub fn steps_enqueued_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.steps.enqueued.total")
        .with_description("Total number of steps enqueued for execution")
        .init()
}

/// Total number of step results processed
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - namespace: Worker namespace
/// - result_type: success, error, cancelled
pub fn step_results_processed_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.steps.results_processed.total")
        .with_description("Total number of step results processed")
        .init()
}

// Histograms

/// Task initialization duration in milliseconds
///
/// Tracks time from task request to first steps enqueued.
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - task_type: Task handler type
pub fn task_initialization_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.task.initialization.duration")
        .with_description("Task initialization duration in milliseconds")
        .with_unit("ms")
        .init()
}

/// Task finalization duration in milliseconds
///
/// Tracks time to finalize task after last step completion.
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - final_state: Task final state (complete, error, cancelled)
pub fn task_finalization_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.task.finalization.duration")
        .with_description("Task finalization duration in milliseconds")
        .with_unit("ms")
        .init()
}

/// Step result processing duration in milliseconds
///
/// Tracks time to process step result and discover next steps.
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - result_type: success, error, cancelled
pub fn step_result_processing_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.step_result.processing.duration")
        .with_description("Step result processing duration in milliseconds")
        .with_unit("ms")
        .init()
}

// Gauges

/// Number of tasks currently in active processing states
///
/// Active states: Initializing, EnqueuingSteps, StepsInProcess, EvaluatingResults
///
/// Labels:
/// - state: Current task state
pub fn active_tasks() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.tasks.active")
        .with_description("Number of tasks currently in active processing states")
        .init()
}

/// Number of steps ready to be enqueued
///
/// Tracks backlog of ready steps awaiting worker assignment.
///
/// Labels:
/// - namespace: Worker namespace
pub fn ready_steps() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.steps.ready")
        .with_description("Number of steps ready to be enqueued")
        .init()
}

// Static instances for convenience

/// Static counter: task_requests_total
pub static TASK_REQUESTS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: task_completions_total
pub static TASK_COMPLETIONS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: task_failures_total
pub static TASK_FAILURES_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: steps_enqueued_total
pub static STEPS_ENQUEUED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: step_results_processed_total
pub static STEP_RESULTS_PROCESSED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static histogram: task_initialization_duration
pub static TASK_INITIALIZATION_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: task_finalization_duration
pub static TASK_FINALIZATION_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: step_result_processing_duration
pub static STEP_RESULT_PROCESSING_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static gauge: active_tasks
pub static ACTIVE_TASKS: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static gauge: ready_steps
pub static READY_STEPS: OnceLock<Gauge<u64>> = OnceLock::new();

/// Initialize all orchestration metrics
///
/// This should be called during application startup after init_metrics().
pub fn init() {
    TASK_REQUESTS_TOTAL.get_or_init(task_requests_total);
    TASK_COMPLETIONS_TOTAL.get_or_init(task_completions_total);
    TASK_FAILURES_TOTAL.get_or_init(task_failures_total);
    STEPS_ENQUEUED_TOTAL.get_or_init(steps_enqueued_total);
    STEP_RESULTS_PROCESSED_TOTAL.get_or_init(step_results_processed_total);
    TASK_INITIALIZATION_DURATION.get_or_init(task_initialization_duration);
    TASK_FINALIZATION_DURATION.get_or_init(task_finalization_duration);
    STEP_RESULT_PROCESSING_DURATION.get_or_init(step_result_processing_duration);
    ACTIVE_TASKS.get_or_init(active_tasks);
    READY_STEPS.get_or_init(ready_steps);
}
