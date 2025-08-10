//! # Step Result Processor
//!
//! ## Architecture: Individual Step Result Processing for pgmq Orchestration
//!
//! The StepResultProcessor handles individual step results from the orchestration_step_results
//! queue, replacing the batch-based approach with individual step processing. This creates
//! the complete feedback loop: Task Request → Task Claiming → Step Enqueueing → Step Execution →
//! Step Results → Task Finalization.
//!
//! ## Key Features
//!
//! - **Individual Step Processing**: Processes one step result at a time, not batches
//! - **State Management Integration**: Updates step states and triggers task state transitions  
//! - **Task Finalization**: Determines when tasks are complete and triggers finalization
//! - **Orchestration Metadata**: Processes worker metadata for intelligent backoff decisions
//! - **Error Handling**: Robust step error processing with retry coordination
//!
//! ## Integration
//!
//! Works with:
//! - `OrchestrationResultProcessor`: For step result processing and metadata handling
//! - `TaskFinalizer`: For task completion logic
//! - `StateManager`: For state transitions
//! - `PgmqClient`: For queue operations
//! - Queue workers: Processes results from autonomous Ruby queue workers
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::orchestration::step_result_processor::StepResultProcessor;
//! use tasker_core::messaging::UnifiedPgmqClient;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool = sqlx::PgPool::connect("postgresql://localhost/test").await?;
//! # let pgmq_client = Arc::new(UnifiedPgmqClient::Standard(
//! #     tasker_core::messaging::PgmqClient::new_with_pool(pool.clone()).await
//! # ));
//! let processor = StepResultProcessor::new(pool, pgmq_client).await?;
//!
//! // Process step results continuously
//! processor.start_processing_loop().await?;
//! # Ok(())
//! # }
//! ```

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::error::{Result, TaskerError};
use crate::events::EventPublisher;
use crate::messaging::{
    PgmqClientTrait, StepExecutionStatus, StepResultMessage, UnifiedPgmqClient,
};
use crate::orchestration::{
    result_processor::OrchestrationResultProcessor, task_finalizer::TaskFinalizer, StateManager,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

/// Result of step result processing operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResultProcessingResult {
    /// Number of step results processed in this batch
    pub results_processed: usize,
    /// Number of step results that failed to process
    pub results_failed: usize,
    /// Number of tasks that were finalized as complete
    pub tasks_completed: usize,
    /// Number of tasks that were marked as failed
    pub tasks_failed: usize,
    /// Processing duration in milliseconds
    pub processing_duration_ms: u64,
    /// Any warnings encountered
    pub warnings: Vec<String>,
}

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
    pub fn from_config_manager(config_manager: &crate::config::ConfigManager) -> Self {
        let config = config_manager.config();

        Self {
            step_results_queue_name: config.orchestration.queues.step_results.clone(),
            batch_size: config.pgmq.batch_size as i32,
            visibility_timeout_seconds: config.pgmq.visibility_timeout_seconds as i32,
            polling_interval_seconds: config.pgmq.poll_interval_ms / 1000, // Convert ms to seconds
            max_processing_attempts: config.pgmq.max_retries as i32,
        }
    }
}

/// Processes individual step results from pgmq queues
#[derive(Clone)]
pub struct StepResultProcessor {
    /// PostgreSQL message queue client (unified for circuit breaker flexibility)
    pgmq_client: Arc<UnifiedPgmqClient>,
    /// Orchestration result processor for step handling
    orchestration_result_processor: OrchestrationResultProcessor,
    /// Configuration
    config: StepResultProcessorConfig,
}

impl StepResultProcessor {
    /// Create a new step result processor with unified client
    pub async fn new(pool: PgPool, pgmq_client: Arc<UnifiedPgmqClient>) -> Result<Self> {
        let config = StepResultProcessorConfig::default();
        Self::with_config(pool, pgmq_client, config).await
    }

    /// Create a new step result processor with custom configuration
    pub async fn with_config(
        pool: PgPool,
        pgmq_client: Arc<UnifiedPgmqClient>,
        config: StepResultProcessorConfig,
    ) -> Result<Self> {
        // Create orchestration result processor with required dependencies
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let event_publisher = EventPublisher::new();
        let state_manager = StateManager::new(sql_executor, event_publisher, pool.clone());
        let task_finalizer = TaskFinalizer::new(pool.clone());
        let orchestration_result_processor =
            OrchestrationResultProcessor::new(state_manager, task_finalizer, pool.clone());

        Ok(Self {
            pgmq_client,
            orchestration_result_processor,
            config,
        })
    }

    /// Start the step result processing loop
    #[instrument(skip(self))]
    pub async fn start_processing_loop(&self) -> Result<()> {
        info!(
            queue = %self.config.step_results_queue_name,
            polling_interval = %self.config.polling_interval_seconds,
            "Starting step result processing loop"
        );

        // Ensure step results queue exists
        self.ensure_queue_exists().await?;

        loop {
            match self.process_step_result_batch().await {
                Ok(processed_count) => {
                    if processed_count == 0 {
                        // No results processed, wait before polling again
                        tokio::time::sleep(Duration::from_secs(
                            self.config.polling_interval_seconds,
                        ))
                        .await;
                    }
                    // If we processed results, continue immediately for better throughput
                }
                Err(e) => {
                    error!(error = %e, "Error in step result processing batch");
                    // Wait before retrying on error
                    tokio::time::sleep(Duration::from_secs(self.config.polling_interval_seconds))
                        .await;
                }
            }
        }
    }

