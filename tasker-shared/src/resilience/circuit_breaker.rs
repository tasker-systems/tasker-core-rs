//! # Circuit Breaker Implementation
//!
//! Provides fault isolation patterns to prevent cascade failures in distributed systems.
//! This implementation follows the classic circuit breaker pattern with three states:
//! Closed (normal operation), Open (failing fast), and Half-Open (testing recovery).

use crate::resilience::{CircuitBreakerConfig, CircuitBreakerMetrics};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Lock-free atomic counters for circuit breaker metrics.
///
/// Eliminates tokio::Mutex contention on every `record_success`/`record_failure`
/// call in the hot path.
#[derive(Debug)]
struct AtomicCircuitBreakerMetrics {
    total_calls: AtomicU64,
    success_count: AtomicU64,
    failure_count: AtomicU64,
    consecutive_failures: AtomicU64,
    half_open_calls: AtomicU64,
    total_duration_nanos: AtomicU64,
}

impl AtomicCircuitBreakerMetrics {
    fn new() -> Self {
        Self {
            total_calls: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
            consecutive_failures: AtomicU64::new(0),
            half_open_calls: AtomicU64::new(0),
            total_duration_nanos: AtomicU64::new(0),
        }
    }

    #[inline]
    fn record_success(&self, duration: Duration) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.success_count.fetch_add(1, Ordering::Relaxed);
        self.total_duration_nanos
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    #[inline]
    fn record_failure(&self, duration: Duration) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        self.total_duration_nanos
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    #[inline]
    fn increment_consecutive_failures(&self) -> u64 {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    #[inline]
    fn reset_consecutive_failures(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    #[inline]
    fn increment_half_open_calls(&self) -> u64 {
        self.half_open_calls.fetch_add(1, Ordering::Relaxed) + 1
    }

    #[inline]
    fn reset_half_open(&self) {
        self.half_open_calls.store(0, Ordering::Relaxed);
    }

    fn snapshot(&self, state: CircuitState) -> CircuitBreakerMetrics {
        let total_calls = self.total_calls.load(Ordering::Relaxed);
        let success_count = self.success_count.load(Ordering::Relaxed);
        let failure_count = self.failure_count.load(Ordering::Relaxed);
        let total_duration_nanos = self.total_duration_nanos.load(Ordering::Relaxed);
        let total_duration = Duration::from_nanos(total_duration_nanos);

        let (failure_rate, success_rate, average_duration) = if total_calls > 0 {
            let fr = failure_count as f64 / total_calls as f64;
            let sr = success_count as f64 / total_calls as f64;
            let avg = if success_count > 0 {
                Duration::from_nanos(total_duration_nanos / success_count)
            } else {
                Duration::ZERO
            };
            (fr, sr, avg)
        } else {
            (0.0, 0.0, Duration::ZERO)
        };

        CircuitBreakerMetrics {
            total_calls,
            success_count,
            failure_count,
            consecutive_failures: self.consecutive_failures.load(Ordering::Relaxed),
            half_open_calls: self.half_open_calls.load(Ordering::Relaxed),
            total_duration,
            current_state: state,
            failure_rate,
            success_rate,
            average_duration,
        }
    }
}

/// Get current epoch nanos from SystemTime
#[inline]
fn epoch_nanos_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos() as u64
}

/// Circuit breaker states representing the current operational mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation - all calls are allowed through
    Closed = 0,
    /// Failure mode - all calls fail fast without executing
    Open = 1,
    /// Testing recovery - limited calls allowed to test system health
    HalfOpen = 2,
}

impl From<u8> for CircuitState {
    fn from(value: u8) -> Self {
        match value {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Open, // Default to safest state
        }
    }
}

/// Errors that can occur during circuit breaker operation
#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError<E> {
    /// Circuit is open, rejecting all calls
    #[error("Circuit breaker is open for {component}")]
    CircuitOpen { component: String },

    /// Operation failed and was recorded
    #[error("Operation failed: {0}")]
    OperationFailed(E),

    /// Circuit breaker configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

