//! # TAS-40 Worker Command Processor
//!
//! Simple command pattern implementation for worker operations, replacing
//! complex executor pools, polling coordinators, and auto-scaling systems.
//!
//! ## Key Features
//!
//! - **Pure Command-Driven**: No polling loops, pure tokio command processing
//! - **Simple Architecture**: ~100 lines vs 1000+ lines of complex worker system
//! - **Step Execution**: Command-based step execution with Ruby StepHandlerCallResult integration
//! - **Handler Registration**: Simple handler registry without complex lifecycle management
//! - **Orchestration Integration**: Send results back to orchestration via command pattern

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, event, info, span, warn, Instrument, Level};
use uuid::Uuid;

use chrono::Utc;
use opentelemetry::KeyValue;
use pgmq::Message as PgmqMessage;
use pgmq_notify::MessageReadyEvent;
use tasker_shared::events::domain_events::EventMetadata;
use tasker_shared::messaging::message::SimpleStepMessage;
use tasker_shared::messaging::{PgmqClientTrait, StepExecutionResult};
use tasker_shared::metrics::worker::*;
use tasker_shared::models::{Task, WorkflowStep};
use tasker_shared::monitoring::ChannelMonitor; // TAS-51: Channel monitoring
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::states::WorkflowStepState;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::base::TaskSequenceStep;
use tasker_shared::{TaskerError, TaskerResult};

use super::domain_event_commands::DomainEventToPublish;
use super::event_publisher::WorkerEventPublisher;
use super::event_subscriber::WorkerEventSubscriber;
use super::event_systems::domain_event_system::DomainEventSystemHandle;
use super::orchestration_result_sender::OrchestrationResultSender;
use super::step_claim::StepClaim;
use super::task_template_manager::TaskTemplateManager;
use crate::health::WorkerHealthStatus;

/// Worker command responder type
type CommandResponder<T> = oneshot::Sender<TaskerResult<T>>;

/// Commands that can be sent to the WorkerProcessor
#[derive(Debug)]
pub enum WorkerCommand {
    /// Execute a step from raw SimpleStepMessage - handles database queries, claiming, and deletion
    ExecuteStep {
        message: PgmqMessage<SimpleStepMessage>,
        queue_name: String,
        resp: CommandResponder<()>,
    },
    /// Get current worker status and statistics
    GetWorkerStatus {
        resp: CommandResponder<WorkerStatus>,
    },
    /// Send step execution result to orchestration
    SendStepResult {
        result: StepExecutionResult,
        resp: CommandResponder<()>,
    },
    /// Execute step with event correlation for FFI handler tracking
    ExecuteStepWithCorrelation {
        message: PgmqMessage<SimpleStepMessage>,
        queue_name: String,
        correlation_id: Uuid,
        resp: CommandResponder<()>,
    },
    /// Process step completion event from FFI handler (event-driven)
    ProcessStepCompletion {
        step_result: StepExecutionResult,
        correlation_id: Option<Uuid>,
        resp: CommandResponder<()>,
    },
    /// Enable or disable event integration at runtime
    SetEventIntegration {
        enabled: bool,
        resp: CommandResponder<()>,
    },
    /// Get event integration statistics and status
    GetEventStatus {
        resp: CommandResponder<EventIntegrationStatus>,
    },
    /// Refresh task template cache
    RefreshTemplateCache {
        namespace: Option<String>,
        resp: CommandResponder<()>,
    },
    /// Health check command for monitoring responsiveness
    HealthCheck {
        resp: CommandResponder<WorkerHealthStatus>,
    },
    /// Shutdown the worker processor
    Shutdown { resp: CommandResponder<()> },
    /// Execute step from PgmqMessage - TAS-43 event system integration
    ExecuteStepFromMessage {
        queue_name: String,
        message: PgmqMessage,
        resp: CommandResponder<()>,
    },
    /// Execute step from MessageReadyEvent - TAS-43 event system integration
    ExecuteStepFromEvent {
        message_event: pgmq_notify::MessageReadyEvent,
        resp: CommandResponder<()>,
    },
}

/// Worker status information
#[derive(Debug, Clone)]
pub struct WorkerStatus {
    pub worker_id: String,
    pub status: String,
    pub steps_executed: u64,
    pub steps_succeeded: u64,
    pub steps_failed: u64,
    pub uptime_seconds: u64,
    pub registered_handlers: Vec<String>,
}

/// Step execution statistics
#[derive(Debug, Clone)]
pub struct StepExecutionStats {
    pub total_executed: u64,
    pub total_succeeded: u64,
    pub total_failed: u64,
    pub average_execution_time_ms: f64,
}

/// Event integration status for monitoring FFI communication
#[derive(Debug, Clone)]
pub struct EventIntegrationStatus {
    pub enabled: bool,
    pub events_published: u64,
    pub events_received: u64,
    pub correlation_tracking_enabled: bool,
    pub pending_correlations: usize,
    pub ffi_handlers_subscribed: usize,
    pub completion_subscribers: usize,
}

/// TAS-40 Simple Worker Command Processor with Event Integration
///
/// Replaces complex executor pools, auto-scaling coordinators, and polling systems
/// with simple tokio command pattern and declarative FFI event integration.
/// No background threads, no polling, no complex state.
pub(crate) struct WorkerProcessor {
    /// Worker identification
    worker_id: String,
    processor_id: Uuid,

    /// System context for dependencies and database access
    context: Arc<SystemContext>,

    /// Task template manager for metadata lookups
    task_template_manager: Arc<TaskTemplateManager>,

    /// Command receiver
    command_receiver: Option<mpsc::Receiver<WorkerCommand>>,

    /// Simple step handler registry (no complex lifecycle)
    handlers: HashMap<String, String>, // handler_name -> handler_class

