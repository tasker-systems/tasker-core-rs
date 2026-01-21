//! # TAS-148: Actor-Based Orchestration Command Processor
//!
//! This module implements the command processor as a unified actor, combining
//! the channel lifecycle management and command routing into a single cohesive
//! component following the TAS-46 actor pattern.
//!
//! ## Architecture
//!
//! The `OrchestrationCommandProcessorActor` receives commands via tokio::sync::mpsc
//! channels and delegates sophisticated orchestration logic to existing components:
//! - TaskRequestActor for task initialization
//! - ResultProcessorActor for step result processing
//! - TaskFinalizerActor for atomic task finalization
//! - StepEnqueuerActor for batch step enqueueing
//!
//! ## TAS-133 Integration
//!
//! Uses provider-agnostic messaging types:
//! - `QueuedMessage<T>`: Provider-agnostic message with explicit `MessageHandle`
//! - `MessageEvent`: Signal-only notification for PGMQ large message flow
//! - `MessageClient`: Provider-agnostic messaging operations
//!
//! ## Command Routing Pattern
//!
//! The `execute_with_stats` helper reduces boilerplate by unifying:
//! - Handler execution
//! - Stats tracking (success/error counting)
//! - Response channel sending

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;

use pgmq::Message as PgmqMessage;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::messaging::client::MessageClient;
use tasker_shared::messaging::service::{
    MessageEvent, MessageHandle, MessageMetadata, QueuedMessage,
};
use tasker_shared::messaging::{StepExecutionResult, TaskRequestMessage};
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use crate::actors::result_processor_actor::ProcessStepResultMessage;
use crate::actors::task_finalizer_actor::FinalizeTaskMessage;
use crate::actors::task_request_actor::ProcessTaskRequestMessage;
use crate::actors::{ActorRegistry, Handler, ProcessBatchMessage};
use crate::health::caches::HealthStatusCaches;
use crate::orchestration::channels::{
    ChannelFactory, OrchestrationCommandReceiver, OrchestrationCommandSender,
};
use crate::orchestration::commands::{
    CommandResponder, OrchestrationCommand, OrchestrationProcessingStats, StepProcessResult,
    SystemHealth, TaskFinalizationResult, TaskInitializeResult, TaskReadinessResult,
};
use crate::orchestration::hydration::{
    FinalizationHydrator, StepResultHydrator, TaskRequestHydrator,
};

/// TAS-148: Actor-based orchestration command processor
///
/// Combines OrchestrationProcessor (channel management) and
/// OrchestrationProcessorCommandHandler (command routing) into
/// a single actor following the TAS-46 actor pattern.
///
/// TAS-46: Uses ActorRegistry for message-based actor coordination.
/// TAS-133: Uses MessageClient for provider-agnostic messaging.
#[derive(Debug)]
pub struct OrchestrationCommandProcessorActor {
    /// Shared system dependencies
    context: Arc<SystemContext>,

    /// Actor registry for message-based coordination (TAS-46)
    actors: Arc<ActorRegistry>,

    /// Message client for queue operations (TAS-133: provider-agnostic)
    message_client: Arc<MessageClient>,

    /// TAS-75: Cached health status for non-blocking health checks
    health_caches: HealthStatusCaches,

    /// Command receiver channel
    /// TAS-133: Uses NewType wrapper for type-safe channel communication
    command_rx: Option<OrchestrationCommandReceiver>,

    /// Processor task handle
    task_handle: Option<JoinHandle<()>>,

    /// Statistics tracking
    stats: Arc<std::sync::RwLock<OrchestrationProcessingStats>>,

    /// Channel monitor for observability (TAS-51)
    channel_monitor: ChannelMonitor,
}

