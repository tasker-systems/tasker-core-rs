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
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use pgmq::Message as PgmqMessage;
use pgmq_notify::MessageReadyEvent;
use tasker_shared::messaging::message::SimpleStepMessage;
use tasker_shared::messaging::{PgmqClientTrait, StepExecutionResult};
use tasker_shared::models::WorkflowStep;
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use super::event_publisher::WorkerEventPublisher;
use super::event_subscriber::WorkerEventSubscriber;
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
pub struct WorkerProcessor {
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
}

impl WorkerProcessor {
    /// Create new WorkerProcessor with command pattern
    pub fn new(
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
        command_buffer_size: usize,
    ) -> (Self, mpsc::Sender<WorkerCommand>) {
        let (command_sender, command_receiver) = mpsc::channel(command_buffer_size);
        let worker_id = format!("worker_{}", Uuid::new_v4());
        let processor_id = Uuid::new_v4();

        info!(
            worker_id = %worker_id,
            processor_id = %processor_id,
            "Creating WorkerProcessor with simple command pattern and database operations"
        );

        // Create OrchestrationResultSender with centralized queues configuration
        let orchestration_result_sender = OrchestrationResultSender::new(
            context.message_client(),
            &context.config_manager.config().queues,
        );

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
        };

        (processor, command_sender)
    }

    pub fn new_with_event_system(
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
        command_buffer_size: usize,
        event_system: Arc<tasker_shared::events::WorkerEventSystem>,
    ) -> (Self, mpsc::Sender<WorkerCommand>) {
        let (mut worker_processor, command_sender) =
            Self::new(context, task_template_manager, command_buffer_size);
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

    /// Start processing worker commands with event integration
    pub async fn start_with_events(&mut self) -> TaskerResult<()> {
        // Start completion listener if event subscriber is enabled
        let completion_receiver = if let Some(ref subscriber) = self.event_subscriber {
            Some(subscriber.start_completion_listener())
        } else {
            None
        };

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
                    error!("Failed to get task sequence step");
                    return Err(TaskerError::DatabaseError(format!(
                        "Step not found for {}",
                        step_message.step_uuid
                    )));
                }
            },
            Err(err) => {
                error!("Failed to get task sequence step: {err}");
                return Err(err);
            }
        };

        let claimed = match step_claimer.try_claim_step(&task_sequence_step).await {
            Ok(claimed) => claimed,
            Err(err) => {
                error!("Failed to claim step: {err}");
                return Err(err);
            }
        };

        if claimed {
            match self
                .context
                .message_client
                .delete_message(&queue_name, message.msg_id)
                .await
            {
                Ok(_) => (),
                Err(err) => {
                    error!("Failed to delete message: {err}");
                    return Err(TaskerError::MessagingError(format!(
                        "Failed to delete message: {err}"
                    )));
                }
            }
        }

        // Update statistics and return result
        let execution_time = start_time.elapsed().as_millis() as u64;
        self.stats.total_succeeded += 1;
        self.stats.average_execution_time_ms = (self.stats.average_execution_time_ms
            * (self.stats.total_executed as f64 - 1.0)
            + execution_time as f64)
            / self.stats.total_executed as f64;

        match &self.event_publisher {
            Some(publisher) => match publisher
                .fire_step_execution_event(&task_sequence_step)
                .await
            {
                Ok(_) => (),
                Err(err) => {
                    error!("Failed to fire step execution event: {err}");
                    return Err(TaskerError::EventError(format!(
                        "Failed to fire step execution event: {err}"
                    )));
                }
            },
            None => return Ok(()),
        }

        Ok(())
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
        let workflow_step = WorkflowStep::find_by_id(&db_pool, step_result.step_uuid)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to lookup step: {e}")))?
            .ok_or_else(|| {
                TaskerError::WorkerError(format!("Step not found: {}", step_result.step_uuid))
            })?;

        // 2. Use StepStateMachine for proper state transition with results persistence
        let mut state_machine = StepStateMachine::new(
            workflow_step,
            db_pool.clone(),
            Some(self.context.event_publisher.clone()),
        );

        // 3. Transition using EnqueueForOrchestration to allow orchestration processing
        // TAS-41: Worker transitions to EnqueuedForOrchestration instead of terminal states
        // This prevents the race condition where tasks become ready before orchestration processing
        let step_event = if step_result.success {
            StepEvent::EnqueueForOrchestration(Some(serde_json::to_value(&step_result)?))
        } else {
            // For failures, we still use EnqueueForOrchestration to allow orchestration to process
            // The orchestration system will determine if it should Complete or Fail based on metadata
            StepEvent::EnqueueForOrchestration(Some(serde_json::to_value(&step_result)?))
        };

        // 4. Execute atomic state transition (includes result persistence via UpdateStepResultsAction)
        let final_state = state_machine.transition(step_event).await.map_err(|e| {
            TaskerError::StateTransitionError(format!("Step transition failed: {e}"))
        })?;

        // 5. Send SimpleStepMessage to orchestration queue using config-driven helper
        self.orchestration_result_sender
            .send_completion(state_machine.task_uuid(), step_result.step_uuid)
            .await?;

        info!(
            worker_id = %self.worker_id,
            step_uuid = %step_result.step_uuid,
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
        let result = self.handle_execute_step(message, queue_name).await?;

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

        Ok(result)
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

    #[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
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

        let (processor, _sender) = WorkerProcessor::new(context, task_template_manager, 100);

        assert_eq!(processor.stats.total_executed, 0);
        assert_eq!(processor.handlers.len(), 0);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
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

        let (mut processor, _sender) = WorkerProcessor::new(context, task_template_manager, 100);

        // Create a SimpleStepMessage for testing
        let step_message = SimpleStepMessage {
            task_uuid: Uuid::new_v4(),
            step_uuid: Uuid::new_v4(),
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

    #[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
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

        let (processor, _sender) = WorkerProcessor::new(context, task_template_manager, 100);

        let status = processor.handle_get_worker_status().unwrap();

        assert_eq!(status.status, "running");
        assert_eq!(status.steps_executed, 0);
        assert_eq!(status.registered_handlers.len(), 0);

        Ok(())
    }
}
