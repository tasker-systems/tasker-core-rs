//! # Web Database Circuit Breaker Helpers
//!
//! Helper functions for circuit breaker operations in the web API.
//! The core circuit breaker types are in `api_common::circuit_breaker`.

use crate::web::state::AppState;
use opentelemetry::KeyValue;
use tasker_shared::metrics::orchestration::api_requests_rejected_total;
use tasker_shared::types::web::{ApiError, ApiResult};
use tracing::warn;

// Re-export types from api_common for backward compatibility
pub use crate::api_common::circuit_breaker::{CircuitState, WebDatabaseCircuitBreaker};

/// Execute a database operation with circuit breaker protection and error mapping
///
/// This function provides a standardized way to wrap database operations with
/// circuit breaker protection and automatic error handling for web API endpoints.
///
/// # Features
/// - **Circuit Breaker Protection**: Checks database health before execution
/// - **Automatic Success/Failure Recording**: Records operation results for circuit breaker state
/// - **Error Mapping**: Converts database errors to appropriate API errors
/// - **Comprehensive Logging**: Logs errors with context for debugging
/// - **Metrics Recording**: Records circuit breaker rejections (TAS-75)
///
/// # Arguments
/// * `state` - Application state containing the circuit breaker
/// * `operation` - Async closure that performs the database operation
///
/// # Returns
/// `ApiResult<T>` - Success result or mapped API error
///
/// # Example
/// ```rust,no_run
/// use tasker_orchestration::web::circuit_breaker::execute_with_circuit_breaker;
/// use tasker_orchestration::web::state::AppState;
/// use tasker_shared::models::core::task::Task;
/// use tasker_shared::types::web::ApiResult;
///
/// async fn get_task_handler(state: &AppState, task_id: uuid::Uuid) -> ApiResult<Option<Task>> {
///     execute_with_circuit_breaker(state, || async {
///         Task::find_by_id(&state.orchestration_db_pool(), task_id).await
///     }).await
/// }
/// ```
pub async fn execute_with_circuit_breaker<T, E, F, Fut>(
    state: &AppState,
    operation: F,
) -> ApiResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    use tracing::error;

    // Check circuit breaker before executing operation
    if !state.is_database_healthy() {
        // TAS-75: Record circuit breaker rejection metric
        record_circuit_breaker_rejection("unknown");
        return Err(ApiError::CircuitBreakerOpen);
    }

    match operation().await {
        Ok(result) => {
            state.record_database_success();
            Ok(result)
        }
        Err(e) => {
            state.record_database_failure();
            error!(error = %e, "Database operation failed");
            Err(ApiError::database_error(format!("Operation failed: {e}")))
        }
    }
}

/// TAS-75: Execute an operation with comprehensive backpressure checking
///
/// This function checks all backpressure conditions before executing an operation:
/// 1. Circuit breaker state
/// 2. Command channel saturation
///
/// Returns 503 with Retry-After header when any backpressure condition is active.
///
/// # Features
/// - **Comprehensive Backpressure**: Checks circuit breaker AND channel saturation
/// - **Retry-After Headers**: Returns appropriate wait time based on condition severity
/// - **Metrics Recording**: Records backpressure rejections for monitoring
///
/// # Arguments
/// * `state` - Application state containing backpressure monitoring
/// * `endpoint` - Endpoint name for metrics (e.g., "/v1/tasks")
/// * `operation` - Async closure that performs the operation
///
/// # Returns
/// `ApiResult<T>` - Success result or `ApiError::Backpressure` with Retry-After
///
/// # Example
/// ```rust,ignore
/// use tasker_orchestration::web::circuit_breaker::execute_with_backpressure_check;
///
/// async fn create_task_handler(state: &AppState, request: CreateTaskRequest) -> ApiResult<Task> {
///     execute_with_backpressure_check(state, "/v1/tasks", || async {
///         // Task creation logic
///         Ok(task)
///     }).await
/// }
/// ```
pub async fn execute_with_backpressure_check<T, E, F, Fut>(
    state: &AppState,
    endpoint: &str,
    operation: F,
) -> ApiResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    use tracing::error;

    // TAS-75: Check comprehensive backpressure status
    if let Some(backpressure_error) = state.check_backpressure_status() {
        // Record backpressure rejection metric
        record_backpressure_rejection(endpoint, &backpressure_error);
        return Err(backpressure_error);
    }

    match operation().await {
        Ok(result) => {
            state.record_database_success();
            Ok(result)
        }
        Err(e) => {
            state.record_database_failure();
            error!(error = %e, endpoint = endpoint, "Operation failed");
            Err(ApiError::database_error(format!("Operation failed: {e}")))
        }
    }
}

