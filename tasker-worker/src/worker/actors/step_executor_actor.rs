//! # Step Executor Actor
//!
//! TAS-69: Actor for step execution operations.
//!
//! Handles step claiming, state verification, and handler dispatch
//! by delegating to StepExecutorService.
//!
//! ## Stateless Design
//!
//! The underlying `StepExecutorService` is stateless during execution.
//! All dependencies are provided at construction time, eliminating
//! two-phase initialization. This allows the actor to hold
//! `Arc<StepExecutorService>` without an `RwLock`, eliminating lock contention.
//!
//! ## TAS-67: Non-Blocking Dispatch (Canonical Path)
//!
//! All step execution uses non-blocking dispatch:
//! 1. Claim the step
//! 2. Send DispatchHandlerMessage to dispatch channel (fire-and-forget)
//! 3. Return immediately without waiting for handler completion
//!
//! Handler invocation and completion processing are handled by separate services.
//! There is no fallback path - dispatch is the only execution mechanism.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use tasker_shared::messaging::PgmqClientTrait;
use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;

use super::messages::{
    DispatchHandlerMessage, ExecuteStepFromEventMessage, ExecuteStepFromPgmqMessage,
    ExecuteStepMessage, ExecuteStepWithCorrelationMessage, TraceContext,
};
use super::traits::{Handler, Message, WorkerActor};
use crate::worker::event_publisher::WorkerEventPublisher;
use crate::worker::event_systems::domain_event_system::DomainEventSystemHandle;
use crate::worker::services::StepExecutorService;
use crate::worker::step_claim::StepClaim;
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
///
/// ## TAS-67: Non-Blocking Dispatch (Canonical Path)
///
/// All step execution flows through the dispatch channel:
/// - Steps are claimed and dispatched to a channel (fire-and-forget)
/// - Handler invocation happens in HandlerDispatchService or via FFI polling
/// - Completion flows back through a separate channel
///
/// The dispatch channel is REQUIRED - there is no fallback path.
pub struct StepExecutorActor {
    context: Arc<SystemContext>,
    service: Arc<StepExecutorService>,
    /// TAS-67: Dispatch channel for non-blocking handler invocation (required)
    dispatch_sender: mpsc::Sender<DispatchHandlerMessage>,
    /// TAS-67: Task template manager for hydrating step context
    task_template_manager: Arc<TaskTemplateManager>,
}

impl std::fmt::Debug for StepExecutorActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepExecutorActor")
            .field("context", &"Arc<SystemContext>")
            .field("service", &"Arc<StepExecutorService>")
            .field("dispatch_sender", &"mpsc::Sender")
            .finish()
    }
}

impl StepExecutorActor {
    /// Create a new StepExecutorActor with all dependencies
    ///
    /// TAS-67: All dependencies are required at construction time, including
    /// the dispatch_sender. All step execution flows through the dispatch
    /// channel - there is no fallback path.
    ///
    /// # Arguments
    ///
    /// * `context` - Shared system context
    /// * `worker_id` - Unique identifier for this worker
    /// * `task_template_manager` - Template manager for step hydration
    /// * `event_publisher` - Event publisher for worker events
    /// * `domain_event_handle` - Handle for domain event dispatch
    /// * `dispatch_sender` - Channel for non-blocking handler dispatch (required)
    pub fn new(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
        event_publisher: WorkerEventPublisher,
        domain_event_handle: DomainEventSystemHandle,
        dispatch_sender: mpsc::Sender<DispatchHandlerMessage>,
    ) -> Self {
        let service = StepExecutorService::new(
            worker_id,
            context.clone(),
            task_template_manager.clone(),
            event_publisher,
            domain_event_handle,
        );

        Self {
            context,
            service: Arc::new(service),
            dispatch_sender,
            task_template_manager,
        }
    }

    /// Dispatch domain events after step completion
    ///
    /// Queries the database for step details and publishes events.
    pub async fn dispatch_domain_events(
        &self,
        step_uuid: Uuid,
        step_result: &tasker_shared::messaging::StepExecutionResult,
        correlation_id: Option<Uuid>,
    ) {
        self.service
            .dispatch_domain_events(step_uuid, step_result, correlation_id)
            .await;
    }

