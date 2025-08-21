//! # Orchestration Executor Traits
//!
//! Core traits and types for the orchestration executor system. This module defines
//! the standard interface for orchestration workers that can be pooled, monitored,
//! and dynamically scaled.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use tasker_shared::config::orchestration::executor::{ExecutorConfig, ExecutorType};
use tasker_shared::TaskerResult;

/// Result of processing a batch of items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessBatchResult {
    /// Number of items processed successfully
    pub processed_count: usize,
    /// Number of items that failed processing
    pub failed_count: usize,
    /// Number of items that were skipped
    pub skipped_count: usize,
    /// Total processing time in milliseconds
    pub processing_time_ms: u64,
    /// Average processing time per item in milliseconds
    pub avg_processing_time_ms: f64,
    /// Any warnings or non-fatal errors encountered
    pub warnings: Vec<String>,
    /// Whether more items are available for processing
    pub has_more_items: bool,
}

impl ProcessBatchResult {
    /// Create a new empty result
    pub fn empty() -> Self {
        Self {
            processed_count: 0,
            failed_count: 0,
            skipped_count: 0,
            processing_time_ms: 0,
            avg_processing_time_ms: 0.0,
            warnings: Vec::new(),
            has_more_items: false,
        }
    }

    /// Create a result with processed items
    pub fn processed(count: usize, processing_time_ms: u64) -> Self {
        let avg_time = if count > 0 {
            processing_time_ms as f64 / count as f64
        } else {
            0.0
        };

        Self {
            processed_count: count,
            failed_count: 0,
            skipped_count: 0,
            processing_time_ms,
            avg_processing_time_ms: avg_time,
            warnings: Vec::new(),
            has_more_items: false,
        }
    }

    /// Get total items processed (success + failed + skipped)
    pub fn total_items(&self) -> usize {
        self.processed_count + self.failed_count + self.skipped_count
    }

    /// Get success rate as a percentage (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        let total = self.total_items();
        if total == 0 {
            1.0
        } else {
            self.processed_count as f64 / total as f64
        }
    }

    /// Add a warning message
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Check if the result indicates good performance
    pub fn is_performing_well(&self) -> bool {
        // Consider performance good if:
        // - Success rate > 90%
        // - Average processing time < 1000ms per item
        // - No more than 5 warnings
        self.success_rate() > 0.9
            && self.avg_processing_time_ms < 1000.0
            && self.warnings.len() <= 5
    }
}

/// Core trait for orchestration executors
///
/// This trait defines the standard interface for orchestration workers that can be
/// pooled, monitored, and dynamically scaled by the OrchestrationLoopCoordinator.
#[async_trait]
pub trait OrchestrationExecutor: Send + Sync {
    /// Get unique identifier for this executor instance
    fn id(&self) -> Uuid;

    /// Get the type of this executor
    fn executor_type(&self) -> ExecutorType;

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
    fn test_executor_type_properties() {
        for executor_type in ExecutorType::all() {
            assert!(!executor_type.name().is_empty());
            assert!(executor_type.default_polling_interval_ms() > 0);
            assert!(executor_type.default_batch_size() > 0);
        }
    }

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

    #[test]
    fn test_executor_config() {
        let config = ExecutorConfig::default_for_type(ExecutorType::TaskRequestProcessor);
        assert!(config.validate().is_ok());

        let interval = config.effective_polling_interval_ms();
        assert_eq!(interval, config.polling_interval_ms);

        let batch_size = config.effective_batch_size();
        assert_eq!(batch_size, config.batch_size);
    }

    #[test]
    fn test_executor_config_validation() {
        let mut config = ExecutorConfig::default_for_type(ExecutorType::OrchestrationLoop);

        // Test invalid polling interval
        config.polling_interval_ms = 0;
        assert!(config.validate().is_err());

        // Test invalid backpressure factor
        config.polling_interval_ms = 100;
        config.backpressure_factor = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_backpressure_effects() {
        let mut config = ExecutorConfig::default_for_type(ExecutorType::StepResultProcessor);
        config.backpressure_factor = 0.5; // 50% backpressure

        let effective_interval = config.effective_polling_interval_ms();
        let effective_batch_size = config.effective_batch_size();

        // With 50% backpressure, polling should be faster (shorter interval)
        assert!(effective_interval >= config.polling_interval_ms);
        // With 50% backpressure, batch size should be smaller
        assert!(effective_batch_size <= config.batch_size);
    }
}
