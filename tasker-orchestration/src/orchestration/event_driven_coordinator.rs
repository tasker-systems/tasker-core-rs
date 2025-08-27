//! # TAS-43 Event-Driven Orchestration Coordinator
//!
//! This module integrates pgmq-notify with tasker-orchestration to enable event-driven
//! orchestration operations, replacing polling-based approaches with PostgreSQL LISTEN/NOTIFY
//! for <10ms latency orchestration responses.
//!
//! ## Key Features
//!
//! - **Event-Driven Architecture**: Uses pgmq-notify for real-time step result notifications
//! - **Namespace-Aware**: Coordinates orchestration for specific namespaces
//! - **Hybrid Reliability**: Event-driven with fallback polling for resilience
//! - **Command Pattern Integration**: Works with existing OrchestrationCore command architecture
//! - **TAS-37 Compatible**: Integrates with atomic finalization claiming and race condition prevention

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use pgmq_notify::{
    listener::PgmqEventHandler, MessageReadyEvent, PgmqNotifyConfig, PgmqNotifyEvent,
    PgmqNotifyListener,
};
use tasker_shared::{
    messaging::{PgmqClientTrait, StepExecutionResult, TaskRequestMessage},
    system_context::SystemContext,
    TaskerError, TaskerResult,
};

use crate::orchestration::command_processor::OrchestrationCommand;
use crate::orchestration::OrchestrationCore;

/// Configuration for event-driven orchestration coordination
#[derive(Debug, Clone)]
pub struct EventDrivenCoordinatorConfig {
    /// Namespace for orchestration coordination (e.g., "orchestration")
    pub namespace: String,
    /// Enable event-driven coordination (vs polling-only)
    pub event_driven_enabled: bool,
    /// Fallback polling interval for resilience
    pub fallback_polling_interval: Duration,
    /// Message batch size for polling fallback
    pub batch_size: u32,
    /// Visibility timeout for message processing
    pub visibility_timeout: Duration,
}

impl Default for EventDrivenCoordinatorConfig {
    fn default() -> Self {
        Self {
            namespace: "orchestration".to_string(),
            event_driven_enabled: true,
            fallback_polling_interval: Duration::from_millis(500),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
        }
    }
}

/// Event-driven orchestration coordinator that integrates pgmq-notify with OrchestrationCore
pub struct EventDrivenOrchestrationCoordinator {
    /// Configuration
    config: EventDrivenCoordinatorConfig,

    /// System context for dependency injection
    /// Holding this ensures we don't go out of scope and prevent premature shutdown
    #[allow(dead_code)]
    context: Arc<SystemContext>,

    /// PGMQ notification listener for event-driven coordination
    listener: Option<PgmqNotifyListener>,

    /// Orchestration core for command processing
    orchestration_core: Arc<OrchestrationCore>,

    /// Command sender for orchestration operations
    orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,

    /// Coordinator ID for logging
    coordinator_id: Uuid,

    /// Statistics counters
    operations_coordinated: Arc<std::sync::atomic::AtomicU64>,
    events_received: Arc<std::sync::atomic::AtomicU64>,

    /// Running state
    is_running: bool,
}

impl EventDrivenOrchestrationCoordinator {
    /// Create new event-driven orchestration coordinator
    pub async fn new(
        config: EventDrivenCoordinatorConfig,
        context: Arc<SystemContext>,
        orchestration_core: Arc<OrchestrationCore>,
        orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,
    ) -> TaskerResult<Self> {
        info!(
            namespace = %config.namespace,
            "Creating EventDrivenOrchestrationCoordinator for TAS-43"
        );

        // Create pgmq-notify listener for orchestration coordination
        let notify_config = PgmqNotifyConfig::new()
            .with_queue_naming_pattern(&format!(r"(?P<namespace>{})", config.namespace))
            .with_default_namespace(&config.namespace);

        let pool = context.database_pool();
        let listener = if config.event_driven_enabled {
            Some(
                PgmqNotifyListener::new(pool.clone(), notify_config)
                    .await
                    .map_err(|e| {
                        TaskerError::OrchestrationError(format!(
                            "Failed to create notify listener: {}",
                            e
                        ))
                    })?,
            )
        } else {
            None
        };

        let coordinator_id = Uuid::now_v7();

        Ok(Self {
            config,
            context,
            listener,
            orchestration_core,
            orchestration_command_sender,
            coordinator_id,
            operations_coordinated: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            events_received: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            is_running: false,
        })
    }

