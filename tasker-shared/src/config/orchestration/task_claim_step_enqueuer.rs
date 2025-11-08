use crate::config::orchestration::step_enqueuer::StepEnqueuerConfig;
use crate::config::orchestration::step_result_processor::StepResultProcessorConfig;
// TAS-61 Phase 6C/6D: TaskerConfigV2 is the canonical config
use crate::config::tasker::TaskerConfigV2;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Configuration for orchestration loop behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimStepEnqueuerConfig {
    /// Number of tasks to claim per cycle
    pub batch_size: u32,
    /// Namespace filter (None = all namespaces)
    pub namespace_filter: Option<String>,
    /// Maximum number of continuous cycles (None = infinite)
    /// Enable detailed performance logging
    pub enable_performance_logging: bool,
    /// Enable heartbeat for long-running operations
    pub enable_heartbeat: bool,
    // REMOVED: task_claimer_config for TAS-41
    /// Step enqueuer configuration
    pub step_enqueuer_config: StepEnqueuerConfig,
    /// Step result processor configuration
    pub step_result_processor_config: StepResultProcessorConfig,
}

impl Default for TaskClaimStepEnqueuerConfig {
    fn default() -> Self {
        Self {
            batch_size: 5,
            namespace_filter: None,
            enable_performance_logging: false,
            enable_heartbeat: true,
            // REMOVED: task_claimer_config for TAS-41
            step_enqueuer_config: StepEnqueuerConfig::default(),
            step_result_processor_config: StepResultProcessorConfig::default(),
        }
    }
}

// TAS-61 Phase 6C/6D: V2 configuration is canonical
impl From<&TaskerConfigV2> for TaskClaimStepEnqueuerConfig {
    fn from(config: &TaskerConfigV2) -> TaskClaimStepEnqueuerConfig {
        TaskClaimStepEnqueuerConfig {
            batch_size: config.common.queues.default_batch_size,
            namespace_filter: None,
            enable_performance_logging: config
                .orchestration
                .as_ref()
                .map(|o| o.enable_performance_logging)
                .unwrap_or(false),
            enable_heartbeat: true,
            step_enqueuer_config: config.into(),
            step_result_processor_config: config.into(),
        }
    }
}

impl From<Arc<TaskerConfigV2>> for TaskClaimStepEnqueuerConfig {
    fn from(config: Arc<TaskerConfigV2>) -> TaskClaimStepEnqueuerConfig {
        TaskClaimStepEnqueuerConfig {
            batch_size: config.common.queues.default_batch_size,
            namespace_filter: None,
            enable_performance_logging: config
                .orchestration
                .as_ref()
                .map(|o| o.enable_performance_logging)
                .unwrap_or(false),
            enable_heartbeat: true,
            step_enqueuer_config: config.as_ref().into(),
            step_result_processor_config: config.as_ref().into(),
        }
    }
}
