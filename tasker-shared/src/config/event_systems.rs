//! # Unified Event System Configuration
//!
//! Provides a standardized configuration approach for all event systems across
//! orchestration, task readiness, and worker components. This eliminates configuration
//! drift and provides consistent naming and structure.
//!
//! ## TAS-61 Phase 6C/6D: V2 Configuration Integration
//!
//! This module uses V2 configuration types directly and adds:
//! - **Generic wrapper**: `EventSystemConfig<T>` adds system-specific metadata
//! - **Duration helpers**: Convenience methods for time-based fields
//! - **Metadata structs**: System-specific configuration (not in V2)
//!
//! ### Architecture
//!
//! ```text
//! V2 Types (TOML)              Generic Wrapper (Architecture)
//! ├─ EventSystemTimingConfig   EventSystemConfig<T> {
//! ├─ EventSystemProcessingConfig   timing: V2,
//! ├─ EventSystemHealthConfig       processing: V2,
//! └─ EventSystemBackoffConfig      health: V2,
//!                                  metadata: T  // System-specific
//!                              }
//! ```

// TAS-61 Phase 6C/6D: Import V2 configuration types directly
pub use crate::config::tasker::{
    EventSystemBackoffConfig as BackoffConfig, EventSystemConfig, EventSystemHealthConfig,
    EventSystemProcessingConfig, EventSystemTimingConfig, FallbackPollerConfig,
    InProcessEventsConfig, ListenerConfig, OrchestrationEventSystemConfig,
    OrchestrationEventSystemMetadata, ResourceLimitsConfig, TaskReadinessEventSystemConfig,
    TaskReadinessEventSystemMetadata, WorkerEventSystemConfig, WorkerEventSystemMetadata,
};

use std::time::Duration;

/// Core event system configuration shared across all event systems
///
/// This struct contains the common configuration elements that all event systems
/// need, with system-specific details handled through the generic metadata parameter.
///
/// **V2 Integration**: Uses V2 types (EventSystemTimingConfig, EventSystemProcessingConfig, etc.)
/// directly, adding only the generic wrapper pattern for system-specific metadata.
impl WorkerEventSystemConfig {
    // TAS-61 Phase 6C/6D: from_tasker_config method deleted - not used and incompatible with V2 structure
    // Use V2 config directly: config.worker.map(|w| w.event_systems.worker)

    /// Get fallback polling interval as Duration
    pub fn fallback_polling_interval(&self) -> Duration {
        self.timing.fallback_polling_interval()
    }

    /// Get health check interval as Duration
    pub fn health_check_interval(&self) -> Duration {
        self.timing.health_check_interval()
    }

    /// Get processing timeout as Duration
    pub fn processing_timeout(&self) -> Duration {
        Duration::from_secs(self.timing.processing_timeout_seconds as u64)
    }
}

// TAS-61 Phase 6C/6D: Duration convenience methods for V2 types
// These impl blocks add Duration helpers to the imported V2 types
impl EventSystemTimingConfig {
    /// Get health check interval as Duration
    pub fn health_check_interval(&self) -> Duration {
        Duration::from_secs(self.health_check_interval_seconds as u64)
    }

    /// Get fallback polling interval as Duration
    pub fn fallback_polling_interval(&self) -> Duration {
        Duration::from_secs(self.fallback_polling_interval_seconds as u64)
    }

    /// Get visibility timeout as Duration
    pub fn visibility_timeout(&self) -> Duration {
        Duration::from_secs(self.visibility_timeout_seconds as u64)
    }

    /// Get processing timeout as Duration
    pub fn processing_timeout(&self) -> Duration {
        Duration::from_secs(self.processing_timeout_seconds as u64)
    }

    /// Get claim timeout as Duration
    pub fn claim_timeout(&self) -> Duration {
        Duration::from_secs(self.claim_timeout_seconds as u64)
    }
}

impl BackoffConfig {
    /// Get initial delay as Duration
    pub fn initial_delay(&self) -> Duration {
        Duration::from_millis(self.initial_delay_ms as u64)
    }

    /// Get maximum delay as Duration
    pub fn max_delay(&self) -> Duration {
        Duration::from_millis(self.max_delay_ms as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_system::DeploymentMode;

    #[test]
    fn test_orchestration_event_system_config_creation() {
        let config = OrchestrationEventSystemConfig::default();

        assert_eq!(config.system_id, "orchestration-event-system");
        assert_eq!(config.deployment_mode, DeploymentMode::Hybrid);
        // V2 default is 60 seconds
        assert_eq!(config.timing.health_check_interval_seconds, 60);
        // V2 default batch_size is 50
        assert_eq!(config.processing.batch_size, 50);
        assert!(config.health.enabled);
    }

    #[test]
    fn test_task_readiness_event_system_config_creation() {
        let config = TaskReadinessEventSystemConfig::default();

        assert_eq!(config.system_id, "task-readiness-event-system");
        assert_eq!(config.deployment_mode, DeploymentMode::Hybrid);
        // TAS-50: Metadata fields removed, configuration consolidated to task_readiness.rs
        assert!(config.metadata._reserved.is_none());
    }

    #[test]
    fn test_worker_event_system_config_creation() {
        let config = WorkerEventSystemConfig::default();

        assert_eq!(config.system_id, "worker-event-system");
        assert_eq!(config.deployment_mode, DeploymentMode::Hybrid);
        assert_eq!(config.metadata.resource_limits.max_memory_mb, 2048);
        // NOTE: broadcast_buffer_size migrated to mpsc_channels.toml (TAS-51)
        assert!(config.metadata.in_process_events.ffi_integration_enabled);
    }

    #[test]
    fn test_duration_conversion_methods() {
        let timing = EventSystemTimingConfig::default();

        // V2 default is 60 seconds (u32)
        assert_eq!(timing.health_check_interval(), Duration::from_secs(60));
        // V2 default is 10 seconds (u32)
        assert_eq!(timing.fallback_polling_interval(), Duration::from_secs(10));
        // V2 default is 30 seconds (u32)
        assert_eq!(timing.claim_timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_backoff_config_durations() {
        let backoff = BackoffConfig::default();

        // V2 defaults: initial=100ms, max=30000ms (30s), multiplier=2.0
        assert_eq!(backoff.initial_delay(), Duration::from_millis(100));
        assert_eq!(backoff.max_delay(), Duration::from_millis(30000));
        assert_eq!(backoff.multiplier, 2.0);
    }

    #[test]
    fn test_serde_serialization() {
        let config = OrchestrationEventSystemConfig::default();

        // Test serialization
        let serialized = toml::to_string(&config).expect("Failed to serialize config");
        assert!(!serialized.is_empty());

        // Test deserialization
        let deserialized: OrchestrationEventSystemConfig =
            toml::from_str(&serialized).expect("Failed to deserialize config");

        assert_eq!(config.system_id, deserialized.system_id);
        assert_eq!(config.deployment_mode, deserialized.deployment_mode);
    }

    #[test]
    fn test_v2_types_used_directly() {
        // Verify that we're using V2 types (u32 fields)
        let timing = EventSystemTimingConfig::default();
        let processing = EventSystemProcessingConfig::default();
        let backoff = BackoffConfig::default();

        // V2 types use u32
        assert_eq!(timing.health_check_interval_seconds, 60u32);
        assert_eq!(processing.max_concurrent_operations, 100u32);
        assert_eq!(backoff.initial_delay_ms, 100u32);

        // Duration helpers convert to u64
        assert_eq!(timing.health_check_interval(), Duration::from_secs(60u64));
    }
}
