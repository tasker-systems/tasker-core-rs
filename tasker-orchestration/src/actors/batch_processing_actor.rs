//! # Batch Processing Actor
//!
//! Actor implementation wrapping BatchProcessingService for message-based
//! dynamic batch worker creation from batchable steps.

use crate::actors::{Handler, Message, OrchestrationActor};
use crate::orchestration::lifecycle::batch_processing::BatchProcessingService;
use async_trait::async_trait;
use sqlx::types::Uuid;
use std::sync::Arc;
use tasker_shared::messaging::execution_types::BatchProcessingOutcome;
use tasker_shared::models::WorkflowStep;
use tasker_shared::system_context::SystemContext;
use tasker_shared::StepExecutionResult;
use tasker_shared::TaskerResult;
use tracing::{debug, info};

/// Message for processing a batchable step that has completed
///
/// Wraps the batchable step and its result for actor-based batch processing.
/// The actor will delegate to BatchProcessingService for:
/// - Analyzing batch requirements from step result
/// - Validating worker templates
/// - Creating batch worker instances with cursor configurations
#[derive(Debug, Clone)]
pub struct ProcessBatchableStepMessage {
    /// The task UUID
    pub task_uuid: Uuid,
    /// The completed batchable step
    pub batchable_step: WorkflowStep,
    /// The step execution result containing batch outcome
    pub step_result: StepExecutionResult,
}

impl Message for ProcessBatchableStepMessage {
    type Response = BatchProcessingOutcome;
}

/// Actor for processing batchable steps and creating batch workers
///
/// This actor wraps BatchProcessingService and provides message-based
/// access to batch processing functionality. It handles:
///
/// 1. Analyzing batch requirements from completed batchable steps
/// 2. Validating worker templates and batch configurations
/// 3. Creating dynamic batch worker instances with cursor configs
/// 4. Transaction-safe worker creation via WorkflowStepCreator
///
/// # Example
///
/// ```rust,no_run
/// use tasker_orchestration::actors::{Handler, batch_processing_actor::*};
/// use uuid::Uuid;
///
/// # async fn example(
/// #     actor: BatchProcessingActor,
/// #     task_uuid: Uuid,
/// #     batchable_step: tasker_shared::models::WorkflowStep,
/// #     step_result: tasker_shared::StepExecutionResult
/// # ) -> Result<(), Box<dyn std::error::Error>> {
/// let msg = ProcessBatchableStepMessage {
///     task_uuid,
///     batchable_step,
///     step_result,
/// };
/// let outcome = actor.handle(msg).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct BatchProcessingActor {
    /// System context for framework operations
    context: Arc<SystemContext>,

    /// Underlying service that performs the actual work
    service: Arc<BatchProcessingService>,
}

impl BatchProcessingActor {
    /// Create a new BatchProcessingActor
    ///
    /// # Arguments
    ///
    /// * `context` - System context for framework operations
    /// * `service` - BatchProcessingService to delegate work to
    pub fn new(context: Arc<SystemContext>, service: Arc<BatchProcessingService>) -> Self {
        Self { context, service }
    }
}

impl OrchestrationActor for BatchProcessingActor {
    fn name(&self) -> &'static str {
        "BatchProcessingActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "BatchProcessingActor started - ready to process batchable steps"
        );
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "BatchProcessingActor stopped"
        );
        Ok(())
    }
}

#[async_trait]
impl Handler<ProcessBatchableStepMessage> for BatchProcessingActor {
    type Response = BatchProcessingOutcome;

    async fn handle(&self, msg: ProcessBatchableStepMessage) -> TaskerResult<Self::Response> {
        debug!(
            actor = %self.name(),
            task_uuid = %msg.task_uuid,
            step_uuid = %msg.batchable_step.workflow_step_uuid,
            "Processing batchable step message"
        );

        // Delegate to underlying service (service manages its own transaction)
        let outcome = self
            .service
            .process_batchable_step(msg.task_uuid, &msg.batchable_step, &msg.step_result)
            .await
            .map_err(|e| tasker_shared::TaskerError::OrchestrationError(e.to_string()))?;

        debug!(
            actor = %self.name(),
            task_uuid = %msg.task_uuid,
            step_uuid = %msg.batchable_step.workflow_step_uuid,
            requires_workers = outcome.requires_worker_creation(),
            "Batchable step processed successfully"
        );

        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_processing_actor_implements_traits() {
        // Verify trait implementations compile
        fn assert_orchestration_actor<T: OrchestrationActor>() {}
        fn assert_handler<T: Handler<ProcessBatchableStepMessage>>() {}

        assert_orchestration_actor::<BatchProcessingActor>();
        assert_handler::<BatchProcessingActor>();
    }

    #[test]
    fn test_process_batchable_step_message_implements_message() {
        fn assert_message<T: Message>() {}
        assert_message::<ProcessBatchableStepMessage>();
    }
}
