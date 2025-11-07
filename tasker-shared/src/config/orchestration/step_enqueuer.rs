use crate::config::tasker::TaskerConfigV2;
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

impl From<&TaskerConfig> for StepEnqueuerConfig {
    fn from(config: &TaskerConfig) -> StepEnqueuerConfig {
        StepEnqueuerConfig {
            max_steps_per_task: config.execution.step_batch_size as usize,
            enqueue_delay_seconds: 0, // No direct mapping, keep default
            enable_detailed_logging: config.orchestration.enable_performance_logging,
            enqueue_timeout_seconds: config.execution.step_execution_timeout_seconds,
        }
    }
}

impl From<Arc<TaskerConfig>> for StepEnqueuerConfig {
    fn from(config: Arc<TaskerConfig>) -> StepEnqueuerConfig {
        StepEnqueuerConfig {
            max_steps_per_task: config.execution.step_batch_size as usize,
            enqueue_delay_seconds: 0, // No direct mapping, keep default
            enable_detailed_logging: config.orchestration.enable_performance_logging,
            enqueue_timeout_seconds: config.execution.step_execution_timeout_seconds,
        }
    }
}

// TAS-61 Phase 6B: V2 configuration support
impl From<&TaskerConfigV2> for StepEnqueuerConfig {
    fn from(config: &TaskerConfigV2) -> StepEnqueuerConfig {
        StepEnqueuerConfig {
            max_steps_per_task: config.common.execution.step_batch_size as usize,
            enqueue_delay_seconds: 0, // No direct mapping, keep default
            enable_detailed_logging: config
                .orchestration
                .as_ref()
                .map(|o| o.enable_performance_logging)
                .unwrap_or(false),
            enqueue_timeout_seconds: config.common.execution.step_execution_timeout_seconds as u64,
        }
    }
}

impl From<Arc<TaskerConfigV2>> for StepEnqueuerConfig {
    fn from(config: Arc<TaskerConfigV2>) -> StepEnqueuerConfig {
        StepEnqueuerConfig {
            max_steps_per_task: config.common.execution.step_batch_size as usize,
            enqueue_delay_seconds: 0, // No direct mapping, keep default
            enable_detailed_logging: config
                .orchestration
                .as_ref()
                .map(|o| o.enable_performance_logging)
                .unwrap_or(false),
            enqueue_timeout_seconds: config.common.execution.step_execution_timeout_seconds as u64,
        }
    }
}
