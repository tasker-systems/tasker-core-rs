use crate::config::{ConfigManager, QueuesConfig, TaskerConfig};
use crate::event_system::DeploymentMode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for orchestration event system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationEventSystemConfig {
    /// System identifier
    pub system_id: String,
    /// Deployment mode (determines event-driven behavior)
    pub deployment_mode: DeploymentMode,
    /// Namespace for orchestration
    pub namespace: String,
    /// Fallback polling interval in seconds
    pub fallback_polling_interval_seconds: u64,
    /// Message batch size for polling
    pub batch_size: u32,
    /// Queue configuration (TAS-43: Updated to use new QueuesConfig)
    /// Note: This field is populated from the main queues configuration, not from TOML
    #[serde(skip)]
    pub queues: QueuesConfig,
    /// Visibility timeout for messages in seconds
    pub visibility_timeout_seconds: u64,
    /// Health check interval in seconds
    pub health_check_interval_seconds: u64,
}

impl Default for OrchestrationEventSystemConfig {
    fn default() -> Self {
        Self {
            system_id: "orchestration-event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            namespace: "orchestration".to_string(),
            fallback_polling_interval_seconds: 5,
            batch_size: 10,
            queues: QueuesConfig::default(),
            visibility_timeout_seconds: 30,
            health_check_interval_seconds: 30,
        }
    }
}

impl OrchestrationEventSystemConfig {
    /// Create OrchestrationEventSystemConfig from ConfigManager with proper configuration loading
    ///
    /// This replaces ::default() usage with proper configuration integration for TAS-43
    pub fn from_config_manager(config_manager: &ConfigManager) -> Self {
        let config = config_manager.config();

        Self::from_tasker_config(config)
    }

    /// Create from TaskerConfig directly (for cases where ConfigManager isn't available)
    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        Self {
            system_id: "orchestration-event-system".to_string(),
            deployment_mode: config.orchestration.event_systems.deployment_mode.clone(),
            namespace: config.queues.orchestration_namespace.clone(),
            fallback_polling_interval_seconds: config
                .orchestration
                .event_systems
                .fallback_polling_interval_seconds,
            batch_size: config.queues.default_batch_size,
            queues: config.queues.clone(),
            visibility_timeout_seconds: config.queues.default_visibility_timeout_seconds as u64,
            health_check_interval_seconds: config.queues.health_check_interval,
        }
    }

    /// Get fallback polling interval as Duration
    pub fn fallback_polling_interval(&self) -> Duration {
        Duration::from_secs(self.fallback_polling_interval_seconds)
    }

    /// Get visibility timeout as Duration
    pub fn visibility_timeout(&self) -> Duration {
        Duration::from_secs(self.visibility_timeout_seconds)
    }

    /// Get health check interval as Duration
    pub fn health_check_interval(&self) -> Duration {
        Duration::from_secs(self.health_check_interval_seconds)
    }
}
