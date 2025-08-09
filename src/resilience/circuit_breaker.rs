//! # Circuit Breaker Implementation
//!
//! Provides fault isolation patterns to prevent cascade failures in distributed systems.
//! This implementation follows the classic circuit breaker pattern with three states:
//! Closed (normal operation), Open (failing fast), and Half-Open (testing recovery).

use crate::resilience::{CircuitBreakerConfig, CircuitBreakerMetrics};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

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
    
    /// Metrics tracking protected by mutex
    metrics: Arc<Mutex<CircuitBreakerMetrics>>,
    
    /// Time when circuit was opened (for timeout calculations)
    opened_at: Arc<Mutex<Option<Instant>>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given name and configuration
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        info!(
            component = %name,
            failure_threshold = config.failure_threshold,
            timeout_seconds = config.timeout.as_secs(),
            success_threshold = config.success_threshold,
            "🛡️ Circuit breaker initialized"
        );
        
        Self {
            name,
            state: AtomicU8::new(CircuitState::Closed as u8),
            config,
            metrics: Arc::new(Mutex::new(CircuitBreakerMetrics::new())),
            opened_at: Arc::new(Mutex::new(None)),
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
        if !self.should_allow_call().await? {
            return Err(CircuitBreakerError::CircuitOpen {
                component: self.name.clone(),
            });
        }
        
        // Execute the operation
        let start_time = Instant::now();
        let result = operation().await;
        let duration = start_time.elapsed();
        
        // Record the result
        match &result {
            Ok(_) => {
                self.record_success(duration).await;
            }
            Err(_) => {
                self.record_failure(duration).await;
            }
        }
        
        // Map error type
        result.map_err(CircuitBreakerError::OperationFailed)
    }
    
    /// Check if a call should be allowed based on current state
    async fn should_allow_call<E>(&self) -> Result<bool, CircuitBreakerError<E>> {
        match self.state() {
            CircuitState::Closed => Ok(true),
            CircuitState::Open => {
                // Check if timeout has elapsed
                let opened_at = self.opened_at.lock().await;
                if let Some(opened_time) = *opened_at {
                    if opened_time.elapsed() >= self.config.timeout {
                        drop(opened_at);
                        self.transition_to_half_open().await;
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                } else {
                    // Circuit is open but no timestamp - shouldn't happen, but allow call
                    warn!(component = %self.name, "Circuit open but no timestamp recorded");
                    Ok(true)
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited calls to test recovery
                let metrics = self.metrics.lock().await;
                Ok(metrics.half_open_calls < self.config.success_threshold as u64)
            }
        }
    }
    
    /// Record a successful operation
    async fn record_success(&self, duration: Duration) {
        let mut metrics = self.metrics.lock().await;
        metrics.total_calls += 1;
        metrics.success_count += 1;
        metrics.total_duration += duration;
        
        debug!(
            component = %self.name,
            duration_ms = duration.as_millis(),
            "🟢 Operation succeeded"
        );
        
        match self.state() {
            CircuitState::HalfOpen => {
                metrics.half_open_calls += 1;
                if metrics.half_open_calls >= self.config.success_threshold as u64 {
                    drop(metrics);
                    self.transition_to_closed().await;
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success
                metrics.consecutive_failures = 0;
            }
            CircuitState::Open => {
                // Shouldn't happen, but log it
                warn!(component = %self.name, "Success recorded while circuit is open");
            }
        }
    }
    
    /// Record a failed operation
    async fn record_failure(&self, duration: Duration) {
        let mut metrics = self.metrics.lock().await;
        metrics.total_calls += 1;
        metrics.failure_count += 1;
        metrics.total_duration += duration;
        
        error!(
            component = %self.name,
            duration_ms = duration.as_millis(),
            "🔴 Operation failed"
        );
        
        match self.state() {
            CircuitState::Closed => {
                metrics.consecutive_failures += 1;
                if metrics.consecutive_failures >= self.config.failure_threshold as u64 {
                    drop(metrics);
                    self.transition_to_open().await;
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open state immediately opens circuit
                drop(metrics);
                self.transition_to_open().await;
            }
            CircuitState::Open => {
                // Already open, just record the failure
            }
        }
    }
    
    /// Transition to closed state (normal operation)
    async fn transition_to_closed(&self) {
        self.state.store(CircuitState::Closed as u8, Ordering::Release);
        
        // Reset metrics for new cycle
        let mut metrics = self.metrics.lock().await;
        metrics.consecutive_failures = 0;
        metrics.half_open_calls = 0;
        
        // Clear opened timestamp
        let mut opened_at = self.opened_at.lock().await;
        *opened_at = None;
        
        info!(
            component = %self.name,
            total_calls = metrics.total_calls,
            "🟢 Circuit breaker closed (recovered)"
        );
    }
    
    /// Transition to open state (failing fast)
    async fn transition_to_open(&self) {
        self.state.store(CircuitState::Open as u8, Ordering::Release);
        
        // Record when circuit was opened
        let mut opened_at = self.opened_at.lock().await;
        *opened_at = Some(Instant::now());
        
        // Reset half-open call count
        let mut metrics = self.metrics.lock().await;
        metrics.half_open_calls = 0;
        
        error!(
            component = %self.name,
            consecutive_failures = metrics.consecutive_failures,
            failure_threshold = self.config.failure_threshold,
            timeout_seconds = self.config.timeout.as_secs(),
            "🔴 Circuit breaker opened (failing fast)"
        );
    }
    
    /// Transition to half-open state (testing recovery)
    async fn transition_to_half_open(&self) {
        self.state.store(CircuitState::HalfOpen as u8, Ordering::Release);
        
        // Reset half-open call count
        let mut metrics = self.metrics.lock().await;
        metrics.half_open_calls = 0;
        
        info!(
            component = %self.name,
            success_threshold = self.config.success_threshold,
            "🟡 Circuit breaker half-open (testing recovery)"
        );
    }
    
    /// Force circuit to open state (for emergency situations)
    pub async fn force_open(&self) {
        warn!(component = %self.name, "🚨 Circuit breaker forced open");
        self.transition_to_open().await;
    }
    
    /// Force circuit to closed state (for emergency recovery)
    pub async fn force_closed(&self) {
        warn!(component = %self.name, "🚨 Circuit breaker forced closed");
        self.transition_to_closed().await;
    }
    
    /// Get current metrics snapshot
    pub async fn metrics(&self) -> CircuitBreakerMetrics {
        let metrics = self.metrics.lock().await;
        let mut snapshot = metrics.clone();
        
        // Add current state information
        snapshot.current_state = self.state();
        
        // Calculate derived metrics
        if metrics.total_calls > 0 {
            snapshot.failure_rate = metrics.failure_count as f64 / metrics.total_calls as f64;
            snapshot.success_rate = metrics.success_count as f64 / metrics.total_calls as f64;
            
            if metrics.success_count > 0 {
                snapshot.average_duration = metrics.total_duration / metrics.success_count as u32;
            }
        }
        
        snapshot
    }
    
    /// Get component name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Check if circuit is healthy (closed state with low failure rate)
    pub async fn is_healthy(&self) -> bool {
        let state = self.state();
        if state != CircuitState::Closed {
            return false;
        }
        
        let metrics = self.metrics.lock().await;
        if metrics.total_calls < 10 {
            // Too few calls to determine health
            return true;
        }
        
        // Consider healthy if failure rate is below 10%
        let failure_rate = metrics.failure_count as f64 / metrics.total_calls as f64;
        failure_rate < 0.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
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
        
        let metrics = circuit.metrics().await;
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
        let result = circuit.call(|| async { Ok::<_, String>("should not execute") }).await;
        assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen { .. })));
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
        circuit.force_open().await;
        assert_eq!(circuit.state(), CircuitState::Open);
        
        // Force closed
        circuit.force_closed().await;
        assert_eq!(circuit.state(), CircuitState::Closed);
    }
}