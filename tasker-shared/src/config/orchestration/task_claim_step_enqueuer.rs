use crate::config::orchestration::step_enqueuer::StepEnqueuerConfig;
use crate::config::orchestration::step_result_processor::StepResultProcessorConfig;
// REMOVED: TaskClaimerConfig for TAS-41
use crate::config::TaskerConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for orchestration loop behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimStepEnqueuerConfig {
    /// Number of tasks to claim per cycle
    pub max_batch_size: i32,
    /// Namespace filter (None = all namespaces)
    pub namespace_filter: Option<String>,
    /// Continuous run interval
    pub cycle_interval: Duration,
    /// Maximum number of continuous cycles (None = infinite)
    pub max_cycles: Option<usize>,
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
            max_batch_size: 5,
            namespace_filter: None,
            cycle_interval: Duration::from_secs(1),
            max_cycles: None,
            enable_performance_logging: false,
            enable_heartbeat: true,
            // REMOVED: task_claimer_config for TAS-41
            step_enqueuer_config: StepEnqueuerConfig::default(),
            step_result_processor_config: StepResultProcessorConfig::default(),
        }
    }
}

impl TaskClaimStepEnqueuerConfig {
    /// Create OrchestrationLoopConfig from ConfigManager
    pub fn from_config_manager(config_manager: &crate::config::ConfigManager) -> Self {
        let config = config_manager.config();

        Self::from_tasker_config(config)
    }

    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        Self {
            max_batch_size: 5,      // Default value, was config.task_claimer.max_batch_size
            namespace_filter: None, // No direct mapping in config, keep as runtime parameter
            cycle_interval: Duration::from_secs(1), // Default value, was config.task_claimer.cycle_interval()
            max_cycles: None, // No direct mapping in config, keep as runtime parameter
            enable_performance_logging: config.orchestration.enable_performance_logging,
            enable_heartbeat: true, // Default value, was config.task_claimer.enable_heartbeat
            // REMOVED: task_claimer_config for TAS-41
            step_enqueuer_config: StepEnqueuerConfig::from_tasker_config(config),
            step_result_processor_config: StepResultProcessorConfig::from_tasker_config(config),
        }
    }
}
