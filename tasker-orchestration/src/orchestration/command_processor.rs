//! # TAS-40 Command Pattern Implementation for Orchestration
//!
//! This module implements the command pattern architecture to replace the complex
//! polling-based coordinator/executor system with a simple tokio mpsc channel-based
//! command processor.
//!
//! ## Key Features
//!
//! - **No Polling**: Pure command-driven architecture using tokio channels
//! - **Sophisticated Delegation**: Commands delegate to existing components for business logic
//! - **Race Condition Prevention**: Atomic finalization claiming and proper error handling
//! - **Transaction Safety**: Maintains existing transaction patterns through delegation
//! - **Observability**: Preserves metrics and logging through delegated components
//!
//! ## Architecture
//!
//! The OrchestrationProcessor receives commands via tokio::sync::mpsc channels and delegates
//! sophisticated orchestration logic to existing components:
//! - TaskRequestProcessor for task initialization
//! - StepResultProcessor for step result processing
//! - FinalizationClaimer for atomic task finalization
//!
//! This preserves the sophisticated orchestration logic while eliminating polling complexity.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use pgmq::Message as PgmqMessage;
use pgmq_notify::MessageReadyEvent;
use tasker_shared::messaging::{StepExecutionResult, TaskRequestMessage};
use tasker_shared::{TaskerError, TaskerResult};

use tracing::{debug, error, info, warn};

/// Type alias for command response channels
pub type CommandResponder<T> = oneshot::Sender<TaskerResult<T>>;

/// Commands for orchestration operations (TAS-40 Command Pattern)
///
/// These commands replace direct method calls with async command pattern,
/// eliminating polling while preserving sophisticated orchestration logic.
#[derive(Debug)]
pub enum OrchestrationCommand {
    /// Initialize a new task - delegates to TaskRequestProcessor
    InitializeTask {
        request: TaskRequestMessage, // Use existing message format
        resp: CommandResponder<TaskInitializeResult>,
    },
    /// Process a step execution result - delegates to StepResultProcessor
    ProcessStepResult {
        result: StepExecutionResult, // Use existing result format
        resp: CommandResponder<StepProcessResult>,
    },
    /// Finalize a completed task - uses FinalizationClaimer for atomic operation
    FinalizeTask {
        task_uuid: Uuid,
        resp: CommandResponder<TaskFinalizationResult>,
    },
    /// Process step result from message - delegates full message lifecycle to worker
    ProcessStepResultFromMessage {
        queue_name: String,
        message: PgmqMessage,
        resp: CommandResponder<StepProcessResult>,
    },
    /// Initialize task from message - delegates full message lifecycle to worker
    InitializeTaskFromMessage {
        queue_name: String,
        message: PgmqMessage,
        resp: CommandResponder<TaskInitializeResult>,
    },
    /// Finalize task from message - delegates full message lifecycle to worker
    FinalizeTaskFromMessage {
        queue_name: String,
        message: PgmqMessage,
        resp: CommandResponder<TaskFinalizationResult>,
    },
    /// Process step result from message event - delegates full message lifecycle to worker
    ProcessStepResultFromMessageEvent {
        message_event: MessageReadyEvent,
        resp: CommandResponder<StepProcessResult>,
    },
    /// Initialize task from message event - delegates full message lifecycle to worker
    InitializeTaskFromMessageEvent {
        message_event: MessageReadyEvent,
        resp: CommandResponder<TaskInitializeResult>,
    },
    /// Finalize task from message event - delegates full message lifecycle to worker
    FinalizeTaskFromMessageEvent {
        message_event: MessageReadyEvent,
        resp: CommandResponder<TaskFinalizationResult>,
    },
    /// Process task readiness event from PostgreSQL LISTEN/NOTIFY
    /// Delegates to TaskClaimStepEnqueuer for atomic task claiming and step enqueueing
    ProcessTaskReadiness {
        task_uuid: Uuid,
        namespace: String,
        priority: i32,
        ready_steps: i32,
        triggered_by: String, // "step_transition", "task_start", "fallback_polling"
        step_uuid: Option<Uuid>, // Present for step_transition triggers
        step_state: Option<String>, // Present for step_transition triggers
        task_state: Option<String>, // Present for task_start triggers
        resp: CommandResponder<TaskReadinessResult>,
    },
    /// Get orchestration processing statistics
    GetProcessingStats {
        resp: CommandResponder<OrchestrationProcessingStats>,
    },
    /// Perform health check
    HealthCheck {
        resp: CommandResponder<SystemHealth>,
    },
    /// Shutdown orchestration processor
    Shutdown { resp: CommandResponder<()> },
}