/// TAS-75: Record a backpressure rejection metric
///
/// Tracks API requests rejected due to any backpressure condition.
///
/// # Arguments
/// * `endpoint` - The API endpoint that was rejected
/// * `error` - The backpressure error (for extracting reason)
pub fn record_backpressure_rejection(endpoint: &str, error: &ApiError) {
    let reason = match error {
        ApiError::Backpressure { reason, .. } => reason.as_str(),
        ApiError::CircuitBreakerOpen => "circuit_breaker",
        _ => "unknown",
    };

    let counter = api_requests_rejected_total();
    counter.add(
        1,
        &[
            KeyValue::new("endpoint", endpoint.to_string()),
            KeyValue::new("reason", reason.to_string()),
        ],
    );

    warn!(
        endpoint = endpoint,
        reason = reason,
        "API request rejected due to backpressure"
    );
}

/// Record a circuit breaker rejection metric
///
/// TAS-75: Tracks API requests rejected due to circuit breaker being open.
///
/// # Arguments
/// * `endpoint` - The API endpoint that was rejected (e.g., "/v1/tasks")
pub fn record_circuit_breaker_rejection(endpoint: &str) {
    // Get the counter from the static or create a new one
    // Note: api_requests_rejected_total() returns a new counter each time,
    // but OpenTelemetry will aggregate them by the same metric name
    let counter = api_requests_rejected_total();

    counter.add(
        1,
        &[
            KeyValue::new("endpoint", endpoint.to_string()),
            KeyValue::new("reason", "circuit_breaker"),
        ],
    );

    warn!(
        endpoint = endpoint,
        reason = "circuit_breaker",
        "API request rejected due to circuit breaker open"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = WebDatabaseCircuitBreaker::new(3, Duration::from_secs(5), "test");
        assert!(!cb.is_circuit_open());
        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_opens_after_threshold_failures() {
        let cb = WebDatabaseCircuitBreaker::new(3, Duration::from_secs(5), "test");

        // Record failures below threshold
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_circuit_open());
        assert_eq!(cb.current_state(), CircuitState::Closed);

        // Record failure that exceeds threshold
        cb.record_failure();
        assert!(cb.is_circuit_open());
        assert_eq!(cb.current_state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_closes_on_success() {
        let cb = WebDatabaseCircuitBreaker::new(2, Duration::from_secs(5), "test");

        // Open the circuit
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_circuit_open());

        // Success should close the circuit (after recovery timeout or in half-open)
        cb.record_success();
        assert!(!cb.is_circuit_open());
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(cb.current_failures(), 0);
    }

    // Note: Recovery timeout tests are timing-sensitive and can be flaky in CI environments.
    // The core circuit breaker functionality (open/close/threshold) is tested above.
    // Integration tests will verify the full behavior in real scenarios.

    // =========================================================================
    // TAS-75: Extended Circuit Breaker Tests
    // =========================================================================

    #[test]
    fn test_circuit_state_from_u8_conversion() {
        assert_eq!(CircuitState::from(0), CircuitState::Closed);
        assert_eq!(CircuitState::from(1), CircuitState::Open);
        assert_eq!(CircuitState::from(2), CircuitState::HalfOpen);
        // Invalid values default to Closed
        assert_eq!(CircuitState::from(3), CircuitState::Closed);
        assert_eq!(CircuitState::from(255), CircuitState::Closed);
    }

    #[test]
    fn test_default_circuit_breaker_configuration() {
        let cb = WebDatabaseCircuitBreaker::default();

        // Default values: 5 failures, 30s recovery, "web_database" component
        assert_eq!(cb.component_name(), "web_database");
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(cb.current_failures(), 0);
        assert!(!cb.is_circuit_open());
    }

    #[test]
    fn test_component_name_accessor() {
        let cb = WebDatabaseCircuitBreaker::new(5, Duration::from_secs(30), "custom_component");
        assert_eq!(cb.component_name(), "custom_component");
    }

    #[test]
    fn test_failure_count_increments_correctly() {
        let cb = WebDatabaseCircuitBreaker::new(10, Duration::from_secs(30), "test");

        assert_eq!(cb.current_failures(), 0);
        cb.record_failure();
        assert_eq!(cb.current_failures(), 1);
        cb.record_failure();
        assert_eq!(cb.current_failures(), 2);
        cb.record_failure();
        assert_eq!(cb.current_failures(), 3);
    }

    #[test]
    fn test_success_resets_failure_count() {
        let cb = WebDatabaseCircuitBreaker::new(10, Duration::from_secs(30), "test");

        // Accumulate some failures (but not enough to open)
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.current_failures(), 3);

        // Success should reset count
        cb.record_success();
        assert_eq!(cb.current_failures(), 0);
    }

    #[test]
    fn test_success_failure_success_sequence() {
        let cb = WebDatabaseCircuitBreaker::new(3, Duration::from_secs(30), "test");

        // Start with failures
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.current_failures(), 2);
        assert_eq!(cb.current_state(), CircuitState::Closed);

        // Success resets
        cb.record_success();
        assert_eq!(cb.current_failures(), 0);

        // More failures
        cb.record_failure();
        assert_eq!(cb.current_failures(), 1);

        // Another success
        cb.record_success();
        assert_eq!(cb.current_failures(), 0);
    }

    #[test]
    fn test_half_open_state_allows_requests() {
        let cb = WebDatabaseCircuitBreaker::new(2, Duration::from_secs(30), "test");

        // Manually set to half-open state (simulating recovery timeout elapsed)
        cb.state.store(
            CircuitState::HalfOpen as u8,
            std::sync::atomic::Ordering::Relaxed,
        );

        // Half-open should allow requests (is_circuit_open returns false)
        assert!(!cb.is_circuit_open());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_exact_threshold() {
        // Test that circuit opens at exactly the threshold, not before
        let cb = WebDatabaseCircuitBreaker::new(5, Duration::from_secs(30), "test");

        // 1 through 4 failures - should stay closed
        for i in 1..5 {
            cb.record_failure();
            assert!(
                !cb.is_circuit_open(),
                "Circuit should be closed at {} failures (threshold is 5)",
                i
            );
        }

        // 5th failure - should open
        cb.record_failure();
        assert!(
            cb.is_circuit_open(),
            "Circuit should be open at threshold (5 failures)"
        );
    }

    #[test]
    fn test_multiple_successes_keep_circuit_closed() {
        let cb = WebDatabaseCircuitBreaker::new(3, Duration::from_secs(30), "test");

        // Multiple successes when healthy
        cb.record_success();
        cb.record_success();
        cb.record_success();

        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(cb.current_failures(), 0);
    }

    #[test]
    fn test_open_circuit_stays_open_without_recovery() {
        let cb = WebDatabaseCircuitBreaker::new(2, Duration::from_secs(3600), "test"); // 1 hour recovery

        // Open the circuit
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_circuit_open());

        // Additional failures should keep it open
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_circuit_open());
        assert_eq!(cb.current_state(), CircuitState::Open);
    }

    #[test]
    fn test_success_from_open_state_closes_circuit() {
        let cb = WebDatabaseCircuitBreaker::new(2, Duration::from_secs(30), "test");

        // Open the circuit
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_circuit_open());

        // Direct success call closes it (simulates half-open test succeeded)
        cb.record_success();
        assert!(!cb.is_circuit_open());
        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    // =========================================================================
    // TAS-75: Backpressure Recording Tests
    // =========================================================================

    #[test]
    fn test_record_backpressure_rejection_runs_without_panic() {
        // Smoke test: verify the function completes without panic
        // Note: We can't easily verify OpenTelemetry metrics in unit tests,
        // but test completion without panic confirms the function works
        let error = tasker_shared::types::web::ApiError::backpressure("test", 5);
        record_backpressure_rejection("/v1/tasks", &error);
        // Test passes if we reach here without panicking
    }

    #[test]
    fn test_record_circuit_breaker_rejection_runs_without_panic() {
        // Smoke test: verify the function completes without panic
        record_circuit_breaker_rejection("/v1/tasks");
        // Test passes if we reach here without panicking
    }
}
