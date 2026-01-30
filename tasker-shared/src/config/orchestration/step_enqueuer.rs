//! # Step Enqueuer Configuration
//!
//! Configuration for step enqueueing behavior in the orchestration system.
//!
//! ## Overview
//!
//! Controls how workflow steps are enqueued to PGMQ for processing:
//! - **Batch Sizing**: Maximum steps to process per task (`max_steps_per_task`)
//! - **Visibility Delays**: Optional delay before steps become visible in queues
//! - **Timeouts**: Per-operation timeout limits for enqueueing
//! - **Logging**: Optional detailed logging for debugging
//!
//! ## Default Values
//!
//! | Setting | Default | Description |
//! |---------|---------|-------------|
//! | `max_steps_per_task` | 100 | Maximum steps per task |
//! | `enqueue_delay_seconds` | 0 | Visibility delay (0 = immediate) |
//! | `enable_detailed_logging` | false | Debug logging |
//! | `enqueue_timeout_seconds` | 30 | Operation timeout |
//!
//! ## Configuration Mapping
//!
//! Values are derived from `TaskerConfig`:
//! - `max_steps_per_task` ← `common.execution.step_batch_size`
//! - `enqueue_timeout_seconds` ← `common.execution.step_execution_timeout_seconds`
//! - `enable_detailed_logging` ← `orchestration.enable_performance_logging`

// TAS-61 Phase 6C/6D: TaskerConfig is the canonical config
use crate::config::tasker::TaskerConfig;
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

// TAS-61 Phase 6C/6D: V2 configuration is canonical
impl From<&TaskerConfig> for StepEnqueuerConfig {
    fn from(config: &TaskerConfig) -> StepEnqueuerConfig {
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

impl From<Arc<TaskerConfig>> for StepEnqueuerConfig {
    fn from(config: Arc<TaskerConfig>) -> StepEnqueuerConfig {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_enqueuer_config_default_values() {
        let config = StepEnqueuerConfig::default();
        assert_eq!(config.max_steps_per_task, 100);
        assert_eq!(config.enqueue_delay_seconds, 0);
        assert!(!config.enable_detailed_logging);
        assert_eq!(config.enqueue_timeout_seconds, 30);
    }
}
