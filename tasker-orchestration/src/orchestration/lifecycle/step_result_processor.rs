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
//! // Create orchestration system context (TAS-50 Phase 2: context-specific loading)
//! let context = Arc::new(SystemContext::new_for_orchestration().await?);
//! let processor = StepResultProcessor::new(context).await?;
//!
//! // Process step results from the queue
//! processor.process_batch().await?;
//! # Ok(())
//! # }
//! ```

use crate::actors::decision_point_actor::DecisionPointActor;
use crate::orchestration::lifecycle::{
    result_processing::OrchestrationResultProcessor, step_enqueuer_services::StepEnqueuerService,
    task_finalization::TaskFinalizer, DecisionPointService,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::config::orchestration::StepResultProcessorConfig;
use tasker_shared::messaging::service::QueuedMessage;
use tasker_shared::messaging::StepResultMessage;
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
#[derive(Clone, Debug)]
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
    ///
    /// TAS-53 Phase 6: Now creates DecisionPointActor for dynamic workflow step creation
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        // Use From<Arc<TaskerConfig>> implementation (V2 config is canonical)
        let config: StepResultProcessorConfig = context.tasker_config.clone().into();
        // Create orchestration result processor with required dependencies

        let task_claim_step_enqueuer = StepEnqueuerService::new(context.clone()).await?;
        let task_claim_step_enqueuer = Arc::new(task_claim_step_enqueuer);

        let task_finalizer = TaskFinalizer::new(context.clone(), task_claim_step_enqueuer);

        // TAS-53 Phase 6: Create DecisionPointActor for dynamic workflow step creation
        let decision_point_service = Arc::new(DecisionPointService::new(context.clone()));
        let decision_point_actor = Arc::new(DecisionPointActor::new(
            context.clone(),
            decision_point_service,
        ));

        // TAS-59 Phase 4: Create BatchProcessingActor for dynamic batch worker creation
        let batch_processing_service = Arc::new(
            crate::orchestration::lifecycle::batch_processing::BatchProcessingService::new(
                context.clone(),
            ),
        );
        let batch_processing_actor = Arc::new(
            crate::actors::batch_processing_actor::BatchProcessingActor::new(
                context.clone(),
                batch_processing_service,
            ),
        );

        let orchestration_result_processor = OrchestrationResultProcessor::new(
            task_finalizer,
            context.clone(),
            decision_point_actor,
            batch_processing_actor,
        );

        Ok(Self {
            context,
            orchestration_result_processor,
            config,
        })
    }

    /// Process a batch of step result messages
    ///
    /// TAS-133e: Updated to use MessageClient with provider-agnostic API
    #[instrument(skip(self))]
    pub async fn process_batch(&self) -> TaskerResult<usize> {
        // TAS-133e: Use receive_messages with Duration-based visibility timeout
        let visibility_timeout = Duration::from_secs(self.config.visibility_timeout_seconds as u64);
        let messages: Vec<QueuedMessage<StepResultMessage>> = self
            .context
            .message_client()
            .receive_messages(
                &self.config.step_results_queue_name,
                self.config.batch_size as usize,
                visibility_timeout,
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
            let msg_id = message.receipt_handle.as_str();
            match self
                .process_single_step_result(&message.message, msg_id)
                .await
            {
                Ok(()) => {
                    // TAS-133e: Ack the successfully processed message
                    if let Err(e) = self
                        .context
                        .message_client()
                        .ack_message(
                            &self.config.step_results_queue_name,
                            &message.receipt_handle,
                        )
                        .await
                    {
                        warn!(
                            msg_id = %msg_id,
                            error = %e,
                            "Failed to ack processed step result message"
                        );
                    } else {
                        processed_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        msg_id = %msg_id,
                        error = %e,
                        "Failed to process step result message"
                    );

                    // TAS-133e: Nack failed messages without requeue (goes to DLQ)
                    if let Err(nack_err) = self
                        .context
                        .message_client()
                        .nack_message(
                            &self.config.step_results_queue_name,
                            &message.receipt_handle,
                            false,
                        )
                        .await
                    {
                        warn!(
                            msg_id = %msg_id,
                            error = %nack_err,
                            "Failed to nack failed step result message"
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
    ///
    /// TAS-133e: Now receives typed StepResultMessage directly (no JSON parsing needed)
    #[instrument(skip(self, step_result))]
    async fn process_single_step_result(
        &self,
        step_result: &StepResultMessage,
        msg_id: &str,
    ) -> TaskerResult<()> {
        debug!(msg_id = %msg_id, "Processing step result message");

        info!(
            step_uuid = %step_result.step_uuid,
            task_uuid = %step_result.task_uuid,
            status = ?step_result.status,
            execution_time_ms = step_result.execution_time_ms,
            msg_id = %msg_id,
            "Processing step result"
        );

        // Process the step result using the orchestration result processor
        self.orchestration_result_processor
            .handle_step_result_with_metadata(step_result.clone())
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
