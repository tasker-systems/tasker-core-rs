//! # TAS-69 Actor-Based Worker Command Processor
//!
//! Pure routing command processor that delegates all business logic to actors.
//! This replaces the 1575 LOC command_processor.rs with ~200 LOC of pure routing.
//!
//! ## Architecture
//!
//! ```text
//! WorkerCommand ──→ ActorCommandProcessor ──→ Actor
//!                          │                    │
//!                          │                    └─→ Handler<M>
//!                          │                            │
//!                          └────────────────────────────┴─→ Service
//! ```
//!
//! ## Key Features
//!
//! - **Pure Routing**: No business logic, just command→actor delegation
//! - **Type-Safe Messages**: Typed actor messages via Handler<M> trait
//! - **Backward Compatible**: Same WorkerCommand enum and response types

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use super::actors::{
    ExecuteStepFromEventMessage, ExecuteStepFromPgmqMessage, ExecuteStepMessage,
    ExecuteStepWithCorrelationMessage, GetEventStatusMessage, GetWorkerStatusMessage, Handler,
    HealthCheckMessage, ProcessStepCompletionMessage, RefreshTemplateCacheMessage,
    SendStepResultMessage, SetEventIntegrationMessage, WorkerActorRegistry,
};
use super::command_processor::WorkerCommand;
use super::event_publisher::WorkerEventPublisher;
use super::event_subscriber::WorkerEventSubscriber;
use super::event_systems::domain_event_system::DomainEventSystemHandle;
use super::task_template_manager::TaskTemplateManager;

/// TAS-69: Actor-based Worker Command Processor
///
/// Pure routing implementation that delegates all business logic to actors.
/// This is the replacement for the legacy WorkerProcessor.
pub struct ActorCommandProcessor {
    /// Worker identification
    worker_id: String,

    /// System context for dependencies
    context: Arc<SystemContext>,

    /// Actor registry with all worker actors
    actors: WorkerActorRegistry,

    /// Command receiver
    command_receiver: Option<mpsc::Receiver<WorkerCommand>>,

    /// Channel monitor for command channel observability (TAS-51)
    command_channel_monitor: ChannelMonitor,

    /// Event publisher for FFI handlers (shared with actors)
    event_publisher: Option<WorkerEventPublisher>,

    /// Event subscriber for completion events
    event_subscriber: Option<WorkerEventSubscriber>,
}

impl std::fmt::Debug for ActorCommandProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorCommandProcessor")
            .field("worker_id", &self.worker_id)
            .field("actors", &self.actors)
            .finish()
    }
}

impl ActorCommandProcessor {
    /// Create new ActorCommandProcessor with all dependencies
    ///
    /// All dependencies required for step execution are provided upfront,
    /// eliminating two-phase initialization complexity.
    ///
    /// # Arguments
    ///
    /// * `context` - Shared system context
    /// * `worker_id` - Unique identifier for this worker
    /// * `task_template_manager` - Pre-initialized template manager
    /// * `event_publisher` - Event publisher for FFI handler invocation
    /// * `domain_event_handle` - Handle for domain event dispatch
    /// * `command_buffer_size` - Size of the command channel buffer
    /// * `command_channel_monitor` - Monitor for channel observability
    pub async fn new(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
        event_publisher: WorkerEventPublisher,
        domain_event_handle: DomainEventSystemHandle,
        command_buffer_size: usize,
        command_channel_monitor: ChannelMonitor,
    ) -> TaskerResult<(Self, mpsc::Sender<WorkerCommand>)> {
        let (command_sender, command_receiver) = mpsc::channel(command_buffer_size);

        // Build actor registry with all dependencies
        let actors = WorkerActorRegistry::build(
            context.clone(),
            worker_id.clone(),
            task_template_manager,
            event_publisher.clone(),
            domain_event_handle,
        )
        .await?;

        info!(
            worker_id = %worker_id,
            channel = %command_channel_monitor.channel_name(),
            "Creating ActorCommandProcessor with all dependencies"
        );

        let processor = Self {
            worker_id,
            context,
            actors,
            command_receiver: Some(command_receiver),
            command_channel_monitor,
            event_publisher: Some(event_publisher),
            event_subscriber: None,
        };

        Ok((processor, command_sender))
    }

    /// Enable event subscriber for completion events
    ///
    /// This sets up the WorkerStatusActor with event tracking capabilities.
    /// Note: StepExecutorActor already has its event_publisher from construction.
    pub async fn enable_event_subscriber(
        &mut self,
        event_system: Option<Arc<tasker_shared::events::WorkerEventSystem>>,
    ) {
        info!(
            worker_id = %self.worker_id,
            "Enabling event subscriber for completion events"
        );

        let shared_event_system = event_system
            .unwrap_or_else(|| Arc::new(tasker_shared::events::WorkerEventSystem::new()));

        let event_subscriber =
            WorkerEventSubscriber::with_event_system(self.worker_id.clone(), shared_event_system);

        // Update status actor with event components
        if let Some(ref event_publisher) = self.event_publisher {
            self.actors
                .worker_status_actor
                .set_event_publisher(event_publisher.clone())
                .await;
        }
        self.actors
            .worker_status_actor
            .set_event_subscriber(event_subscriber.clone())
            .await;

        self.event_subscriber = Some(event_subscriber);
    }

