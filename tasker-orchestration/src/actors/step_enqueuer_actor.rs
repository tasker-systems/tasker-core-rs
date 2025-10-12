//! # Step Enqueuer Actor
//!
//! Actor implementation wrapping StepEnqueuerService for message-based
//! batch processing of ready tasks and step enqueueing.

use crate::actors::{Handler, Message, OrchestrationActor};
use crate::orchestration::lifecycle::step_enqueuer_services::{
    StepEnqueuerService, StepEnqueuerServiceResult,
};
use async_trait::async_trait;
use std::sync::Arc;
use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;
use tracing::{debug, info};

/// Message for processing a batch of ready tasks
///
/// Triggers batch processing of tasks that are ready for step enqueueing.
/// The actor will delegate to StepEnqueuerService for:
/// - Claiming ready tasks atomically
/// - Discovering ready steps for each task
/// - Enqueueing steps to namespace queues
/// - Collecting metrics and performance data
#[derive(Debug, Clone)]
pub struct ProcessBatchMessage;

impl Message for ProcessBatchMessage {
    type Response = StepEnqueuerServiceResult;
}

/// Actor for processing ready tasks and enqueueing steps
///
/// This actor wraps StepEnqueuerService and provides message-based
/// access to batch processing functionality. It handles:
///
/// 1. Batch claiming of ready tasks (TAS-43 atomic claiming)
/// 2. Step discovery for each claimed task
/// 3. Enqueueing steps to namespace-specific queues
/// 4. Performance metrics and namespace statistics
///
/// # Example
///
/// ```rust,no_run
/// use tasker_orchestration::actors::{Handler, step_enqueuer_actor::*};
///
/// # async fn example(actor: StepEnqueuerActor) -> Result<(), Box<dyn std::error::Error>> {
/// let msg = ProcessBatchMessage;
/// let result = actor.handle(msg).await?;
/// println!("Processed {} tasks", result.tasks_processed);
/// # Ok(())
/// # }
/// ```
pub struct StepEnqueuerActor {
    /// System context for framework operations
    context: Arc<SystemContext>,

    /// Underlying service that performs the actual work
    service: Arc<StepEnqueuerService>,
}

impl StepEnqueuerActor {
    /// Create a new StepEnqueuerActor
    ///
    /// # Arguments
    ///
    /// * `context` - System context for framework operations
    /// * `service` - StepEnqueuerService to delegate work to
    pub fn new(context: Arc<SystemContext>, service: Arc<StepEnqueuerService>) -> Self {
        Self { context, service }
    }
}

impl OrchestrationActor for StepEnqueuerActor {
    fn name(&self) -> &'static str {
        "StepEnqueuerActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "StepEnqueuerActor started - ready to process ready tasks and enqueue steps"
        );
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "StepEnqueuerActor stopped"
        );
        Ok(())
    }
}

#[async_trait]
impl Handler<ProcessBatchMessage> for StepEnqueuerActor {
    type Response = StepEnqueuerServiceResult;

    async fn handle(&self, _msg: ProcessBatchMessage) -> TaskerResult<Self::Response> {
        debug!(
            actor = %self.name(),
            "Processing batch of ready tasks for step enqueueing"
        );

        // Delegate to underlying service
        let result = self.service.process_batch().await?;

        debug!(
            actor = %self.name(),
            tasks_processed = result.tasks_processed,
            tasks_failed = result.tasks_failed,
            cycle_duration_ms = result.cycle_duration_ms,
            "Batch processing completed successfully"
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_enqueuer_actor_implements_traits() {
        // Verify trait implementations compile
        fn assert_orchestration_actor<T: OrchestrationActor>() {}
        fn assert_handler<T: Handler<ProcessBatchMessage>>() {}

        assert_orchestration_actor::<StepEnqueuerActor>();
        assert_handler::<StepEnqueuerActor>();
    }

    #[test]
    fn test_process_batch_message_implements_message() {
        fn assert_message<T: Message>() {}
        assert_message::<ProcessBatchMessage>();
    }
}
