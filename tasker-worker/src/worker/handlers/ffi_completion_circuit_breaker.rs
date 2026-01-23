//! # FFI Completion Circuit Breaker
//!
//! TAS-75 Phase 5a: Latency-based circuit breaker for FFI completion channel sends.
//!
//! This circuit breaker monitors completion channel send latency, NOT handler execution time.
//! Healthy sends complete in milliseconds; slow sends indicate channel backpressure.
//!
//! ## Design Rationale
//!
//! The FFI completion flow is:
//! ```text
//! Handler completes (has its own timeout, e.g., 30s)
//!       │
//!       ▼
//! FfiDispatchChannel.complete()
//!       │
//!       ├── 1. Remove from pending_events (instant)
//!       │
//!       ├── 2. completion_sender.try_send() ← THIS IS WHAT WE PROTECT
//!       │      └── Should complete in milliseconds when healthy
//!       │
//!       └── 3. Spawn post-handler callback (fire-and-forget)
//! ```
//!
//! A healthy completion channel send completes in single-digit milliseconds.
//! If sends consistently take > 100ms, this indicates backpressure that should
//! trigger circuit breaker protection.
//!
//! ## Fail-Fast Behavior
//!
//! When the circuit is open:
//! - `complete()` returns `false` immediately
//! - Step result is NOT sent to orchestration
//! - Step remains claimed but incomplete
//! - PGMQ visibility timeout expires, step returns to queue
//! - Another worker (or same worker after recovery) claims it
//!
//! This is idempotent because the step result was never committed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tasker_shared::resilience::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
use tracing::{debug, info, warn};

/// Configuration for the FFI completion send circuit breaker
#[derive(Debug, Clone)]
pub struct FfiCompletionCircuitBreakerConfig {
    /// Number of slow/failed sends before circuit opens
    pub failure_threshold: u32,
    /// How long circuit stays open before testing recovery (seconds)
    pub recovery_timeout_seconds: u64,
    /// Successful fast sends needed in half-open to close circuit
    pub success_threshold: u32,
    /// Send latency above this threshold (ms) counts as "slow" (failure)
    pub slow_send_threshold_ms: u64,
}

impl Default for FfiCompletionCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout_seconds: 5,
            success_threshold: 2,
            slow_send_threshold_ms: 100,
        }
    }
}

impl From<tasker_shared::config::tasker::FfiCompletionSendCircuitBreakerConfig>
    for FfiCompletionCircuitBreakerConfig
{
    fn from(config: tasker_shared::config::tasker::FfiCompletionSendCircuitBreakerConfig) -> Self {
        Self {
            failure_threshold: config.failure_threshold,
            recovery_timeout_seconds: config.recovery_timeout_seconds as u64,
            success_threshold: config.success_threshold,
            slow_send_threshold_ms: config.slow_send_threshold_ms as u64,
        }
    }
}

/// Latency-based circuit breaker for FFI completion channel sends
///
/// Unlike error-based circuit breakers, this breaker treats slow sends as failures.
/// A send that takes > `slow_send_threshold_ms` is considered a sign of backpressure
/// and counts toward opening the circuit.
///
/// ## Metrics Tracked
///
/// - `slow_send_count`: Total sends exceeding slow threshold
/// - `circuit_open_rejections`: Total fail-fast rejections when circuit is open
///
/// ## Usage
///
/// ```rust,ignore
/// let config = FfiCompletionCircuitBreakerConfig::default();
/// let breaker = FfiCompletionCircuitBreaker::new(config);
///
/// // Before attempting send
/// if !breaker.should_allow() {
///     // Fail fast - circuit is open
///     return false;
/// }
///
/// // Time the send operation
/// let start = Instant::now();
/// let success = completion_sender.try_send(result).is_ok();
/// let elapsed = start.elapsed();
///
/// // Record result based on latency
/// breaker.record_send_result(elapsed, success);
/// ```
pub struct FfiCompletionCircuitBreaker {
    /// Underlying circuit breaker
    breaker: CircuitBreaker,
    /// Threshold in ms above which a send is considered "slow" (failure)
    slow_send_threshold_ms: u64,
    /// Counter for slow sends (for metrics)
    slow_send_count: AtomicU64,
    /// Counter for circuit-open rejections (for metrics)
    circuit_open_rejections: AtomicU64,
}

impl std::fmt::Debug for FfiCompletionCircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FfiCompletionCircuitBreaker")
            .field("state", &self.state())
            .field("slow_send_threshold_ms", &self.slow_send_threshold_ms)
            .field("slow_send_count", &self.slow_send_count())
            .field("circuit_open_rejections", &self.circuit_open_rejections())
            .finish()
    }
}