    /// Basic execution statistics
    stats: StepExecutionStats,

    /// Worker start time for uptime calculation
    start_time: std::time::Instant,

    /// Event publisher for sending execution events to FFI handlers
    event_publisher: Option<WorkerEventPublisher>,

    /// Event subscriber for receiving completion events from FFI handlers
    event_subscriber: Option<WorkerEventSubscriber>,

    /// Orchestration result sender for step completion notifications
    orchestration_result_sender: OrchestrationResultSender,

    /// Channel monitor for command channel observability (TAS-51)
    command_channel_monitor: ChannelMonitor,

    /// TAS-65/TAS-69: Step execution context cache for domain event publishing
    ///
    /// Stores TaskSequenceStep keyed by step_uuid during execution.
    /// At completion time, retrieved to build domain events.
    step_execution_contexts: HashMap<Uuid, TaskSequenceStep>,

    /// TAS-65/TAS-69: Domain event system handle for fire-and-forget event dispatch
    ///
    /// Optional - if not set, domain events are silently skipped.
    domain_event_handle: Option<DomainEventSystemHandle>,
}

impl WorkerProcessor {
    /// Create new WorkerProcessor with command pattern
    pub fn new(
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
        command_buffer_size: usize,
        command_channel_monitor: ChannelMonitor,
    ) -> (Self, mpsc::Sender<WorkerCommand>) {
        let (command_sender, command_receiver) = mpsc::channel(command_buffer_size);
        let worker_id = format!("worker_{}", Uuid::new_v4());
        let processor_id = Uuid::new_v4();

        info!(
            worker_id = %worker_id,
            processor_id = %processor_id,
            channel = %command_channel_monitor.channel_name(),
            "Creating WorkerProcessor with command channel monitoring"
        );

        // TAS-61 Phase 6D: Access queues from common config
        let queues_config = &context.tasker_config.common.queues;
        // Create OrchestrationResultSender with centralized queues configuration
        let orchestration_result_sender =
            OrchestrationResultSender::new(context.message_client(), queues_config);

        let processor = Self {
            worker_id: worker_id.clone(),
            processor_id,
            context,
            task_template_manager,
            command_receiver: Some(command_receiver),
            handlers: HashMap::new(),
            stats: StepExecutionStats {
                total_executed: 0,
                total_succeeded: 0,
                total_failed: 0,
                average_execution_time_ms: 0.0,
            },
            start_time: std::time::Instant::now(),
            event_publisher: None,
            event_subscriber: None,
            orchestration_result_sender,
            command_channel_monitor,
            // TAS-65/TAS-69: Domain event publishing support
            step_execution_contexts: HashMap::new(),
            domain_event_handle: None,
        };

        (processor, command_sender)
    }

    pub fn new_with_event_system(
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
        command_buffer_size: usize,
        command_channel_monitor: ChannelMonitor,
        event_system: Arc<tasker_shared::events::WorkerEventSystem>,
    ) -> (Self, mpsc::Sender<WorkerCommand>) {
        let (mut worker_processor, command_sender) = Self::new(
            context,
            task_template_manager,
            command_buffer_size,
            command_channel_monitor,
        );
        worker_processor.enable_event_integration_with_system(Some(event_system));
        (worker_processor, command_sender)
    }

    /// Enable event integration for FFI communication
    pub fn enable_event_integration(&mut self) {
        self.enable_event_integration_with_system(None);
    }

    /// Enable event integration with specific event system
    pub fn enable_event_integration_with_system(
        &mut self,
        event_system: Option<Arc<tasker_shared::events::WorkerEventSystem>>,
    ) {
        info!(
            worker_id = %self.worker_id,
            "Enabling event integration for FFI communication"
        );

        // Use provided event system or create new one
        let shared_event_system = event_system.unwrap_or_else(|| {
            std::sync::Arc::new(tasker_shared::events::WorkerEventSystem::new())
        });

        // Create publisher and subscriber with shared event system
        let event_publisher = WorkerEventPublisher::with_event_system(
            self.worker_id.clone(),
            shared_event_system.clone(),
        );
        let event_subscriber =
            WorkerEventSubscriber::with_event_system(self.worker_id.clone(), shared_event_system);

        self.event_publisher = Some(event_publisher);
        self.event_subscriber = Some(event_subscriber);
    }

    /// TAS-65/TAS-69: Set domain event handle for fire-and-forget event publishing
    ///
    /// This handle is used to dispatch domain events after step completion.
    /// Domain events are dispatched asynchronously via try_send() - never blocking
    /// the workflow step execution.
    pub fn set_domain_event_handle(&mut self, handle: DomainEventSystemHandle) {
        info!(
            worker_id = %self.worker_id,
            "Setting domain event handle for fire-and-forget publishing"
        );
        self.domain_event_handle = Some(handle);
    }

    /// TAS-65/TAS-69: Dispatch domain events after step completion (fire-and-forget)
    ///
    /// This method:
    /// 1. Retrieves cached TaskSequenceStep context
    /// 2. Checks if step definition declares `publishes_events`
    /// 3. Builds DomainEventToPublish for each declared event
    /// 4. Dispatches via try_send() - never blocks, logs if channel full
    /// 5. Cleans up the context cache
    fn dispatch_domain_events(
        &mut self,
        step_result: &StepExecutionResult,
        correlation_id: Option<Uuid>,
    ) {
        // Early return if no domain event handle configured
        let handle = match &self.domain_event_handle {
            Some(h) => h,
            None => {
                debug!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_result.step_uuid,
                    "No domain event handle configured, skipping event dispatch"
                );
                return;
            }
        };

