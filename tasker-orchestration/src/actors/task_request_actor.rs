//! # Task Request Actor
//!
//! Actor implementation wrapping TaskRequestProcessor for message-based
//! task initialization. This actor provides the first concrete implementation
//! of the actor pattern for lifecycle component coordination.

use crate::actors::{Handler, Message, OrchestrationActor};
use crate::orchestration::lifecycle::task_request_processor::TaskRequestProcessor;
use async_trait::async_trait;
use std::sync::Arc;
use tasker_shared::messaging::TaskRequestMessage;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{debug, info};
use uuid::Uuid;

/// Message for processing a task request
///
/// Wraps the TaskRequestMessage for actor-based processing.
/// The actor will delegate to TaskRequestProcessor for actual processing.
#[derive(Debug, Clone)]
pub struct ProcessTaskRequestMessage {
    /// The task request to process
    pub request: TaskRequestMessage,
}

impl Message for ProcessTaskRequestMessage {
    type Response = Uuid;
}

/// Actor for processing task requests
///
/// This actor wraps TaskRequestProcessor and provides message-based
/// access to task initialization functionality. It demonstrates the
/// actor pattern by:
///
/// 1. Encapsulating the service (TaskRequestProcessor)
/// 2. Providing lifecycle hooks (started/stopped)
/// 3. Handling messages via Handler<M> trait
/// 4. Managing its own state and dependencies
///
/// # Example
///
/// ```rust,no_run
/// use tasker_orchestration::actors::{Handler, task_request_actor::*};
///
/// # async fn example(actor: TaskRequestActor, request: TaskRequestMessage) -> Result<(), Box<dyn std::error::Error>> {
/// let msg = ProcessTaskRequestMessage { request };
/// let task_uuid = actor.handle(msg).await?;
/// println!("Created task: {}", task_uuid);
/// # Ok(())
/// # }
/// ```
pub struct TaskRequestActor {
    /// System context for framework operations
    context: Arc<SystemContext>,

    /// Underlying service that performs the actual work
    service: Arc<TaskRequestProcessor>,
}

impl TaskRequestActor {
    /// Create a new TaskRequestActor
    ///
    /// # Arguments
    ///
    /// * `context` - System context for framework operations
    /// * `service` - TaskRequestProcessor to delegate work to
    pub fn new(context: Arc<SystemContext>, service: Arc<TaskRequestProcessor>) -> Self {
        Self { context, service }
    }
}

impl OrchestrationActor for TaskRequestActor {
    fn name(&self) -> &'static str {
        "TaskRequestActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "TaskRequestActor started - ready to process task requests"
        );
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "TaskRequestActor stopped"
        );
        Ok(())
    }
}

#[async_trait]
impl Handler<ProcessTaskRequestMessage> for TaskRequestActor {
    type Response = Uuid;

    async fn handle(&self, msg: ProcessTaskRequestMessage) -> TaskerResult<Self::Response> {
        debug!(
            actor = %self.name(),
            namespace = %msg.request.task_request.namespace,
            name = %msg.request.task_request.name,
            "Processing task request message"
        );

        // Convert TaskRequestMessage to JSON payload for existing processor
        let payload = serde_json::to_value(&msg.request).map_err(|e| {
            TaskerError::ValidationError(format!("Failed to serialize task request: {e}"))
        })?;

        // Delegate to underlying service
        let task_uuid = self
            .service
            .process_task_request(&payload)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Task initialization failed: {e}"))
            })?;

        debug!(
            actor = %self.name(),
            task_uuid = %task_uuid,
            "Task request processed successfully"
        );

        Ok(task_uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_request_actor_implements_traits() {
        // Verify trait implementations compile
        fn assert_orchestration_actor<T: OrchestrationActor>() {}
        fn assert_handler<T: Handler<ProcessTaskRequestMessage>>() {}

        assert_orchestration_actor::<TaskRequestActor>();
        assert_handler::<TaskRequestActor>();
    }

    #[test]
    fn test_process_task_request_message_implements_message() {
        fn assert_message<T: Message>() {}
        assert_message::<ProcessTaskRequestMessage>();
    }
}
