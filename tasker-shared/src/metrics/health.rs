//! # Health Metrics (TAS-75 Phase 5d - Metrics Integration)
//!
//! OpenTelemetry metrics for health monitoring subsystem including:
//! - Health evaluation cycle counters
//! - Backpressure active/inactive gauges
//! - Queue depth current values
//! - Database connectivity status
//! - Channel saturation metrics
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::metrics::health::*;
//! use opentelemetry::KeyValue;
//!
//! // Record health evaluation
//! health_evaluations_total().add(1, &[]);
//!
//! // Record backpressure state
//! backpressure_active().record(1, &[]); // 1 = active, 0 = inactive
//!
//! // Record queue depth
//! queue_depth_current().record(
//!     500,
//!     &[KeyValue::new("queue", "orchestration_step_results")],
//! );
//! ```
//!
//! ## Integration with StatusEvaluator
//!
//! The StatusEvaluator calls these metrics functions at the end of each
//! evaluation cycle to export current health state to OpenTelemetry.

use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter};
use std::sync::OnceLock;

/// Lazy-initialized meter for health metrics
static HEALTH_METER: OnceLock<Meter> = OnceLock::new();

/// Get or initialize the health meter
fn meter() -> &'static Meter {
    HEALTH_METER.get_or_init(|| opentelemetry::global::meter_provider().meter("tasker-health"))
}

// ============================================================================
// Counters - Health Evaluation Operations
// ============================================================================

/// Total number of health evaluation cycles completed
///
/// Tracks how many times the StatusEvaluator has run a full evaluation cycle.
///
/// Labels:
/// - None (low-cardinality, high-frequency counter)
pub fn health_evaluations_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.health.evaluations.total")
        .with_description("Total number of health evaluation cycles completed")
        .build()
}

/// Total number of backpressure activations
///
/// Tracks how many times backpressure has been activated.
///
/// Labels:
/// - source: circuit_breaker, channel_saturation, queue_depth
pub fn backpressure_activations_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.health.backpressure.activations.total")
        .with_description("Total number of backpressure activations")
        .build()
}

/// Total number of health evaluation errors
///
/// Tracks evaluation cycles that failed to complete.
///
/// Labels:
/// - error_type: database, channel, queue
pub fn health_evaluation_errors_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.health.evaluation_errors.total")
        .with_description("Total number of health evaluation errors")
        .build()
}

// ============================================================================
// Gauges - Current Health State
// ============================================================================

/// Current backpressure state (0 = inactive, 1 = active)
///
/// Indicates whether the system is currently applying backpressure to incoming requests.
///
/// Labels:
/// - None
pub fn backpressure_active() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.health.backpressure.active")
        .with_description("Current backpressure state (0=inactive, 1=active)")
        .build()
}

/// Current queue depth for monitored queues
///
/// Tracks the current message count in each monitored queue.
///
/// Labels:
/// - queue: Queue name (e.g., "orchestration_step_results")
pub fn queue_depth_current() -> Gauge<i64> {
    meter()
        .i64_gauge("tasker.health.queue_depth.current")
        .with_description("Current message count in monitored queue")
        .build()
}

/// Current queue depth tier (0=Unknown, 1=Normal, 2=Warning, 3=Critical, 4=Overflow)
///
/// Indicates the severity tier of the worst queue depth.
///
/// Labels:
/// - None (aggregated worst tier)
pub fn queue_depth_tier() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.health.queue_depth.tier")
        .with_description(
            "Current queue depth tier (0=Unknown, 1=Normal, 2=Warning, 3=Critical, 4=Overflow)",
        )
        .build()
}

/// Current command channel saturation percentage (0-100)
///
/// Tracks the saturation level of the orchestration command channel.
///
/// Labels:
/// - None
pub fn channel_saturation_percent() -> Gauge<f64> {
    meter()
        .f64_gauge("tasker.health.channel.saturation_percent")
        .with_description("Current command channel saturation percentage (0-100)")
        .build()
}

/// Current database connectivity status (0 = disconnected, 1 = connected)
///
/// Indicates whether the database is responsive.
///
/// Labels:
/// - None
pub fn database_connected() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.health.database.connected")
        .with_description("Current database connectivity status (0=disconnected, 1=connected)")
        .build()
}

