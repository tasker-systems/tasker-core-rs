//! # Task Readiness Circuit Breaker
//!
//! TAS-75 Phase 5b: Circuit breaker for task readiness polling operations.
//!
//! This circuit breaker protects the fallback poller from cascading failures
//! when the database is unavailable or under stress. It follows the same pattern
//! as `WebDatabaseCircuitBreaker` in the web module.
//!
//! ## Design Rationale
//!
//! The fallback poller queries the database periodically to find ready tasks.
//! Without protection, repeated database failures can:
//! - Exhaust connection pool resources
//! - Waste CPU cycles on futile retry attempts
//! - Contribute to cascading failures during outages
//!
//! ## States
//!
//! - **Closed**: Normal operation, polling proceeds
//! - **Open**: Failing fast, skip polling cycles
//! - **Half-Open**: Testing recovery after timeout

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Circuit breaker states (matches web/circuit_breaker.rs for consistency)
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

/// Configuration for the task readiness circuit breaker
#[derive(Debug, Clone)]
pub struct TaskReadinessCircuitBreakerConfig {
    /// Number of failures needed to open the circuit
    pub failure_threshold: u32,
    /// How long to wait before testing recovery (seconds)
    pub recovery_timeout_seconds: u64,
    /// Number of successes needed in half-open to close
    pub success_threshold: u32,
}

impl Default for TaskReadinessCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 10,
            recovery_timeout_seconds: 60,
            success_threshold: 3,
        }
    }
}

/// Circuit breaker for task readiness polling operations
///
/// This circuit breaker protects the fallback poller from repeated failures
/// when the database is unavailable. It operates independently of the web
/// circuit breaker to allow fine-grained control over orchestration polling.
///
/// ## Usage
///
/// ```rust,ignore
/// let config = TaskReadinessCircuitBreakerConfig::default();
/// let breaker = TaskReadinessCircuitBreaker::new(config);
///
/// // Before polling
/// if breaker.is_circuit_open() {
///     // Skip this polling cycle
///     return;
/// }
///
/// // After polling
/// match poll_result {
///     Ok(_) => breaker.record_success(),
///     Err(_) => breaker.record_failure(),
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TaskReadinessCircuitBreaker {
    /// Number of failures needed to open the circuit
    failure_threshold: u32,
    /// How long to wait before testing recovery
    recovery_timeout: Duration,
    /// Number of successes needed in half-open to close
    success_threshold: u32,
    /// Current failure count
    current_failures: Arc<AtomicU32>,
    /// Consecutive successes in half-open state
    half_open_successes: Arc<AtomicU32>,
    /// Timestamp of last failure (seconds since UNIX epoch)
    last_failure_time: Arc<AtomicU64>,
    /// Current circuit state (0 = Closed, 1 = Open, 2 = HalfOpen)
    state: Arc<AtomicU8>,
}

impl TaskReadinessCircuitBreaker {
    /// Create a new circuit breaker with the given configuration
    pub fn new(config: TaskReadinessCircuitBreakerConfig) -> Self {
        info!(
            failure_threshold = config.failure_threshold,
            recovery_timeout_seconds = config.recovery_timeout_seconds,
            success_threshold = config.success_threshold,
            "Task readiness circuit breaker initialized"
        );

        Self {
            failure_threshold: config.failure_threshold,
            recovery_timeout: Duration::from_secs(config.recovery_timeout_seconds),
            success_threshold: config.success_threshold,
            current_failures: Arc::new(AtomicU32::new(0)),
            half_open_successes: Arc::new(AtomicU32::new(0)),
            last_failure_time: Arc::new(AtomicU64::new(0)),
            state: Arc::new(AtomicU8::new(CircuitState::Closed as u8)),
        }
    }

    /// Check if the circuit is currently open (should skip polling)
    ///
    /// This also handles the transition from Open to Half-Open when
    /// the recovery timeout has elapsed.
    pub fn is_circuit_open(&self) -> bool {
        let current_state = CircuitState::from(self.state.load(Ordering::Relaxed));

        match current_state {
            CircuitState::Closed => false,
            CircuitState::HalfOpen => false, // Allow request to test recovery
            CircuitState::Open => {
                // Check if recovery timeout has passed
                let now = self.current_timestamp();
                let last_failure = self.last_failure_time.load(Ordering::Relaxed);

                if now.saturating_sub(last_failure) >= self.recovery_timeout.as_secs() {
                    // Transition to half-open state
                    info!(
                        recovery_timeout_secs = self.recovery_timeout.as_secs(),
                        "Task readiness circuit breaker transitioning to half-open for recovery testing"
                    );
                    self.state
                        .store(CircuitState::HalfOpen as u8, Ordering::Relaxed);
                    self.half_open_successes.store(0, Ordering::Relaxed);
                    false
                } else {
                    true
                }
            }
        }
    }