/// Result types matching existing orchestration patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskInitializeResult {
    Success { task_uuid: Uuid, message: String },
    Failed { error: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepProcessResult {
    Success { message: String },
    Failed { error: String },
    Skipped { reason: String },
}

/// Result of processing a task readiness event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadinessResult {
    pub task_uuid: Uuid,
    pub namespace: String,
    pub steps_enqueued: u32,
    pub steps_discovered: u32,
    pub triggered_by: String,
    pub processing_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskFinalizationResult {
    Success {
        task_uuid: Uuid,
        final_status: String,
        completion_time: Option<chrono::DateTime<chrono::Utc>>,
    },
    NotClaimed {
        reason: String,
        already_claimed_by: Option<Uuid>,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationProcessingStats {
    pub task_requests_processed: u64,
    pub step_results_processed: u64,
    pub tasks_finalized: u64,
    pub tasks_ready_processed: u64, // TAS-43: Task readiness events processed
    pub processing_errors: u64,
    pub current_queue_sizes: HashMap<String, i64>,
}

/// TAS-75: Enhanced system health status
///
/// This struct contains comprehensive health information derived from
/// cached health status data updated by the background StatusEvaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// Overall health status: "healthy", "degraded", or "unhealthy"
    pub status: String,

    /// Whether the database is connected (from cached DB health check)
    pub database_connected: bool,

    /// Whether message queues are healthy (not in Critical/Overflow)
    pub message_queues_healthy: bool,

    /// Number of active orchestration processors
    pub active_processors: u32,

    // TAS-75: Enhanced health fields from cached status
    /// Circuit breaker state for database operations
    pub circuit_breaker_open: bool,

    /// Number of consecutive circuit breaker failures
    pub circuit_breaker_failures: u32,

    /// Command channel saturation percentage (0.0-100.0)
    pub command_channel_saturation_percent: f64,

    /// Whether backpressure is currently active
    pub backpressure_active: bool,

    /// Queue depth tier: "Unknown", "Normal", "Warning", "Critical", "Overflow"
    pub queue_depth_tier: String,

    /// Maximum queue depth across all monitored queues
    pub queue_depth_max: i64,

    /// Name of the queue with the highest depth
    pub queue_depth_worst_queue: String,

    /// Whether health data has been evaluated (false means Unknown state)
    pub health_evaluated: bool,
}

use std::sync::Arc;
use tokio::task::JoinHandle;

// TAS-40 Phase 2.2 - Sophisticated delegation imports
use crate::actors::result_processor_actor::ProcessStepResultMessage;
use crate::actors::task_finalizer_actor::FinalizeTaskMessage;
use crate::actors::task_request_actor::ProcessTaskRequestMessage;
use crate::actors::{ActorRegistry, Handler, ProcessBatchMessage};
use crate::health::caches::HealthStatusCaches; // TAS-75: Cached health status
use crate::orchestration::hydration::{
    FinalizationHydrator, StepResultHydrator, TaskRequestHydrator,
}; // TAS-46 Phase 4
   // use crate::orchestration::lifecycle::result_processing::OrchestrationResultProcessor;
   // use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
   // use crate::orchestration::lifecycle::task_request_processor::TaskRequestProcessor;
use tasker_shared::messaging::{PgmqClientTrait, UnifiedPgmqClient};
use tasker_shared::monitoring::ChannelMonitor; // TAS-51: Channel monitoring
use tasker_shared::system_context::SystemContext;

/// TAS-40 Command Pattern Orchestration Processor
///
/// Replaces complex polling-based coordinator/executor system with simple command pattern.
/// Delegates sophisticated orchestration logic to existing components while eliminating
/// polling complexity.
///
/// TAS-46: Uses ActorRegistry for message-based actor coordination.
#[derive(Debug)]
pub struct OrchestrationProcessor {
    /// Shared system dependencies
    context: Arc<SystemContext>,

    /// Actor registry for message-based coordination (TAS-46)
    actors: Arc<ActorRegistry>,

    /// PGMQ client for message operations
    pgmq_client: Arc<UnifiedPgmqClient>,

    /// TAS-75: Cached health status for non-blocking health checks
    health_caches: HealthStatusCaches,

    /// Command receiver channel
    command_rx: Option<mpsc::Receiver<OrchestrationCommand>>,

    /// Processor task handle
    task_handle: Option<JoinHandle<()>>,

    /// Statistics tracking
    stats: Arc<std::sync::RwLock<OrchestrationProcessingStats>>,

    /// Channel monitor for observability (TAS-51)
    channel_monitor: ChannelMonitor,
}

impl OrchestrationProcessor {
    /// Create new OrchestrationProcessor with actor-based coordination (TAS-46)
    ///
    /// TAS-75: Now accepts `HealthStatusCaches` for non-blocking health checks.
    pub fn new(
        context: Arc<SystemContext>,
        actors: Arc<ActorRegistry>,
        pgmq_client: Arc<UnifiedPgmqClient>,
        health_caches: HealthStatusCaches,
        buffer_size: usize,
        channel_monitor: ChannelMonitor,
    ) -> (Self, mpsc::Sender<OrchestrationCommand>) {
        let (command_tx, command_rx) = mpsc::channel(buffer_size);

        let stats = Arc::new(std::sync::RwLock::new(OrchestrationProcessingStats {
            task_requests_processed: 0,
            step_results_processed: 0,
            tasks_finalized: 0,
            tasks_ready_processed: 0, // TAS-43: Task readiness events processed
            processing_errors: 0,
            current_queue_sizes: HashMap::new(),
        }));

        info!(
            channel = %channel_monitor.channel_name(),
            buffer_size = buffer_size,
            "Creating OrchestrationProcessor with channel monitoring"
        );

        let processor = Self {
            context,
            actors,
            pgmq_client,
            health_caches,
            command_rx: Some(command_rx),
            task_handle: None,
            stats,
            channel_monitor,
        };

        (processor, command_tx)
    }

    /// Start the command processing loop
    pub async fn start(&mut self) -> TaskerResult<()> {
        let context = self.context.clone();
        let stats = self.stats.clone();
        let pgmq_client = self.pgmq_client.clone();
        let health_caches = self.health_caches.clone(); // TAS-75: Clone health caches for spawned task
        let channel_monitor = self.channel_monitor.clone(); // TAS-51: Clone monitor for spawned task
        let mut command_rx = self.command_rx.take().ok_or_else(|| {
            TaskerError::OrchestrationError("Processor already started".to_string())
        })?;

        let actors = self.actors.clone();
        let handle = tokio::spawn(async move {
            let handler = OrchestrationProcessorCommandHandler::new(
                context,
                actors,
                stats,
                pgmq_client,
                health_caches, // TAS-75: Pass health caches
            );
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

#[derive(Debug)]
pub struct OrchestrationProcessorCommandHandler {
    #[expect(
        dead_code,
        reason = "SystemContext available for future command handler operations"
    )]
    context: Arc<SystemContext>,
    actors: Arc<ActorRegistry>, // TAS-46: Actor registry for message-based coordination
    stats: Arc<std::sync::RwLock<OrchestrationProcessingStats>>,
    pgmq_client: Arc<UnifiedPgmqClient>,
    health_caches: HealthStatusCaches, // TAS-75: Cached health status for non-blocking health checks
    step_result_hydrator: StepResultHydrator, // TAS-46 Phase 4: Hydrates step results from messages
    task_request_hydrator: TaskRequestHydrator, // TAS-46 Phase 4: Hydrates task requests from messages
    finalization_hydrator: FinalizationHydrator, // TAS-46 Phase 4: Hydrates finalization requests from messages
}

impl OrchestrationProcessorCommandHandler {
    pub fn new(
        context: Arc<SystemContext>,
        actors: Arc<ActorRegistry>,
        stats: Arc<std::sync::RwLock<OrchestrationProcessingStats>>,
        pgmq_client: Arc<UnifiedPgmqClient>,
        health_caches: HealthStatusCaches,
    ) -> Self {
        // TAS-46 Phase 4: Initialize hydrators for message transformation
        let step_result_hydrator = StepResultHydrator::new(context.clone());
        let task_request_hydrator = TaskRequestHydrator::new();
        let finalization_hydrator = FinalizationHydrator::new();

        Self {
            context,
            actors,
            stats,
            pgmq_client,
            health_caches,
            step_result_hydrator,
            task_request_hydrator,
            finalization_hydrator,
        }
    }

    /// Process individual commands (no polling) with sophisticated delegation
    pub async fn process_command(&self, command: OrchestrationCommand) {
        match command {
            OrchestrationCommand::InitializeTask { request, resp } => {
                let result = self.handle_initialize_task(request).await;
                if result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .task_requests_processed += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(result);
            }
            OrchestrationCommand::ProcessStepResult { result, resp } => {
                let process_result = self.handle_process_step_result(result).await;
                if process_result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .step_results_processed += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(process_result);
            }
            OrchestrationCommand::FinalizeTask { task_uuid, resp } => {
                let result = self.handle_finalize_task(task_uuid).await;
                if result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .tasks_finalized += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(result);
            }
            OrchestrationCommand::ProcessStepResultFromMessageEvent {
                message_event,
                resp,
            } => {
                let result = self
                    .handle_step_result_from_message_event(message_event)
                    .await;
                if result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .step_results_processed += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(result);
            }
            OrchestrationCommand::InitializeTaskFromMessageEvent {
                message_event,
                resp,
            } => {
                let result = self
                    .handle_task_initialize_from_message_event(message_event)
                    .await;
                if result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .task_requests_processed += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(result);
            }
            OrchestrationCommand::FinalizeTaskFromMessageEvent {
                message_event,
                resp,
            } => {
                let result = self
                    .handle_task_finalize_from_message_event(message_event)
                    .await;
                if result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .tasks_finalized += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(result);
            }
            OrchestrationCommand::ProcessStepResultFromMessage {
                queue_name,
                message,
                resp,
            } => {
                debug!(
                    msg_id = message.msg_id,
                    queue = %queue_name,
                    "Starting ProcessStepResultFromMessage"
                );

                let result = self
                    .handle_step_result_from_message(&queue_name, message.clone())
                    .await;

                match &result {
                    Ok(step_result) => {
                        info!(
                            msg_id = message.msg_id,
                            queue = %queue_name,
                            result = ?step_result,
                            "ProcessStepResultFromMessage succeeded"
                        );
                        self.stats
                            .write()
                            .unwrap_or_else(|p| p.into_inner())
                            .step_results_processed += 1;
                    }
                    Err(error) => {
                        error!(
                            msg_id = message.msg_id,
                            queue = %queue_name,
                            error = %error,
                            "ProcessStepResultFromMessage failed"
                        );
                        self.stats
                            .write()
                            .unwrap_or_else(|p| p.into_inner())
                            .processing_errors += 1;
                    }
                }
                let _ = resp.send(result);
            }
            OrchestrationCommand::InitializeTaskFromMessage {
                queue_name,
                message,
                resp,
            } => {
                let result = self
                    .handle_task_initialize_from_message(&queue_name, message)
                    .await;
                if result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .task_requests_processed += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(result);
            }
            OrchestrationCommand::FinalizeTaskFromMessage {
                queue_name,
                message,
                resp,
            } => {
                let result = self
                    .handle_task_finalize_from_message(&queue_name, message)
                    .await;
                if result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .tasks_finalized += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(result);
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
                let result = self
                    .handle_process_task_readiness(
                        task_uuid,
                        namespace,
                        priority,
                        ready_steps,
                        triggered_by,
                    )
                    .await;
                if result.is_ok() {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .tasks_ready_processed += 1;
                } else {
                    self.stats
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .processing_errors += 1;
                }
                let _ = resp.send(result);
            }
            OrchestrationCommand::GetProcessingStats { resp } => {
                let stats_copy = self.stats.read().unwrap_or_else(|p| p.into_inner()).clone();
                let _ = resp.send(Ok(stats_copy));
            }
            OrchestrationCommand::HealthCheck { resp } => {
                let result = self.handle_health_check().await;
                let _ = resp.send(result);
            }
            OrchestrationCommand::Shutdown { resp } => {
                let _ = resp.send(Ok(())); // Exit the processing loop
            }
        }
    }

    /// Handle task initialization using TaskRequestActor directly (TAS-46)
    async fn handle_initialize_task(
        &self,
        request: TaskRequestMessage,
    ) -> TaskerResult<TaskInitializeResult> {
        // TAS-46: Direct actor-based task initialization
        let msg = ProcessTaskRequestMessage { request };
        let task_uuid = self.actors.task_request_actor.handle(msg).await?;

        Ok(TaskInitializeResult::Success {
            task_uuid,
            message: "Task initialized successfully".to_string(),
        })
    }

    /// Handle step result processing using ResultProcessorActor directly (TAS-46)
    async fn handle_process_step_result(
        &self,
        step_result: StepExecutionResult,
    ) -> TaskerResult<StepProcessResult> {
        // TAS-46: Direct actor-based step result processing
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
            Err(e) => {
                // Handle different status cases based on original result status
                match step_result.status.as_str() {
                    "failed" => Ok(StepProcessResult::Failed {
                        error: format!("{e}"),
                    }),
                    "skipped" => Ok(StepProcessResult::Skipped {
                        reason: format!("{e}"),
                    }),
                    _ => Err(TaskerError::OrchestrationError(format!("{e}"))),
                }
            }
        }
    }

    async fn handle_finalize_task(&self, task_uuid: Uuid) -> TaskerResult<TaskFinalizationResult> {
        // TAS-46: Direct actor-based task finalization
        let msg = FinalizeTaskMessage { task_uuid };

        let result = self.actors.task_finalizer_actor.handle(msg).await?;

        Ok(TaskFinalizationResult::Success {
            task_uuid: result.task_uuid,
            final_status: format!("{:?}", result.action),
            completion_time: Some(chrono::Utc::now()),
        })
    }

    /// Handle step result processing from message event - SimpleStepMessage approach with database hydration
    async fn handle_step_result_from_message_event(
        &self,
        message_event: MessageReadyEvent,
    ) -> TaskerResult<StepProcessResult> {
        // Read the specific message by ID (the correct approach for event-driven processing)
        let message = self
            .pgmq_client
            .read_specific_message::<serde_json::Value>(
                &message_event.queue_name,
                message_event.msg_id,
                30, // visibility timeout
            )
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!(
                    "Failed to read specific step result message {} from queue {}: {e}",
                    message_event.msg_id, message_event.queue_name
                ))
            })?
            .ok_or_else(|| {
                TaskerError::ValidationError(format!(
                    "Step result message {} not found in queue {}",
                    message_event.msg_id, message_event.queue_name
                ))
            })?;

        self.handle_step_result_from_message(&message_event.queue_name, message)
            .await
    }

    /// Handle step result processing from message event - SimpleStepMessage approach with database hydration
    async fn handle_step_result_from_message(
        &self,
        queue_name: &str,
        message: PgmqMessage,
    ) -> TaskerResult<StepProcessResult> {
        debug!(
            msg_id = message.msg_id,
            queue = %queue_name,
            "STEP_RESULT_HANDLER: Processing step result message via hydrator"
        );

        // TAS-46 Phase 4: Hydrate full StepExecutionResult from lightweight message
        let step_execution_result = self
            .step_result_hydrator
            .hydrate_from_message(&message)
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
                    msg_id = message.msg_id,
                    step_uuid = %step_execution_result.step_uuid,
                    result = ?step_result,
                    "STEP_RESULT_HANDLER: Result processing succeeded"
                );

                // Delete the message only if processing was successful
                debug!(
                    msg_id = message.msg_id,
                    queue = %queue_name,
                    "STEP_RESULT_HANDLER: Deleting successfully processed message"
                );

                match self
                    .pgmq_client
                    .delete_message(queue_name, message.msg_id)
                    .await
                {
                    Ok(_) => {
                        info!(
                            msg_id = message.msg_id,
                            queue = %queue_name,
                            "STEP_RESULT_HANDLER: Successfully deleted processed message"
                        );
                    }
                    Err(e) => {
                        warn!(
                            msg_id = message.msg_id,
                            queue = %queue_name,
                            error = %e,
                            "STEP_RESULT_HANDLER: Failed to delete processed step result message (will be reprocessed)"
                        );
                    }
                }
            }
            Err(error) => {
                error!(
                    msg_id = message.msg_id,
                    step_uuid = %step_execution_result.step_uuid,
                    error = %error,
                    "STEP_RESULT_HANDLER: Result processing failed (message will remain for retry)"
                );
            }
        }

        result
    }

    /// Handle task initialization from message event - delegates full message lifecycle to worker
    async fn handle_task_initialize_from_message_event(
        &self,
        message_event: MessageReadyEvent,
    ) -> TaskerResult<TaskInitializeResult> {
        tracing::debug!(
            msg_id = message_event.msg_id,
            queue = %message_event.queue_name,
            "Processing task initialization from message event"
        );

        // Read the specific message by ID (the correct approach for event-driven processing)
        let message = self
            .pgmq_client
            .read_specific_message::<serde_json::Value>(
                &message_event.queue_name,
                message_event.msg_id,
                30, // visibility timeout
            )
            .await
            .map_err(|e| {
                tracing::error!(
                    msg_id = message_event.msg_id,
                    queue = %message_event.queue_name,
                    error = %e,
                    "Failed to read specific task request message from queue"
                );
                TaskerError::MessagingError(format!(
                    "Failed to read specific task request message {} from queue {}: {e}",
                    message_event.msg_id, message_event.queue_name
                ))
            })?
            .ok_or_else(|| {
                tracing::error!(
                    msg_id = message_event.msg_id,
                    queue = %message_event.queue_name,
                    "Task request message not found in queue"
                );
                TaskerError::ValidationError(format!(
                    "Task request message {} not found in queue {}",
                    message_event.msg_id, message_event.queue_name
                ))
            })?;

        tracing::debug!(
            msg_id = message_event.msg_id,
            queue = %message_event.queue_name,
            "Successfully read specific task request message"
        );

        self.handle_task_initialize_from_message(&message_event.queue_name, message)
            .await
    }

    /// Handle task initialization from message event - delegates full message lifecycle to worker
    async fn handle_task_initialize_from_message(
        &self,
        queue_name: &str,
        message: PgmqMessage,
    ) -> TaskerResult<TaskInitializeResult> {
        debug!(
            msg_id = message.msg_id,
            queue = %queue_name,
            "TASK_INIT_HANDLER: Processing task initialization via hydrator"
        );

        // TAS-46 Phase 4: Hydrate TaskRequestMessage from PGMQ message
        let task_request = self
            .task_request_hydrator
            .hydrate_from_message(&message)
            .await?;

        debug!(
            msg_id = message.msg_id,
            namespace = %task_request.task_request.namespace,
            handler = %task_request.task_request.name,
            "TASK_INIT_HANDLER: Hydration complete, delegating to task initialization"
        );

        // Delegate to existing task initialization logic
        let result = self.handle_initialize_task(task_request).await;

        match &result {
            Ok(_) => {
                tracing::debug!(
                    msg_id = message.msg_id,
                    queue = %queue_name,
                    "Task initialization successful, deleting message"
                );
                // Delete the message only if processing was successful
                match self
                    .pgmq_client
                    .delete_message(queue_name, message.msg_id)
                    .await
                {
                    Ok(_) => {
                        tracing::debug!(
                            msg_id = message.msg_id,
                            queue = %queue_name,
                            "Successfully deleted processed task request message"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            msg_id = message.msg_id,
                            queue = %queue_name,
                            error = %e,
                            "Failed to delete processed task request message"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    msg_id = message.msg_id,
                    queue = %queue_name,
                    error = %e,
                    "Task initialization failed, keeping message in queue"
                );
            }
        }

        result
    }

    /// Handle task finalization from message event - delegates full message lifecycle to worker
    async fn handle_task_finalize_from_message_event(
        &self,
        message_event: MessageReadyEvent,
    ) -> TaskerResult<TaskFinalizationResult> {
        // Read the specific message by ID (the correct approach for event-driven processing)
        let message = self
            .pgmq_client
            .read_specific_message::<serde_json::Value>(
                &message_event.queue_name,
                message_event.msg_id,
                30, // visibility timeout
            )
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!(
                    "Failed to read specific finalization message {} from queue {}: {e}",
                    message_event.msg_id, message_event.queue_name
                ))
            })?
            .ok_or_else(|| {
                TaskerError::OrchestrationError(format!(
                    "Finalization message {} not found in queue {}",
                    message_event.msg_id, message_event.queue_name
                ))
            })?;

        self.handle_task_finalize_from_message(&message_event.queue_name, message)
            .await
    }

    /// Handle task finalization from message event - delegates full message lifecycle to worker
    async fn handle_task_finalize_from_message(
        &self,
        queue_name: &str,
        message: PgmqMessage,
    ) -> TaskerResult<TaskFinalizationResult> {
        debug!(
            msg_id = message.msg_id,
            queue = %queue_name,
            "FINALIZATION_HANDLER: Processing finalization via hydrator"
        );

        // TAS-46 Phase 4: Hydrate task_uuid from PGMQ message
        let task_uuid = self
            .finalization_hydrator
            .hydrate_from_message(&message)
            .await?;

        debug!(
            msg_id = message.msg_id,
            task_uuid = %task_uuid,
            "FINALIZATION_HANDLER: Hydration complete, delegating to task finalization"
        );

        // Delegate to existing task finalization logic
        let result = self.handle_finalize_task(task_uuid).await;

        // Delete the message only if processing was successful
        if matches!(result, Ok(TaskFinalizationResult::Success { .. })) {
            match self
                .pgmq_client
                .delete_message(queue_name, message.msg_id)
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    tracing::warn!(
                        msg_id = message.msg_id,
                        queue = %queue_name,
                        task_uuid = %task_uuid,
                        error = %e,
                        "Failed to delete processed finalization message"
                    );
                }
            }
        } else {
            // Get error details from result
            let error_msg = match &result {
                Ok(TaskFinalizationResult::NotClaimed { reason, .. }) => {
                    format!("Not claimed: {}", reason)
                }
                Ok(TaskFinalizationResult::Failed { error }) => format!("Failed: {}", error),
                Err(e) => format!("Error: {}", e),
                _ => "Unknown finalization result".to_string(),
            };

            tracing::warn!(
                msg_id = message.msg_id,
                queue = %queue_name,
                task_uuid = %task_uuid,
                result = %error_msg,
                "Task finalization was not successful - keeping message in queue"
            );
        }

        result
    }

    /// Handle task readiness processing - converts readiness event to ClaimedTask and delegates to step enqueueing
    ///
    /// This is the core TAS-43 implementation that:
    /// 1. Creates a synthetic ClaimedTask from the task readiness parameters
    /// 2. Delegates to task_claim_step_enqueuer.process_claimed_task() for atomic step enqueueing
    /// 3. Returns processing metrics for observability
    async fn handle_process_task_readiness(
        &self,
        task_uuid: Uuid,
        namespace: String,
        priority: i32,
        ready_steps: i32,
        triggered_by: String,
    ) -> TaskerResult<TaskReadinessResult> {
        let start_time = std::time::Instant::now();

        tracing::debug!(
            task_uuid = %task_uuid,
            namespace = %namespace,
            priority = priority,
            ready_steps = ready_steps,
            triggered_by = %triggered_by,
            "Processing task readiness event via command pattern"
        );

        // TAS-46 Phase 3: Use StepEnqueuerActor for batch processing
        // This uses the actor-based approach for atomic task claiming and step enqueueing
        let msg = ProcessBatchMessage;
        let process_result = match self.actors.step_enqueuer_actor.handle(msg).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(
                    task_uuid = %task_uuid,
                    namespace = %namespace,
                    error = %e,
                    "Failed to process task readiness via StepEnqueuerActor"
                );
                return Err(e);
            }
        };

        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        // Log the successful processing
        tracing::info!(
            task_uuid = %task_uuid,
            namespace = %namespace,
            priority = priority,
            ready_steps = ready_steps,
            tasks_processed = process_result.tasks_processed,
            tasks_failed = process_result.tasks_failed,
            processing_time_ms = processing_time_ms,
            triggered_by = %triggered_by,
            "Task readiness processed successfully via TaskClaimStepEnqueuer"
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

    /// Handle health check using cached health status (TAS-75)
    ///
    /// Reads from `HealthStatusCaches` for non-blocking health checks.
    /// Uses fail-open semantics: unevaluated status returns healthy to avoid
    /// blocking requests during startup or when health evaluation is disabled.
    async fn handle_health_check(&self) -> TaskerResult<SystemHealth> {
        // TAS-75: Read from cached health status
        let db_status = self.health_caches.get_db_status().await;
        let channel_status = self.health_caches.get_channel_status().await;
        let queue_status = self.health_caches.get_queue_status().await;
        let backpressure = self.health_caches.get_backpressure().await;

        // Determine if we've ever evaluated health
        let health_evaluated =
            db_status.evaluated || channel_status.evaluated || queue_status.tier.is_evaluated();

        // Determine database connectivity from cached status
        // Fail-open: if not evaluated, assume connected
        let database_connected = if db_status.evaluated {
            db_status.is_connected && !db_status.circuit_breaker_open
        } else {
            true // Fail-open during startup
        };

        // Determine queue health from cached status
        // Critical/Overflow tiers indicate unhealthy queues
        let message_queues_healthy = !queue_status.tier.is_critical();

        // Calculate overall status
        let status = if !health_evaluated {
            "unknown".to_string() // Not yet evaluated
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
            // TAS-142: Return actual actor count as active processors
            active_processors: self.actors.actor_count() as u32,

            // TAS-75: Enhanced health fields from cached status
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