/// Current circuit breaker failure count
///
/// Tracks the number of consecutive failures on the circuit breaker.
///
/// Labels:
/// - component: Component protected by circuit breaker (e.g., "web_database")
pub fn circuit_breaker_failures() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.health.circuit_breaker.failures")
        .with_description("Current circuit breaker consecutive failure count")
        .build()
}

// ============================================================================
// Histograms - Health Check Durations
// ============================================================================

/// Health evaluation cycle duration in milliseconds
///
/// Tracks time to complete a full health evaluation cycle.
///
/// Labels:
/// - None
pub fn health_evaluation_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.health.evaluation.duration")
        .with_description("Health evaluation cycle duration in milliseconds")
        .with_unit("ms")
        .build()
}

/// Database health check duration in milliseconds
///
/// Tracks time to complete a database health check query.
///
/// Labels:
/// - None
pub fn database_check_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.health.database_check.duration")
        .with_description("Database health check duration in milliseconds")
        .with_unit("ms")
        .build()
}

// ============================================================================
// Static instances for convenience
// ============================================================================

/// Static counter: health_evaluations_total
pub static HEALTH_EVALUATIONS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: backpressure_activations_total
pub static BACKPRESSURE_ACTIVATIONS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: health_evaluation_errors_total
pub static HEALTH_EVALUATION_ERRORS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static gauge: backpressure_active
pub static BACKPRESSURE_ACTIVE: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static gauge: queue_depth_current
pub static QUEUE_DEPTH_CURRENT: OnceLock<Gauge<i64>> = OnceLock::new();

/// Static gauge: queue_depth_tier
pub static QUEUE_DEPTH_TIER: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static gauge: channel_saturation_percent
pub static CHANNEL_SATURATION_PERCENT: OnceLock<Gauge<f64>> = OnceLock::new();

/// Static gauge: database_connected
pub static DATABASE_CONNECTED: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static gauge: circuit_breaker_failures
pub static CIRCUIT_BREAKER_FAILURES: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static histogram: health_evaluation_duration
pub static HEALTH_EVALUATION_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: database_check_duration
pub static DATABASE_CHECK_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Initialize all health metrics
///
/// This should be called during application startup after init_metrics().
pub fn init() {
    HEALTH_EVALUATIONS_TOTAL.get_or_init(health_evaluations_total);
    BACKPRESSURE_ACTIVATIONS_TOTAL.get_or_init(backpressure_activations_total);
    HEALTH_EVALUATION_ERRORS_TOTAL.get_or_init(health_evaluation_errors_total);
    BACKPRESSURE_ACTIVE.get_or_init(backpressure_active);
    QUEUE_DEPTH_CURRENT.get_or_init(queue_depth_current);
    QUEUE_DEPTH_TIER.get_or_init(queue_depth_tier);
    CHANNEL_SATURATION_PERCENT.get_or_init(channel_saturation_percent);
    DATABASE_CONNECTED.get_or_init(database_connected);
    CIRCUIT_BREAKER_FAILURES.get_or_init(circuit_breaker_failures);
    HEALTH_EVALUATION_DURATION.get_or_init(health_evaluation_duration);
    DATABASE_CHECK_DURATION.get_or_init(database_check_duration);

    tracing::debug!("Health metrics initialized");
}

// ============================================================================
// Metrics Recording Helper Functions
// ============================================================================