    /// Start event-driven orchestration coordination
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            coordinator_id = %self.coordinator_id,
            namespace = %self.config.namespace,
            event_driven = %self.config.event_driven_enabled,
            "Starting EventDrivenOrchestrationCoordinator"
        );

        self.is_running = true;

        if self.config.event_driven_enabled {
            self.start_event_driven_coordination().await?;
        }

        self.start_fallback_polling().await?;

        info!(
            coordinator_id = %self.coordinator_id,
            "EventDrivenOrchestrationCoordinator started successfully"
        );

        Ok(())
    }

    /// Stop event-driven orchestration coordination
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            coordinator_id = %self.coordinator_id,
            "Stopping EventDrivenOrchestrationCoordinator"
        );

        self.is_running = false;

        if let Some(ref mut listener) = self.listener {
            listener.disconnect().await.map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to disconnect listener: {}", e))
            })?;
        }

        Ok(())
    }

    /// Start event-driven coordination with pgmq-notify
    async fn start_event_driven_coordination(&mut self) -> TaskerResult<()> {
        if let Some(ref mut listener) = self.listener {
            // Connect to PostgreSQL
            listener.connect().await.map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to connect listener: {}", e))
            })?;

            // Listen to message ready events for orchestration namespace
            listener
                .listen_message_ready_for_namespace(&self.config.namespace)
                .await
                .map_err(|e| {
                    TaskerError::OrchestrationError(format!("Failed to listen to namespace: {}", e))
                })?;

            let handler = OrchestrationEventHandler::new(
                self.config.clone(),
                self.orchestration_core.clone(),
                self.orchestration_command_sender.clone(),
                self.coordinator_id,
                self.operations_coordinated.clone(),
                self.events_received.clone(),
            );

            // Start listening with the event handler
            listener.listen_with_handler(handler).await.map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to start event handler: {}", e))
            })?;

            info!(
                coordinator_id = %self.coordinator_id,
                namespace = %self.config.namespace,
                "Event-driven orchestration coordination started successfully"
            );
        }

        Ok(())
    }

    /// Start fallback polling for reliability
    async fn start_fallback_polling(&self) -> TaskerResult<()> {
        let config = self.config.clone();
        let orchestration_core = self.orchestration_core.clone(); // For future use
        let coordinator_id = self.coordinator_id;
        let context = self.context.clone();

        tokio::spawn(async move {
            info!(
                coordinator_id = %coordinator_id,
                interval_ms = %config.fallback_polling_interval.as_millis(),
                "Starting fallback polling for orchestration coordination reliability"
            );

            let mut interval = tokio::time::interval(config.fallback_polling_interval);
            let queue_name = format!("{}_step_results", config.namespace);

            loop {
                interval.tick().await;

                // Fallback polling for step results when event-driven notifications aren't available
                debug!(
                    coordinator_id = %coordinator_id,
                    queue = %queue_name,
                    "Performing fallback polling check for step results"
                );

                // Read messages from the orchestration_step_results queue
                match context
                    .message_client
                    .read_messages(&queue_name, Some(30), Some(config.batch_size as i32))
                    .await
                {
                    Ok(messages) => {
                        debug!(
                            coordinator_id = %coordinator_id,
                            queue = %queue_name,
                            count = messages.len(),
                            "Read messages from fallback polling"
                        );

                        for message in messages {
                            // Try to deserialize as StepExecutionResult
                            match serde_json::from_value::<StepExecutionResult>(
                                message.message.clone(),
                            ) {
                                Ok(step_result) => {
                                    debug!(
                                        coordinator_id = %coordinator_id,
                                        msg_id = message.msg_id,
                                        step_uuid = %step_result.step_uuid,
                                        "Processing step result from fallback polling"
                                    );

                                    // Process the step result through the orchestration core
                                    let step_uuid = step_result.step_uuid;
                                    if let Err(e) =
                                        orchestration_core.process_step_result(step_result).await
                                    {
                                        error!(
                                            coordinator_id = %coordinator_id,
                                            step_uuid = %step_uuid,
                                            error = %e,
                                            "Failed to process step result from fallback polling"
                                        );
                                    }

                                    // Delete the processed message
                                    if let Err(e) = context
                                        .message_client
                                        .delete_message(&queue_name, message.msg_id)
                                        .await
                                    {
                                        warn!(
                                            coordinator_id = %coordinator_id,
                                            msg_id = message.msg_id,
                                            error = %e,
                                            "Failed to delete processed message from fallback polling"
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        coordinator_id = %coordinator_id,
                                        msg_id = message.msg_id,
                                        error = %e,
                                        "Failed to deserialize step result from fallback polling"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            coordinator_id = %coordinator_id,
                            queue = %queue_name,
                            error = %e,
                            "Failed to read messages during fallback polling"
                        );
                    }
                }
            }
        });

        Ok(())
    }

    /// Check if coordinator is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Get coordinator statistics
    pub async fn get_stats(&self) -> EventDrivenCoordinatorStats {
        let listener_connected = if let Some(ref listener) = self.listener {
            listener.is_healthy().await
        } else {
            false
        };

        EventDrivenCoordinatorStats {
            coordinator_id: self.coordinator_id,
            namespace: self.config.namespace.clone(),
            event_driven_enabled: self.config.event_driven_enabled,
            listener_connected,
            operations_coordinated: self
                .operations_coordinated
                .load(std::sync::atomic::Ordering::Relaxed),
            events_received: self
                .events_received
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// Event handler for PGMQ notifications in orchestration coordination
struct OrchestrationEventHandler {
    config: EventDrivenCoordinatorConfig,
    #[allow(dead_code)] // Will be used for actual orchestration operations in the future
    orchestration_core: Arc<OrchestrationCore>,
    /// Command sender for orchestration operations
    orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,
    coordinator_id: Uuid,
    /// Statistics counters (shared with coordinator)
    operations_coordinated: Arc<std::sync::atomic::AtomicU64>,
    events_received: Arc<std::sync::atomic::AtomicU64>,
}

impl OrchestrationEventHandler {
    fn new(
        config: EventDrivenCoordinatorConfig,
        orchestration_core: Arc<OrchestrationCore>,
        orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,
        coordinator_id: Uuid,
        operations_coordinated: Arc<std::sync::atomic::AtomicU64>,
        events_received: Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            config,
            orchestration_core,
            orchestration_command_sender,
            coordinator_id,
            operations_coordinated,
            events_received,
        }
    }
}

#[async_trait::async_trait]
impl PgmqEventHandler for OrchestrationEventHandler {
    async fn handle_event(&self, event: PgmqNotifyEvent) -> pgmq_notify::Result<()> {
        match event {
            PgmqNotifyEvent::MessageReady(msg_event) => {
                // Only process messages for our orchestration namespace
                if msg_event.namespace != self.config.namespace {
                    return Ok(());
                }

                debug!(
                    coordinator_id = %self.coordinator_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    namespace = %msg_event.namespace,
                    "Received orchestration message ready event"
                );

                // Increment events received counter
                self.events_received
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // For orchestration coordination, we're primarily interested in step results
                // and task initialization requests. The specific handling would depend on
                // queue naming conventions:

                // orchestration_step_results - Process step execution results
                // orchestration_task_requests - Process task initialization requests
                // orchestration_task_finalizations - Process task finalization requests

                if msg_event.queue_name.contains("step_results") {
                    self.handle_step_results_message_event(msg_event).await?;
                } else if msg_event.queue_name.contains("task_requests") {
                    self.handle_task_requests_message_event(msg_event).await?;
                } else if msg_event.queue_name.contains("task_finalizations") {
                    self.handle_task_finalizations_message_event(msg_event)
                        .await?;
                }
            }
            PgmqNotifyEvent::QueueCreated(queue_event) => {
                info!(
                    coordinator_id = %self.coordinator_id,
                    queue = %queue_event.queue_name,
                    namespace = %queue_event.namespace,
                    "Queue created event received in orchestration coordinator"
                );
                // Could use this for dynamic namespace discovery or queue management
            }
        }

        Ok(())
    }

    async fn handle_parse_error(
        &self,
        channel: &str,
        payload: &str,
        error: pgmq_notify::PgmqNotifyError,
    ) {
        warn!(
            coordinator_id = %self.coordinator_id,
            channel = %channel,
            payload = %payload,
            error = %error,
            "Failed to parse PGMQ notification in orchestration coordinator"
        );
    }

    async fn handle_connection_error(&self, error: pgmq_notify::PgmqNotifyError) {
        error!(
            coordinator_id = %self.coordinator_id,
            error = %error,
            "PGMQ notification connection error in orchestration coordinator"
        );
    }
}

impl OrchestrationEventHandler {
    async fn handle_task_finalizations_message_event(
        &self,
        msg_event: MessageReadyEvent,
    ) -> pgmq_notify::Result<()> {
        debug!(
            coordinator_id = %self.coordinator_id,
            queue = %msg_event.queue_name,
            msg_id = %msg_event.msg_id,
            "Processing task finalization message via event-driven PGMQ reading"
        );

        // Read the actual task UUID message from PGMQ
        match self
            .orchestration_core
            .context
            .message_client
            .read_specific_message::<uuid::Uuid>(
                &msg_event.queue_name,
                msg_event.msg_id,
                30, // 30 second visibility timeout
            )
            .await
        {
            Ok(Some(pgmq_message)) => {
                let task_uuid = pgmq_message.message;

                debug!(
                    coordinator_id = %self.coordinator_id,
                    task_uuid = %task_uuid,
                    msg_id = %pgmq_message.msg_id,
                    "Read task finalization UUID from queue, forwarding to command processor"
                );

                // Create FinalizeTask command and send to orchestration processor
                let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                if let Err(e) = self
                    .orchestration_command_sender
                    .send(OrchestrationCommand::FinalizeTask {
                        task_uuid,
                        resp: resp_tx,
                    })
                    .await
                {
                    warn!(
                        coordinator_id = %self.coordinator_id,
                        task_uuid = %task_uuid,
                        error = %e,
                        "Failed to send FinalizeTask command"
                    );
                } else {
                    debug!(
                        coordinator_id = %self.coordinator_id,
                        task_uuid = %task_uuid,
                        "Successfully sent FinalizeTask command to orchestration processor"
                    );
                    // Successfully coordinated a task finalization operation
                    self.operations_coordinated
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }

                // Delete the processed message from the queue
                if let Err(e) = self
                    .orchestration_core
                    .context
                    .message_client
                    .delete_message(&msg_event.queue_name, pgmq_message.msg_id)
                    .await
                {
                    error!(
                        coordinator_id = %self.coordinator_id,
                        queue = %msg_event.queue_name,
                        msg_id = %pgmq_message.msg_id,
                        task_uuid = %task_uuid,
                        error = %e,
                        "Failed to delete processed task finalization message"
                    );
                    return Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                        "Failed to delete processed task finalization message: {e}"
                    )));
                } else {
                    Ok(())
                }
            }
            Ok(None) => {
                debug!(
                    coordinator_id = %self.coordinator_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    "Task finalization message not found - already processed by another coordinator"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    coordinator_id = %self.coordinator_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    error = %e,
                    "Failed to read task finalization message from queue"
                );
                return Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                    "Failed to read task finalization message: {e}"
                )));
            }
        }
    }

    async fn handle_task_requests_message_event(
        &self,
        msg_event: MessageReadyEvent,
    ) -> pgmq_notify::Result<()> {
        debug!(
            coordinator_id = %self.coordinator_id,
            queue = %msg_event.queue_name,
            msg_id = %msg_event.msg_id,
            "Processing task request message via command pattern"
        );

        // Read the specific TaskRequestMessage from PGMQ using enhanced client
        match self
            .orchestration_core
            .context
            .message_client
            .read_specific_message::<TaskRequestMessage>(
                &msg_event.queue_name,
                msg_event.msg_id,
                30, // 30 second visibility timeout
            )
            .await
        {
            Ok(Some(pgmq_message)) => {
                let task_request = pgmq_message.message;

                debug!(
                    coordinator_id = %self.coordinator_id,
                    namespace = %task_request.namespace,
                    task_name = %task_request.task_name,
                    task_version = %task_request.task_version,
                    "Retrieved task request message from queue"
                );

                let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                if let Err(e) = self
                    .orchestration_command_sender
                    .send(OrchestrationCommand::InitializeTask {
                        request: task_request,
                        resp: resp_tx,
                    })
                    .await
                {
                    warn!(
                        coordinator_id = %self.coordinator_id,
                        error = %e,
                        "Failed to send InitializeTask command"
                    );
                } else {
                    // Successfully coordinated a task request operation
                    self.operations_coordinated
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }

                // Delete the processed message from the queue
                if let Err(e) = self
                    .orchestration_core
                    .context
                    .message_client
                    .delete_message(&msg_event.queue_name, msg_event.msg_id)
                    .await
                {
                    warn!(
                        coordinator_id = %self.coordinator_id,
                        queue = %msg_event.queue_name,
                        msg_id = %msg_event.msg_id,
                        error = %e,
                        "Failed to delete processed task request message"
                    );
                    return Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                        "Failed to delete message: {e}"
                    )));
                } else {
                    Ok(())
                }
            }
            Ok(None) => {
                debug!(
                    coordinator_id = %self.coordinator_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    "Task request message not found - already processed by another coordinator"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    coordinator_id = %self.coordinator_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    error = %e,
                    "Failed to read task request message from queue"
                );
                return Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                    "Failed to read task request message: {e}"
                )));
            }
        }
    }
    async fn handle_step_results_message_event(
        &self,
        msg_event: MessageReadyEvent,
    ) -> pgmq_notify::Result<()> {
        debug!(
            coordinator_id = %self.coordinator_id,
            queue = %msg_event.queue_name,
            msg_id = %msg_event.msg_id,
            "Processing step result message via command pattern"
        );

        // Read the specific StepExecutionResult message from PGMQ using enhanced client
        match self
            .orchestration_core
            .context
            .message_client
            .read_specific_message::<StepExecutionResult>(
                &msg_event.queue_name,
                msg_event.msg_id,
                30, // 30 second visibility timeout
            )
            .await
        {
            Ok(Some(pgmq_message)) => {
                let step_result = pgmq_message.message;

                debug!(
                    coordinator_id = %self.coordinator_id,
                    step_uuid = %step_result.step_uuid,
                    success = step_result.success,
                    "Retrieved step execution result from queue"
                );

                let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                if let Err(e) = self
                    .orchestration_command_sender
                    .send(OrchestrationCommand::ProcessStepResult {
                        result: step_result,
                        resp: resp_tx,
                    })
                    .await
                {
                    warn!(
                        coordinator_id = %self.coordinator_id,
                        error = %e,
                        "Failed to send ProcessStepResult command"
                    );
                } else {
                    // Successfully coordinated a step result operation
                    self.operations_coordinated
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }

                // Delete the processed message from the queue
                if let Err(e) = self
                    .orchestration_core
                    .context
                    .message_client
                    .delete_message(&msg_event.queue_name, msg_event.msg_id)
                    .await
                {
                    warn!(
                        coordinator_id = %self.coordinator_id,
                        queue = %msg_event.queue_name,
                        msg_id = %msg_event.msg_id,
                        error = %e,
                        "Failed to delete processed step result message"
                    );
                    return Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                        "Failed to delete processed step resultmessage: {e}"
                    )));
                } else {
                    Ok(())
                }
            }
            Ok(None) => {
                debug!(
                    coordinator_id = %self.coordinator_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    "Step result message not found - already processed by another coordinator"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    coordinator_id = %self.coordinator_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    error = %e,
                    "Failed to read step result message from queue"
                );
                return Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                    "Failed to read step result message: {e}"
                )));
            }
        }
    }
}

/// Statistics for event-driven orchestration coordination
#[derive(Debug, Clone)]
pub struct EventDrivenCoordinatorStats {
    pub coordinator_id: Uuid,
    pub namespace: String,
    pub event_driven_enabled: bool,
    pub listener_connected: bool,
    pub operations_coordinated: u64,
    pub events_received: u64,
}
