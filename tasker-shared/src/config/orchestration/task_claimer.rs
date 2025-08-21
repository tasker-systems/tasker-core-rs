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
    /// Heartbeat interval for extending claims
    pub heartbeat_interval: Duration,
    /// Enable claim extension during processing
    pub enable_heartbeat: bool,
}

impl Default for TaskClaimerConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 10,
            default_claim_timeout: 300,                  // 5 minutes
            heartbeat_interval: Duration::from_secs(60), // 1 minute
            enable_heartbeat: true,
        }
    }
}

impl TaskClaimerConfig {
    /// Create TaskClaimerConfig from ConfigManager
    pub fn from_config_manager(config_manager: &ConfigManager) -> Self {
        let config = config_manager.config();

        Self {
            max_batch_size: config.orchestration.tasks_per_cycle as i32,
            default_claim_timeout: config.orchestration.default_claim_timeout_seconds as i32,
            heartbeat_interval: config.orchestration.heartbeat_interval(),
            enable_heartbeat: config.orchestration.enable_heartbeat,
        }
    }

    /// Create TaskClaimerConfig from TaskerConfig
    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        Self {
            max_batch_size: config.orchestration.tasks_per_cycle as i32,
            default_claim_timeout: config.orchestration.default_claim_timeout_seconds as i32,
            heartbeat_interval: config.orchestration.heartbeat_interval(),
            enable_heartbeat: config.orchestration.enable_heartbeat,
        }
    }
}
