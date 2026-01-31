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

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::task::JoinHandle;

use tracing::{debug, error, info};

use tasker_shared::messaging::client::MessageClient;
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use crate::actors::ActorRegistry;
use crate::health::caches::HealthStatusCaches;
use crate::orchestration::channels::{
    ChannelFactory, OrchestrationCommandReceiver, OrchestrationCommandSender,
};
use crate::orchestration::commands::CommandProcessingService;
use crate::orchestration::commands::{
    AtomicProcessingStats, CommandResponder, OrchestrationCommand,
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

    /// Statistics tracking (SWMR: lock-free atomic counters)
    stats: Arc<AtomicProcessingStats>,

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

        let stats = Arc::new(AtomicProcessingStats::default());

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

/// Internal command handler: thin routing + stats layer
///
/// Delegates all business logic to `CommandProcessingService`, keeping only
/// command routing (`process_command`) and stats tracking (`execute_with_stats`).
/// This follows the TAS-46 actor/service separation pattern.
#[derive(Debug)]
struct CommandHandler {
    service: CommandProcessingService,
    stats: Arc<AtomicProcessingStats>,
}

impl CommandHandler {
    fn new(
        context: Arc<SystemContext>,
        actors: Arc<ActorRegistry>,
        stats: Arc<AtomicProcessingStats>,
        message_client: Arc<MessageClient>,
        health_caches: HealthStatusCaches,
    ) -> Self {
        let service = CommandProcessingService::new(context, actors, message_client, health_caches);
        Self { service, stats }
    }

    // =========================================================================
    // Command Routing
    // =========================================================================

    /// Route commands to the appropriate service method with stats tracking
    pub async fn process_command(&self, command: OrchestrationCommand) {
        match command {
            OrchestrationCommand::InitializeTask { request, resp } => {
                self.execute_with_stats(
                    self.service.initialize_task(request),
                    |stats| &stats.task_requests_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::ProcessStepResult { result, resp } => {
                self.execute_with_stats(
                    self.service.process_step_result(result),
                    |stats| &stats.step_results_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::FinalizeTask { task_uuid, resp } => {
                self.execute_with_stats(
                    self.service.finalize_task(task_uuid),
                    |stats| &stats.tasks_finalized,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::ProcessStepResultFromMessageEvent {
                message_event,
                resp,
            } => {
                self.execute_with_stats(
                    self.service.step_result_from_message_event(message_event),
                    |stats| &stats.step_results_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::InitializeTaskFromMessageEvent {
                message_event,
                resp,
            } => {
                self.execute_with_stats(
                    self.service
                        .task_initialize_from_message_event(message_event),
                    |stats| &stats.task_requests_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::FinalizeTaskFromMessageEvent {
                message_event,
                resp,
            } => {
                self.execute_with_stats(
                    self.service.task_finalize_from_message_event(message_event),
                    |stats| &stats.tasks_finalized,
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
                    self.service.step_result_from_message(message),
                    |stats| &stats.step_results_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::InitializeTaskFromMessage { message, resp } => {
                self.execute_with_stats(
                    self.service.task_initialize_from_message(message),
                    |stats| &stats.task_requests_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::FinalizeTaskFromMessage { message, resp } => {
                self.execute_with_stats(
                    self.service.task_finalize_from_message(message),
                    |stats| &stats.tasks_finalized,
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
                debug!(
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
                    self.service.process_task_readiness(
                        task_uuid,
                        namespace,
                        priority,
                        ready_steps,
                        triggered_by,
                    ),
                    |stats| &stats.tasks_ready_processed,
                    resp,
                )
                .await;
            }
            OrchestrationCommand::GetProcessingStats { resp } => {
                let stats_snapshot = self.stats.snapshot();
                if resp.send(Ok(stats_snapshot)).is_err() {
                    error!("GetProcessingStats response channel closed - receiver dropped");
                }
            }
            OrchestrationCommand::HealthCheck { resp } => {
                let result = self.service.health_check().await;
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
    /// 2. Update stats based on success/failure (lock-free via AtomicU64)
    /// 3. Send response through the channel
    ///
    /// # Channel Send Failures
    ///
    /// TAS-162: All production callers (OrchestrationEventSystem, FallbackPoller) use
    /// fire-and-forget semantics - they create a oneshot channel but immediately drop
    /// the receiver (`_resp_rx`). This means `resp.send()` will always fail in normal
    /// operation. This is expected behavior, not an error condition.
    async fn execute_with_stats<T, Fut>(
        &self,
        handler: Fut,
        stat_selector: impl FnOnce(&AtomicProcessingStats) -> &AtomicU64,
        resp: CommandResponder<T>,
    ) where
        Fut: Future<Output = TaskerResult<T>>,
        T: std::fmt::Debug,
    {
        let result = handler.await;
        let was_success = result.is_ok();
        if was_success {
            stat_selector(&self.stats).fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats.processing_errors.fetch_add(1, Ordering::Relaxed);
        }
        // TAS-162: All callers use fire-and-forget (drop receiver immediately), so
        // a closed channel is expected. Only log at debug to avoid false alarm noise.
        if resp.send(result).is_err() {
            debug!(
                was_success = was_success,
                "Command response channel closed - receiver dropped (fire-and-forget caller)"
            );
        }
    }
}
