// TAS-61 Phase 6C/6D: TaskerConfigV2 is the canonical config
use crate::config::{ConfigManager, tasker::TaskerConfigV2};
use std::time::Duration;

// Re-export and use the unified event system configuration
pub use crate::config::event_systems::{
    EventSystemConfig, EventSystemHealthConfig, EventSystemProcessingConfig,
    EventSystemTimingConfig, OrchestrationEventSystemConfig, OrchestrationEventSystemMetadata,
};

impl OrchestrationEventSystemConfig {
    /// Create OrchestrationEventSystemConfig from ConfigManager with proper configuration loading
    ///
    /// Uses the unified event systems configuration from the centralized loader
    pub fn from_config_manager(config_manager: &ConfigManager) -> Self {
        // TAS-61 Phase 6C/6D: Use V2 config
        let config = config_manager.config_v2();
        Self::from_tasker_config(config)
    }

    /// Create from TaskerConfigV2 directly using unified event systems configuration
    pub fn from_tasker_config(config: &TaskerConfigV2) -> Self {
        // TAS-61 Phase 6C/6D: Use V2 config structure
        // Convert V2 EventSystemConfig to legacy EventSystemConfig<OrchestrationEventSystemMetadata>
        let v2_config = config
            .orchestration
            .as_ref()
            .map(|o| o.event_systems.orchestration.clone())
            .unwrap_or_default();

        // Convert from V2 to legacy generic type using From implementations
        EventSystemConfig {
            system_id: v2_config.system_id,
            deployment_mode: v2_config.deployment_mode.into(),
            timing: v2_config.timing.into(),
            processing: v2_config.processing.into(),
            health: v2_config.health.into(),
            metadata: OrchestrationEventSystemMetadata::default(),
        }
    }
}

// Convenience methods for Duration conversion
impl OrchestrationEventSystemConfig {
    /// Get fallback polling interval as Duration
    pub fn fallback_polling_interval(&self) -> Duration {
        self.timing.fallback_polling_interval()
    }

    /// Get visibility timeout as Duration
    pub fn visibility_timeout(&self) -> Duration {
        self.timing.visibility_timeout()
    }

    /// Get health check interval as Duration
    pub fn health_check_interval(&self) -> Duration {
        self.timing.health_check_interval()
    }

    /// Get processing timeout as Duration
    pub fn processing_timeout(&self) -> Duration {
        self.timing.claim_timeout()
    }

    /// Get message batch size as usize
    pub fn message_batch_size(&self) -> usize {
        self.processing.batch_size as usize
    }
}
