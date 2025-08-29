use crate::config::manager::ConfigManager;
use crate::config::TaskerConfig;
use serde::{Deserialize, Serialize};

/// Configuration for step result processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResultProcessorConfig {
    /// Queue name for step results
    pub step_results_queue_name: String,
    /// Number of results to read per batch
    pub batch_size: i32,
    /// Visibility timeout for result messages (seconds)
    pub visibility_timeout_seconds: i32,
    /// Polling interval when no messages (seconds)
    pub polling_interval_seconds: u64,
    /// Maximum processing attempts before giving up
    pub max_processing_attempts: i32,
}

impl Default for StepResultProcessorConfig {
    fn default() -> Self {
        Self {
            step_results_queue_name: "orchestration_step_results".to_string(),
            batch_size: 10,
            visibility_timeout_seconds: 300, // 5 minutes
            polling_interval_seconds: 1,
            max_processing_attempts: 3,
        }
    }
}

impl StepResultProcessorConfig {
    /// Create StepResultProcessorConfig from ConfigManager
    pub fn from_config_manager(config_manager: &ConfigManager) -> Self {
        let config = config_manager.config();

        Self {
            step_results_queue_name: config
                .orchestration
                .queues
                .orchestration_owned
                .step_results
                .clone(),
            batch_size: config.pgmq.batch_size as i32,
            visibility_timeout_seconds: config.pgmq.visibility_timeout_seconds as i32,
            polling_interval_seconds: config.pgmq.poll_interval_ms / 1000, // Convert ms to seconds
            max_processing_attempts: config.pgmq.max_retries as i32,
        }
    }

    pub fn from_tasker_config(tasker_config: &TaskerConfig) -> Self {
        Self {
            step_results_queue_name: tasker_config
                .orchestration
                .queues
                .orchestration_owned
                .step_results
                .clone(),
            batch_size: tasker_config.pgmq.batch_size as i32,
            visibility_timeout_seconds: tasker_config.pgmq.visibility_timeout_seconds as i32,
            polling_interval_seconds: tasker_config.pgmq.poll_interval_ms / 1000, // Convert ms to seconds
            max_processing_attempts: tasker_config.pgmq.max_retries as i32,
        }
    }
}