    /// Record a successful polling operation
    ///
    /// In closed state: resets failure count
    /// In half-open state: increments success count, may close circuit
    pub fn record_success(&self) {
        let current_state = CircuitState::from(self.state.load(Ordering::Relaxed));

        match current_state {
            CircuitState::Closed => {
                // Reset failure count on success
                let previous_failures = self.current_failures.swap(0, Ordering::Relaxed);
                if previous_failures > 0 {
                    debug!(
                        previous_failures = previous_failures,
                        "Task readiness circuit breaker: failures reset after success"
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Count successes, close circuit if threshold reached
                let successes = self.half_open_successes.fetch_add(1, Ordering::Relaxed) + 1;
                if successes >= self.success_threshold {
                    info!(
                        successes = successes,
                        threshold = self.success_threshold,
                        "Task readiness circuit breaker recovered - closing circuit"
                    );
                    self.state
                        .store(CircuitState::Closed as u8, Ordering::Relaxed);
                    self.current_failures.store(0, Ordering::Relaxed);
                    self.half_open_successes.store(0, Ordering::Relaxed);
                } else {
                    debug!(
                        successes = successes,
                        threshold = self.success_threshold,
                        "Task readiness circuit breaker: half-open success recorded"
                    );
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle gracefully
                warn!("Task readiness circuit breaker: success recorded while circuit is open");
            }
        }
    }

    /// Record a failed polling operation
    ///
    /// Increments the failure count and opens the circuit if threshold is exceeded.
    /// In half-open state, any failure immediately opens the circuit.
    pub fn record_failure(&self) {
        let current_state = CircuitState::from(self.state.load(Ordering::Relaxed));

        match current_state {
            CircuitState::Closed => {
                let failures = self.current_failures.fetch_add(1, Ordering::Relaxed) + 1;

                if failures >= self.failure_threshold {
                    self.open_circuit();
                } else {
                    debug!(
                        failures = failures,
                        threshold = self.failure_threshold,
                        "Task readiness circuit breaker: failure recorded"
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open immediately opens circuit
                warn!("Task readiness circuit breaker: failure in half-open state, reopening");
                self.open_circuit();
            }
            CircuitState::Open => {
                // Already open, just update failure time for timeout calculation
                let now = self.current_timestamp();
                self.last_failure_time.store(now, Ordering::Relaxed);
            }
        }
    }

    /// Transition to open state
    fn open_circuit(&self) {
        let previous_state =
            CircuitState::from(self.state.swap(CircuitState::Open as u8, Ordering::Relaxed));
        let now = self.current_timestamp();
        self.last_failure_time.store(now, Ordering::Relaxed);
        self.half_open_successes.store(0, Ordering::Relaxed);

        if previous_state != CircuitState::Open {
            warn!(
                failures = self.current_failures.load(Ordering::Relaxed),
                threshold = self.failure_threshold,
                recovery_timeout_secs = self.recovery_timeout.as_secs(),
                "Task readiness circuit breaker opened due to repeated failures"
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

    /// Get half-open success count
    pub fn half_open_successes(&self) -> u32 {
        self.half_open_successes.load(Ordering::Relaxed)
    }

    /// Check if circuit is healthy (closed state)
    pub fn is_healthy(&self) -> bool {
        self.current_state() == CircuitState::Closed
    }

    /// Force the circuit open (for emergency situations)
    pub fn force_open(&self) {
        warn!("Task readiness circuit breaker: forced open");
        self.open_circuit();
    }

    /// Force the circuit closed (for emergency recovery)
    pub fn force_closed(&self) {
        warn!("Task readiness circuit breaker: forced closed");
        self.state
            .store(CircuitState::Closed as u8, Ordering::Relaxed);
        self.current_failures.store(0, Ordering::Relaxed);
        self.half_open_successes.store(0, Ordering::Relaxed);
    }

    /// Get current timestamp as seconds since UNIX epoch
    fn current_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Get metrics for health reporting
    pub fn metrics(&self) -> TaskReadinessCircuitBreakerMetrics {
        TaskReadinessCircuitBreakerMetrics {
            state: self.current_state(),
            current_failures: self.current_failures(),
            half_open_successes: self.half_open_successes(),
            failure_threshold: self.failure_threshold,
            success_threshold: self.success_threshold,
            recovery_timeout_secs: self.recovery_timeout.as_secs(),
        }
    }
}

impl Default for TaskReadinessCircuitBreaker {
    fn default() -> Self {
        Self::new(TaskReadinessCircuitBreakerConfig::default())
    }
}

/// Metrics snapshot for health reporting
#[derive(Debug, Clone)]
pub struct TaskReadinessCircuitBreakerMetrics {
    /// Current circuit state
    pub state: CircuitState,
    /// Current failure count
    pub current_failures: u32,
    /// Successes in half-open state
    pub half_open_successes: u32,
    /// Configured failure threshold
    pub failure_threshold: u32,
    /// Configured success threshold
    pub success_threshold: u32,
    /// Configured recovery timeout
    pub recovery_timeout_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let breaker = TaskReadinessCircuitBreaker::default();
        assert!(!breaker.is_circuit_open());
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        assert!(breaker.is_healthy());
    }

    #[test]
    fn test_circuit_opens_after_threshold() {
        let config = TaskReadinessCircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout_seconds: 60,
            success_threshold: 2,
        };
        let breaker = TaskReadinessCircuitBreaker::new(config);

        // Record failures below threshold
        breaker.record_failure();
        breaker.record_failure();
        assert!(!breaker.is_circuit_open());

        // Third failure should open circuit
        breaker.record_failure();
        assert!(breaker.is_circuit_open());
        assert_eq!(breaker.current_state(), CircuitState::Open);
    }

    #[test]
    fn test_success_resets_failures() {
        let config = TaskReadinessCircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout_seconds: 60,
            success_threshold: 1,
        };
        let breaker = TaskReadinessCircuitBreaker::new(config);

        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.current_failures(), 2);

        breaker.record_success();
        assert_eq!(breaker.current_failures(), 0);
    }

    #[test]
    fn test_half_open_closes_after_successes() {
        let config = TaskReadinessCircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_seconds: 0, // Immediate recovery for test
            success_threshold: 2,
        };
        let breaker = TaskReadinessCircuitBreaker::new(config);

        // Open circuit
        breaker.record_failure();
        // State should be Open after recording failure
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // is_circuit_open() with recovery_timeout=0 immediately transitions to half-open
        assert!(!breaker.is_circuit_open());
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // First success in half-open
        breaker.record_success();
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);
        assert_eq!(breaker.half_open_successes(), 1);

        // Second success should close circuit
        breaker.record_success();
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        assert!(breaker.is_healthy());
    }

    #[test]
    fn test_failure_in_half_open_reopens() {
        let config = TaskReadinessCircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_seconds: 0,
            success_threshold: 2,
        };
        let breaker = TaskReadinessCircuitBreaker::new(config);

        // Open circuit, then transition to half-open
        breaker.record_failure();
        let _ = breaker.is_circuit_open(); // Triggers transition to half-open

        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // Failure in half-open should reopen
        breaker.record_failure();
        assert_eq!(breaker.current_state(), CircuitState::Open);
    }

    #[test]
    fn test_force_operations() {
        let breaker = TaskReadinessCircuitBreaker::default();

        breaker.force_open();
        assert_eq!(breaker.current_state(), CircuitState::Open);

        breaker.force_closed();
        assert_eq!(breaker.current_state(), CircuitState::Closed);
    }

    #[test]
    fn test_metrics() {
        let config = TaskReadinessCircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout_seconds: 30,
            success_threshold: 3,
        };
        let breaker = TaskReadinessCircuitBreaker::new(config);

        breaker.record_failure();
        breaker.record_failure();

        let metrics = breaker.metrics();
        assert_eq!(metrics.state, CircuitState::Closed);
        assert_eq!(metrics.current_failures, 2);
        assert_eq!(metrics.failure_threshold, 5);
        assert_eq!(metrics.success_threshold, 3);
        assert_eq!(metrics.recovery_timeout_secs, 30);
    }
}
