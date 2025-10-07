//! # Circuit Breaker Metrics
//!
//! Provides comprehensive metrics collection for circuit breaker operations.
//! These metrics enable monitoring, alerting, and performance analysis of
//! circuit breaker behavior in production.

use crate::resilience::CircuitState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Metrics for a single circuit breaker instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerMetrics {
    /// Total number of calls attempted
    pub total_calls: u64,

    /// Number of successful calls
    pub success_count: u64,

    /// Number of failed calls
    pub failure_count: u64,

    /// Current consecutive failure count
    pub consecutive_failures: u64,

    /// Number of calls made in half-open state
    pub half_open_calls: u64,

    /// Total duration of all operations
    pub total_duration: Duration,

    /// Current circuit breaker state
    pub current_state: CircuitState,

    /// Calculated failure rate (0.0 to 1.0)
    pub failure_rate: f64,

    /// Calculated success rate (0.0 to 1.0)
    pub success_rate: f64,

    /// Average operation duration
    pub average_duration: Duration,
}

impl CircuitBreakerMetrics {
    /// Create new metrics instance with zero values
    pub fn new() -> Self {
        Self {
            total_calls: 0,
            success_count: 0,
            failure_count: 0,
            consecutive_failures: 0,
            half_open_calls: 0,
            total_duration: Duration::ZERO,
            current_state: CircuitState::Closed,
            failure_rate: 0.0,
            success_rate: 0.0,
            average_duration: Duration::ZERO,
        }
    }

    /// Calculate calls per second based on total duration
    pub fn calls_per_second(&self) -> f64 {
        if self.total_duration.is_zero() {
            return 0.0;
        }

        self.total_calls as f64 / self.total_duration.as_secs_f64()
    }

    /// Check if metrics indicate healthy operation
    pub fn is_healthy(&self) -> bool {
        match self.current_state {
            CircuitState::Closed => {
                // Closed is healthy if failure rate is reasonable
                self.failure_rate < 0.1 // Less than 10% failure rate
            }
            CircuitState::Open => false,
            CircuitState::HalfOpen => true, // Half-open is attempting recovery
        }
    }

    /// Get human-readable state description
    pub fn state_description(&self) -> &'static str {
        match self.current_state {
            CircuitState::Closed => "Healthy - Normal operation",
            CircuitState::Open => "Failing - Rejecting all calls",
            CircuitState::HalfOpen => "Recovering - Testing system health",
        }
    }

    /// Format metrics for logging
    pub fn format_summary(&self) -> String {
        format!(
            "State: {} | Calls: {} | Success: {:.1}% | Failures: {} | Avg Duration: {:.2}ms",
            self.state_description(),
            self.total_calls,
            self.success_rate * 100.0,
            self.failure_count,
            self.average_duration.as_millis()
        )
    }
}

impl Default for CircuitBreakerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// System-wide circuit breaker metrics aggregator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCircuitBreakerMetrics {
    /// Metrics for individual circuit breakers by name
    pub circuit_breakers: HashMap<String, CircuitBreakerMetrics>,

    /// Timestamp of last metrics collection
    pub collected_at: std::time::SystemTime,
}

impl SystemCircuitBreakerMetrics {
    /// Create new system metrics
    pub fn new() -> Self {
        Self {
            circuit_breakers: HashMap::new(),
            collected_at: std::time::SystemTime::now(),
        }
    }

    /// Add metrics for a circuit breaker
    pub fn add_circuit_breaker(&mut self, name: String, metrics: CircuitBreakerMetrics) {
        self.circuit_breakers.insert(name, metrics);
        self.collected_at = std::time::SystemTime::now();
    }

    /// Get count of circuit breakers by state
    pub fn count_by_state(&self) -> HashMap<CircuitState, usize> {
        let mut counts = HashMap::new();

        for metrics in self.circuit_breakers.values() {
            let count = counts.entry(metrics.current_state).or_insert(0);
            *count += 1;
        }

        counts
    }

    /// Get list of unhealthy circuit breakers
    pub fn unhealthy_circuits(&self) -> Vec<(&String, &CircuitBreakerMetrics)> {
        self.circuit_breakers
            .iter()
            .filter(|(_, metrics)| !metrics.is_healthy())
            .collect()
    }

    /// Calculate system-wide health score (0.0 to 1.0)
    pub fn health_score(&self) -> f64 {
        if self.circuit_breakers.is_empty() {
            return 1.0; // No circuit breakers = healthy
        }

        let healthy_count = self
            .circuit_breakers
            .values()
            .filter(|metrics| metrics.is_healthy())
            .count();

        healthy_count as f64 / self.circuit_breakers.len() as f64
    }

    /// Get total calls across all circuit breakers
    pub fn total_calls(&self) -> u64 {
        self.circuit_breakers
            .values()
            .map(|metrics| metrics.total_calls)
            .sum()
    }

    /// Get total failures across all circuit breakers
    pub fn total_failures(&self) -> u64 {
        self.circuit_breakers
            .values()
            .map(|metrics| metrics.failure_count)
            .sum()
    }

