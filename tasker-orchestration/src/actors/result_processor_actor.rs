//! # Result Processor Actor
//!
//! Actor implementation wrapping OrchestrationResultProcessor for message-based
//! step result processing and task finalization coordination.

use crate::actors::{Handler, Message, OrchestrationActor};
use crate::orchestration::lifecycle::result_processor::OrchestrationResultProcessor;
use async_trait::async_trait;
use std::sync::Arc;
use tasker_shared::StepExecutionResult;
use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;
use tracing::{debug, info};

/// Message for processing a step execution result
///
/// Wraps StepExecutionResult for actor-based processing.
/// The actor will delegate to OrchestrationResultProcessor for:
/// - Result validation and orchestration metadata processing
/// - Task finalization coordination with atomic claiming
/// - Error handling, retry logic, and failure state management
#[derive(Debug, Clone)]
pub struct ProcessStepResultMessage {
    /// The step execution result to process
    pub result: StepExecutionResult,
}

impl Message for ProcessStepResultMessage {
    type Response = ();
}

/// Actor for processing step execution results
///
/// This actor wraps OrchestrationResultProcessor and provides message-based
/// access to step result processing functionality. It handles:
///
/// 1. Result validation and orchestration metadata processing
/// 2. Task finalization coordination with atomic claiming (TAS-37)
/// 3. Error handling, retry logic, and failure state management
/// 4. Backoff calculations for intelligent retry coordination
///
/// # Example
///
/// ```rust,no_run
/// use tasker_orchestration::actors::{Handler, result_processor_actor::*};
///
/// # async fn example(actor: ResultProcessorActor, result: StepExecutionResult) -> Result<(), Box<dyn std::error::Error>> {
/// let msg = ProcessStepResultMessage { result };
/// actor.handle(msg).await?;
/// # Ok(())
/// # }
/// ```
pub struct ResultProcessorActor {
    /// System context for framework operations
    context: Arc<SystemContext>,

    /// Underlying service that performs the actual work
    service: Arc<OrchestrationResultProcessor>,
}

impl ResultProcessorActor {
    /// Create a new ResultProcessorActor
    ///
    /// # Arguments
    ///
    /// * `context` - System context for framework operations
    /// * `service` - OrchestrationResultProcessor to delegate work to
    pub fn new(context: Arc<SystemContext>, service: Arc<OrchestrationResultProcessor>) -> Self {
        Self { context, service }
    }
}

impl OrchestrationActor for ResultProcessorActor {
    fn name(&self) -> &'static str {
        "ResultProcessorActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "ResultProcessorActor started - ready to process step results"
        );
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "ResultProcessorActor stopped"
        );
        Ok(())
    }
}

#[async_trait]
impl Handler<ProcessStepResultMessage> for ResultProcessorActor {
    type Response = ();

    async fn handle(&self, msg: ProcessStepResultMessage) -> TaskerResult<Self::Response> {
        debug!(
            actor = %self.name(),
            step_uuid = %msg.result.step_uuid,
            status = %msg.result.status,
            execution_time_ms = msg.result.metadata.execution_time_ms,
            has_orchestration_metadata = msg.result.orchestration_metadata.is_some(),
            "Processing step result message"
        );

        // Delegate to underlying service
        self.service
            .handle_step_execution_result(&msg.result)
            .await
            .map_err(|e| tasker_shared::TaskerError::OrchestrationError(e.to_string()))?;

        debug!(
            actor = %self.name(),
            step_uuid = %msg.result.step_uuid,
            status = %msg.result.status,
            "Step result processed successfully"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_processor_actor_implements_traits() {
        // Verify trait implementations compile
        fn assert_orchestration_actor<T: OrchestrationActor>() {}
        fn assert_handler<T: Handler<ProcessStepResultMessage>>() {}

        assert_orchestration_actor::<ResultProcessorActor>();
        assert_handler::<ResultProcessorActor>();
    }

    #[test]
    fn test_process_step_result_message_implements_message() {
        fn assert_message<T: Message>() {}
        assert_message::<ProcessStepResultMessage>();
    }
}