impl FfiCompletionCircuitBreaker {
    /// Create a new latency-based circuit breaker
    pub fn new(config: FfiCompletionCircuitBreakerConfig) -> Self {
        let breaker_config = CircuitBreakerConfig {
            failure_threshold: config.failure_threshold,
            timeout: Duration::from_secs(config.recovery_timeout_seconds),
            success_threshold: config.success_threshold,
        };

        info!(
            failure_threshold = config.failure_threshold,
            recovery_timeout_seconds = config.recovery_timeout_seconds,
            success_threshold = config.success_threshold,
            slow_send_threshold_ms = config.slow_send_threshold_ms,
            "FFI completion circuit breaker initialized"
        );

        Self {
            breaker: CircuitBreaker::new("ffi_completion_send".to_string(), breaker_config),
            slow_send_threshold_ms: config.slow_send_threshold_ms,
            slow_send_count: AtomicU64::new(0),
            circuit_open_rejections: AtomicU64::new(0),
        }
    }

    /// Check if the circuit allows calls (pre-flight check)
    ///
    /// Returns `true` if the circuit is closed or has transitioned to half-open.
    /// Returns `false` if the circuit is open and calls should fail fast.
    pub fn should_allow(&self) -> bool {
        let allowed = self.breaker.should_allow();
        if !allowed {
            self.circuit_open_rejections.fetch_add(1, Ordering::Relaxed);
            warn!(
                state = ?self.state(),
                total_rejections = self.circuit_open_rejections(),
                "FFI completion circuit breaker: rejecting send (circuit open)"
            );
        }
        allowed
    }

    /// Record a send result based on latency
    ///
    /// - Fast send (< threshold) + success: success
    /// - Slow send (>= threshold): failure (even if send succeeded)
    /// - Send error: failure
    ///
    /// # Arguments
    ///
    /// * `elapsed` - Duration of the send operation
    /// * `success` - Whether the send operation succeeded
    pub fn record_send_result(&self, elapsed: Duration, success: bool) {
        let is_slow = elapsed.as_millis() as u64 >= self.slow_send_threshold_ms;

        if is_slow {
            self.slow_send_count.fetch_add(1, Ordering::Relaxed);
            warn!(
                elapsed_ms = elapsed.as_millis(),
                threshold_ms = self.slow_send_threshold_ms,
                success = success,
                slow_send_count = self.slow_send_count(),
                "FFI completion circuit breaker: slow send detected"
            );
        }

        if !success || is_slow {
            // Treat both errors and slow sends as failures
            self.breaker.record_failure_manual(elapsed);
            debug!(
                elapsed_ms = elapsed.as_millis(),
                success = success,
                is_slow = is_slow,
                state = ?self.state(),
                "FFI completion circuit breaker: recorded failure"
            );
        } else {
            self.breaker.record_success_manual(elapsed);
            debug!(
                elapsed_ms = elapsed.as_millis(),
                state = ?self.state(),
                "FFI completion circuit breaker: recorded success"
            );
        }
    }

    /// Get the current circuit state
    pub fn state(&self) -> CircuitState {
        self.breaker.state()
    }

    /// Check if the circuit is healthy (closed state)
    pub fn is_healthy(&self) -> bool {
        self.breaker.state() == CircuitState::Closed
    }

    /// Get the total count of slow sends
    pub fn slow_send_count(&self) -> u64 {
        self.slow_send_count.load(Ordering::Relaxed)
    }

    /// Get the total count of circuit-open rejections
    pub fn circuit_open_rejections(&self) -> u64 {
        self.circuit_open_rejections.load(Ordering::Relaxed)
    }

    /// Get the slow send threshold in milliseconds
    pub fn slow_send_threshold_ms(&self) -> u64 {
        self.slow_send_threshold_ms
    }

    /// Get circuit breaker metrics
    pub fn metrics(&self) -> FfiCompletionCircuitBreakerMetrics {
        let breaker_metrics = self.breaker.metrics();
        FfiCompletionCircuitBreakerMetrics {
            state: self.state(),
            failure_count: breaker_metrics.failure_count,
            success_count: breaker_metrics.success_count,
            total_calls: breaker_metrics.total_calls,
            consecutive_failures: breaker_metrics.consecutive_failures,
            slow_send_count: self.slow_send_count(),
            circuit_open_rejections: self.circuit_open_rejections(),
            slow_send_threshold_ms: self.slow_send_threshold_ms,
        }
    }

    /// Force the circuit open (for emergency situations)
    pub fn force_open(&self) {
        warn!("FFI completion circuit breaker: forced open");
        self.breaker.force_open();
    }

    /// Force the circuit closed (for emergency recovery)
    pub fn force_closed(&self) {
        warn!("FFI completion circuit breaker: forced closed");
        self.breaker.force_closed();
    }
}

