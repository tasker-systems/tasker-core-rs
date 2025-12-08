//! # Web Database Circuit Breaker
//!
//! Circuit breaker implementation specifically for web API database operations.
//! Protects against database connection failures and query timeouts without
//! interfering with the orchestration system's PGMQ operations.

use crate::web::state::AppState;
use opentelemetry::KeyValue;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tasker_shared::metrics::orchestration::api_requests_rejected_total;
use tasker_shared::types::web::{ApiError, ApiResult};
use tracing::{debug, warn};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed = 0,   // Normal operation
    Open = 1,     // Failing fast
    HalfOpen = 2, // Testing recovery
}

impl From<u8> for CircuitState {
    fn from(value: u8) -> Self {
        match value {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed, // Default to closed for invalid values
        }
    }
}

/// Web-specific circuit breaker for database health monitoring
///
/// This circuit breaker is designed specifically for web API database operations
/// and operates independently of the orchestration system's circuit breakers.
///
/// # States
/// - **Closed**: Normal operation, all requests pass through
/// - **Open**: Too many failures, reject requests with fast failure
/// - **Half-Open**: Testing if the database has recovered
#[derive(Debug, Clone)]
pub struct WebDatabaseCircuitBreaker {
    /// Number of failures needed to open the circuit
    failure_threshold: u32,
    /// How long to wait before testing recovery
    recovery_timeout: Duration,
    /// Current failure count
    current_failures: Arc<AtomicU32>,
    /// Timestamp of last failure (seconds since UNIX epoch)
    last_failure_time: Arc<AtomicU64>,
    /// Current circuit state (0 = Closed, 1 = Open, 2 = HalfOpen)
    state: Arc<AtomicU8>,
    /// Component name for logging
    component_name: String,
}

impl WebDatabaseCircuitBreaker {
    /// Create a new circuit breaker for web database operations
    ///
    /// # Arguments
    /// * `failure_threshold` - Number of failures before opening circuit
    /// * `recovery_timeout` - Duration to wait before testing recovery
    /// * `component_name` - Name for logging and monitoring
    pub fn new(
        failure_threshold: u32,
        recovery_timeout: Duration,
        component_name: impl Into<String>,
    ) -> Self {
        Self {
            failure_threshold,
            recovery_timeout,
            current_failures: Arc::new(AtomicU32::new(0)),
            last_failure_time: Arc::new(AtomicU64::new(0)),
            state: Arc::new(AtomicU8::new(CircuitState::Closed as u8)),
            component_name: component_name.into(),
        }
    }

    /// Check if the circuit is currently open (failing fast)
    ///
    /// This also handles the transition from Open to Half-Open when
    /// the recovery timeout has elapsed.
    pub fn is_circuit_open(&self) -> bool {
        let current_state = CircuitState::from(self.state.load(Ordering::Relaxed));

        match current_state {
            CircuitState::Closed => false,
            CircuitState::HalfOpen => false, // Allow requests in half-open state
            CircuitState::Open => {
                // Check if recovery timeout has passed
                let now = self.current_timestamp();
                let last_failure = self.last_failure_time.load(Ordering::Relaxed);

                if now.saturating_sub(last_failure) >= self.recovery_timeout.as_secs() {
                    // Transition to half-open state
                    debug!(
                        component = %self.component_name,
                        "Circuit breaker transitioning to half-open state for recovery testing"
                    );
                    self.state
                        .store(CircuitState::HalfOpen as u8, Ordering::Relaxed);
                    false
                } else {
                    true
                }
            }
        }
    }

    /// Record a successful operation
    ///
    /// Resets the failure count and closes the circuit if it was open.
    pub fn record_success(&self) {
        let previous_failures = self.current_failures.swap(0, Ordering::Relaxed);
        let previous_state = CircuitState::from(
            self.state
                .swap(CircuitState::Closed as u8, Ordering::Relaxed),
        );

        if previous_failures > 0 || previous_state != CircuitState::Closed {
            debug!(
                component = %self.component_name,
                previous_failures = previous_failures,
                previous_state = ?previous_state,
                "Circuit breaker recovered - resetting to closed state"
            );
        }
    }

    /// Record a failed operation
    ///
    /// Increments the failure count and opens the circuit if threshold is exceeded.
    pub fn record_failure(&self) {
        let failures = self.current_failures.fetch_add(1, Ordering::Relaxed) + 1;

        if failures >= self.failure_threshold {
            let previous_state =
                CircuitState::from(self.state.swap(CircuitState::Open as u8, Ordering::Relaxed));
            let now = self.current_timestamp();
            self.last_failure_time.store(now, Ordering::Relaxed);

            if previous_state != CircuitState::Open {
                warn!(
                    component = %self.component_name,
                    failures = failures,
                    threshold = self.failure_threshold,
                    recovery_timeout_secs = self.recovery_timeout.as_secs(),
                    "Circuit breaker opened due to repeated failures"
                );
            }
        } else {
            debug!(
                component = %self.component_name,
                failures = failures,
                threshold = self.failure_threshold,
                "Recorded database operation failure"
            );
        }
    }

    /// Get current circuit state for monitoring
    pub fn current_state(&self) -> CircuitState {
        CircuitState::from(self.state.load(Ordering::Relaxed))
    }

    /// Get current failure count
    pub fn current_failures(&self) -> u32 {
        self.current_failures.load(Ordering::Relaxed)
    }

    /// Get the component name
    pub fn component_name(&self) -> &str {
        &self.component_name
    }

    /// Get current timestamp as seconds since UNIX epoch
    fn current_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl Default for WebDatabaseCircuitBreaker {
    fn default() -> Self {
        Self::new(
            5,                       // failure_threshold
            Duration::from_secs(30), // recovery_timeout
            "web_database",          // component_name
        )
    }
}

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
///         Task::find_by_id(&state.orchestration_db_pool, task_id).await
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
        cb.state.store(CircuitState::HalfOpen as u8, std::sync::atomic::Ordering::Relaxed);

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
        // Test that the function runs without panic
        // Note: We can't easily verify OpenTelemetry metrics in unit tests,
        // but we can verify the function doesn't panic
        let error = tasker_shared::types::web::ApiError::backpressure("test", 5);
        record_backpressure_rejection("/v1/tasks", &error);

        // If we get here, the function worked
        assert!(true);
    }

    #[test]
    fn test_record_circuit_breaker_rejection_runs_without_panic() {
        // Test that the function runs without panic
        record_circuit_breaker_rejection("/v1/tasks");

        // If we get here, the function worked
        assert!(true);
    }
}