/// Record a complete health evaluation cycle's metrics
///
/// This function is designed to be called from StatusEvaluator at the end of
/// each evaluation cycle, updating all health-related gauges and counters.
///
/// # Arguments
/// * `duration_ms` - Duration of the evaluation cycle in milliseconds
/// * `backpressure_active` - Whether backpressure is currently active
/// * `backpressure_source` - Source of backpressure if active (for activation counter)
/// * `db_connected` - Whether database is connected
/// * `db_check_duration_ms` - Duration of database health check
/// * `circuit_breaker_failures` - Current failure count on circuit breaker
/// * `channel_saturation` - Current channel saturation percentage
/// * `queue_tier` - Current queue depth tier (as numeric value)
/// * `queue_depths` - Map of queue names to current depths
pub fn record_evaluation_cycle(
    duration_ms: f64,
    is_backpressure_active: bool,
    backpressure_source: Option<&str>,
    db_connected: bool,
    db_check_duration_ms: Option<f64>,
    cb_failures: u32,
    channel_saturation: f64,
    queue_tier_value: u8,
    queue_depths: &[(String, i64)],
) {
    use opentelemetry::KeyValue;

    // Record evaluation count and duration
    if let Some(counter) = HEALTH_EVALUATIONS_TOTAL.get() {
        counter.add(1, &[]);
    }

    if let Some(histogram) = HEALTH_EVALUATION_DURATION.get() {
        histogram.record(duration_ms, &[]);
    }

    // Record backpressure state
    if let Some(gauge) = BACKPRESSURE_ACTIVE.get() {
        gauge.record(u64::from(is_backpressure_active), &[]);
    }

    // Record backpressure activation if newly active
    if is_backpressure_active {
        if let Some(counter) = BACKPRESSURE_ACTIVATIONS_TOTAL.get() {
            let labels = match backpressure_source {
                Some(source) => vec![KeyValue::new("source", source.to_string())],
                None => vec![KeyValue::new("source", "unknown")],
            };
            counter.add(1, &labels);
        }
    }

    // Record database status
    if let Some(gauge) = DATABASE_CONNECTED.get() {
        gauge.record(u64::from(db_connected), &[]);
    }

    if let Some(duration) = db_check_duration_ms {
        if let Some(histogram) = DATABASE_CHECK_DURATION.get() {
            histogram.record(duration, &[]);
        }
    }

    // Record circuit breaker failures
    if let Some(gauge) = CIRCUIT_BREAKER_FAILURES.get() {
        gauge.record(u64::from(cb_failures), &[]);
    }

    // Record channel saturation
    if let Some(gauge) = CHANNEL_SATURATION_PERCENT.get() {
        gauge.record(channel_saturation, &[]);
    }

    // Record queue depth tier
    if let Some(gauge) = QUEUE_DEPTH_TIER.get() {
        gauge.record(u64::from(queue_tier_value), &[]);
    }

    // Record individual queue depths
    if let Some(gauge) = QUEUE_DEPTH_CURRENT.get() {
        for (queue_name, depth) in queue_depths {
            gauge.record(*depth, &[KeyValue::new("queue", queue_name.clone())]);
        }
    }
}

/// Record a health evaluation error
///
/// # Arguments
/// * `error_type` - Category of error (database, channel, queue)
pub fn record_evaluation_error(error_type: &str) {
    use opentelemetry::KeyValue;

    if let Some(counter) = HEALTH_EVALUATION_ERRORS_TOTAL.get() {
        counter.add(1, &[KeyValue::new("error_type", error_type.to_string())]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_does_not_panic() {
        init();
    }

    #[test]
    fn test_counter_factories() {
        let _c = health_evaluations_total();
        let _c = backpressure_activations_total();
        let _c = health_evaluation_errors_total();
    }

    #[test]
    fn test_gauge_factories() {
        let _g = backpressure_active();
        let _g = queue_depth_current();
        let _g = queue_depth_tier();
        let _g = channel_saturation_percent();
        let _g = database_connected();
        let _g = circuit_breaker_failures();
    }

    #[test]
    fn test_histogram_factories() {
        let _h = health_evaluation_duration();
        let _h = database_check_duration();
    }

    #[test]
    fn test_record_evaluation_cycle_does_not_panic() {
        // The helper function guards each static with `if let Some()`,
        // so calling it without prior init() should not panic.
        let queue_depths = vec![
            ("orchestration_step_results".to_string(), 42_i64),
            ("worker_queue".to_string(), 7_i64),
        ];
        record_evaluation_cycle(
            12.5,                    // duration_ms
            true,                    // is_backpressure_active
            Some("circuit_breaker"), // backpressure_source
            true,                    // db_connected
            Some(3.2),               // db_check_duration_ms
            0,                       // cb_failures
            25.0,                    // channel_saturation
            1,                       // queue_tier_value
            &queue_depths,           // queue_depths
        );
    }

    #[test]
    fn test_record_evaluation_error_does_not_panic() {
        // Same guard pattern -- should not panic even without statics initialized
        record_evaluation_error("database");
        record_evaluation_error("channel");
        record_evaluation_error("queue");
    }
}