    /// TAS-67: Claim step and dispatch to handler channel (non-blocking)
    ///
    /// This method:
    /// 1. Hydrates step context from database
    /// 2. Claims the step via state machine
    /// 3. Sends DispatchHandlerMessage to dispatch channel (fire-and-forget)
    /// 4. Returns immediately
    ///
    /// Returns true if step was claimed and dispatched, false if not claimed.
    async fn claim_and_dispatch(
        &self,
        step_uuid: Uuid,
        task_uuid: Uuid,
        correlation_id: Uuid,
        trace_context: Option<TraceContext>,
    ) -> TaskerResult<bool> {
        // Create step claim helper
        let step_claimer = StepClaim::new(
            task_uuid,
            step_uuid,
            self.context.clone(),
            self.task_template_manager.clone(),
        );

        // Get TaskSequenceStep (hydrate step context)
        let task_sequence_step = match step_claimer.get_task_sequence_step_by_uuid(step_uuid).await
        {
            Ok(Some(step)) => step,
            Ok(None) => {
                warn!(
                    actor = "StepExecutorActor",
                    step_uuid = %step_uuid,
                    "Step not found for dispatch"
                );
                return Ok(false);
            }
            Err(e) => return Err(e),
        };

        // Claim the step
        let claimed = step_claimer
            .try_claim_step(&task_sequence_step, correlation_id)
            .await?;

        if !claimed {
            debug!(
                actor = "StepExecutorActor",
                step_uuid = %step_uuid,
                "Step not claimed - skipping dispatch"
            );
            return Ok(false);
        }

        // Create dispatch message
        let event_id = Uuid::new_v4();
        let msg = DispatchHandlerMessage {
            event_id,
            step_uuid,
            task_uuid,
            task_sequence_step,
            correlation_id,
            trace_context,
        };

        // Send to dispatch channel (fire-and-forget)
        if let Err(e) = self.dispatch_sender.try_send(msg) {
            warn!(
                actor = "StepExecutorActor",
                step_uuid = %step_uuid,
                error = %e,
                "Failed to dispatch step - channel full or closed"
            );
            // Note: Step is already claimed, but dispatch failed
            // The step will eventually timeout and be retried
            return Err(tasker_shared::TaskerError::WorkerError(format!(
                "Dispatch channel error: {e}"
            )));
        }

        debug!(
            actor = "StepExecutorActor",
            step_uuid = %step_uuid,
            event_id = %event_id,
            "Step claimed and dispatched"
        );

        Ok(true)
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
        let step_uuid = msg.message.message.step_uuid;
        let task_uuid = msg.message.message.task_uuid;

        debug!(
            actor = self.name(),
            step_uuid = %step_uuid,
            queue = %msg.queue_name,
            "Handling ExecuteStepMessage via dispatch"
        );

        // TAS-67: All execution flows through dispatch (no fallback)
        let correlation_id = Uuid::new_v4(); // Generate correlation for tracing
        let trace_context = Some(TraceContext {
            trace_id: correlation_id.to_string(),
            span_id: format!("span-{}", step_uuid),
        });

        // Claim and dispatch (fire-and-forget)
        let claimed = self
            .claim_and_dispatch(step_uuid, task_uuid, correlation_id, trace_context)
            .await?;

        if claimed {
            // Delete the PGMQ message after successful dispatch
            if let Err(e) = self
                .context
                .message_client
                .delete_message(&msg.queue_name, msg.message.msg_id)
                .await
            {
                warn!(
                    actor = self.name(),
                    step_uuid = %step_uuid,
                    error = %e,
                    "Failed to delete PGMQ message after dispatch"
                );
            }
        }

        Ok(claimed)
    }
}

