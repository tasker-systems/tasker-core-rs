use crate::config::ConfigManager;
use crate::config::TaskerConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for task claiming behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimerConfig {
    /// Maximum number of tasks to claim in a single batch
    pub max_batch_size: i32,
    /// Default claim timeout in seconds
    pub default_claim_timeout: i32,
    /// Heartbeat interval for extending claims in seconds
    pub heartbeat_interval_seconds: u64,
    /// Enable claim extension during processing
    pub enable_heartbeat: bool,
    /// Cycle interval for task claiming in seconds
    pub cycle_interval_seconds: u64,
}

impl Default for TaskClaimerConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 10,
            default_claim_timeout: 300,     // 5 minutes
            heartbeat_interval_seconds: 60, // 1 minute
            enable_heartbeat: true,
            cycle_interval_seconds: 60, // 1 minute
        }
    }
}

impl TaskClaimerConfig {
    /// Create TaskClaimerConfig from ConfigManager
    pub fn from_config_manager(config_manager: &ConfigManager) -> Self {
        let config = config_manager.config();
        Self::from_tasker_config(config)
    }

    /// Create TaskClaimerConfig from TaskerConfig
    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        Self {
            max_batch_size: config.task_claimer.max_batch_size,
            default_claim_timeout: config.task_claimer.default_claim_timeout,
            heartbeat_interval_seconds: config.task_claimer.heartbeat_interval_seconds,
            enable_heartbeat: config.task_claimer.enable_heartbeat,
            cycle_interval_seconds: config.task_claimer.cycle_interval_seconds,
        }
    }

    /// Get heartbeat interval as Duration
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_secs(self.heartbeat_interval_seconds)
    }

    /// Get cycle interval as Duration
    pub fn cycle_interval(&self) -> Duration {
        Duration::from_secs(self.cycle_interval_seconds)
    }
}
