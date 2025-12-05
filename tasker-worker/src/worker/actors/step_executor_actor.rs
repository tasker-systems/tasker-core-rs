//! # Step Executor Actor
//!
//! TAS-69: Actor for step execution operations.
//!
//! Handles step claiming, state verification, and FFI handler invocation
//! by delegating to StepExecutorService.
//!
//! ## Stateless Design
//!
//! The underlying `StepExecutorService` is stateless during execution.
//! All dependencies are provided at construction time, eliminating
//! two-phase initialization. This allows the actor to hold
//! `Arc<StepExecutorService>` without an `RwLock`, eliminating lock contention.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info};

use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;

use super::messages::{
    ExecuteStepFromEventMessage, ExecuteStepFromPgmqMessage, ExecuteStepMessage,
    ExecuteStepWithCorrelationMessage,
};
use super::traits::{Handler, Message, WorkerActor};
use crate::worker::event_publisher::WorkerEventPublisher;
use crate::worker::event_systems::domain_event_system::DomainEventSystemHandle;
use crate::worker::services::StepExecutorService;
use crate::worker::task_template_manager::TaskTemplateManager;

/// Step Executor Actor
///
/// TAS-69: Wraps StepExecutorService with actor interface for message-based
/// step execution coordination.
///
/// ## Lock-Free Design
///
/// The service is wrapped in `Arc` without `RwLock` because:
/// - Service methods use `&self` (not `&mut self`)
/// - All dependencies provided at construction time
/// - No mutable state during step execution
pub struct StepExecutorActor {
    context: Arc<SystemContext>,
    service: Arc<StepExecutorService>,
}

impl std::fmt::Debug for StepExecutorActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepExecutorActor")
            .field("context", &"Arc<SystemContext>")
            .field("service", &"Arc<StepExecutorService>")
            .finish()
    }
}

impl StepExecutorActor {
    /// Create a new StepExecutorActor with all dependencies
    ///
    /// All dependencies are required at construction time, eliminating
    /// two-phase initialization complexity.
    pub fn new(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
        event_publisher: WorkerEventPublisher,
        domain_event_handle: DomainEventSystemHandle,
    ) -> Self {
        let service = StepExecutorService::new(
            worker_id,
            context.clone(),
            task_template_manager,
            event_publisher,
            domain_event_handle,
        );

        Self {
            context,
            service: Arc::new(service),
        }
    }

    /// Dispatch domain events after step completion
    ///
    /// Queries the database for step details and publishes events.
    pub async fn dispatch_domain_events(
        &self,
        step_uuid: uuid::Uuid,
        step_result: &tasker_shared::messaging::StepExecutionResult,
        correlation_id: Option<uuid::Uuid>,
    ) {
        self.service
            .dispatch_domain_events(step_uuid, step_result, correlation_id)
            .await;
    }
}

impl WorkerActor for StepExecutorActor {
    fn name(&self) -> &'static str {
        "StepExecutorActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "StepExecutorActor started");
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "StepExecutorActor stopped");
        Ok(())
    }
}

#[async_trait]
impl Handler<ExecuteStepMessage> for StepExecutorActor {
    async fn handle(
        &self,
        msg: ExecuteStepMessage,
    ) -> TaskerResult<<ExecuteStepMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            step_uuid = %msg.message.message.step_uuid,
            queue = %msg.queue_name,
            "Handling ExecuteStepMessage"
        );

        self.service
            .execute_step(msg.message, &msg.queue_name)
            .await
    }
}

#[async_trait]
impl Handler<ExecuteStepWithCorrelationMessage> for StepExecutorActor {
    async fn handle(
        &self,
        msg: ExecuteStepWithCorrelationMessage,
    ) -> TaskerResult<<ExecuteStepWithCorrelationMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            step_uuid = %msg.message.message.step_uuid,
            correlation_id = %msg.correlation_id,
            "Handling ExecuteStepWithCorrelationMessage"
        );

        self.service
            .execute_step(msg.message, &msg.queue_name)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl Handler<ExecuteStepFromPgmqMessage> for StepExecutorActor {
    async fn handle(
        &self,
        msg: ExecuteStepFromPgmqMessage,
    ) -> TaskerResult<<ExecuteStepFromPgmqMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            msg_id = msg.message.msg_id,
            queue = %msg.queue_name,
            "StepExecutorActor handling ExecuteStepFromPgmqMessage"
        );

        // Deserialize the message payload
        let step_message: tasker_shared::messaging::message::SimpleStepMessage =
            serde_json::from_value(msg.message.message.clone()).map_err(|e| {
                tasker_shared::TaskerError::MessagingError(format!(
                    "Failed to deserialize step message: {}",
                    e
                ))
            })?;

        let typed_message = pgmq::Message {
            msg_id: msg.message.msg_id,
            message: step_message,
            vt: msg.message.vt,
            read_ct: msg.message.read_ct,
            enqueued_at: msg.message.enqueued_at,
        };

        self.service
            .execute_step(typed_message, &msg.queue_name)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl Handler<ExecuteStepFromEventMessage> for StepExecutorActor {
    async fn handle(
        &self,
        msg: ExecuteStepFromEventMessage,
    ) -> TaskerResult<<ExecuteStepFromEventMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            msg_id = msg.message_event.msg_id,
            queue = %msg.message_event.queue_name,
            "Handling ExecuteStepFromEventMessage"
        );

        // Read the specific message from the queue
        let message = self
            .context
            .message_client
            .read_specific_message::<tasker_shared::messaging::message::SimpleStepMessage>(
                &msg.message_event.queue_name,
                msg.message_event.msg_id,
                msg.message_event.visibility_timeout_seconds.unwrap_or(30),
            )
            .await
            .map_err(|e| {
                tasker_shared::TaskerError::MessagingError(format!(
                    "Failed to read specific message: {}",
                    e
                ))
            })?;

        match message {
            Some(m) => {
                self.service
                    .execute_step(m, &msg.message_event.queue_name)
                    .await?;
                Ok(())
            }
            None => {
                tracing::warn!(
                    actor = self.name(),
                    msg_id = msg.message_event.msg_id,
                    "Message not found when processing event"
                );
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::event_router::EventRouter;
    use crate::worker::event_systems::domain_event_system::{
        DomainEventSystem, DomainEventSystemConfig,
    };
    use crate::worker::in_process_event_bus::{InProcessEventBus, InProcessEventBusConfig};
    use tasker_shared::events::domain_events::DomainEventPublisher;
    use tokio::sync::RwLock;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_step_executor_actor_creation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        // Create required dependencies
        let event_publisher = WorkerEventPublisher::new("test_worker".to_string());
        let domain_publisher = Arc::new(DomainEventPublisher::new(context.message_client.clone()));
        let in_process_bus = Arc::new(RwLock::new(InProcessEventBus::new(
            InProcessEventBusConfig::default(),
        )));
        let event_router = Arc::new(EventRouter::new(domain_publisher, in_process_bus));
        let (_domain_event_system, domain_event_handle) =
            DomainEventSystem::new(event_router, DomainEventSystemConfig::default());

        let actor = StepExecutorActor::new(
            context.clone(),
            "test_worker".to_string(),
            task_template_manager,
            event_publisher,
            domain_event_handle,
        );

        assert_eq!(actor.name(), "StepExecutorActor");
    }

    #[test]
    fn test_step_executor_actor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StepExecutorActor>();
    }
}
