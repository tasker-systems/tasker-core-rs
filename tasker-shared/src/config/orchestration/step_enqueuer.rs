use crate::config::manager::ConfigManager;
use crate::config::TaskerConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Configuration for step enqueueing behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEnqueuerConfig {
    /// Maximum number of steps to process per task
    pub max_steps_per_task: usize,
    /// Delay in seconds before making steps visible in queues
    pub enqueue_delay_seconds: i32,
    /// Enable detailed logging for debugging
    pub enable_detailed_logging: bool,
    /// Timeout for individual step enqueueing operations
    pub enqueue_timeout_seconds: u64,
}

impl Default for StepEnqueuerConfig {
    fn default() -> Self {
        Self {
            max_steps_per_task: 100,
            enqueue_delay_seconds: 0,
            enable_detailed_logging: false,
            enqueue_timeout_seconds: 30,
        }
    }
}

impl StepEnqueuerConfig {
    /// Create StepEnqueuerConfig from ConfigManager
    pub fn from_config_manager(config_manager: Arc<ConfigManager>) -> Self {
        let config = config_manager.config();

        Self {
            max_steps_per_task: config.execution.step_batch_size as usize,
            enqueue_delay_seconds: 0, // No direct mapping, keep default
            enable_detailed_logging: config.orchestration.enable_performance_logging,
            enqueue_timeout_seconds: config.execution.step_execution_timeout_seconds,
        }
    }

    /// Create StepEnqueuerConfig from TaskerConfig
    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        Self {
            max_steps_per_task: config.execution.step_batch_size as usize,
            enqueue_delay_seconds: 0, // No direct mapping, keep default
            enable_detailed_logging: config.orchestration.enable_performance_logging,
            enqueue_timeout_seconds: config.execution.step_execution_timeout_seconds,
        }
    }
}