#[async_trait]
impl Handler<ExecuteStepWithCorrelationMessage> for StepExecutorActor {
    async fn handle(
        &self,
        msg: ExecuteStepWithCorrelationMessage,
    ) -> TaskerResult<<ExecuteStepWithCorrelationMessage as Message>::Response> {
        let step_uuid = msg.message.message.step_uuid;
        let task_uuid = msg.message.message.task_uuid;
        let correlation_id = msg.correlation_id;

        debug!(
            actor = self.name(),
            step_uuid = %step_uuid,
            correlation_id = %correlation_id,
            "Handling ExecuteStepWithCorrelationMessage via dispatch"
        );

        // TAS-67: All execution flows through dispatch (no fallback)
        // Create trace context from correlation
        let trace_context = Some(TraceContext {
            trace_id: correlation_id.to_string(),
            span_id: format!("span-{}", step_uuid),
        });

        // Claim and dispatch (fire-and-forget)
        let claimed = self
            .claim_and_dispatch(step_uuid, task_uuid, correlation_id, trace_context)
            .await?;

        if claimed {
            // Delete the PGMQ message after successful dispatch
            if let Err(e) = self
                .context
                .message_client
                .delete_message(&msg.queue_name, msg.message.msg_id)
                .await
            {
                warn!(
                    actor = self.name(),
                    step_uuid = %step_uuid,
                    error = %e,
                    "Failed to delete PGMQ message after dispatch"
                );
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Handler<ExecuteStepFromPgmqMessage> for StepExecutorActor {
    async fn handle(
        &self,
        msg: ExecuteStepFromPgmqMessage,
    ) -> TaskerResult<<ExecuteStepFromPgmqMessage as Message>::Response> {
        // Deserialize the message payload to get step/task UUIDs
        let step_message: tasker_shared::messaging::message::SimpleStepMessage =
            serde_json::from_value(msg.message.message.clone()).map_err(|e| {
                tasker_shared::TaskerError::MessagingError(format!(
                    "Failed to deserialize step message: {}",
                    e
                ))
            })?;

        let step_uuid = step_message.step_uuid;
        let task_uuid = step_message.task_uuid;

        debug!(
            actor = self.name(),
            msg_id = msg.message.msg_id,
            step_uuid = %step_uuid,
            queue = %msg.queue_name,
            "Handling ExecuteStepFromPgmqMessage via dispatch"
        );

        // TAS-67: All execution flows through dispatch (no fallback)
        let correlation_id = Uuid::new_v4();
        let trace_context = Some(TraceContext {
            trace_id: correlation_id.to_string(),
            span_id: format!("span-{}", step_uuid),
        });

        // Claim and dispatch (fire-and-forget)
        let claimed = self
            .claim_and_dispatch(step_uuid, task_uuid, correlation_id, trace_context)
            .await?;

        if claimed {
            // Delete the PGMQ message after successful dispatch
            if let Err(e) = self
                .context
                .message_client
                .delete_message(&msg.queue_name, msg.message.msg_id)
                .await
            {
                warn!(
                    actor = self.name(),
                    step_uuid = %step_uuid,
                    error = %e,
                    "Failed to delete PGMQ message after dispatch"
                );
            }
        }

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

        // Read the specific message from the queue to get step/task UUIDs
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
                let step_uuid = m.message.step_uuid;
                let task_uuid = m.message.task_uuid;

                debug!(
                    actor = self.name(),
                    step_uuid = %step_uuid,
                    "Processing event message via dispatch"
                );

                // TAS-67: All execution flows through dispatch (no fallback)
                let correlation_id = Uuid::new_v4();
                let trace_context = Some(TraceContext {
                    trace_id: correlation_id.to_string(),
                    span_id: format!("span-{}", step_uuid),
                });

                // Claim and dispatch (fire-and-forget)
                let claimed = self
                    .claim_and_dispatch(step_uuid, task_uuid, correlation_id, trace_context)
                    .await?;

                if claimed {
                    // Delete the PGMQ message after successful dispatch
                    if let Err(e) = self
                        .context
                        .message_client
                        .delete_message(&msg.message_event.queue_name, m.msg_id)
                        .await
                    {
                        warn!(
                            actor = self.name(),
                            step_uuid = %step_uuid,
                            error = %e,
                            "Failed to delete PGMQ message after dispatch"
                        );
                    }
                }

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

        // TAS-67: dispatch_sender is required
        let (dispatch_sender, _dispatch_receiver) = mpsc::channel(100);

        let actor = StepExecutorActor::new(
            context.clone(),
            "test_worker".to_string(),
            task_template_manager,
            event_publisher,
            domain_event_handle,
            dispatch_sender,
        );

        assert_eq!(actor.name(), "StepExecutorActor");
    }

    #[test]
    fn test_step_executor_actor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StepExecutorActor>();
    }
}
