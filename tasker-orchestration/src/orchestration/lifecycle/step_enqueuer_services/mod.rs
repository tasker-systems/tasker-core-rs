//! Step Enqueuer Services Module
//!
//! Service-oriented architecture for step enqueueing with clear separation of concerns:
//!
//! - **Types**: Result types, statistics, and metrics
//! - **StateHandlers**: State-specific processing logic
//! - **TaskProcessor**: Single task processing
//! - **BatchProcessor**: Batch processing orchestration
//! - **Summary**: Continuous orchestration summary management
//! - **Service**: Main orchestration service composition

pub use service::StepEnqueuerService;
pub use types::*;

mod batch_processor;
mod service;
mod state_handlers;
mod summary;
mod task_processor;
mod types;

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::config::orchestration::TaskClaimStepEnqueuerConfig;

    #[test]
    fn test_orchestration_loop_config_defaults() {
        let config = TaskClaimStepEnqueuerConfig::default();
        assert_eq!(config.batch_size, 5);
        assert!(config.namespace_filter.is_none());
        assert!(!config.enable_performance_logging);
        assert!(config.enable_heartbeat);
    }

    #[test]
    fn test_priority_distribution_calculation() {
        let distribution = PriorityDistribution {
            urgent_tasks: 2,
            high_tasks: 1,
            normal_tasks: 3,
            escalated_tasks: 1,
            ..Default::default()
        };

        assert_eq!(distribution.urgent_tasks, 2);
        assert_eq!(distribution.escalated_tasks, 1);
    }
}
