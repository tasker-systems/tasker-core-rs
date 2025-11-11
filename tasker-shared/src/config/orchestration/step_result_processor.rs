// TAS-61 Phase 6C/6D: TaskerConfig is the canonical config
use crate::config::tasker::TaskerConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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

// TAS-61 Phase 6C/6D: V2 configuration is canonical
impl From<&TaskerConfig> for StepResultProcessorConfig {
    fn from(config: &TaskerConfig) -> StepResultProcessorConfig {
        // Build queue name from pattern: {orchestration_namespace}_{step_results}_queue
        let step_results_queue_name = format!(
            "{}_{}_queue",
            config.common.queues.orchestration_namespace,
            config.common.queues.orchestration_queues.step_results
        );

        StepResultProcessorConfig {
            step_results_queue_name,
            batch_size: config.common.queues.default_batch_size as i32,
            visibility_timeout_seconds: config.common.queues.default_visibility_timeout_seconds
                as i32,
            polling_interval_seconds: (config.common.queues.pgmq.poll_interval_ms / 1000) as u64,
            max_processing_attempts: config.common.queues.pgmq.max_retries as i32,
        }
    }
}

impl From<Arc<TaskerConfig>> for StepResultProcessorConfig {
    fn from(config: Arc<TaskerConfig>) -> StepResultProcessorConfig {
        // Build queue name from pattern: {orchestration_namespace}_{step_results}_queue
        let step_results_queue_name = format!(
            "{}_{}_queue",
            config.common.queues.orchestration_namespace,
            config.common.queues.orchestration_queues.step_results
        );

        StepResultProcessorConfig {
            step_results_queue_name,
            batch_size: config.common.queues.default_batch_size as i32,
            visibility_timeout_seconds: config.common.queues.default_visibility_timeout_seconds
                as i32,
            polling_interval_seconds: (config.common.queues.pgmq.poll_interval_ms / 1000) as u64,
            max_processing_attempts: config.common.queues.pgmq.max_retries as i32,
        }
    }
}
