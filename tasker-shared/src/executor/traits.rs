//! # Orchestration Executor Traits
//!
//! Core traits and types for the orchestration executor system. This module defines
//! the standard interface for orchestration workers that can be pooled, monitored,
//! and dynamically scaled.

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::executor::ExecutorConfig;
use crate::config::TaskerConfig;
use crate::executor::process_batch_result::ProcessBatchResult;
use crate::messaging::UnifiedPgmqClient;
use crate::TaskerResult;

#[async_trait]
pub trait ExecutorProcessor: Send + Sync {
    async fn new(
        pool: PgPool,
        pgmq_client: Arc<UnifiedPgmqClient>,
        tasker_config: TaskerConfig,
        executor_id: Uuid,
    ) -> TaskerResult<Self>
    where
        Self: Sized;

    /// Process a batch of items
    ///
    /// This method should process the given batch of items and return a result
    /// indicating whether the processing was successful or not.
    async fn process_batch(&self) -> TaskerResult<ProcessBatchResult>;

    fn executor_id(&self) -> &Uuid;

    fn name(&self) -> &str;

    fn config(&self) -> &ExecutorConfig;
}

use core::fmt::Debug;
impl Debug for dyn ExecutorProcessor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ExecutorProcessor {{ orchestrator_id: {}, name: {} }}",
            self.executor_id(),
            self.name()
        )
    }
}

/// Core trait for orchestration executors
///
/// This trait defines the standard interface for orchestration workers that can be
/// pooled, monitored, and dynamically scaled by the OrchestrationLoopCoordinator.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Get unique identifier for this executor instance
    fn id(&self) -> Uuid;

    /// Start the executor (begin processing loop)
    ///
    /// This method should start the executor's main processing loop and return
    /// once the executor is ready to process items. The actual processing should
    /// continue in the background.
    ///
    /// Takes `Arc<Self>` to prevent memory leaks from circular references.
    async fn start(self: Arc<Self>) -> TaskerResult<()>;

    /// Stop the executor gracefully
    ///
    /// This method should signal the executor to stop and wait for it to complete
    /// any in-flight processing within the specified timeout.
    async fn stop(&self, timeout: Duration) -> TaskerResult<()>;

    /// Get current health status of the executor
    ///
    /// This method should return the current health state including recent
    /// performance metrics and any error conditions.
    async fn health(&self) -> super::health::ExecutorHealth;

    /// Get current metrics for the executor
    ///
    /// This method should return detailed performance metrics for monitoring
    /// and scaling decisions.
    async fn metrics(&self) -> super::metrics::ExecutorMetrics;

    /// Process a batch of items
    ///
    /// This is the core processing method that should be called by the coordinator
    /// to process a batch of work items. The implementation should handle:
    /// - Fetching work items from appropriate sources
    /// - Processing each item according to executor type
    /// - Recording metrics and handling errors
    /// - Returning detailed results
    async fn process_batch(&self) -> TaskerResult<ProcessBatchResult>;

    /// Send heartbeat signal
    ///
    /// This method should be called periodically to indicate the executor is
    /// alive and responsive. It can also be used to update health metrics.
    async fn heartbeat(&self) -> TaskerResult<()>;

    /// Check if the executor should continue running
    ///
    /// This method should return false if the executor has been signaled to stop
    /// or has encountered a fatal error.
    fn should_continue(&self) -> bool;

    /// Apply backpressure to the executor
    ///
    /// This method should adjust the executor's behavior to reduce load when
    /// the system is under pressure. The factor ranges from 0.0 (full stop)
    /// to 1.0 (normal operation).
    ///
    /// # Parameters
    /// - `factor`: Backpressure factor (0.0 = full stop, 1.0 = normal)
    async fn apply_backpressure(&self, factor: f64) -> TaskerResult<()>;

    /// Get executor configuration
    ///
    /// Returns configuration parameters that can be adjusted by the coordinator
    /// for scaling and optimization purposes.
    async fn get_config(&self) -> ExecutorConfig;

    /// Update executor configuration
    ///
    /// Allows the coordinator to adjust executor parameters for optimization.
    async fn update_config(&self, config: ExecutorConfig) -> TaskerResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_batch_result() {
        let mut result = ProcessBatchResult::processed(10, 1000);
        assert_eq!(result.total_items(), 10);
        assert_eq!(result.success_rate(), 1.0);
        assert_eq!(result.avg_processing_time_ms, 100.0);

        result.add_warning("Test warning".to_string());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.is_performing_well());
    }
}