    /// Get system-wide failure rate
    pub fn system_failure_rate(&self) -> f64 {
        let total_calls = self.total_calls();
        if total_calls == 0 {
            return 0.0;
        }

        self.total_failures() as f64 / total_calls as f64
    }

    /// Format summary for logging
    pub fn format_summary(&self) -> String {
        let state_counts = self.count_by_state();
        let closed_count = state_counts.get(&CircuitState::Closed).unwrap_or(&0);
        let open_count = state_counts.get(&CircuitState::Open).unwrap_or(&0);
        let half_open_count = state_counts.get(&CircuitState::HalfOpen).unwrap_or(&0);

        format!(
            "Circuit Breakers: {} total | {} closed | {} open | {} half-open | Health: {:.1}% | System failure rate: {:.2}%",
            self.circuit_breakers.len(),
            closed_count,
            open_count,
            half_open_count,
            self.health_score() * 100.0,
            self.system_failure_rate() * 100.0
        )
    }
}

impl Default for SystemCircuitBreakerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics collection trait for integration with monitoring systems
pub trait MetricsCollector {
    /// Record circuit breaker metrics
    fn record_circuit_breaker_metrics(&self, name: &str, metrics: &CircuitBreakerMetrics);

    /// Record circuit breaker state transition
    fn record_state_transition(&self, name: &str, from: CircuitState, to: CircuitState);

    /// Record operation timing
    fn record_operation_timing(&self, name: &str, duration: Duration, success: bool);
}

/// Prometheus-style metrics exporter
pub struct PrometheusMetricsExporter;

impl MetricsCollector for PrometheusMetricsExporter {
    fn record_circuit_breaker_metrics(&self, name: &str, metrics: &CircuitBreakerMetrics) {
        // In a real implementation, this would export to Prometheus
        tracing::info!(
            circuit_breaker = name,
            total_calls = metrics.total_calls,
            success_count = metrics.success_count,
            failure_count = metrics.failure_count,
            failure_rate = metrics.failure_rate,
            state = ?metrics.current_state,
            "Circuit breaker metrics"
        );
    }

    fn record_state_transition(&self, name: &str, from: CircuitState, to: CircuitState) {
        tracing::info!(
            circuit_breaker = name,
            from_state = ?from,
            to_state = ?to,
            "Circuit breaker state transition"
        );
    }

    fn record_operation_timing(&self, name: &str, duration: Duration, success: bool) {
        tracing::debug!(
            circuit_breaker = name,
            duration_ms = duration.as_millis(),
            success = success,
            "Operation timing"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_metrics_creation() {
        let metrics = CircuitBreakerMetrics::new();

        assert_eq!(metrics.total_calls, 0);
        assert_eq!(metrics.success_count, 0);
        assert_eq!(metrics.failure_count, 0);
        assert_eq!(metrics.current_state, CircuitState::Closed);
        assert!(metrics.is_healthy());
    }

    #[test]
    fn test_system_metrics_aggregation() {
        let mut system_metrics = SystemCircuitBreakerMetrics::new();

        let mut cb1_metrics = CircuitBreakerMetrics::new();
        cb1_metrics.current_state = CircuitState::Closed;
        cb1_metrics.total_calls = 100;
        cb1_metrics.success_count = 95;
        cb1_metrics.failure_count = 5;
        cb1_metrics.failure_rate = 0.05;

        let mut cb2_metrics = CircuitBreakerMetrics::new();
        cb2_metrics.current_state = CircuitState::Open;
        cb2_metrics.total_calls = 50;
        cb2_metrics.success_count = 25;
        cb2_metrics.failure_count = 25;
        cb2_metrics.failure_rate = 0.5;

        system_metrics.add_circuit_breaker("database".to_string(), cb1_metrics);
        system_metrics.add_circuit_breaker("queue".to_string(), cb2_metrics);

        assert_eq!(system_metrics.total_calls(), 150);
        assert_eq!(system_metrics.total_failures(), 30);
        assert_eq!(system_metrics.system_failure_rate(), 0.2);

        let state_counts = system_metrics.count_by_state();
        assert_eq!(state_counts.get(&CircuitState::Closed), Some(&1));
        assert_eq!(state_counts.get(&CircuitState::Open), Some(&1));

        // Health score should be 0.5 (1 healthy out of 2)
        assert_eq!(system_metrics.health_score(), 0.5);

        let unhealthy = system_metrics.unhealthy_circuits();
        assert_eq!(unhealthy.len(), 1);
        assert_eq!(unhealthy[0].0, "queue");
    }

    #[test]
    fn test_metrics_health_calculation() {
        let mut metrics = CircuitBreakerMetrics::new();

        // Healthy closed state
        metrics.current_state = CircuitState::Closed;
        metrics.failure_rate = 0.05;
        assert!(metrics.is_healthy());

        // Unhealthy closed state (high failure rate)
        metrics.failure_rate = 0.15;
        assert!(!metrics.is_healthy());

        // Open state is never healthy
        metrics.current_state = CircuitState::Open;
        metrics.failure_rate = 0.0;
        assert!(!metrics.is_healthy());

        // Half-open is considered healthy (recovering)
        metrics.current_state = CircuitState::HalfOpen;
        assert!(metrics.is_healthy());
    }
}
