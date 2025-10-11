//! # Task Finalizer Actor
//!
//! Actor implementation wrapping TaskFinalizer for message-based
//! task finalization with atomic claiming (TAS-37).

use crate::actors::{Handler, Message, OrchestrationActor};
use crate::orchestration::lifecycle::task_finalization::{FinalizationResult, TaskFinalizer};
use async_trait::async_trait;
use std::sync::Arc;
use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;
use tracing::{debug, info};
use uuid::Uuid;

/// Message for finalizing a task
///
/// Wraps a task UUID for actor-based task finalization.
/// The actor will delegate to TaskFinalizer for:
/// - Atomic task claiming for finalization (TAS-37)
/// - State validation and transition
/// - Step completion verification
/// - Final state determination (complete/error)
#[derive(Debug, Clone)]
pub struct FinalizeTaskMessage {
    /// The task UUID to finalize
    pub task_uuid: Uuid,
}

impl Message for FinalizeTaskMessage {
    type Response = FinalizationResult;
}

/// Actor for task finalization with atomic claiming
///
/// This actor wraps TaskFinalizer and provides message-based
/// access to task finalization functionality. It handles:
///
/// 1. Atomic task claiming for finalization (TAS-37)
/// 2. Step completion verification
/// 3. State machine transitions to terminal states
/// 4. Blocked-by-failures detection
/// 5. Retry exhaustion handling
///
/// # Example
///
/// ```rust,no_run
/// use tasker_orchestration::actors::{Handler, task_finalizer_actor::*};
/// use uuid::Uuid;
///
/// # async fn example(actor: TaskFinalizerActor, task_uuid: Uuid) -> Result<(), Box<dyn std::error::Error>> {
/// let msg = FinalizeTaskMessage { task_uuid };
/// let result = actor.handle(msg).await?;
/// println!("Task finalized with action: {:?}", result.action);
/// # Ok(())
/// # }
/// ```
pub struct TaskFinalizerActor {
    /// System context for framework operations
    context: Arc<SystemContext>,

    /// Underlying service that performs the actual work
    service: TaskFinalizer,
}

impl TaskFinalizerActor {
    /// Create a new TaskFinalizerActor
    ///
    /// # Arguments
    ///
    /// * `context` - System context for framework operations
    /// * `service` - TaskFinalizer to delegate work to
    pub fn new(context: Arc<SystemContext>, service: TaskFinalizer) -> Self {
        Self { context, service }
    }
}

impl OrchestrationActor for TaskFinalizerActor {
    fn name(&self) -> &'static str {
        "TaskFinalizerActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "TaskFinalizerActor started - ready to finalize tasks with atomic claiming"
        );
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "TaskFinalizerActor stopped"
        );
        Ok(())
    }
}

#[async_trait]
impl Handler<FinalizeTaskMessage> for TaskFinalizerActor {
    type Response = FinalizationResult;

    async fn handle(&self, msg: FinalizeTaskMessage) -> TaskerResult<Self::Response> {
        debug!(
            actor = %self.name(),
            task_uuid = %msg.task_uuid,
            "Finalizing task with atomic claiming"
        );

        // Delegate to underlying service
        // Convert FinalizationError to TaskerError
        let result = self
            .service
            .finalize_task(msg.task_uuid)
            .await
            .map_err(|e| tasker_shared::TaskerError::OrchestrationError(e.to_string()))?;

        debug!(
            actor = %self.name(),
            task_uuid = %msg.task_uuid,
            action = ?result.action,
            "Task finalization completed successfully"
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_finalizer_actor_implements_traits() {
        // Verify trait implementations compile
        fn assert_orchestration_actor<T: OrchestrationActor>() {}
        fn assert_handler<T: Handler<FinalizeTaskMessage>>() {}

        assert_orchestration_actor::<TaskFinalizerActor>();
        assert_handler::<TaskFinalizerActor>();
    }

    #[test]
    fn test_finalize_task_message_implements_message() {
        fn assert_message<T: Message>() {}
        assert_message::<FinalizeTaskMessage>();
    }
}
