//! # Circuit Breaker Configuration Adapters
//!
//! Provides conversion methods from canonical config (tasker.rs) to resilience module types.
//! TAS-61: All TOML configs now in tasker.rs - this module only provides type conversions.

use std::time::Duration;

// Re-export canonical types
pub use crate::config::tasker::{
    CircuitBreakerComponentConfig, CircuitBreakerConfig, GlobalCircuitBreakerSettings,
};

// Type alias for backward compatibility (legacy name)
pub type CircuitBreakerGlobalSettings = GlobalCircuitBreakerSettings;

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