/// Metrics snapshot for FFI completion circuit breaker
#[derive(Debug, Clone)]
pub struct FfiCompletionCircuitBreakerMetrics {
    /// Current circuit state
    pub state: CircuitState,
    /// Total failure count
    pub failure_count: u64,
    /// Total success count
    pub success_count: u64,
    /// Total call count
    pub total_calls: u64,
    /// Consecutive failures (resets on success)
    pub consecutive_failures: u64,
    /// Total slow sends detected
    pub slow_send_count: u64,
    /// Total rejections due to circuit being open
    pub circuit_open_rejections: u64,
    /// Configured slow send threshold in milliseconds
    pub slow_send_threshold_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_creation() {
        let config = FfiCompletionCircuitBreakerConfig::default();
        let breaker = FfiCompletionCircuitBreaker::new(config);

        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.is_healthy());
        assert_eq!(breaker.slow_send_count(), 0);
        assert_eq!(breaker.circuit_open_rejections(), 0);
    }

    #[tokio::test]
    async fn test_fast_send_success() {
        let config = FfiCompletionCircuitBreakerConfig {
            slow_send_threshold_ms: 100,
            ..Default::default()
        };
        let breaker = FfiCompletionCircuitBreaker::new(config);

        // Fast successful send
        breaker.record_send_result(Duration::from_millis(5), true);

        assert_eq!(breaker.state(), CircuitState::Closed);
        assert_eq!(breaker.slow_send_count(), 0);

        let metrics = breaker.metrics();
        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.failure_count, 0);
    }

    #[tokio::test]
    async fn test_slow_send_treated_as_failure() {
        let config = FfiCompletionCircuitBreakerConfig {
            slow_send_threshold_ms: 100,
            failure_threshold: 3,
            ..Default::default()
        };
        let breaker = FfiCompletionCircuitBreaker::new(config);

        // Slow but successful send - should be treated as failure
        breaker.record_send_result(Duration::from_millis(150), true);

        assert_eq!(breaker.slow_send_count(), 1);

        let metrics = breaker.metrics();
        assert_eq!(metrics.failure_count, 1);
        assert_eq!(metrics.success_count, 0);
    }

    #[tokio::test]
    async fn test_circuit_opens_on_consecutive_slow_sends() {
        let config = FfiCompletionCircuitBreakerConfig {
            slow_send_threshold_ms: 50,
            failure_threshold: 2,
            recovery_timeout_seconds: 1,
            success_threshold: 1,
        };
        let breaker = FfiCompletionCircuitBreaker::new(config);

        // First slow send
        breaker.record_send_result(Duration::from_millis(100), true);
        assert_eq!(breaker.state(), CircuitState::Closed);

        // Second slow send - should open circuit
        breaker.record_send_result(Duration::from_millis(100), true);
        assert_eq!(breaker.state(), CircuitState::Open);
        assert_eq!(breaker.slow_send_count(), 2);
    }

    #[tokio::test]
    async fn test_should_allow_tracks_rejections() {
        let config = FfiCompletionCircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_seconds: 10,
            ..Default::default()
        };
        let breaker = FfiCompletionCircuitBreaker::new(config);

        // Open the circuit
        breaker.record_send_result(Duration::from_millis(1), false);
        assert_eq!(breaker.state(), CircuitState::Open);

        // Try to send - should be rejected
        assert!(!breaker.should_allow());
        assert_eq!(breaker.circuit_open_rejections(), 1);

        // Another attempt
        assert!(!breaker.should_allow());
        assert_eq!(breaker.circuit_open_rejections(), 2);
    }

    #[tokio::test]
    async fn test_circuit_recovery() {
        let config = FfiCompletionCircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_seconds: 1,
            success_threshold: 1,
            slow_send_threshold_ms: 100,
        };
        let breaker = FfiCompletionCircuitBreaker::new(config);

        // Open the circuit
        breaker.record_send_result(Duration::from_millis(1), false);
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for timeout
        sleep(Duration::from_millis(1100)).await;

        // Should allow (transitioning to half-open)
        assert!(breaker.should_allow());

        // Fast successful send should close circuit
        breaker.record_send_result(Duration::from_millis(5), true);
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_force_operations() {
        let config = FfiCompletionCircuitBreakerConfig::default();
        let breaker = FfiCompletionCircuitBreaker::new(config);

        breaker.force_open();
        assert_eq!(breaker.state(), CircuitState::Open);

        breaker.force_closed();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let config = FfiCompletionCircuitBreakerConfig {
            slow_send_threshold_ms: 50,
            ..Default::default()
        };
        let breaker = FfiCompletionCircuitBreaker::new(config);

        // Mix of fast and slow sends
        breaker.record_send_result(Duration::from_millis(10), true); // fast success
        breaker.record_send_result(Duration::from_millis(100), true); // slow (failure)
        breaker.record_send_result(Duration::from_millis(5), false); // fast error

        let metrics = breaker.metrics();
        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.failure_count, 2);
        assert_eq!(metrics.slow_send_count, 1);
        assert_eq!(metrics.slow_send_threshold_ms, 50);
    }
}