/// Core circuit breaker implementation with atomic state management
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Component name for logging and metrics
    name: String,

    /// Current circuit state (atomic for thread safety)
    state: AtomicU8,

    /// Configuration parameters
    config: CircuitBreakerConfig,

    /// Lock-free atomic metrics
    metrics: AtomicCircuitBreakerMetrics,

    /// Epoch nanos when circuit was opened (0 = not open).
    /// Uses Release/Acquire ordering paired with state transitions.
    opened_at_epoch_nanos: AtomicU64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given name and configuration
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        info!(
            component = %name,
            failure_threshold = config.failure_threshold,
            timeout_seconds = config.timeout.as_secs(),
            success_threshold = config.success_threshold,
            "Circuit breaker initialized"
        );

        Self {
            name,
            state: AtomicU8::new(CircuitState::Closed as u8),
            config,
            metrics: AtomicCircuitBreakerMetrics::new(),
            opened_at_epoch_nanos: AtomicU64::new(0),
        }
    }

    /// Get current circuit state
    pub fn state(&self) -> CircuitState {
        CircuitState::from(self.state.load(Ordering::Acquire))
    }

    /// Execute an operation with circuit breaker protection
    pub async fn call<F, T, E, Fut>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        // Check if circuit should allow the call
        if !self.should_allow_call() {
            return Err(CircuitBreakerError::CircuitOpen {
                component: self.name.clone(),
            });
        }

        // Execute the operation
        let start_time = std::time::Instant::now();
        let result = operation().await;
        let duration = start_time.elapsed();

        // Record the result
        match &result {
            Ok(_) => {
                self.record_success(duration);
            }
            Err(_) => {
                self.record_failure(duration);
            }
        }

        // Map error type
        result.map_err(CircuitBreakerError::OperationFailed)
    }

    /// Check if a call should be allowed based on current state
    fn should_allow_call(&self) -> bool {
        match self.state() {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                let opened_nanos = self.opened_at_epoch_nanos.load(Ordering::Acquire);
                if opened_nanos == 0 {
                    // Circuit is open but no timestamp - shouldn't happen, but allow call
                    warn!(component = %self.name, "Circuit open but no timestamp recorded");
                    return true;
                }

                let now_nanos = epoch_nanos_now();
                let elapsed_nanos = now_nanos.saturating_sub(opened_nanos);
                let timeout_nanos = self.config.timeout.as_nanos() as u64;

                if elapsed_nanos >= timeout_nanos {
                    self.transition_to_half_open();
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited calls to test recovery
                let half_open_calls = self.metrics.half_open_calls.load(Ordering::Relaxed);
                half_open_calls < self.config.success_threshold as u64
            }
        }
    }

    /// Record a successful operation (lock-free)
    fn record_success(&self, duration: Duration) {
        self.metrics.record_success(duration);

        debug!(
            component = %self.name,
            duration_ms = duration.as_millis(),
            "Operation succeeded"
        );

        match self.state() {
            CircuitState::HalfOpen => {
                let calls = self.metrics.increment_half_open_calls();
                if calls >= self.config.success_threshold as u64 {
                    self.transition_to_closed();
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success
                self.metrics.reset_consecutive_failures();
            }
            CircuitState::Open => {
                // Shouldn't happen, but log it
                warn!(component = %self.name, "Success recorded while circuit is open");
            }
        }
    }

    /// Record a failed operation (lock-free)
    fn record_failure(&self, duration: Duration) {
        self.metrics.record_failure(duration);

        error!(
            component = %self.name,
            duration_ms = duration.as_millis(),
            "Operation failed"
        );

        match self.state() {
            CircuitState::Closed => {
                let failures = self.metrics.increment_consecutive_failures();
                if failures >= self.config.failure_threshold as u64 {
                    self.transition_to_open();
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open state immediately opens circuit
                self.transition_to_open();
            }
            CircuitState::Open => {
                // Already open, just record the failure
            }
        }
    }

    /// Transition to closed state (normal operation)
    fn transition_to_closed(&self) {
        let total_calls = self.metrics.total_calls.load(Ordering::Relaxed);

        // Reset metrics for new cycle
        self.metrics.reset_consecutive_failures();
        self.metrics.reset_half_open();

        // Clear opened timestamp
        self.opened_at_epoch_nanos.store(0, Ordering::Release);

        // Store state last (after metrics reset)
        self.state
            .store(CircuitState::Closed as u8, Ordering::Release);

        info!(
            component = %self.name,
            total_calls = total_calls,
            "Circuit breaker closed (recovered)"
        );
    }

    /// Transition to open state (failing fast)
    fn transition_to_open(&self) {
        // Record when circuit was opened
        self.opened_at_epoch_nanos
            .store(epoch_nanos_now(), Ordering::Release);

        // Reset half-open call count
        self.metrics.reset_half_open();

        // Store state last
        self.state
            .store(CircuitState::Open as u8, Ordering::Release);

        let consecutive_failures = self.metrics.consecutive_failures.load(Ordering::Relaxed);
        error!(
            component = %self.name,
            consecutive_failures = consecutive_failures,
            failure_threshold = self.config.failure_threshold,
            timeout_seconds = self.config.timeout.as_secs(),
            "Circuit breaker opened (failing fast)"
        );
    }

    /// Transition to half-open state (testing recovery)
    fn transition_to_half_open(&self) {
        // Reset half-open call count
        self.metrics.reset_half_open();

        // Store state
        self.state
            .store(CircuitState::HalfOpen as u8, Ordering::Release);

        info!(
            component = %self.name,
            success_threshold = self.config.success_threshold,
            "Circuit breaker half-open (testing recovery)"
        );
    }

    /// Force circuit to open state (for emergency situations)
    pub fn force_open(&self) {
        warn!(component = %self.name, "Circuit breaker forced open");
        self.transition_to_open();
    }

    /// Force circuit to closed state (for emergency recovery)
    pub fn force_closed(&self) {
        warn!(component = %self.name, "Circuit breaker forced closed");
        self.transition_to_closed();
    }

    /// Get current metrics snapshot
    pub fn metrics(&self) -> CircuitBreakerMetrics {
        self.metrics.snapshot(self.state())
    }

    /// Get component name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if circuit is healthy (closed state with low failure rate)
    pub fn is_healthy(&self) -> bool {
        let state = self.state();
        if state != CircuitState::Closed {
            return false;
        }

        let total_calls = self.metrics.total_calls.load(Ordering::Relaxed);
        if total_calls < 10 {
            // Too few calls to determine health
            return true;
        }

        let failure_count = self.metrics.failure_count.load(Ordering::Relaxed);
        let failure_rate = failure_count as f64 / total_calls as f64;
        failure_rate < 0.1
    }

    /// Manually record a successful operation
    ///
    /// Use this when you need fine-grained control over success/failure recording,
    /// such as when implementing latency-based circuit breaking where a slow success
    /// should be treated as a failure.
    pub fn record_success_manual(&self, duration: Duration) {
        self.record_success(duration);
    }

    /// Manually record a failed operation
    ///
    /// Use this when you need fine-grained control over success/failure recording,
    /// such as when implementing latency-based circuit breaking where a slow success
    /// should be treated as a failure.
    pub fn record_failure_manual(&self, duration: Duration) {
        self.record_failure(duration);
    }

    /// Check if the circuit is allowing calls (not in Open state or timeout elapsed)
    ///
    /// Use this for pre-flight checks before attempting an operation when using
    /// manual recording. Returns true if calls should be allowed.
    pub fn should_allow(&self) -> bool {
        self.should_allow_call()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_normal_operation() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_millis(100),
            success_threshold: 2,
        };

        let circuit = CircuitBreaker::new("test".to_string(), config);

        // Should start in closed state
        assert_eq!(circuit.state(), CircuitState::Closed);

        // Successful operations should work
        let result = circuit.call(|| async { Ok::<_, String>("success") }).await;
        assert!(result.is_ok());

        let metrics = circuit.metrics();
        assert_eq!(metrics.total_calls, 1);
        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.failure_count, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(100),
            success_threshold: 2,
        };

        let circuit = CircuitBreaker::new("test".to_string(), config);

        // First failure
        let _ = circuit.call(|| async { Err::<String, _>("error") }).await;
        assert_eq!(circuit.state(), CircuitState::Closed);

        // Second failure should open circuit
        let _ = circuit.call(|| async { Err::<String, _>("error") }).await;
        assert_eq!(circuit.state(), CircuitState::Open);

        // Next call should fail fast
        let result = circuit
            .call(|| async { Ok::<_, String>("should not execute") })
            .await;
        assert!(matches!(
            result,
            Err(CircuitBreakerError::CircuitOpen { .. })
        ));
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(50),
            success_threshold: 1,
        };

        let circuit = CircuitBreaker::new("test".to_string(), config);

        // Cause circuit to open
        let _ = circuit.call(|| async { Err::<String, _>("error") }).await;
        assert_eq!(circuit.state(), CircuitState::Open);

        // Wait for timeout
        sleep(Duration::from_millis(60)).await;

        // Next call should transition to half-open and succeed
        let result = circuit.call(|| async { Ok::<_, String>("success") }).await;
        assert!(result.is_ok());
        assert_eq!(circuit.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_force_operations() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_secs(1),
            success_threshold: 1,
        };

        let circuit = CircuitBreaker::new("test".to_string(), config);

        // Force open
        circuit.force_open();
        assert_eq!(circuit.state(), CircuitState::Open);

        // Force closed
        circuit.force_closed();
        assert_eq!(circuit.state(), CircuitState::Closed);
    }
}
