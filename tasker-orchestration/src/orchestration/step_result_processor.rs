//! # TAS-32: Step Result Coordination Processor (No State Management)
//!
//! ## Architecture: Task-Level Coordination Only
//!
//! **ARCHITECTURE CHANGE**: The StepResultProcessor has been updated for TAS-32 where Ruby workers
//! handle all step execution, result saving, and state transitions. This processor now handles
//! only orchestration-level coordination.
//!
//! The coordination flow is: Task Request → Step Enqueueing → Ruby Worker Execution →
//! Step Result Coordination → Task Finalization.
//!
//! ## Key Features (TAS-32 Updated)
//!
//! - **Task Finalization Coordination**: Determines when tasks are complete
//! - **Orchestration Metadata Processing**: Processes worker metadata for backoff decisions
//! - **Retry Coordination**: Intelligent backoff calculations based on worker feedback
//! - **Cross-cutting Concerns**: Handles orchestration-level coordination
//!
//! ## What This NO LONGER Does
//!
//! - **Step State Management**: Ruby workers handle with Statesman state machines
//! - **Result Persistence**: Ruby MessageManager saves results to database
//! - **Step Transitions**: Ruby workers create workflow step transitions
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
//! use tasker_orchestration::orchestration::step_result_processor::StepResultProcessor;
//! use tasker_shared::messaging::UnifiedPgmqClient;
//! use tasker_shared::config::TaskerConfig;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool = sqlx::PgPool::connect("postgresql://localhost/test").await?;
//! # let pgmq_client = Arc::new(UnifiedPgmqClient::Standard(
//! #     tasker_shared::messaging::PgmqClient::new_with_pool(pool.clone()).await
//! # ));
//! # let config = TaskerConfig::default();
//! let processor = StepResultProcessor::new(pool, pgmq_client, config).await?;
//!
//! // Process step results continuously
//! processor.start_processing_loop().await?;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::{
    result_processor::OrchestrationResultProcessor, task_finalizer::TaskFinalizer,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::config::orchestration::StepResultProcessorConfig;
use tasker_shared::messaging::{
    PgmqClientTrait, StepExecutionStatus, StepResultMessage, UnifiedPgmqClient,
};
use tasker_shared::{config::TaskerConfig, TaskerError, TaskerResult};
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
    pub async fn new(
        pool: PgPool,
        pgmq_client: Arc<UnifiedPgmqClient>,
        tasker_config: TaskerConfig,
    ) -> TaskerResult<Self> {
        let config = StepResultProcessorConfig::from_tasker_config(&tasker_config);
        Self::with_config(pool, pgmq_client, config, tasker_config).await
    }

    /// Create a new step result processor with custom configuration
    pub async fn with_config(
        pool: PgPool,
        pgmq_client: Arc<UnifiedPgmqClient>,
        config: StepResultProcessorConfig,
        tasker_config: TaskerConfig,
    ) -> TaskerResult<Self> {
        // Create orchestration result processor with required dependencies
        let task_finalizer = TaskFinalizer::new(pool.clone(), tasker_config);
        let orchestration_result_processor =
            OrchestrationResultProcessor::new(task_finalizer, pool.clone());

        Ok(Self {
            pgmq_client,
            orchestration_result_processor,
            config,
        })
    }

    /// Start the step result processing loop
    #[instrument(skip(self))]
    pub async fn start_processing_loop(&self) -> TaskerResult<()> {
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
    pub async fn process_step_result_batch(&self) -> TaskerResult<usize> {
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
    ) -> TaskerResult<()> {
        debug!(msg_id = msg_id, "Processing step result message");

        // Parse the step result message
        let step_result: StepResultMessage =
            serde_json::from_value(payload.clone()).map_err(|e| {
                TaskerError::MessagingError(format!("Failed to parse step result message: {e}"))
            })?;

        info!(
            step_uuid = %step_result.step_uuid,
            task_uuid = %step_result.task_uuid,
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

        // Process the step result using the orchestration result processor
        self.orchestration_result_processor
            .handle_step_result_with_metadata(
                step_result.step_uuid,
                status_string,
                step_result.execution_time_ms,
                step_result.orchestration_metadata,
            )
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to handle step result: {e}"))
            })?;

        info!(
            step_uuid = %step_result.step_uuid,
            task_uuid = %step_result.task_uuid,
            msg_id = msg_id,
            "Step result processed successfully"
        );

        Ok(())
    }

    /// Ensure the step results queue exists
    async fn ensure_queue_exists(&self) -> TaskerResult<()> {
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
