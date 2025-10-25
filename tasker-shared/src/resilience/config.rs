//! # Circuit Breaker Configuration
//!
//! Provides configuration structures and validation for circuit breaker behavior.
//!
//! **Note**: This module now only contains the individual CircuitBreakerConfig struct.
//! For system-wide configuration, use `crate::config::CircuitBreakerConfig` which provides
//! YAML-based configuration with environment-aware settings.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for a single circuit breaker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,

    /// Time to wait in open state before attempting recovery
    pub timeout: Duration,

    /// Number of successful calls in half-open state to close circuit
    pub success_threshold: u32,
}

impl CircuitBreakerConfig {
    /// Create configuration for database operations
    pub fn for_database() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }

    /// Create configuration for queue operations
    pub fn for_queue() -> Self {
        Self {
            failure_threshold: 3,
            timeout: Duration::from_secs(15),
            success_threshold: 2,
        }
    }

    /// Create configuration for external API calls
    pub fn for_external_api() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(45),
            success_threshold: 2,
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.failure_threshold == 0 {
            return Err("failure_threshold must be greater than 0".to_string());
        }

        if self.failure_threshold > 100 {
            return Err("failure_threshold should not exceed 100".to_string());
        }

        if self.timeout.is_zero() {
            return Err("timeout must be greater than 0".to_string());
        }

        if self.timeout > Duration::from_secs(300) {
            return Err("timeout should not exceed 300 seconds".to_string());
        }

        if self.success_threshold == 0 {
            return Err("success_threshold must be greater than 0".to_string());
        }

        if self.success_threshold > 50 {
            return Err("success_threshold should not exceed 50".to_string());
        }

        Ok(())
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

/// Global circuit breaker settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCircuitBreakerSettings {
    /// Maximum number of circuit breakers allowed
    pub max_circuit_breakers: usize,

    /// Interval for metrics collection and reporting
    pub metrics_collection_interval: Duration,

    /// Minimum interval between state transitions (prevents oscillation)
    pub min_state_transition_interval: Duration,
}

impl GlobalCircuitBreakerSettings {
    /// Validate global settings
    pub fn validate(&self) -> Result<(), String> {
        if self.max_circuit_breakers == 0 {
            return Err("max_circuit_breakers must be greater than 0".to_string());
        }

        if self.max_circuit_breakers > 1000 {
            return Err("max_circuit_breakers should not exceed 1000".to_string());
        }

        if self.metrics_collection_interval.is_zero() {
            return Err("metrics_collection_interval must be greater than 0".to_string());
        }

        if self.min_state_transition_interval.is_zero() {
            return Err("min_state_transition_interval must be greater than 0".to_string());
        }

        Ok(())
    }
}

impl Default for GlobalCircuitBreakerSettings {
    fn default() -> Self {
        Self {
            max_circuit_breakers: 50,
            metrics_collection_interval: Duration::from_secs(30),
            min_state_transition_interval: Duration::from_secs(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_config_validation() {
        // Valid config should pass
        let valid_config = CircuitBreakerConfig::default();
        assert!(valid_config.validate().is_ok());

        // Invalid failure threshold
        let mut invalid_config = CircuitBreakerConfig {
            failure_threshold: 0,
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());

        // Invalid timeout
        invalid_config = CircuitBreakerConfig {
            timeout: Duration::ZERO,
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());

        // Invalid success threshold
        invalid_config = CircuitBreakerConfig {
            success_threshold: 0,
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_preset_configurations() {
        let db_config = CircuitBreakerConfig::for_database();
        assert_eq!(db_config.failure_threshold, 5);
        assert!(db_config.validate().is_ok());

        let queue_config = CircuitBreakerConfig::for_queue();
        assert_eq!(queue_config.failure_threshold, 3);
        assert!(queue_config.validate().is_ok());

        let api_config = CircuitBreakerConfig::for_external_api();
        assert_eq!(api_config.failure_threshold, 5);
        assert!(api_config.validate().is_ok());
    }

    #[test]
    fn test_global_settings_validation() {
        // Valid settings should pass
        let valid_settings = GlobalCircuitBreakerSettings::default();
        assert!(valid_settings.validate().is_ok());

        // Invalid max_circuit_breakers
        let mut invalid_settings = GlobalCircuitBreakerSettings {
            max_circuit_breakers: 0,
            ..Default::default()
        };
        assert!(invalid_settings.validate().is_err());

        // Invalid metrics interval
        invalid_settings = GlobalCircuitBreakerSettings {
            max_circuit_breakers: 10,
            metrics_collection_interval: Duration::ZERO,
            ..Default::default()
        };
        assert!(invalid_settings.validate().is_err());
    }
}
