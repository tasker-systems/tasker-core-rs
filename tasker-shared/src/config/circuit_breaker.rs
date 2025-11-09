use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// Import component types from V2
pub use crate::config::tasker::tasker_v2::{
    CircuitBreakerComponentConfig, GlobalCircuitBreakerSettings,
};

// Type alias for backward compatibility (legacy name vs V2 name)
pub type CircuitBreakerGlobalSettings = GlobalCircuitBreakerSettings;

/// Circuit breaker configuration with HashMap-based component flexibility
///
/// This is an adapter over V2's structured CircuitBreakerConfig. While V2 uses
/// a fixed struct (ComponentCircuitBreakerConfigs) with named fields, this version
/// provides HashMap flexibility for dynamic component configuration.
///
/// **Pattern**: HashMap adapter (runtime) over V2 structured config (TOML)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerConfig {
    /// Whether circuit breakers are enabled globally
    pub enabled: bool,

    /// Global circuit breaker settings
    pub global_settings: GlobalCircuitBreakerSettings,

    /// Default configuration for new circuit breakers
    pub default_config: CircuitBreakerComponentConfig,

    /// Specific configurations for named components (HashMap for flexibility)
    pub component_configs: HashMap<String, CircuitBreakerComponentConfig>,
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
    ///
    /// Note: V2 uses u32 for timeout_seconds, resilience module expects u64
    pub fn to_resilience_config(&self) -> crate::resilience::config::CircuitBreakerConfig {
        crate::resilience::config::CircuitBreakerConfig {
            failure_threshold: self.failure_threshold,
            timeout: Duration::from_secs(self.timeout_seconds as u64),
            success_threshold: self.success_threshold,
        }
    }
}

impl GlobalCircuitBreakerSettings {
    /// Convert to resilience module's format
    ///
    /// Note: V2 uses u32 types, resilience module expects usize/u64
    pub fn to_resilience_config(&self) -> crate::resilience::config::GlobalCircuitBreakerSettings {
        crate::resilience::config::GlobalCircuitBreakerSettings {
            max_circuit_breakers: self.max_circuit_breakers as usize,
            metrics_collection_interval: Duration::from_secs(
                self.metrics_collection_interval_seconds as u64,
            ),
            min_state_transition_interval: Duration::from_secs_f64(
                self.min_state_transition_interval_seconds,
            ),
        }
    }
}
