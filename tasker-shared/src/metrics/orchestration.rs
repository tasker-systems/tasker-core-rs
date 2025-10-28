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
//! use opentelemetry::KeyValue;
//!
//! // Record task request
//! let correlation_id = uuid::Uuid::new_v4();
//! let task_type = "order_processing";
//! task_requests_total().add(
//!     1,
//!     &[
//!         KeyValue::new("correlation_id", correlation_id.to_string()),
//!         KeyValue::new("task_type", task_type.to_string()),
//!     ],
//! );
//!
//! // Record task initialization duration
//! let start = std::time::Instant::now();
//! // ... initialization logic ...
//! let duration_ms = start.elapsed().as_millis() as f64;
//! task_initialization_duration().record(duration_ms, &[]);
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

// ============================================================================
// TAS-48: Stale Task Discovery Metrics
// ============================================================================

/// Total number of tasks excluded from discovery due to staleness
///
/// Tracks tasks filtered out by staleness exclusion logic (stuck >60min waiting).
///
/// Labels:
/// - state: waiting_for_dependencies, waiting_for_retry
/// - time_in_state_minutes: Duration bucket (60-120, 120-360, >360)
pub fn tasks_excluded_staleness_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.tasks.excluded_staleness.total")
        .with_description("Total number of tasks excluded from discovery due to staleness")
        .init()
}

/// Distribution of computed priority values for discovered tasks
///
/// Tracks priority distribution to validate decay behavior.
///
/// Labels:
/// - task_age_bucket: fresh (<1hr), aging (1-24hr), stale (>24hr)
pub fn computed_priority_histogram() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.task.computed_priority")
        .with_description("Distribution of computed priority values")
        .init()
}

/// Task age at discovery time in seconds
///
/// Tracks how long tasks wait before being discovered.
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - task_state: pending, waiting_for_dependencies, waiting_for_retry
pub fn task_age_at_discovery_seconds() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.task.age_at_discovery")
        .with_description("Task age at discovery time in seconds")
        .with_unit("s")
        .init()
}

/// Discovery pool saturation ratio
///
/// Ratio of pre-filter candidates to limit (e.g., 500 candidates / (50 * 10) = 1.0 = saturated).
/// Values approaching 1.0 indicate discovery buffer is full.
///
/// Labels:
/// - has_stale_tasks: true/false - whether stale tasks exist in system
pub fn discovery_pool_saturation() -> Gauge<f64> {
    meter()
        .f64_gauge("tasker.discovery.pool_saturation")
        .with_description("Discovery pool saturation ratio (candidates / limit)")
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

// TAS-48: Stale task discovery metrics statics

/// Static counter: tasks_excluded_staleness_total
pub static TASKS_EXCLUDED_STALENESS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static histogram: computed_priority_histogram
pub static COMPUTED_PRIORITY_HISTOGRAM: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: task_age_at_discovery_seconds
pub static TASK_AGE_AT_DISCOVERY_SECONDS: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static gauge: discovery_pool_saturation
pub static DISCOVERY_POOL_SATURATION: OnceLock<Gauge<f64>> = OnceLock::new();

// ============================================================================
// TAS-53: Decision Point Metrics
// ============================================================================

/// Total number of decision point outcomes processed
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - decision_name: Name of the decision step
/// - outcome_type: no_branches, create_steps
pub fn decision_outcomes_processed_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.decision_points.outcomes_processed.total")
        .with_description("Total number of decision point outcomes processed")
        .init()
}

/// Total number of workflow steps created from decision points
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - decision_name: Name of the decision step
pub fn decision_steps_created_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.decision_points.steps_created.total")
        .with_description("Total number of workflow steps created from decision points")
        .init()
}

/// Total number of decision point validation errors
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - decision_name: Name of the decision step
/// - error_type: invalid_descendant, cycle_detected, step_not_found
pub fn decision_validation_errors_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.decision_points.validation_errors.total")
        .with_description("Total number of decision point validation errors")
        .init()
}

/// Total number of decision point warning thresholds exceeded
///
/// Labels:
/// - warning_type: step_count, decision_depth
/// - correlation_id: Request correlation ID
pub fn decision_warnings_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.decision_points.warnings.total")
        .with_description("Total number of decision point warning thresholds exceeded")
        .init()
}

/// Decision point processing duration in milliseconds
///
/// Tracks time to validate and create steps from decision outcomes.
///
/// Labels:
/// - correlation_id: Request correlation ID
/// - decision_name: Name of the decision step
/// - outcome_type: no_branches, create_steps
pub fn decision_processing_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.decision_point.processing.duration")
        .with_description("Decision point processing duration in milliseconds")
        .with_unit("ms")
        .init()
}

/// Distribution of step counts created by decision points
///
/// Tracks distribution to validate configuration limits.
///
/// Labels:
/// - decision_name: Name of the decision step
pub fn decision_step_count_histogram() -> Histogram<u64> {
    meter()
        .u64_histogram("tasker.decision_point.step_count")
        .with_description("Distribution of step counts created by decision points")
        .init()
}

// TAS-53: Decision point metrics statics

/// Static counter: decision_outcomes_processed_total
pub static DECISION_OUTCOMES_PROCESSED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: decision_steps_created_total
pub static DECISION_STEPS_CREATED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: decision_validation_errors_total
pub static DECISION_VALIDATION_ERRORS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: decision_warnings_total
pub static DECISION_WARNINGS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static histogram: decision_processing_duration
pub static DECISION_PROCESSING_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: decision_step_count_histogram
pub static DECISION_STEP_COUNT_HISTOGRAM: OnceLock<Histogram<u64>> = OnceLock::new();

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

    // TAS-48: Stale task discovery metrics
    TASKS_EXCLUDED_STALENESS_TOTAL.get_or_init(tasks_excluded_staleness_total);
    COMPUTED_PRIORITY_HISTOGRAM.get_or_init(computed_priority_histogram);
    TASK_AGE_AT_DISCOVERY_SECONDS.get_or_init(task_age_at_discovery_seconds);
    DISCOVERY_POOL_SATURATION.get_or_init(discovery_pool_saturation);

    // TAS-53: Decision point metrics
    DECISION_OUTCOMES_PROCESSED_TOTAL.get_or_init(decision_outcomes_processed_total);
    DECISION_STEPS_CREATED_TOTAL.get_or_init(decision_steps_created_total);
    DECISION_VALIDATION_ERRORS_TOTAL.get_or_init(decision_validation_errors_total);
    DECISION_WARNINGS_TOTAL.get_or_init(decision_warnings_total);
    DECISION_PROCESSING_DURATION.get_or_init(decision_processing_duration);
    DECISION_STEP_COUNT_HISTOGRAM.get_or_init(decision_step_count_histogram);
}