    /// Get the actor registry
    pub fn actors(&self) -> &WorkerActorRegistry {
        &self.actors
    }

    /// Get the worker ID
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Start processing worker commands with event integration
    pub async fn start_with_events(&mut self) -> TaskerResult<()> {
        // Start completion listener if event subscriber is enabled
        let completion_buffer_size = self
            .context
            .tasker_config
            .worker
            .as_ref()
            .map(|w| w.mpsc_channels.event_subscribers.completion_buffer_size)
            .unwrap_or(1000) as usize;

        let completion_receiver = self
            .event_subscriber
            .as_ref()
            .map(|subscriber| subscriber.start_completion_listener(completion_buffer_size));

        self.start_with_completion_receiver(completion_receiver)
            .await
    }

    /// Start processing with custom completion receiver
    pub async fn start_with_completion_receiver(
        &mut self,
        mut completion_receiver: Option<mpsc::Receiver<StepExecutionResult>>,
    ) -> TaskerResult<()> {
        info!(
            worker_id = %self.worker_id,
            event_integration = self.event_publisher.is_some(),
            "Starting ActorCommandProcessor"
        );

        let mut command_receiver = self.command_receiver.take().ok_or_else(|| {
            TaskerError::WorkerError("Command receiver already taken".to_string())
        })?;

        loop {
            tokio::select! {
                command = command_receiver.recv() => {
                    match command {
                        Some(cmd) => {
                            self.command_channel_monitor.record_receive();
                            if !self.handle_command(cmd).await {
                                break;
                            }
                        }
                        None => {
                            info!(worker_id = %self.worker_id, "Command channel closed");
                            break;
                        }
                    }
                }

                completion = async {
                    match &mut completion_receiver {
                        Some(receiver) => receiver.recv().await,
                        None => {
                            let () = std::future::pending().await;
                            unreachable!()
                        }
                    }
                } => {
                    match completion {
                        Some(step_result) => {
                            self.handle_ffi_completion(step_result).await;
                        }
                        None => {
                            warn!(worker_id = %self.worker_id, "FFI completion channel closed");
                        }
                    }
                }
            }
        }

        info!(worker_id = %self.worker_id, "ActorCommandProcessor shutdown complete");
        Ok(())
    }

