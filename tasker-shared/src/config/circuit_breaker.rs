use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Circuit breaker configuration integrated with YAML config
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerConfig {
    /// Whether circuit breakers are enabled globally
    pub enabled: bool,

    /// Global circuit breaker settings
    pub global_settings: CircuitBreakerGlobalSettings,

    /// Default configuration for new circuit breakers
    pub default_config: CircuitBreakerComponentConfig,

    /// Specific configurations for named components
    pub component_configs: HashMap<String, CircuitBreakerComponentConfig>,
}

/// Global circuit breaker settings from YAML
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerGlobalSettings {
    /// Maximum number of circuit breakers allowed
    pub max_circuit_breakers: usize,

    /// Interval for metrics collection and reporting in seconds
    pub metrics_collection_interval_seconds: u64,

    /// Whether to enable automatic circuit breaker creation
    pub auto_create_enabled: bool,

    /// Minimum interval between state transitions in seconds (prevents oscillation)
    pub min_state_transition_interval_seconds: f64,
}

/// Circuit breaker configuration for a specific component from YAML
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerComponentConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,

    /// Time to wait in open state before attempting recovery (in seconds)
    pub timeout_seconds: u64,

    /// Number of successful calls in half-open state to close circuit
    pub success_threshold: u32,
}

impl CircuitBreakerConfig {
    /// Get configuration for a specific component
    pub fn config_for_component(&self, component_name: &str) -> CircuitBreakerComponentConfig {
        self.component_configs
            .get(component_name)
            .cloned()
            .unwrap_or_else(|| self.default_config.clone())
    }
}

impl CircuitBreakerComponentConfig {
    /// Convert to resilience module's format
    pub fn to_resilience_config(&self) -> crate::resilience::config::CircuitBreakerConfig {
        crate::resilience::config::CircuitBreakerConfig {
            failure_threshold: self.failure_threshold,
            timeout: Duration::from_secs(self.timeout_seconds),
            success_threshold: self.success_threshold,
        }
    }
}

impl CircuitBreakerGlobalSettings {
    /// Convert to resilience module's format
    pub fn to_resilience_config(&self) -> crate::resilience::config::GlobalCircuitBreakerSettings {
        crate::resilience::config::GlobalCircuitBreakerSettings {
            max_circuit_breakers: self.max_circuit_breakers,
            metrics_collection_interval: Duration::from_secs(
                self.metrics_collection_interval_seconds,
            ),
            auto_create_enabled: self.auto_create_enabled,
            min_state_transition_interval: Duration::from_secs_f64(
                self.min_state_transition_interval_seconds,
            ),
        }
    }
}
