use crate::config::orchestration::step_enqueuer::StepEnqueuerConfig;
use crate::config::orchestration::step_result_processor::StepResultProcessorConfig;
// TAS-61 Phase 6C/6D: TaskerConfig is the canonical config
use crate::config::tasker::TaskerConfig;
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
impl From<&TaskerConfig> for TaskClaimStepEnqueuerConfig {
    fn from(config: &TaskerConfig) -> TaskClaimStepEnqueuerConfig {
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

impl From<Arc<TaskerConfig>> for TaskClaimStepEnqueuerConfig {
    fn from(config: Arc<TaskerConfig>) -> TaskClaimStepEnqueuerConfig {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_claim_step_enqueuer_config_default_values() {
        let config = TaskClaimStepEnqueuerConfig::default();
        assert_eq!(config.batch_size, 5);
        assert!(config.namespace_filter.is_none());
        assert!(!config.enable_performance_logging);
        assert!(config.enable_heartbeat);
    }

    #[test]
    fn test_task_claim_step_enqueuer_config_nested_defaults() {
        let config = TaskClaimStepEnqueuerConfig::default();

        // Verify nested StepEnqueuerConfig defaults
        assert_eq!(config.step_enqueuer_config.max_steps_per_task, 100);
        assert_eq!(config.step_enqueuer_config.enqueue_delay_seconds, 0);
        assert!(!config.step_enqueuer_config.enable_detailed_logging);
        assert_eq!(config.step_enqueuer_config.enqueue_timeout_seconds, 30);

        // Verify nested StepResultProcessorConfig defaults
        assert_eq!(
            config.step_result_processor_config.step_results_queue_name,
            "orchestration_step_results"
        );
        assert_eq!(config.step_result_processor_config.batch_size, 10);
        assert_eq!(
            config
                .step_result_processor_config
                .visibility_timeout_seconds,
            300
        );
        assert_eq!(
            config.step_result_processor_config.polling_interval_seconds,
            1
        );
        assert_eq!(
            config.step_result_processor_config.max_processing_attempts,
            3
        );
    }
}