impl OrchestrationCommandProcessorActor {
    /// Create new OrchestrationCommandProcessorActor with actor-based coordination (TAS-46)
    ///
    /// TAS-75: Now accepts `HealthStatusCaches` for non-blocking health checks.
    /// TAS-133: Now accepts `MessageClient` for provider-agnostic messaging.
    pub fn new(
        context: Arc<SystemContext>,
        actors: Arc<ActorRegistry>,
        message_client: Arc<MessageClient>,
        health_caches: HealthStatusCaches,
        buffer_size: usize,
        channel_monitor: ChannelMonitor,
    ) -> (Self, OrchestrationCommandSender) {
        // TAS-133: Use ChannelFactory for type-safe channel creation
        let (command_tx, command_rx) = ChannelFactory::orchestration_command_channel(buffer_size);

        let stats = Arc::new(std::sync::RwLock::new(OrchestrationProcessingStats {
            task_requests_processed: 0,
            step_results_processed: 0,
            tasks_finalized: 0,
            tasks_ready_processed: 0,
            processing_errors: 0,
            current_queue_sizes: HashMap::new(),
        }));

        info!(
            channel = %channel_monitor.channel_name(),
            buffer_size = buffer_size,
            "Creating OrchestrationCommandProcessorActor with channel monitoring"
        );

        let actor = Self {
            context,
            actors,
            message_client,
            health_caches,
            command_rx: Some(command_rx),
            task_handle: None,
            stats,
            channel_monitor,
        };

        (actor, command_tx)
    }

    /// Start the command processing loop
    pub async fn start(&mut self) -> TaskerResult<()> {
        let context = self.context.clone();
        let actors = self.actors.clone();
        let stats = self.stats.clone();
        let message_client = self.message_client.clone();
        let health_caches = self.health_caches.clone();
        let channel_monitor = self.channel_monitor.clone();
        let mut command_rx = self.command_rx.take().ok_or_else(|| {
            TaskerError::OrchestrationError("Processor already started".to_string())
        })?;

        // TAS-158: Named spawn for tokio-console visibility
        let handle = tasker_shared::spawn_named!("orchestration_command_processor", async move {
            let handler =
                CommandHandler::new(context, actors, stats, message_client, health_caches);
            while let Some(command) = command_rx.recv().await {
                // TAS-51: Record message receive for channel monitoring
                channel_monitor.record_receive();
                handler.process_command(command).await;
            }
        });

        self.task_handle = Some(handle);
        Ok(())
    }
}

/// Internal command handler with all the orchestration logic
///
/// This struct contains all the state and methods needed for command processing,
/// separated from the lifecycle management in the actor.
#[derive(Debug)]
struct CommandHandler {
    #[expect(
        dead_code,
        reason = "SystemContext available for future command handler operations"
    )]
    context: Arc<SystemContext>,
    actors: Arc<ActorRegistry>,
    stats: Arc<std::sync::RwLock<OrchestrationProcessingStats>>,
    message_client: Arc<MessageClient>,
    health_caches: HealthStatusCaches,
    step_result_hydrator: StepResultHydrator,
    task_request_hydrator: TaskRequestHydrator,
    finalization_hydrator: FinalizationHydrator,
}

impl CommandHandler {
    fn new(
        context: Arc<SystemContext>,
        actors: Arc<ActorRegistry>,
        stats: Arc<std::sync::RwLock<OrchestrationProcessingStats>>,
        message_client: Arc<MessageClient>,
        health_caches: HealthStatusCaches,
    ) -> Self {
        let step_result_hydrator = StepResultHydrator::new(context.clone());
        let task_request_hydrator = TaskRequestHydrator::new();
        let finalization_hydrator = FinalizationHydrator::new();

        Self {
            context,
            actors,
            stats,
            message_client,
            health_caches,
            step_result_hydrator,
            task_request_hydrator,
            finalization_hydrator,
        }
    }

    // =========================================================================
    // Command Routing
    // =========================================================================

