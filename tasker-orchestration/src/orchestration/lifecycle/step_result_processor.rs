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
//! use tasker_orchestration::orchestration::lifecycle::step_result_processor::StepResultProcessor;
//! use tasker_shared::system_context::SystemContext;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create system context and step result processor
//! # use tasker_shared::config::ConfigManager;
//! # let config_manager = ConfigManager::load()?;
//! let context = Arc::new(SystemContext::from_config(config_manager).await?);
//! let processor = StepResultProcessor::new(context).await?;
//!
//! // Process step results from the queue
//! processor.process_batch().await?;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::lifecycle::{
    result_processor::OrchestrationResultProcessor, step_enqueuer_service::StepEnqueuerService,
    task_finalizer::TaskFinalizer,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tasker_shared::config::orchestration::StepResultProcessorConfig;
use tasker_shared::messaging::{PgmqClientTrait, StepResultMessage};
use tasker_shared::{SystemContext, TaskerError, TaskerResult};
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
    context: Arc<SystemContext>,
    /// Orchestration result processor for step handling
    orchestration_result_processor: OrchestrationResultProcessor,
    /// Configuration
    config: StepResultProcessorConfig,
}

impl StepResultProcessor {
    /// Create a new step result processor with unified client
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        let config = StepResultProcessorConfig::from_tasker_config(context.config_manager.config());
        // Create orchestration result processor with required dependencies

        let task_claim_step_enqueuer = StepEnqueuerService::new(context.clone()).await?;
        let task_claim_step_enqueuer = Arc::new(task_claim_step_enqueuer);

        let task_finalizer = TaskFinalizer::new(context.clone(), task_claim_step_enqueuer);
        let orchestration_result_processor =
            OrchestrationResultProcessor::new(task_finalizer, context.clone());

        Ok(Self {
            context,
            orchestration_result_processor,
            config,
        })
    }

    /// Process a batch of step result messages
    #[instrument(skip(self))]
    pub async fn process_batch(&self) -> TaskerResult<usize> {
        // Read messages from the step results queue
        let messages = self
            .context
            .message_client
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
                        .context
                        .message_client
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
                        .context
                        .message_client
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

        // Process the step result using the orchestration result processor
        self.orchestration_result_processor
            .handle_step_result_with_metadata(step_result)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to handle step result: {e}"))
            })?;

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
        assert_eq!(
            config.step_results_queue_name,
            "orchestration_step_results_queue"
        );
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
