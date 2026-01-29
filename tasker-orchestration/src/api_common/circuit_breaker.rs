//! # API Database Circuit Breaker
//!
//! Circuit breaker implementation for API database operations.
//! Protects against database connection failures and query timeouts without
//! interfering with the orchestration system's PGMQ operations.
//!
//! This module contains the core circuit breaker types that are shared between
//! REST and gRPC APIs. Transport-specific helper functions remain in their
//! respective modules (e.g., `web::circuit_breaker` for REST-specific helpers).

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

/// API database circuit breaker for health monitoring
///
/// This circuit breaker is designed specifically for API database operations
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
    pub(crate) state: Arc<AtomicU8>,
    /// Component name for logging
    component_name: String,
}

impl WebDatabaseCircuitBreaker {
    /// Create a new circuit breaker for API database operations
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
}