    /// Process individual commands with sophisticated delegation
    pub async fn process_command(&self, command: OrchestrationCommand) {
        match command {
            OrchestrationCommand::InitializeTask { request, resp } => {
                self.execute_with_stats(
                    self.handle_initialize_task(request),
                    |stats| &mut stats.task_requests_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::ProcessStepResult { result, resp } => {
                self.execute_with_stats(
                    self.handle_process_step_result(result),
                    |stats| &mut stats.step_results_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::FinalizeTask { task_uuid, resp } => {
                self.execute_with_stats(
                    self.handle_finalize_task(task_uuid),
                    |stats| &mut stats.tasks_finalized,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::ProcessStepResultFromMessageEvent {
                message_event,
                resp,
            } => {
                self.execute_with_stats(
                    self.handle_step_result_from_message_event(message_event),
                    |stats| &mut stats.step_results_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::InitializeTaskFromMessageEvent {
                message_event,
                resp,
            } => {
                self.execute_with_stats(
                    self.handle_task_initialize_from_message_event(message_event),
                    |stats| &mut stats.task_requests_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::FinalizeTaskFromMessageEvent {
                message_event,
                resp,
            } => {
                self.execute_with_stats(
                    self.handle_task_finalize_from_message_event(message_event),
                    |stats| &mut stats.tasks_finalized,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::ProcessStepResultFromMessage { message, resp } => {
                let queue_name = message.queue_name();
                debug!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    "Starting ProcessStepResultFromMessage"
                );
                self.execute_with_stats(
                    self.handle_step_result_from_message(message),
                    |stats| &mut stats.step_results_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::InitializeTaskFromMessage { message, resp } => {
                self.execute_with_stats(
                    self.handle_task_initialize_from_message(message),
                    |stats| &mut stats.task_requests_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::FinalizeTaskFromMessage { message, resp } => {
                self.execute_with_stats(
                    self.handle_task_finalize_from_message(message),
                    |stats| &mut stats.tasks_finalized,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::ProcessTaskReadiness {
                task_uuid,
                namespace,
                priority,
                ready_steps,
                triggered_by,
                step_uuid,
                step_state,
                task_state,
                resp,
            } => {
                info!(
                    task_uuid = %task_uuid,
                    namespace = %namespace,
                    priority = %priority,
                    ready_steps = %ready_steps,
                    triggered_by = %triggered_by,
                    step_uuid = format!("{:?}", step_uuid),
                    step_state = format!("{:?}", step_state),
                    task_state = format!("{:?}", task_state),
                    "Processing task readiness for task with UUID {task_uuid} in namespace {namespace}",
                );
                self.execute_with_stats(
                    self.handle_process_task_readiness(
                        task_uuid,
                        namespace,
                        priority,
                        ready_steps,
                        triggered_by,
                    ),
                    |stats| &mut stats.tasks_ready_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::GetProcessingStats { resp } => {
                let stats_copy = self.stats.read().unwrap_or_else(|p| p.into_inner()).clone();
                if resp.send(Ok(stats_copy)).is_err() {
                    error!("GetProcessingStats response channel closed - receiver dropped");
                }
            }
            OrchestrationCommand::HealthCheck { resp } => {
                let result = self.handle_health_check().await;
                if resp.send(result).is_err() {
                    error!("HealthCheck response channel closed - receiver dropped");
                }
            }
            OrchestrationCommand::Shutdown { resp } => {
                if resp.send(Ok(())).is_err() {
                    error!("Shutdown response channel closed - receiver dropped");
                }
            }
        }
    }

    /// Execute handler with automatic stats tracking and response sending
    ///
    /// TAS-148: This helper reduces boilerplate by unifying the common pattern of:
    /// 1. Execute the handler
    /// 2. Update stats based on success/failure
    /// 3. Send response through the channel
    ///
    /// # Channel Send Failures
    ///
    /// If the response channel is closed (receiver dropped), this is logged as an ERROR.
    /// This typically indicates a serious issue - either the caller timed out or crashed
    /// before receiving the response, which could lead to orphaned operations.
    async fn execute_with_stats<T, Fut>(
        &self,
        handler: Fut,
        stat_selector: impl FnOnce(&mut OrchestrationProcessingStats) -> &mut u64,
        resp: CommandResponder<T>,
    ) where
        Fut: Future<Output = TaskerResult<T>>,
        T: std::fmt::Debug,
    {
        let result = handler.await;
        let was_success = result.is_ok();
        {
            let mut stats = self.stats.write().unwrap_or_else(|p| p.into_inner());
            if was_success {
                *stat_selector(&mut stats) += 1;
            } else {
                stats.processing_errors += 1;
            }
        }
        if resp.send(result).is_err() {
            error!(
                was_success = was_success,
                "Command response channel closed - receiver dropped before response could be sent. \
                 This may indicate caller timeout or crash, potentially leading to orphaned operations."
            );
        }
    }

    // =========================================================================
    // PGMQ Client Access
    // =========================================================================

    /// Get the underlying PGMQ client for event-driven operations (TAS-133)
    ///
    /// Returns an error if the provider is not PGMQ. Event-driven operations
    /// using `MessageEvent` with PGMQ-specific `read_specific_message` require a PGMQ provider.
    fn pgmq_client(&self) -> TaskerResult<&pgmq_notify::PgmqClient> {
        self.message_client
            .provider()
            .as_pgmq()
            .map(|s| s.client())
            .ok_or_else(|| {
                TaskerError::ConfigurationError(
                    "Event-driven operations require PGMQ provider. Use polling mode with RabbitMQ."
                        .to_string(),
                )
            })
    }

    /// TAS-133: Ack a message using provider-agnostic MessageHandle
    async fn ack_message_with_handle(&self, handle: &MessageHandle) -> TaskerResult<()> {
        let queue_name = handle.queue_name();
        let receipt_handle = handle.as_receipt_handle();
        self.message_client
            .ack_message(queue_name, &receipt_handle)
            .await
    }

    /// TAS-133: Convert a PGMQ message to a provider-agnostic QueuedMessage
    fn wrap_pgmq_message(
        queue_name: &str,
        message: PgmqMessage,
    ) -> QueuedMessage<serde_json::Value> {
        QueuedMessage::with_handle(
            message.message.clone(),
            MessageHandle::Pgmq {
                msg_id: message.msg_id,
                queue_name: queue_name.to_string(),
            },
            MessageMetadata::new(message.read_ct as u32, message.enqueued_at),
        )
    }

    // =========================================================================
    // Task Initialization Handlers
    // =========================================================================

    /// Handle task initialization using TaskRequestActor directly (TAS-46)
    async fn handle_initialize_task(
        &self,
        request: TaskRequestMessage,
    ) -> TaskerResult<TaskInitializeResult> {
        let msg = ProcessTaskRequestMessage { request };
        let task_uuid = self.actors.task_request_actor.handle(msg).await?;

        Ok(TaskInitializeResult::Success {
            task_uuid,
            message: "Task initialized successfully".to_string(),
        })
    }

    /// Handle task initialization from message event
    ///
    /// ## PGMQ-Specific Code Path
    ///
    /// This handler is **exclusively for PGMQ's signal-only notification flow** for large
    /// messages (>7KB). RabbitMQ should use `InitializeTaskFromMessage` instead.
    async fn handle_task_initialize_from_message_event(
        &self,
        message_event: MessageEvent,
    ) -> TaskerResult<TaskInitializeResult> {
        // TAS-133: Guard clause - this code path is only valid for PGMQ
        let provider = self.message_client.provider();
        if !provider.supports_fetch_by_message_id() {
            error!(
                provider = provider.provider_name(),
                msg_id = %message_event.message_id,
                queue = %message_event.queue_name,
                "CRITICAL: Signal-only notification received but provider does not support fetch-by-message-ID"
            );
            return Err(TaskerError::MessagingError(format!(
                "Provider '{}' does not support fetch-by-message-ID flow.",
                provider.provider_name()
            )));
        }

        let msg_id: i64 = message_event.message_id.as_str().parse().map_err(|e| {
            TaskerError::ValidationError(format!(
                "Invalid message ID '{}' for PGMQ: {e}",
                message_event.message_id
            ))
        })?;

        debug!(
            msg_id = msg_id,
            queue = %message_event.queue_name,
            "Processing task initialization from message event"
        );

        let message = self
            .pgmq_client()?
            .read_specific_message::<serde_json::Value>(&message_event.queue_name, msg_id, 30)
            .await
            .map_err(|e| {
                error!(
                    msg_id = msg_id,
                    queue = %message_event.queue_name,
                    error = %e,
                    "Failed to read specific task request message"
                );
                TaskerError::MessagingError(format!(
                    "Failed to read specific task request message {} from queue {}: {e}",
                    msg_id, message_event.queue_name
                ))
            })?
            .ok_or_else(|| {
                error!(
                    msg_id = msg_id,
                    queue = %message_event.queue_name,
                    "Task request message not found in queue"
                );
                TaskerError::MessagingError(format!(
                    "Task request message {} not found in queue {}",
                    msg_id, message_event.queue_name
                ))
            })?;

        let queued_message = Self::wrap_pgmq_message(&message_event.queue_name, message);
        self.handle_task_initialize_from_message(queued_message)
            .await
    }

    /// Handle task initialization from message
    ///
    /// TAS-133: Accepts provider-agnostic QueuedMessage with explicit MessageHandle
    async fn handle_task_initialize_from_message(
        &self,
        message: QueuedMessage<serde_json::Value>,
    ) -> TaskerResult<TaskInitializeResult> {
        let queue_name = message.queue_name();
        debug!(
            handle = ?message.handle,
            queue = %queue_name,
            "TASK_INIT_HANDLER: Processing task initialization via hydrator"
        );

        let task_request = self
            .task_request_hydrator
            .hydrate_from_queued_message(&message)
            .await?;

        debug!(
            handle = ?message.handle,
            namespace = %task_request.task_request.namespace,
            handler = %task_request.task_request.name,
            "TASK_INIT_HANDLER: Hydration complete, delegating to task initialization"
        );

        let result = self.handle_initialize_task(task_request).await;

        match &result {
            Ok(_) => {
                debug!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    "Task initialization successful, acknowledging message"
                );
                if let Err(e) = self.ack_message_with_handle(&message.handle).await {
                    warn!(
                        handle = ?message.handle,
                        queue = %queue_name,
                        error = %e,
                        "Failed to acknowledge processed task request message"
                    );
                }
            }
            Err(e) => {
                error!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    error = %e,
                    "Task initialization failed, keeping message in queue"
                );
            }
        }

        result
    }

    // =========================================================================
    // Step Result Handlers
    // =========================================================================

    /// Handle step result processing using ResultProcessorActor directly (TAS-46)
    async fn handle_process_step_result(
        &self,
        step_result: StepExecutionResult,
    ) -> TaskerResult<StepProcessResult> {
        let msg = ProcessStepResultMessage {
            result: step_result.clone(),
        };

        match self.actors.result_processor_actor.handle(msg).await {
            Ok(()) => Ok(StepProcessResult::Success {
                message: format!(
                    "Step {} result processed successfully",
                    step_result.step_uuid
                ),
            }),
            Err(e) => match step_result.status.as_str() {
                "failed" => Ok(StepProcessResult::Failed {
                    error: format!("{e}"),
                }),
                "skipped" => Ok(StepProcessResult::Skipped {
                    reason: format!("{e}"),
                }),
                _ => Err(TaskerError::OrchestrationError(format!("{e}"))),
            },
        }
    }

    /// Handle step result from message event
    ///
    /// ## PGMQ-Specific Code Path
    ///
    /// This handler is **exclusively for PGMQ's signal-only notification flow** for large
    /// messages (>7KB). RabbitMQ should use `ProcessStepResultFromMessage` instead.
    async fn handle_step_result_from_message_event(
        &self,
        message_event: MessageEvent,
    ) -> TaskerResult<StepProcessResult> {
        let provider = self.message_client.provider();
        if !provider.supports_fetch_by_message_id() {
            error!(
                provider = provider.provider_name(),
                msg_id = %message_event.message_id,
                queue = %message_event.queue_name,
                "CRITICAL: Signal-only notification received but provider does not support fetch-by-message-ID"
            );
            return Err(TaskerError::MessagingError(format!(
                "Provider '{}' does not support fetch-by-message-ID flow.",
                provider.provider_name()
            )));
        }

        let msg_id: i64 = message_event.message_id.as_str().parse().map_err(|e| {
            TaskerError::ValidationError(format!(
                "Invalid message ID '{}' for PGMQ: {e}",
                message_event.message_id
            ))
        })?;

        let message = self
            .pgmq_client()?
            .read_specific_message::<serde_json::Value>(&message_event.queue_name, msg_id, 30)
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!(
                    "Failed to read specific step result message {} from queue {}: {e}",
                    msg_id, message_event.queue_name
                ))
            })?
            .ok_or_else(|| {
                TaskerError::MessagingError(format!(
                    "Step result message {} not found in queue {}",
                    msg_id, message_event.queue_name
                ))
            })?;

        let queued_message = Self::wrap_pgmq_message(&message_event.queue_name, message);
        self.handle_step_result_from_message(queued_message).await
    }

    /// Handle step result from message
    ///
    /// TAS-133: Accepts provider-agnostic QueuedMessage with explicit MessageHandle
    async fn handle_step_result_from_message(
        &self,
        message: QueuedMessage<serde_json::Value>,
    ) -> TaskerResult<StepProcessResult> {
        let queue_name = message.queue_name();
        debug!(
            handle = ?message.handle,
            queue = %queue_name,
            "STEP_RESULT_HANDLER: Processing step result message via hydrator"
        );

        let step_execution_result = self
            .step_result_hydrator
            .hydrate_from_queued_message(&message)
            .await?;

        debug!(
            step_uuid = %step_execution_result.step_uuid,
            status = %step_execution_result.status,
            "STEP_RESULT_HANDLER: Hydration complete, delegating to result processor"
        );

        let result = self
            .handle_process_step_result(step_execution_result.clone())
            .await;

        match &result {
            Ok(step_result) => {
                info!(
                    handle = ?message.handle,
                    step_uuid = %step_execution_result.step_uuid,
                    result = ?step_result,
                    "STEP_RESULT_HANDLER: Result processing succeeded"
                );

                debug!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    "STEP_RESULT_HANDLER: Acknowledging successfully processed message"
                );

                match self.ack_message_with_handle(&message.handle).await {
                    Ok(()) => {
                        info!(
                            handle = ?message.handle,
                            queue = %queue_name,
                            "STEP_RESULT_HANDLER: Successfully acknowledged processed message"
                        );
                    }
                    Err(e) => {
                        warn!(
                            handle = ?message.handle,
                            queue = %queue_name,
                            error = %e,
                            "STEP_RESULT_HANDLER: Failed to acknowledge (will be reprocessed)"
                        );
                    }
                }
            }
            Err(err) => {
                error!(
                    handle = ?message.handle,
                    step_uuid = %step_execution_result.step_uuid,
                    error = %err,
                    "STEP_RESULT_HANDLER: Result processing failed (message will remain for retry)"
                );
            }
        }

        result
    }

    // =========================================================================
    // Task Finalization Handlers
    // =========================================================================

    /// Handle task finalization using TaskFinalizerActor (TAS-46)
    async fn handle_finalize_task(&self, task_uuid: Uuid) -> TaskerResult<TaskFinalizationResult> {
        let msg = FinalizeTaskMessage { task_uuid };
        let result = self.actors.task_finalizer_actor.handle(msg).await?;

        Ok(TaskFinalizationResult::Success {
            task_uuid: result.task_uuid,
            final_status: format!("{:?}", result.action),
            completion_time: Some(chrono::Utc::now()),
        })
    }

    /// Handle task finalization from message event
    ///
    /// ## PGMQ-Specific Code Path
    ///
    /// This handler is **exclusively for PGMQ's signal-only notification flow** for large
    /// messages (>7KB). RabbitMQ should use `FinalizeTaskFromMessage` instead.
    async fn handle_task_finalize_from_message_event(
        &self,
        message_event: MessageEvent,
    ) -> TaskerResult<TaskFinalizationResult> {
        let provider = self.message_client.provider();
        if !provider.supports_fetch_by_message_id() {
            error!(
                provider = provider.provider_name(),
                msg_id = %message_event.message_id,
                queue = %message_event.queue_name,
                "CRITICAL: Signal-only notification received but provider does not support fetch-by-message-ID"
            );
            return Err(TaskerError::MessagingError(format!(
                "Provider '{}' does not support fetch-by-message-ID flow.",
                provider.provider_name()
            )));
        }

        let msg_id: i64 = message_event.message_id.as_str().parse().map_err(|e| {
            TaskerError::ValidationError(format!(
                "Invalid message ID '{}' for PGMQ: {e}",
                message_event.message_id
            ))
        })?;

        let message = self
            .pgmq_client()?
            .read_specific_message::<serde_json::Value>(&message_event.queue_name, msg_id, 30)
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!(
                    "Failed to read specific finalization message {} from queue {}: {e}",
                    msg_id, message_event.queue_name
                ))
            })?
            .ok_or_else(|| {
                TaskerError::MessagingError(format!(
                    "Finalization message {} not found in queue {}",
                    msg_id, message_event.queue_name
                ))
            })?;

        let queued_message = Self::wrap_pgmq_message(&message_event.queue_name, message);
        self.handle_task_finalize_from_message(queued_message).await
    }

    /// Handle task finalization from message
    ///
    /// TAS-133: Accepts provider-agnostic QueuedMessage with explicit MessageHandle
    async fn handle_task_finalize_from_message(
        &self,
        message: QueuedMessage<serde_json::Value>,
    ) -> TaskerResult<TaskFinalizationResult> {
        let queue_name = message.queue_name();
        debug!(
            handle = ?message.handle,
            queue = %queue_name,
            "FINALIZATION_HANDLER: Processing finalization via hydrator"
        );

        let task_uuid = self
            .finalization_hydrator
            .hydrate_from_queued_message(&message)
            .await?;

        debug!(
            handle = ?message.handle,
            task_uuid = %task_uuid,
            "FINALIZATION_HANDLER: Hydration complete, delegating to task finalization"
        );

        let result = self.handle_finalize_task(task_uuid).await;

        if matches!(result, Ok(TaskFinalizationResult::Success { .. })) {
            if let Err(e) = self.ack_message_with_handle(&message.handle).await {
                warn!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    task_uuid = %task_uuid,
                    error = %e,
                    "Failed to acknowledge processed finalization message"
                );
            }
        } else {
            let error_msg = match &result {
                Ok(TaskFinalizationResult::NotClaimed { reason, .. }) => {
                    format!("Not claimed: {}", reason)
                }
                Ok(TaskFinalizationResult::Failed { error }) => format!("Failed: {}", error),
                Err(e) => format!("Error: {}", e),
                _ => "Unknown finalization result".to_string(),
            };

            warn!(
                handle = ?message.handle,
                queue = %queue_name,
                task_uuid = %task_uuid,
                result = %error_msg,
                "Task finalization was not successful - keeping message in queue"
            );
        }

        result
    }

    // =========================================================================
    // Task Readiness Handler
    // =========================================================================

    /// Handle task readiness processing
    ///
    /// This is the core TAS-43 implementation that:
    /// 1. Delegates to StepEnqueuerActor for atomic step enqueueing
    /// 2. Returns processing metrics for observability
    async fn handle_process_task_readiness(
        &self,
        task_uuid: Uuid,
        namespace: String,
        priority: i32,
        ready_steps: i32,
        triggered_by: String,
    ) -> TaskerResult<TaskReadinessResult> {
        let start_time = std::time::Instant::now();

        debug!(
            task_uuid = %task_uuid,
            namespace = %namespace,
            priority = priority,
            ready_steps = ready_steps,
            triggered_by = %triggered_by,
            "Processing task readiness event via command pattern"
        );

        let msg = ProcessBatchMessage;
        let process_result = match self.actors.step_enqueuer_actor.handle(msg).await {
            Ok(result) => result,
            Err(e) => {
                error!(
                    task_uuid = %task_uuid,
                    namespace = %namespace,
                    error = %e,
                    "Failed to process task readiness via StepEnqueuerActor"
                );
                return Err(e);
            }
        };

        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        info!(
            task_uuid = %task_uuid,
            namespace = %namespace,
            priority = priority,
            ready_steps = ready_steps,
            tasks_processed = process_result.tasks_processed,
            tasks_failed = process_result.tasks_failed,
            processing_time_ms = processing_time_ms,
            triggered_by = %triggered_by,
            "Task readiness processed successfully"
        );

        Ok(TaskReadinessResult {
            task_uuid,
            namespace,
            steps_enqueued: ready_steps as u32,
            steps_discovered: ready_steps as u32,
            processing_time_ms,
            triggered_by,
        })
    }

    // =========================================================================
    // Health Check Handler
    // =========================================================================

    /// Handle health check using cached health status (TAS-75)
    ///
    /// Reads from `HealthStatusCaches` for non-blocking health checks.
    /// Uses fail-open semantics: unevaluated status returns healthy.
    async fn handle_health_check(&self) -> TaskerResult<SystemHealth> {
        let db_status = self.health_caches.get_db_status().await;
        let channel_status = self.health_caches.get_channel_status().await;
        let queue_status = self.health_caches.get_queue_status().await;
        let backpressure = self.health_caches.get_backpressure().await;

        let health_evaluated =
            db_status.evaluated || channel_status.evaluated || queue_status.tier.is_evaluated();

        let database_connected = if db_status.evaluated {
            db_status.is_connected && !db_status.circuit_breaker_open
        } else {
            true // Fail-open during startup
        };

        let message_queues_healthy = !queue_status.tier.is_critical();

        let status = if !health_evaluated {
            "unknown".to_string()
        } else if !database_connected || !message_queues_healthy || backpressure.active {
            "unhealthy".to_string()
        } else if channel_status.is_saturated || queue_status.tier.is_warning() {
            "degraded".to_string()
        } else {
            "healthy".to_string()
        };

        Ok(SystemHealth {
            status,
            database_connected,
            message_queues_healthy,
            active_processors: self.actors.actor_count() as u32,
            circuit_breaker_open: db_status.circuit_breaker_open,
            circuit_breaker_failures: db_status.circuit_breaker_failures,
            command_channel_saturation_percent: channel_status.command_saturation_percent,
            backpressure_active: backpressure.active,
            queue_depth_tier: format!("{:?}", queue_status.tier),
            queue_depth_max: queue_status.max_depth,
            queue_depth_worst_queue: queue_status.worst_queue.clone(),
            health_evaluated,
        })
    }
}
