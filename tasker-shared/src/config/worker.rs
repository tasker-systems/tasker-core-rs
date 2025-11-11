//! # Worker Configuration Adapters
//!
//! TAS-61 Phase 6C/6D: This module provides **type adapters** over V2's storage-optimized configuration.
//!
//! ## Type Adapter Pattern
//!
//! V2 configuration (tasker.rs) uses storage-optimized types for TOML efficiency:
//! - `u32` for numeric fields (claim_timeout_seconds, health_check_interval_seconds, etc.)
//!
//! These adapters convert to runtime-optimized types for performance:
//! - `u64` for time values (prevents overflow, matches std::time::Duration)
//! - `usize` for concurrency limits (matches Rust collection/threading APIs)
//!
//! ## Type Mappings
//!
//! - `StepProcessingConfig` → Adapter (u32 → u64/usize conversion)
//!   - claim_timeout_seconds: u32 → u64
//!   - max_concurrent_steps: u32 → usize
//! - `HealthMonitoringConfig` → Adapter (u32 → u64 conversion)
//!   - health_check_interval_seconds: u32 → u64
//!
//! ## Usage Pattern
//!
//! ```rust,ignore
//! // V2 config loaded from TOML
//! let config: TaskerConfig = load_from_toml();
//!
//! // Convert to runtime adapter types
//! let step_config: StepProcessingConfig = config.worker.unwrap().step_processing.into();
//! // Now: claim_timeout_seconds is u64 (was u32)
//! //      max_concurrent_steps is usize (was u32)
//! ```

use serde::{Deserialize, Serialize};

/// Step processing configuration adapter (u32 → u64/usize)
///
/// Converts V2's storage-optimized types to runtime performance types.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepProcessingConfig {
    /// Timeout for claiming a step (seconds) - u64 for Duration compatibility
    pub claim_timeout_seconds: u64,

    /// Maximum retries for failed steps
    pub max_retries: u32,

    /// Maximum concurrent steps - usize for threading/collection APIs
    pub max_concurrent_steps: usize,
}

/// Worker health monitoring configuration adapter (u32 → u64)
///
/// Converts V2's storage-optimized types to runtime performance types.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthMonitoringConfig {
    /// Health check interval (seconds) - u64 for Duration compatibility
    pub health_check_interval_seconds: u64,

    /// Enable performance monitoring
    pub performance_monitoring_enabled: bool,

    /// Error rate threshold (0.0-1.0)
    pub error_rate_threshold: f64,
}

// TAS-61 Phase 6C/6D: From implementations to convert V2 → adapter types

impl From<crate::config::tasker::StepProcessingConfig> for StepProcessingConfig {
    fn from(v2: crate::config::tasker::StepProcessingConfig) -> Self {
        Self {
            claim_timeout_seconds: v2.claim_timeout_seconds as u64,
            max_retries: v2.max_retries,
            max_concurrent_steps: v2.max_concurrent_steps as usize,
        }
    }
}

impl From<crate::config::tasker::HealthMonitoringConfig> for HealthMonitoringConfig {
    fn from(v2: crate::config::tasker::HealthMonitoringConfig) -> Self {
        Self {
            health_check_interval_seconds: v2.health_check_interval_seconds as u64,
            performance_monitoring_enabled: v2.performance_monitoring_enabled,
            error_rate_threshold: v2.error_rate_threshold,
        }
    }
}

// Default implementations for adapter types (mirror V2 defaults)

impl Default for StepProcessingConfig {
    fn default() -> Self {
        Self {
            claim_timeout_seconds: 300,
            max_retries: 3,
            max_concurrent_steps: 100,
        }
    }
}

impl Default for HealthMonitoringConfig {
    fn default() -> Self {
        Self {
            health_check_interval_seconds: 10,
            performance_monitoring_enabled: true,
            error_rate_threshold: 0.05,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_processing_config_from_v2() {
        // Use V2 default and manually construct for testing
        let v2 = crate::config::tasker::StepProcessingConfig {
            claim_timeout_seconds: 600,
            max_retries: 5,
            max_concurrent_steps: 50,
        };

        let adapter: StepProcessingConfig = v2.into();

        assert_eq!(adapter.claim_timeout_seconds, 600u64);
        assert_eq!(adapter.max_retries, 5);
        assert_eq!(adapter.max_concurrent_steps, 50usize);
    }

    #[test]
    fn test_health_monitoring_config_from_v2() {
        // Use V2 default and manually construct for testing
        let v2 = crate::config::tasker::HealthMonitoringConfig {
            health_check_interval_seconds: 60,
            performance_monitoring_enabled: false,
            error_rate_threshold: 0.10,
        };

        let adapter: HealthMonitoringConfig = v2.into();

        assert_eq!(adapter.health_check_interval_seconds, 60u64);
        assert!(!adapter.performance_monitoring_enabled);
        assert_eq!(adapter.error_rate_threshold, 0.10);
    }

    #[test]
    fn test_default_values() {
        let step_config = StepProcessingConfig::default();
        assert_eq!(step_config.claim_timeout_seconds, 300);
        assert_eq!(step_config.max_retries, 3);
        assert_eq!(step_config.max_concurrent_steps, 100);

        let health_config = HealthMonitoringConfig::default();
        assert_eq!(health_config.health_check_interval_seconds, 10);
        assert!(health_config.performance_monitoring_enabled);
        assert_eq!(health_config.error_rate_threshold, 0.05);
    }
}
