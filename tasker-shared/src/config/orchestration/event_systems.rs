// TAS-61 Phase 6C/6D: TaskerConfig is the canonical config
use crate::config::{tasker::TaskerConfig, ConfigManager};
use std::time::Duration;

// TAS-61 Phase 6C/6D: Import from V2 (types moved to tasker.rs)
pub use crate::config::tasker::{
    EventSystemConfig, EventSystemHealthConfig, EventSystemProcessingConfig,
    EventSystemTimingConfig, OrchestrationEventSystemConfig, OrchestrationEventSystemMetadata,
};

impl OrchestrationEventSystemConfig {
    /// Create OrchestrationEventSystemConfig from ConfigManager with proper configuration loading
    ///
    /// Uses the unified event systems configuration from the centralized loader
    pub fn from_config_manager(config_manager: &ConfigManager) -> Self {
        // TAS-61 Phase 6C/6D: Use V2 config
        let config = config_manager.config();
        Self::from_tasker_config(config)
    }

    /// Create from TaskerConfig directly using unified event systems configuration
    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        // TAS-61 Phase 6C/6D: Use V2 config structure
        // Convert V2 EventSystemConfig to legacy EventSystemConfig<OrchestrationEventSystemMetadata>
        let event_sys_config = config
            .orchestration
            .as_ref()
            .map(|o| o.event_systems.orchestration.clone())
            .unwrap_or_default();

        // Convert from V2 to legacy generic type using From implementations
        EventSystemConfig {
            system_id: event_sys_config.system_id,
            deployment_mode: event_sys_config.deployment_mode,
            timing: event_sys_config.timing,
            processing: event_sys_config.processing,
            health: event_sys_config.health,
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