    /// Handle FFI completion event from completion channel
    ///
    /// TAS-69: Critical ordering - domain events are dispatched ONLY AFTER successful
    /// orchestration notification. Domain events are declarative of what HAS happened -
    /// the step is only truly complete once orchestration has been notified successfully.
    async fn handle_ffi_completion(&self, step_result: StepExecutionResult) {
        debug!(
            worker_id = %self.worker_id,
            step_uuid = %step_result.step_uuid,
            success = step_result.success,
            "Processing FFI completion"
        );

        // Record stats
        if step_result.success {
            self.actors
                .worker_status_actor
                .record_success(step_result.metadata.execution_time_ms as f64)
                .await;
        } else {
            self.actors.worker_status_actor.record_failure().await;
        }

        // Send result to orchestration via actor FIRST
        let msg = SendStepResultMessage {
            result: step_result.clone(),
        };
        match self.actors.ffi_completion_actor.handle(msg).await {
            Ok(()) => {
                // TAS-69: Dispatch domain events AFTER successful orchestration notification
                // Domain events are declarative of what HAS happened - the step is only
                // truly complete once orchestration has been notified successfully.
                // Fire-and-forget semantics - never blocks the worker.
                self.actors
                    .step_executor_actor
                    .dispatch_domain_events(step_result.step_uuid, &step_result, None)
                    .await;

                info!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_result.step_uuid,
                    "Step completion forwarded to orchestration successfully"
                );
            }
            Err(e) => {
                // Don't dispatch domain events - orchestration wasn't notified,
                // so the step isn't truly complete from the system's perspective
                tracing::error!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_result.step_uuid,
                    error = %e,
                    "Failed to forward step completion to orchestration - domain events NOT dispatched"
                );
            }
        }
    }

    /// Handle individual worker command - pure routing to actors
    async fn handle_command(&self, command: WorkerCommand) -> bool {
        match command {
            // Step Execution Commands → StepExecutorActor
            WorkerCommand::ExecuteStep {
                message,
                queue_name,
                resp,
            } => {
                let msg = ExecuteStepMessage {
                    message,
                    queue_name,
                };
                let result = self
                    .actors
                    .step_executor_actor
                    .handle(msg)
                    .await
                    .map(|_| ());
                let _ = resp.send(result);
                true
            }

            WorkerCommand::ExecuteStepWithCorrelation {
                message,
                queue_name,
                correlation_id,
                resp,
            } => {
                let msg = ExecuteStepWithCorrelationMessage {
                    message,
                    queue_name,
                    correlation_id,
                };
                let result = self.actors.step_executor_actor.handle(msg).await;
                let _ = resp.send(result);
                true
            }

            WorkerCommand::ExecuteStepFromMessage {
                queue_name,
                message,
                resp,
            } => {
                debug!(
                    worker_id = %self.worker_id,
                    msg_id = message.msg_id,
                    queue = %queue_name,
                    "Received ExecuteStepFromMessage command from fallback poller"
                );
                let msg = ExecuteStepFromPgmqMessage {
                    queue_name: queue_name.clone(),
                    message,
                };
                let result = self.actors.step_executor_actor.handle(msg).await;
                if let Err(ref e) = result {
                    warn!(
                        worker_id = %self.worker_id,
                        queue = %queue_name,
                        error = %e,
                        "ExecuteStepFromMessage failed"
                    );
                }
                let _ = resp.send(result);
                true
            }

            WorkerCommand::ExecuteStepFromEvent {
                message_event,
                resp,
            } => {
                let msg = ExecuteStepFromEventMessage { message_event };
                let result = self.actors.step_executor_actor.handle(msg).await;
                let _ = resp.send(result);
                true
            }

            // Completion Commands → FFICompletionActor
            WorkerCommand::SendStepResult { result, resp } => {
                let msg = SendStepResultMessage { result };
                let send_result = self.actors.ffi_completion_actor.handle(msg).await;
                let _ = resp.send(send_result);
                true
            }

            WorkerCommand::ProcessStepCompletion {
                step_result,
                correlation_id,
                resp,
            } => {
                let msg = ProcessStepCompletionMessage {
                    step_result,
                    correlation_id,
                };
                let result = self.actors.ffi_completion_actor.handle(msg).await;
                let _ = resp.send(result);
                true
            }

            // Template Commands → TemplateCacheActor
            WorkerCommand::RefreshTemplateCache { namespace, resp } => {
                let msg = RefreshTemplateCacheMessage { namespace };
                let result = self.actors.template_cache_actor.handle(msg).await;
                let _ = resp.send(result);
                true
            }

            // Status Commands → WorkerStatusActor
            WorkerCommand::GetWorkerStatus { resp } => {
                let result = self
                    .actors
                    .worker_status_actor
                    .handle(GetWorkerStatusMessage)
                    .await;
                let _ = resp.send(result);
                true
            }

            WorkerCommand::GetEventStatus { resp } => {
                let result = self
                    .actors
                    .worker_status_actor
                    .handle(GetEventStatusMessage)
                    .await;
                let _ = resp.send(result);
                true
            }

            WorkerCommand::HealthCheck { resp } => {
                let result = self
                    .actors
                    .worker_status_actor
                    .handle(HealthCheckMessage)
                    .await;
                let _ = resp.send(result);
                true
            }

            WorkerCommand::SetEventIntegration { enabled, resp } => {
                let msg = SetEventIntegrationMessage { enabled };
                let result = self.actors.worker_status_actor.handle(msg).await;
                let _ = resp.send(result);
                true
            }

            // System Commands
            WorkerCommand::Shutdown { resp } => {
                info!(worker_id = %self.worker_id, "Shutting down ActorCommandProcessor");
                let _ = resp.send(Ok(()));
                false
            }
        }
    }

    /// Shutdown the processor
    pub async fn shutdown(&mut self) {
        info!(worker_id = %self.worker_id, "Shutting down ActorCommandProcessor");
        // Actors are managed by the registry
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
    use tasker_shared::messaging::UnifiedPgmqClient;
    use tokio::sync::RwLock;

    /// Helper to create test dependencies
    fn create_test_deps(
        worker_id: &str,
        message_client: Arc<UnifiedPgmqClient>,
    ) -> (WorkerEventPublisher, DomainEventSystemHandle) {
        let event_publisher = WorkerEventPublisher::new(worker_id.to_string());

        // Create dependencies for EventRouter
        let domain_publisher = Arc::new(DomainEventPublisher::new(message_client));
        let in_process_bus = Arc::new(RwLock::new(
            InProcessEventBus::new(InProcessEventBusConfig::default()),
        ));
        let event_router = Arc::new(EventRouter::new(domain_publisher, in_process_bus));

        let (_domain_event_system, domain_event_handle) =
            DomainEventSystem::new(event_router, DomainEventSystemConfig::default());
        (event_publisher, domain_event_handle)
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_actor_command_processor_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let worker_id = format!("worker_{}", uuid::Uuid::new_v4());
        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));
        let (event_publisher, domain_event_handle) =
            create_test_deps(&worker_id, context.message_client.clone());
        let channel_monitor = ChannelMonitor::new("test_command_channel", 100);

        let (processor, _sender) = ActorCommandProcessor::new(
            context,
            worker_id.clone(),
            task_template_manager,
            event_publisher,
            domain_event_handle,
            100,
            channel_monitor,
        )
        .await?;

        assert_eq!(processor.worker_id(), worker_id);

        Ok(())
    }

    #[test]
    fn test_actor_command_processor_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ActorCommandProcessor>();
    }
}
