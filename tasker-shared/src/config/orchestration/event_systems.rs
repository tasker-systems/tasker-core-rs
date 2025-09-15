use crate::config::{ConfigManager, TaskerConfig};
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
        let config = config_manager.config();
        Self::from_tasker_config(config)
    }

    /// Create from TaskerConfig directly using unified event systems configuration
    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        // Use the unified orchestration event system config from the centralized loader
        config.event_systems.orchestration.clone()
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