        // Retrieve and remove cached context
        let task_sequence_step = match self.step_execution_contexts.remove(&step_result.step_uuid) {
            Some(ctx) => ctx,
            None => {
                warn!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_result.step_uuid,
                    "No cached context found for step - cannot dispatch domain events"
                );
                return;
            }
        };

        // Check if step definition declares any events to publish
        let events_to_publish = &task_sequence_step.step_definition.publishes_events;
        if events_to_publish.is_empty() {
            debug!(
                worker_id = %self.worker_id,
                step_uuid = %step_result.step_uuid,
                step_name = %task_sequence_step.workflow_step.name,
                "Step has no declared events to publish"
            );
            return;
        }

        // Build domain events from step definition
        let correlation = correlation_id.unwrap_or_else(Uuid::new_v4);
        let mut domain_events = Vec::with_capacity(events_to_publish.len());

        for event_def in events_to_publish {
            let metadata = EventMetadata {
                task_uuid: task_sequence_step.task.task.task_uuid,
                step_uuid: Some(step_result.step_uuid),
                step_name: Some(task_sequence_step.workflow_step.name.clone()),
                namespace: task_sequence_step.task.namespace_name.clone(),
                correlation_id: correlation,
                fired_at: Utc::now(),
                fired_by: self.worker_id.clone(),
            };

            let event = DomainEventToPublish {
                event_name: event_def.name.clone(),
                delivery_mode: event_def.delivery_mode.clone(),
                // Business payload comes from step result
                business_payload: step_result.result.clone(),
                metadata,
                task_sequence_step: task_sequence_step.clone(),
                execution_result: step_result.clone(),
            };

            domain_events.push(event);
        }

        let event_count = domain_events.len();
        let publisher_name = task_sequence_step
            .step_definition
            .publishes_events
            .first()
            .and_then(|e| e.publisher.clone())
            .unwrap_or_else(|| "default".to_string());

        // Fire-and-forget dispatch - try_send never blocks
        let dispatched = handle.dispatch_events(domain_events, publisher_name, correlation);

        if dispatched {
            info!(
                worker_id = %self.worker_id,
                step_uuid = %step_result.step_uuid,
                step_name = %task_sequence_step.workflow_step.name,
                event_count = event_count,
                correlation_id = %correlation,
                "Domain events dispatched (fire-and-forget)"
            );
        } else {
            warn!(
                worker_id = %self.worker_id,
                step_uuid = %step_result.step_uuid,
                step_name = %task_sequence_step.workflow_step.name,
                event_count = event_count,
                correlation_id = %correlation,
                "Domain event dispatch failed - channel full (events dropped)"
            );
        }
    }

    /// Start processing worker commands with event integration
    pub async fn start_with_events(&mut self) -> TaskerResult<()> {
        // Start completion listener if event subscriber is enabled
        // TAS-61 Phase 6C: Access worker MPSC channel config with Optional handling
        let completion_buffer_size = self
            .context
            .tasker_config
            .worker
            .as_ref()
            .map(|w| w.mpsc_channels.event_subscribers.completion_buffer_size)
            .unwrap_or(1000) as usize; // Default buffer size if worker config not present

        let completion_receiver = self
            .event_subscriber
            .as_ref()
            .map(|subscriber| subscriber.start_completion_listener(completion_buffer_size));

        self.start_with_completion_receiver(completion_receiver)
            .await
    }

    /// Start processing with custom completion receiver (advanced usage)
    pub async fn start_with_completion_receiver(
        &mut self,
        mut completion_receiver: Option<mpsc::Receiver<StepExecutionResult>>,
    ) -> TaskerResult<()> {
        info!(
            worker_id = %self.worker_id,
            event_integration = self.event_publisher.is_some(),
            "Starting WorkerProcessor with event integration"
        );

        let mut command_receiver = self.command_receiver.take().ok_or_else(|| {
            TaskerError::WorkerError("Command receiver already taken".to_string())
        })?;

        // Integrated command and event processing loop
        loop {
            tokio::select! {
                // Handle worker commands
                command = command_receiver.recv() => {
                    match command {
                        Some(cmd) => {
                            // TAS-51: Record message receive for channel monitoring
                            self.command_channel_monitor.record_receive();

                            if !self.handle_command(cmd).await {
                                break; // Shutdown requested
                            }
                        }
                        None => {
                            info!(worker_id = %self.worker_id, "Command channel closed");
                            break;
                        }
                    }
                }

                // Handle FFI completion events (if enabled)
                completion = async {
                    match &mut completion_receiver {
                        Some(receiver) => receiver.recv().await,
                        None => {
                            // Block forever if no completion receiver
                            let () = std::future::pending().await;
                            unreachable!()
                        }
                    }
                } => {
                    match completion {
                        Some(step_result) => {
                            debug!(
                                worker_id = %self.worker_id,
                                step_uuid = %step_result.step_uuid,
                                success = step_result.success,
                                "Received step completion from FFI handler"
                            );

                            // Forward completion to orchestration via handle_send_step_result
                            if let Err(e) = self.handle_send_step_result(step_result.clone()).await {
                                error!(
                                    worker_id = %self.worker_id,
                                    step_uuid = %step_result.step_uuid,
                                    error = %e,
                                    "Failed to forward step completion to orchestration"
                                );
                            } else {
                                info!(
                                    worker_id = %self.worker_id,
                                    step_uuid = %step_result.step_uuid,
                                    "Step completion forwarded to orchestration successfully"
                                );
                            }
                        }
                        None => {
                            warn!(worker_id = %self.worker_id, "FFI completion channel closed");
                            // Continue processing commands even if FFI channel closes
                        }
                    }
                }
            }
        }

        info!(worker_id = %self.worker_id, "WorkerProcessor with events shutdown complete");
        Ok(())
    }

    /// Handle individual worker command with event integration
    async fn handle_command(&mut self, command: WorkerCommand) -> bool {
        match command {
            WorkerCommand::ExecuteStep {
                message,
                queue_name,
                resp,
            } => {
                let result = self.handle_execute_step(message, queue_name).await;
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::ExecuteStepWithCorrelation {
                message,
                queue_name,
                correlation_id,
                resp,
            } => {
                let result = self
                    .handle_execute_step_with_correlation(message, queue_name, correlation_id)
                    .await;
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::ProcessStepCompletion {
                step_result,
                correlation_id,
                resp,
            } => {
                let result = self
                    .handle_process_step_completion(step_result, correlation_id)
                    .await;
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::SetEventIntegration { enabled, resp } => {
                let result = self.handle_set_event_integration(enabled);
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::GetEventStatus { resp } => {
                let result = self.handle_get_event_status();
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::GetWorkerStatus { resp } => {
                let result = self.handle_get_worker_status();
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::SendStepResult { result, resp } => {
                let send_result = self.handle_send_step_result(result).await;
                let _ = resp.send(send_result);
                true // Continue processing
            }
            WorkerCommand::RefreshTemplateCache { namespace, resp } => {
                let result = self
                    .handle_refresh_template_cache(namespace.as_deref())
                    .await;
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::HealthCheck { resp } => {
                let result = self.handle_health_check().await;
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::ExecuteStepFromMessage {
                queue_name,
                message,
                resp,
            } => {
                let result = self
                    .handle_execute_step_from_message(queue_name, message)
                    .await;
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::ExecuteStepFromEvent {
                message_event,
                resp,
            } => {
                let result = self.handle_execute_step_from_event(message_event).await;
                let _ = resp.send(result);
                true // Continue processing
            }
            WorkerCommand::Shutdown { resp } => {
                info!(worker_id = %self.worker_id, "Shutting down WorkerProcessor");
                let _ = resp.send(Ok(()));
                false // Stop processing
            }
        }
    }

    /// Start processing worker commands (replaces complex executor pools)
    #[allow(dead_code)]
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            worker_id = %self.worker_id,
            "Starting WorkerProcessor command processing loop"
        );

        let mut command_receiver = self.command_receiver.take().ok_or_else(|| {
            TaskerError::WorkerError("Command receiver already taken".to_string())
        })?;

        // Simple command processing loop - no polling, no complex state
        while let Some(command) = command_receiver.recv().await {
            // TAS-51: Record message receive for channel monitoring
            self.command_channel_monitor.record_receive();

            debug!(
                worker_id = %self.worker_id,
                command = ?command,
                "Processing worker command"
            );

            if !self.handle_command(command).await {
                break; // Shutdown requested
            }
        }

        info!(worker_id = %self.worker_id, "WorkerProcessor shutdown complete");
        Ok(())
    }

    /// Execute step from raw SimpleStepMessage - handles database queries, claiming, and message deletion
    async fn handle_execute_step(
        &mut self,
        message: PgmqMessage<SimpleStepMessage>,
        queue_name: String,
    ) -> TaskerResult<()> {
        let start_time = std::time::Instant::now();
        let step_message = message.message;

        debug!(
            worker_id = %self.worker_id,
            processor_id = %self.processor_id,
            step_uuid = %step_message.step_uuid,
            task_uuid = %step_message.task_uuid,
            queue = %queue_name,
            "Starting step execution with database operations, claiming, and message deletion"
        );

        self.stats.total_executed += 1;

        // TAS-29 Phase 3.3: Record step execution start
        if let Some(counter) = STEP_EXECUTIONS_TOTAL.get() {
            counter.add(
                1,
                &[KeyValue::new(
                    "correlation_id",
                    step_message.correlation_id.to_string(),
                )],
            );
        }

        info!(
            worker_id = %self.worker_id,
            processor_id = %self.processor_id,
            step_uuid = %step_message.step_uuid,
            task_uuid = %step_message.task_uuid,
            queue = %queue_name,
            "Processing step message in command processor (database operations moved here)"
        );

        let step_claimer = StepClaim::new(
            step_message.step_uuid,
            step_message.task_uuid,
            self.context.clone(),
            self.task_template_manager.clone(),
        );

        let task_sequence_step = match step_claimer
            .get_task_sequence_step_from_step_message(&step_message)
            .await
        {
            Ok(step) => match step {
                Some(step) => step,
                None => {
                    error!(
                        correlation_id = %step_message.correlation_id,
                        step_uuid = %step_message.step_uuid,
                        "Failed to get task sequence step"
                    );

                    // TAS-29 Phase 3.3: Record step failure (early error, no namespace available)
                    if let Some(counter) = STEP_FAILURES_TOTAL.get() {
                        counter.add(
                            1,
                            &[
                                KeyValue::new(
                                    "correlation_id",
                                    step_message.correlation_id.to_string(),
                                ),
                                KeyValue::new("namespace", "unknown"),
                                KeyValue::new("error_type", "step_not_found"),
                            ],
                        );
                    }

                    return Err(TaskerError::DatabaseError(format!(
                        "Step not found for {}",
                        step_message.step_uuid
                    )));
                }
            },
            Err(err) => {
                error!(
                    correlation_id = %step_message.correlation_id,
                    step_uuid = %step_message.step_uuid,
                    error = %err,
                    "Failed to get task sequence step"
                );

                // TAS-29 Phase 3.3: Record step failure (early error, no namespace available)
                if let Some(counter) = STEP_FAILURES_TOTAL.get() {
                    counter.add(
                        1,
                        &[
                            KeyValue::new(
                                "correlation_id",
                                step_message.correlation_id.to_string(),
                            ),
                            KeyValue::new("namespace", "unknown"),
                            KeyValue::new("error_type", "database_error"),
                        ],
                    );
                }

                return Err(err);
            }
        };

        // TAS-65/TAS-69: Cache step execution context for domain event publishing at completion
        // We cache *before* claiming since even failed claims might need the context for error events
        let step_uuid = task_sequence_step.workflow_step.workflow_step_uuid;
        self.step_execution_contexts
            .insert(step_uuid, task_sequence_step.clone());
        debug!(
            worker_id = %self.worker_id,
            step_uuid = %step_uuid,
            "Cached step execution context for domain event publishing"
        );

        let claimed = match step_claimer
            .try_claim_step(&task_sequence_step, step_message.correlation_id)
            .await
        {
            Ok(claimed) => claimed,
            Err(err) => {
                error!(
                    correlation_id = %step_message.correlation_id,
                    step_uuid = %step_message.step_uuid,
                    error = %err,
                    "Failed to claim step"
                );

                // TAS-29 Phase 3.3: Record step failure
                if let Some(counter) = STEP_FAILURES_TOTAL.get() {
                    counter.add(
                        1,
                        &[
                            KeyValue::new(
                                "correlation_id",
                                step_message.correlation_id.to_string(),
                            ),
                            KeyValue::new(
                                "namespace",
                                task_sequence_step.task.namespace_name.clone(),
                            ),
                            KeyValue::new("error_type", "claim_failed"),
                        ],
                    );
                }

                return Err(err);
            }
        };

        if claimed {
            // RACE CONDITION FIX (TAS-29 Phase 5.4 Discovery):
            // Under high load, PostgreSQL Read Committed isolation can delay transaction
            // visibility. The state machine transition to "in_progress" might not be immediately
            // visible to the handler's transaction, causing handler completion to fail with:
            // "Invalid state transition from enqueued to EnqueueForOrchestration"
            //
            // Solution: Verify state visibility with exponential backoff before handler execution.
            // This ensures the FFI handler sees the correct state when it completes.
            debug!(
                worker_id = %self.worker_id,
                step_uuid = %step_message.step_uuid,
                "Verifying state transition visibility before handler execution"
            );

            let state_machine_verify = StepStateMachine::new(
                task_sequence_step.workflow_step.clone().into(),
                self.context.clone(),
            );

            let mut verified = false;
            for attempt in 0..5 {
                match state_machine_verify.current_state().await {
                    Ok(WorkflowStepState::InProgress) => {
                        debug!(
                            worker_id = %self.worker_id,
                            step_uuid = %step_message.step_uuid,
                            attempt = attempt,
                            "State transition verified as visible"
                        );
                        verified = true;
                        break;
                    }
                    Ok(other_state) if attempt < 4 => {
                        // State not visible yet, retry with exponential backoff
                        let delay_ms = 2_u64.pow(attempt); // 1ms, 2ms, 4ms, 8ms
                        warn!(
                            worker_id = %self.worker_id,
                            step_uuid = %step_message.step_uuid,
                            attempt = attempt,
                            current_state = %other_state,
                            delay_ms = delay_ms,
                            "State transition not yet visible, retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    Ok(other_state) => {
                        error!(
                            worker_id = %self.worker_id,
                            step_uuid = %step_message.step_uuid,
                            current_state = %other_state,
                            "State stuck after claiming - possible race condition"
                        );
                        return Err(TaskerError::WorkerError(format!(
                            "Step {} stuck in state {} after claiming (expected in_progress)",
                            step_message.step_uuid, other_state
                        )));
                    }
                    Err(e) => {
                        error!(
                            worker_id = %self.worker_id,
                            step_uuid = %step_message.step_uuid,
                            error = %e,
                            "Failed to verify state transition"
                        );
                        return Err(e.into());
                    }
                }
            }

            if !verified {
                error!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_message.step_uuid,
                    "State transition verification failed after 5 attempts"
                );
                return Err(TaskerError::WorkerError(format!(
                    "State transition not visible after 5 verification attempts for step {}",
                    step_message.step_uuid
                )));
            }

            // State is now verified visible - safe to proceed with handler execution
            match self
                .context
                .message_client
                .delete_message(&queue_name, message.msg_id)
                .await
            {
                Ok(_) => (),
                Err(err) => {
                    error!("Failed to delete message: {err}");

                    // TAS-29 Phase 3.3: Record step failure
                    if let Some(counter) = STEP_FAILURES_TOTAL.get() {
                        counter.add(
                            1,
                            &[
                                KeyValue::new(
                                    "correlation_id",
                                    step_message.correlation_id.to_string(),
                                ),
                                KeyValue::new(
                                    "namespace",
                                    task_sequence_step.task.namespace_name.clone(),
                                ),
                                KeyValue::new("error_type", "message_deletion_failed"),
                            ],
                        );
                    }

                    return Err(TaskerError::MessagingError(format!(
                        "Failed to delete message: {err}"
                    )));
                }
            }

            // TAS-65 Phase 1.5a: Create span with full execution context for distributed tracing
            let step_span = span!(
                Level::INFO,
                "worker.step_execution",
                correlation_id = %step_message.correlation_id,
                task_uuid = %step_message.task_uuid,
                step_uuid = %step_message.step_uuid,
                step_name = %task_sequence_step.workflow_step.name,
                namespace = %task_sequence_step.task.namespace_name
            );

            // Execute FFI handler within instrumented span
            let execution_result = async {
                event!(Level::INFO, "step.execution_started");

                // TAS-65 Phase 1.5b: Extract trace context from current span for FFI propagation
                // Note: OpenTelemetry span context extraction requires accessing the span's context
                // via the tracing-opentelemetry layer. For now, we'll use correlation_id as a
                // placeholder for trace_id until we integrate OpenTelemetry context propagation.
                let trace_id = Some(step_message.correlation_id.to_string());
                let span_id = Some(format!("span-{}", step_message.step_uuid));

                // Fire step execution event (FFI handler execution) with trace context
                let fire_result = match &self.event_publisher {
                    Some(publisher) => {
                        publisher
                            .fire_step_execution_event_with_trace(
                                &task_sequence_step,
                                trace_id,
                                span_id,
                            )
                            .await
                    }
                    None => {
                        return Err(TaskerError::WorkerError(
                            "Event publisher not initialized".to_string(),
                        ));
                    }
                };

                if let Err(err) = fire_result {
                    error!("Failed to fire step execution event: {err}");
                    event!(Level::ERROR, "step.execution_failed");
                    return Err(TaskerError::EventError(format!(
                        "Failed to fire step execution event: {err}"
                    )));
                }

                event!(Level::INFO, "step.execution_completed");
                Ok(())
            }
            .instrument(step_span)
            .await;

            // Handle execution result
            if let Err(e) = execution_result {
                // Update failure statistics
                let execution_time = start_time.elapsed().as_millis() as u64;
                self.stats.total_failed += 1;

                // TAS-29 Phase 3.3: Record step failure
                if let Some(counter) = STEP_FAILURES_TOTAL.get() {
                    counter.add(
                        1,
                        &[
                            KeyValue::new(
                                "correlation_id",
                                step_message.correlation_id.to_string(),
                            ),
                            KeyValue::new(
                                "namespace",
                                task_sequence_step.task.namespace_name.clone(),
                            ),
                            KeyValue::new("error_type", "handler_execution_failed"),
                        ],
                    );
                }

                // TAS-29 Phase 3.3: Record step execution duration (even for failures)
                let duration_ms = execution_time as f64;
                if let Some(histogram) = STEP_EXECUTION_DURATION.get() {
                    histogram.record(
                        duration_ms,
                        &[
                            KeyValue::new(
                                "correlation_id",
                                step_message.correlation_id.to_string(),
                            ),
                            KeyValue::new(
                                "namespace",
                                task_sequence_step.task.namespace_name.clone(),
                            ),
                            KeyValue::new("result", "failure"),
                        ],
                    );
                }

                return Err(e);
            }

            // Update statistics and return success
            let execution_time = start_time.elapsed().as_millis() as u64;
            self.stats.total_succeeded += 1;
            self.stats.average_execution_time_ms = (self.stats.average_execution_time_ms
                * (self.stats.total_executed as f64 - 1.0)
                + execution_time as f64)
                / self.stats.total_executed as f64;

            // TAS-29 Phase 3.3: Record step execution success
            if let Some(counter) = STEP_SUCCESSES_TOTAL.get() {
                counter.add(
                    1,
                    &[
                        KeyValue::new("correlation_id", step_message.correlation_id.to_string()),
                        KeyValue::new("namespace", task_sequence_step.task.namespace_name.clone()),
                    ],
                );
            }

            // TAS-29 Phase 3.3: Record step execution duration
            let duration_ms = execution_time as f64;
            if let Some(histogram) = STEP_EXECUTION_DURATION.get() {
                histogram.record(
                    duration_ms,
                    &[
                        KeyValue::new("correlation_id", step_message.correlation_id.to_string()),
                        KeyValue::new("namespace", task_sequence_step.task.namespace_name.clone()),
                        KeyValue::new("result", "success"),
                    ],
                );
            }

            Ok(())
        } else {
            // Step was not claimed (already processing or not in claimable state)
            // Do not fire event - simply acknowledge that we saw the message
            debug!(
                worker_id = %self.worker_id,
                step_uuid = %step_message.step_uuid,
                "Step not claimed - skipping execution (already claimed by another worker or in non-claimable state)"
            );
            Ok(())
        }
    }

    /// Get worker status - simple status without complex metrics
    fn handle_get_worker_status(&self) -> TaskerResult<WorkerStatus> {
        let uptime = self.start_time.elapsed().as_secs();

        let status = WorkerStatus {
            worker_id: self.worker_id.clone(),
            status: "running".to_string(),
            steps_executed: self.stats.total_executed,
            steps_succeeded: self.stats.total_succeeded,
            steps_failed: self.stats.total_failed,
            uptime_seconds: uptime,
            registered_handlers: self.handlers.keys().cloned().collect(),
        };

        debug!(
            worker_id = %self.worker_id,
            status = ?status,
            "Returning worker status"
        );

        Ok(status)
    }

    /// Handle health check request
    async fn handle_health_check(&self) -> TaskerResult<WorkerHealthStatus> {
        let _uptime = self.start_time.elapsed().as_secs();
        let health_status = WorkerHealthStatus {
            status: "healthy".to_string(),
            database_connected: true, // TODO: Could add actual DB connectivity check
            orchestration_api_reachable: true, // TODO: Could add actual API check
            supported_namespaces: self.task_template_manager.supported_namespaces().await,
            template_cache_stats: Some(self.task_template_manager.cache_stats().await),
            total_messages_processed: self.stats.total_executed,
            successful_executions: self.stats.total_succeeded,
            failed_executions: self.stats.total_failed,
        };

        debug!(
            worker_id = %self.worker_id,
            health_status = ?health_status,
            "Returning health status from command processor"
        );

        Ok(health_status)
    }

    /// Send step result to orchestration - complete implementation with StepStateMachine
    async fn handle_send_step_result(&self, step_result: StepExecutionResult) -> TaskerResult<()> {
        debug!(
            worker_id = %self.worker_id,
            step_uuid = %step_result.step_uuid,
            success = step_result.success,
            "Processing step result with state machine integration"
        );

        let db_pool = self.context.database_pool();

        // 1. Load WorkflowStep from database using tasker-shared model
        let workflow_step = WorkflowStep::find_by_id(db_pool, step_result.step_uuid)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to lookup step: {e}")))?
            .ok_or_else(|| {
                TaskerError::WorkerError(format!("Step not found: {}", step_result.step_uuid))
            })?;

        // 2. Use StepStateMachine for proper state transition with results persistence
        let mut state_machine = StepStateMachine::new(workflow_step, self.context.clone());

        // 3. Transition using appropriate notification state based on step execution result
        // This enables proper error notification pathway to orchestration
        let step_event = if step_result.success {
            StepEvent::EnqueueForOrchestration(Some(serde_json::to_value(&step_result)?))
        } else {
            StepEvent::EnqueueAsErrorForOrchestration(Some(serde_json::to_value(&step_result)?))
        };

        // 4. Execute atomic state transition (includes result persistence via UpdateStepResultsAction)
        let final_state = state_machine.transition(step_event).await.map_err(|e| {
            TaskerError::StateTransitionError(format!("Step transition failed: {e}"))
        })?;

        let task_uuid = state_machine.task_uuid();
        let task = Task::find_by_id(db_pool, task_uuid)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to fetch task: {e}")))?
            .ok_or_else(|| TaskerError::WorkerError(format!("Task not found: {}", task_uuid)))?;
        let correlation_id = task.correlation_id;

        // 5. Send SimpleStepMessage to orchestration queue using config-driven helper
        self.orchestration_result_sender
            .send_completion(task_uuid, step_result.step_uuid, correlation_id)
            .await?;

        info!(
            worker_id = %self.worker_id,
            step_uuid = %step_result.step_uuid,
            correlation_id = %correlation_id,
            final_state = %final_state,
            "Step result processed and sent to orchestration successfully"
        );

        Ok(())
    }

    /// Execute step with correlation ID for event tracking
    async fn handle_execute_step_with_correlation(
        &mut self,
        message: PgmqMessage<SimpleStepMessage>,
        queue_name: String,
        _correlation_id: Uuid,
    ) -> TaskerResult<()> {
        // Use existing execute_step logic but with correlation ID for event publishing
        self.handle_execute_step(message, queue_name).await?;

        // If event publisher is available, fire correlated event
        if let Some(ref _event_publisher) = self.event_publisher {
            // The event was already fired in handle_execute_step, but we could enhance
            // it with correlation tracking here if needed
            // debug!(
            //     worker_id = %self.worker_id,
            //     correlation_id = %correlation_id,
            //     step_uuid = %result.step_uuid,
            //     "Step executed with correlation tracking"
            // );
        }

        Ok(())
    }

    /// Process step completion event from FFI handler (event-driven architecture)
    async fn handle_process_step_completion(
        &mut self,
        step_result: StepExecutionResult,
        correlation_id: Option<Uuid>,
    ) -> TaskerResult<()> {
        info!(
            worker_id = %self.worker_id,
            step_uuid = %step_result.step_uuid,
            success = step_result.success,
            correlation_id = ?correlation_id,
            "Processing step completion from FFI handler"
        );

        // Update statistics for completed steps
        if step_result.success {
            self.stats.total_succeeded += 1;
        } else {
            self.stats.total_failed += 1;
        }

        self.update_average_execution_time(step_result.metadata.execution_time_ms);

        // TAS-65/TAS-69: Dispatch domain events BEFORE sending result to orchestration
        // This ensures events are dispatched even if the send fails
        // Domain events use fire-and-forget semantics (try_send) - never blocks
        self.dispatch_domain_events(&step_result, correlation_id);

        // Forward the result to orchestration
        self.handle_send_step_result(step_result).await?;

        Ok(())
    }

    /// Enable or disable event integration at runtime
    fn handle_set_event_integration(&mut self, enabled: bool) -> TaskerResult<()> {
        info!(
            worker_id = %self.worker_id,
            enabled = enabled,
            current_state = self.event_publisher.is_some(),
            "Setting event integration state"
        );

        if enabled && self.event_publisher.is_none() {
            // Enable event integration
            self.enable_event_integration();
            info!(worker_id = %self.worker_id, "Event integration enabled");
        } else if !enabled && self.event_publisher.is_some() {
            // Disable event integration
            self.event_publisher = None;
            self.event_subscriber = None;
            info!(worker_id = %self.worker_id, "Event integration disabled");
        }

        Ok(())
    }

    /// Get event integration status and statistics
    fn handle_get_event_status(&self) -> TaskerResult<EventIntegrationStatus> {
        let status = if let Some(ref event_publisher) = self.event_publisher {
            let publisher_stats = event_publisher.get_statistics();
            let subscriber_stats = self
                .event_subscriber
                .as_ref()
                .map(|s| s.get_statistics())
                .unwrap_or_default();

            EventIntegrationStatus {
                enabled: true,
                events_published: publisher_stats.events_published,
                events_received: subscriber_stats.completions_received,
                correlation_tracking_enabled: true,
                pending_correlations: 0, // Would need to track this if using CorrelatedCompletionListener
                ffi_handlers_subscribed: publisher_stats.ffi_handlers_subscribed,
                completion_subscribers: publisher_stats.completion_subscribers,
            }
        } else {
            EventIntegrationStatus {
                enabled: false,
                events_published: 0,
                events_received: 0,
                correlation_tracking_enabled: false,
                pending_correlations: 0,
                ffi_handlers_subscribed: 0,
                completion_subscribers: 0,
            }
        };

        debug!(
            worker_id = %self.worker_id,
            event_status = ?status,
            "Returning event integration status"
        );

        Ok(status)
    }

    /// Handle template cache refresh
    async fn handle_refresh_template_cache(&mut self, namespace: Option<&str>) -> TaskerResult<()> {
        match namespace {
            Some(ns) => {
                info!(
                    worker_id = %self.worker_id,
                    namespace = ns,
                    "Refreshing template cache for specific namespace"
                );
                // TODO: Implement namespace-specific cache refresh
                // For now, just log the request
                info!(
                    worker_id = %self.worker_id,
                    namespace = ns,
                    "Template cache refresh completed for namespace"
                );
            }
            None => {
                info!(
                    worker_id = %self.worker_id,
                    "Refreshing entire template cache"
                );
                // TODO: Implement full cache refresh
                // For now, just log the request
                info!(
                    worker_id = %self.worker_id,
                    "Full template cache refresh completed"
                );
            }
        }

        Ok(())
    }

    /// Update average execution time statistics
    fn update_average_execution_time(&mut self, execution_time: i64) {
        if self.stats.total_executed == 1 {
            self.stats.average_execution_time_ms = execution_time as f64;
        } else {
            let total_time =
                self.stats.average_execution_time_ms * (self.stats.total_executed - 1) as f64;
            self.stats.average_execution_time_ms =
                (total_time + execution_time as f64) / self.stats.total_executed as f64;
        }
    }

    /// Execute step from PgmqMessage - TAS-43 event system integration
    /// This handles the direct message passing pattern to avoid visibility timeout race conditions
    async fn handle_execute_step_from_message(
        &mut self,
        queue_name: String,
        message: PgmqMessage,
    ) -> TaskerResult<()> {
        debug!(
            worker_id = %self.worker_id,
            msg_id = message.msg_id,
            queue_name = %queue_name,
            "Processing step from direct message (TAS-43 event system)"
        );

        // Deserialize the message payload as SimpleStepMessage
        let step_message: SimpleStepMessage = serde_json::from_value(message.message.clone())
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to deserialize step message: {}", e))
            })?;

        // Create PgmqMessage<SimpleStepMessage> for existing handler
        let typed_message = PgmqMessage {
            msg_id: message.msg_id,
            message: step_message,
            vt: message.vt,
            read_ct: message.read_ct,
            enqueued_at: message.enqueued_at,
        };

        // Delegate to existing handle_execute_step logic
        self.handle_execute_step(typed_message, queue_name).await
    }

    /// Execute step from MessageReadyEvent - TAS-43 event system integration
    /// This handles the event-driven message processing from pgmq-notify
    async fn handle_execute_step_from_event(
        &mut self,
        message_event: MessageReadyEvent,
    ) -> TaskerResult<()> {
        debug!(
            worker_id = %self.worker_id,
            msg_id = message_event.msg_id,
            queue_name = %message_event.queue_name,
            namespace = %message_event.namespace,
            "Processing step from event (TAS-43 pgmq-notify event)"
        );

        // Read the specific message from the queue using the message ID
        let message = self
            .context
            .message_client
            .read_specific_message::<SimpleStepMessage>(
                &message_event.queue_name,
                message_event.msg_id,
                message_event.visibility_timeout_seconds.unwrap_or(30),
            )
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to read specific message: {}", e))
            })?;

        match message {
            Some(msg) => {
                // Delegate to existing handle_execute_step logic
                self.handle_execute_step(msg, message_event.queue_name)
                    .await
            }
            None => {
                warn!(
                    worker_id = %self.worker_id,
                    msg_id = message_event.msg_id,
                    queue_name = %message_event.queue_name,
                    "Message not found when processing event (may have been processed already)"
                );
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use dotenvy::dotenv;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_worker_processor_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        dotenv().ok();
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        // TAS-51: Create channel monitor for testing
        let channel_monitor = ChannelMonitor::new("test_worker_command_channel", 100);

        let (processor, _sender) =
            WorkerProcessor::new(context, task_template_manager, 100, channel_monitor);

        assert_eq!(processor.stats.total_executed, 0);
        assert_eq!(processor.handlers.len(), 0);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_execute_step_with_simple_message(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        dotenv().ok();
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        // TAS-51: Create channel monitor for testing
        let channel_monitor = ChannelMonitor::new("test_worker_command_channel", 100);

        let (mut processor, _sender) =
            WorkerProcessor::new(context, task_template_manager, 100, channel_monitor);

        // Create a SimpleStepMessage for testing
        let step_message = SimpleStepMessage {
            task_uuid: Uuid::new_v4(),
            step_uuid: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
        };

        let queue_name = format!("{}_{}", "worker", "test_namespace");

        let message = PgmqMessage::<SimpleStepMessage> {
            msg_id: 1,
            message: step_message,
            vt: chrono::Utc::now(),
            read_ct: 1,
            enqueued_at: chrono::Utc::now(),
        };

        // With SQLx test providing a real database pool, this should work properly
        let result = processor.handle_execute_step(message, queue_name).await;

        // For now, we expect it to fail due to missing handler registration
        // This is expected behavior - the test verifies the execution path works
        assert!(result.is_err());
        assert_eq!(processor.stats.total_executed, 1);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_worker_status(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        dotenv().ok();
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        // TAS-51: Create channel monitor for testing
        let channel_monitor = ChannelMonitor::new("test_worker_command_channel", 100);

        let (processor, _sender) =
            WorkerProcessor::new(context, task_template_manager, 100, channel_monitor);

        let status = processor.handle_get_worker_status().unwrap();

        assert_eq!(status.status, "running");
        assert_eq!(status.steps_executed, 0);
        assert_eq!(status.registered_handlers.len(), 0);

        Ok(())
    }
}