    /// Process a batch of step result messages
    #[instrument(skip(self))]
    async fn process_step_result_batch(&self) -> Result<usize> {
        // Read messages from the step results queue
        let messages = self
            .pgmq_client
            .read_messages(
                &self.config.step_results_queue_name,
                Some(self.config.visibility_timeout_seconds),
                Some(self.config.batch_size),
            )
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to read step result messages: {e}"))
            })?;

        if messages.is_empty() {
            return Ok(0);
        }

        let message_count = messages.len();
        debug!(
            message_count = message_count,
            queue = %self.config.step_results_queue_name,
            "Processing batch of step result messages"
        );

        let mut processed_count = 0;

        for message in messages {
            match self
                .process_single_step_result(&message.message, message.msg_id)
                .await
            {
                Ok(()) => {
                    // Delete the successfully processed message
                    if let Err(e) = self
                        .pgmq_client
                        .delete_message(&self.config.step_results_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %e,
                            "Failed to delete processed step result message"
                        );
                    } else {
                        processed_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        msg_id = message.msg_id,
                        error = %e,
                        "Failed to process step result message"
                    );

                    // Archive failed messages
                    if let Err(archive_err) = self
                        .pgmq_client
                        .archive_message(&self.config.step_results_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %archive_err,
                            "Failed to archive failed step result message"
                        );
                    }
                }
            }
        }

        if processed_count > 0 {
            info!(
                processed_count = processed_count,
                total_messages = message_count,
                "Completed step result processing batch"
            );
        }

        Ok(processed_count)
    }

    /// Process a single step result message
    #[instrument(skip(self, payload))]
    async fn process_single_step_result(
        &self,
        payload: &serde_json::Value,
        msg_id: i64,
    ) -> Result<()> {
        debug!(msg_id = msg_id, "Processing step result message");

        // Parse the step result message
        let step_result: StepResultMessage =
            serde_json::from_value(payload.clone()).map_err(|e| {
                TaskerError::MessagingError(format!("Failed to parse step result message: {e}"))
            })?;

        info!(
            step_id = step_result.step_id,
            task_id = step_result.task_id,
            status = ?step_result.status,
            execution_time_ms = step_result.execution_time_ms,
            msg_id = msg_id,
            "Processing step result"
        );

        // Convert step status to string for result processor
        let status_string = match step_result.status {
            StepExecutionStatus::Success => "success".to_string(),
            StepExecutionStatus::Failed => "failed".to_string(),
            StepExecutionStatus::Skipped => "skipped".to_string(),
            StepExecutionStatus::Timeout => "timeout".to_string(),
            StepExecutionStatus::Cancelled => "cancelled".to_string(),
        };

        // Convert step error to result processor format
        let error = step_result
            .error
            .map(|e| crate::orchestration::result_processor::StepError {
                message: e.message,
                error_type: e.error_type,
                retryable: e.retryable,
            });

        // Process the step result using the orchestration result processor
        self.orchestration_result_processor
            .handle_step_result_with_metadata(
                step_result.step_id,
                status_string,
                step_result.results,
                error,
                step_result.execution_time_ms,
                step_result.orchestration_metadata,
            )
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to handle step result: {e}"))
            })?;

        info!(
            step_id = step_result.step_id,
            task_id = step_result.task_id,
            msg_id = msg_id,
            "Step result processed successfully"
        );

        Ok(())
    }

    /// Ensure the step results queue exists
    async fn ensure_queue_exists(&self) -> Result<()> {
        if let Err(e) = self
            .pgmq_client
            .create_queue(&self.config.step_results_queue_name)
            .await
        {
            debug!(
                queue = %self.config.step_results_queue_name,
                error = %e,
                "Step results queue may already exist"
            );
        }

        info!(
            queue = %self.config.step_results_queue_name,
            "Step results queue initialized"
        );

        Ok(())
    }

    /// Get current configuration
    pub fn config(&self) -> &StepResultProcessorConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = StepResultProcessorConfig::default();
        assert_eq!(config.step_results_queue_name, "orchestration_step_results");
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.visibility_timeout_seconds, 300);
        assert_eq!(config.polling_interval_seconds, 1);
        assert_eq!(config.max_processing_attempts, 3);
    }

    #[test]
    fn test_config_customization() {
        let config = StepResultProcessorConfig {
            step_results_queue_name: "custom_step_results".to_string(),
            batch_size: 20,
            visibility_timeout_seconds: 600,
            polling_interval_seconds: 2,
            max_processing_attempts: 5,
        };

        assert_eq!(config.step_results_queue_name, "custom_step_results");
        assert_eq!(config.batch_size, 20);
        assert_eq!(config.visibility_timeout_seconds, 600);
        assert_eq!(config.polling_interval_seconds, 2);
        assert_eq!(config.max_processing_attempts, 5);
    }
}
