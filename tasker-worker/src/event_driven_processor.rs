//! # TAS-43 Event-Driven Message Processor for Workers
//!
//! This module integrates pgmq-notify with tasker-worker to enable event-driven
//! step message processing, replacing the current polling-based approach with
//! PostgreSQL LISTEN/NOTIFY for <10ms latency response times.
//!
//! ## Key Features
//!
//! - **Event-Driven Architecture**: Uses pgmq-notify for real-time message notifications
//! - **Namespace-Aware**: Processes messages for specific worker namespaces
//! - **Hybrid Reliability**: Event-driven with fallback polling for reliability
//! - **Integration with Command Pattern**: Works with existing WorkerProcessor architecture

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use pgmq_notify::{
    client::PgmqNotifyClient, listener::PgmqEventHandler, PgmqNotifyConfig, PgmqNotifyEvent,
    PgmqNotifyListener,
};
use tasker_shared::{
    messaging::message::SimpleStepMessage, system_context::SystemContext, TaskerError, TaskerResult,
};

use crate::command_processor::WorkerCommand;
use crate::task_template_manager::TaskTemplateManager;

/// Configuration for event-driven message processing
#[derive(Debug, Clone)]
pub struct EventDrivenConfig {
    /// Enable event-driven processing (vs polling-only)
    pub event_driven_enabled: bool,
    /// Fallback polling interval (for reliability)
    pub fallback_polling_interval: Duration,
    /// Message batch size for processing
    pub batch_size: u32,
    /// Visibility timeout for message processing
    pub visibility_timeout: Duration,
}

impl Default for EventDrivenConfig {
    fn default() -> Self {
        Self {
            event_driven_enabled: true,
            fallback_polling_interval: Duration::from_millis(500),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
        }
    }
}

/// Event-driven message processor that integrates pgmq-notify with WorkerProcessor
pub struct EventDrivenMessageProcessor {
    /// Configuration
    config: EventDrivenConfig,

    /// System context
    /// holding this context ensures we don't go out of scope and prevent premature shutdown
    #[allow(dead_code)]
    context: Arc<SystemContext>,

    /// Task template manager to determine supported namespaces
    task_template_manager: Arc<TaskTemplateManager>,

    /// PGMQ notification listener for event-driven processing
    listener: Option<PgmqNotifyListener>,

    /// Enhanced PGMQ client with notification capabilities for specific message reading
    pgmq_notify_client: Arc<tokio::sync::Mutex<PgmqNotifyClient>>,

    /// Command sender to WorkerProcessor for step execution
    worker_command_sender: mpsc::Sender<WorkerCommand>,

    /// Processor ID for logging
    processor_id: Uuid,

    /// Running state
    is_running: bool,

    /// Cached supported namespaces (includes "default")
    supported_namespaces: Vec<String>,
}

impl EventDrivenMessageProcessor {
    /// Create new event-driven message processor
    pub async fn new(
        config: EventDrivenConfig,
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
        worker_command_sender: mpsc::Sender<WorkerCommand>,
    ) -> TaskerResult<Self> {
        // Build supported namespaces list from task template manager + "default"
        let mut supported_namespaces = task_template_manager.supported_namespaces().to_vec();
        if !supported_namespaces.contains(&"default".to_string()) {
            supported_namespaces.push("default".to_string());
        }

        info!(
            supported_namespaces = ?supported_namespaces,
            "Creating EventDrivenMessageProcessor with namespace support"
        );

        // Create enhanced PGMQ notify client for specific message reading with VT
        let database_url = context.config_manager.config().database_url();
        let notify_config_for_client = PgmqNotifyConfig::new();

        let pgmq_notify_client = Arc::new(tokio::sync::Mutex::new(
            PgmqNotifyClient::new(&database_url, notify_config_for_client)
                .await
                .map_err(|e| {
                    TaskerError::MessagingError(format!(
                        "Failed to create PGMQ notify client: {}",
                        e
                    ))
                })?,
        ));

        // Create pgmq-notify listener with pattern that matches all supported namespaces
        let namespace_pattern = format!("(?P<namespace>{})", supported_namespaces.join("|"));
        let notify_config = PgmqNotifyConfig::new()
            .with_queue_naming_pattern(&namespace_pattern)
            .with_default_namespace("default"); // Default fallback

        let pool = context.database_pool();
        let listener = if config.event_driven_enabled {
            Some(
                PgmqNotifyListener::new(pool.clone(), notify_config)
                    .await
                    .map_err(|e| {
                        TaskerError::MessagingError(format!(
                            "Failed to create notify listener: {}",
                            e
                        ))
                    })?,
            )
        } else {
            None
        };

        let processor_id = Uuid::now_v7();

        Ok(Self {
            config,
            context,
            task_template_manager,
            listener,
            pgmq_notify_client,
            worker_command_sender,
            processor_id,
            is_running: false,
            supported_namespaces,
        })
    }

    /// Start event-driven message processing
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            processor_id = %self.processor_id,
            supported_namespaces = ?self.supported_namespaces,
            event_driven = %self.config.event_driven_enabled,
            "Starting EventDrivenMessageProcessor"
        );

        self.is_running = true;

        if self.config.event_driven_enabled {
            self.start_event_driven_processing().await?;
        }

        self.start_fallback_polling().await?;

        info!(
            processor_id = %self.processor_id,
            "EventDrivenMessageProcessor started successfully"
        );

        Ok(())
    }

    /// Stop event-driven message processing
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            processor_id = %self.processor_id,
            "Stopping EventDrivenMessageProcessor"
        );

        self.is_running = false;

        if let Some(ref mut listener) = self.listener {
            listener.disconnect().await.map_err(|e| {
                TaskerError::MessagingError(format!("Failed to disconnect listener: {}", e))
            })?;
        }

        Ok(())
    }

    /// Start event-driven processing with pgmq-notify
    async fn start_event_driven_processing(&mut self) -> TaskerResult<()> {
        if let Some(ref mut listener) = self.listener {
            // Connect to PostgreSQL
            listener.connect().await.map_err(|e| {
                TaskerError::MessagingError(format!("Failed to connect listener: {}", e))
            })?;

            // Listen to message ready events for all supported namespaces
            for namespace in &self.supported_namespaces {
                listener
                    .listen_message_ready_for_namespace(namespace)
                    .await
                    .map_err(|e| {
                        TaskerError::MessagingError(format!(
                            "Failed to listen to namespace '{}': {}",
                            namespace, e
                        ))
                    })?;

                info!(
                    processor_id = %self.processor_id,
                    namespace = %namespace,
                    "Listening for events on namespace"
                );
            }

            let handler = MessageEventHandler::new(
                self.supported_namespaces.clone(),
                self.pgmq_notify_client.clone(),
                self.worker_command_sender.clone(),
                self.processor_id,
                self.context.clone(),
                self.task_template_manager.clone(),
            );

            // Start listening with the event handler
            listener.listen_with_handler(handler).await.map_err(|e| {
                TaskerError::MessagingError(format!("Failed to start event handler: {}", e))
            })?;

            info!(
                processor_id = %self.processor_id,
                supported_namespaces = ?self.supported_namespaces,
                "Event-driven processing started successfully"
            );
        }

        Ok(())
    }

    /// Start fallback polling for reliability
    async fn start_fallback_polling(&self) -> TaskerResult<()> {
        let config = self.config.clone();
        let pgmq_notify_client = self.pgmq_notify_client.clone();
        let command_sender = self.worker_command_sender.clone();
        let processor_id = self.processor_id;
        let supported_namespaces = self.supported_namespaces.clone();
        let _context = self.context.clone();
        let _task_template_manager = self.task_template_manager.clone();

        tokio::spawn(async move {
            info!(
                processor_id = %processor_id,
                interval_ms = %config.fallback_polling_interval.as_millis(),
                supported_namespaces = ?supported_namespaces,
                "Starting fallback polling for reliability"
            );

            let mut interval = tokio::time::interval(config.fallback_polling_interval);

            loop {
                interval.tick().await;

                // Check for messages in all supported namespace queues
                for namespace in &supported_namespaces {
                    let queue_name = format!("{}_queue", namespace);

                    // Use the old PGMQ client approach for fallback polling
                    // since we don't have specific message IDs to target
                    let mut client = pgmq_notify_client.lock().await;
                    match client
                        .read::<SimpleStepMessage>(
                            &queue_name,
                            config.visibility_timeout.as_secs() as i32,
                            config.batch_size as i32,
                        )
                        .await
                    {
                        Ok(messages) => {
                            if !messages.is_empty() {
                                debug!(
                                    processor_id = %processor_id,
                                    queue = %queue_name,
                                    namespace = %namespace,
                                    count = %messages.len(),
                                    "Fallback polling found messages"
                                );

                                // Process each message with step verification and claiming
                                for msg in messages {
                                    let step_msg = &msg.message;

                                    // Send raw message to command processor - it will handle DB operations, claiming, and deletion

                                    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                                    if let Err(e) = command_sender
                                        .send(WorkerCommand::ExecuteStep {
                                            message: msg.clone(),
                                            queue_name: queue_name.clone(),
                                            resp: resp_tx,
                                        })
                                        .await
                                    {
                                        warn!(
                                            processor_id = %processor_id,
                                            step_uuid = %step_msg.step_uuid,
                                            error = %e,
                                            "Failed to send step execution command from fallback polling"
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            debug!(
                                processor_id = %processor_id,
                                queue = %queue_name,
                                namespace = %namespace,
                                error = %e,
                                "Fallback polling error for namespace (expected periodically)"
                            );
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Check if processor is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Get processor statistics
    pub async fn get_stats(&self) -> EventDrivenStats {
        let listener_connected = if let Some(ref listener) = self.listener {
            listener.is_healthy().await
        } else {
            false
        };

        EventDrivenStats {
            processor_id: self.processor_id,
            supported_namespaces: self.supported_namespaces.clone(),
            event_driven_enabled: self.config.event_driven_enabled,
            listener_connected,
            messages_processed: 0, // TODO: Track statistics
            events_received: 0,    // TODO: Track statistics
        }
    }
}

/// Event handler for PGMQ notifications
struct MessageEventHandler {
    supported_namespaces: Vec<String>,
    pgmq_notify_client: Arc<tokio::sync::Mutex<PgmqNotifyClient>>,
    command_sender: mpsc::Sender<WorkerCommand>,
    processor_id: Uuid,
    context: Arc<SystemContext>,
    task_template_manager: Arc<TaskTemplateManager>,
}

impl MessageEventHandler {
    fn new(
        supported_namespaces: Vec<String>,
        pgmq_notify_client: Arc<tokio::sync::Mutex<PgmqNotifyClient>>,
        command_sender: mpsc::Sender<WorkerCommand>,
        processor_id: Uuid,
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
    ) -> Self {
        Self {
            supported_namespaces,
            pgmq_notify_client,
            command_sender,
            processor_id,
            context,
            task_template_manager,
        }
    }
}

#[async_trait::async_trait]
impl PgmqEventHandler for MessageEventHandler {
    async fn handle_event(&self, event: PgmqNotifyEvent) -> pgmq_notify::Result<()> {
        match event {
            PgmqNotifyEvent::MessageReady(msg_event) => {
                // Only process messages for our supported namespaces
                if !self.supported_namespaces.contains(&msg_event.namespace) {
                    debug!(
                        processor_id = %self.processor_id,
                        namespace = %msg_event.namespace,
                        supported_namespaces = ?self.supported_namespaces,
                        "Ignoring message for unsupported namespace"
                    );
                    return Ok(());
                }

                debug!(
                    processor_id = %self.processor_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    namespace = %msg_event.namespace,
                    "Received message ready event for supported namespace"
                );

                // Retrieve the SPECIFIC message using our enhanced PGMQ client
                // This ensures we get exactly the message that triggered the notification
                let mut client = self.pgmq_notify_client.lock().await;

                match client
                    .read_specific_message::<SimpleStepMessage>(
                        &msg_event.queue_name,
                        msg_event.msg_id,
                        30, // 30 second visibility timeout
                    )
                    .await
                {
                    Ok(Some(msg)) => {
                        let step_msg = &msg.message;

                        debug!(
                            processor_id = %self.processor_id,
                            queue = %msg_event.queue_name,
                            msg_id = %msg.msg_id,
                            step_uuid = %step_msg.step_uuid,
                            task_uuid = %step_msg.task_uuid,
                            "Successfully retrieved minimal message for event (dependencies queried by workers)"
                        );

                        // Send raw message to command processor - it will handle DB operations, claiming, and deletion
                        // Drop the client lock before sending the command
                        drop(client);

                        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                        if let Err(e) = self
                            .command_sender
                            .send(WorkerCommand::ExecuteStep {
                                message: msg.clone(),
                                queue_name: msg_event.queue_name.clone(),
                                resp: resp_tx,
                            })
                            .await
                        {
                            error!(
                                processor_id = %self.processor_id,
                                step_uuid = %step_msg.step_uuid,
                                error = %e,
                                "Failed to send step execution command for event-driven processing"
                            );
                            return Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                                "Failed to send command: {}",
                                e
                            )));
                        }

                        debug!(
                            processor_id = %self.processor_id,
                            step_uuid = %step_msg.step_uuid,
                            msg_id = %msg.msg_id,
                            "Successfully sent raw message to command processor for processing"
                        );
                    }
                    Ok(None) => {
                        debug!(
                            processor_id = %self.processor_id,
                            queue = %msg_event.queue_name,
                            msg_id = %msg_event.msg_id,
                            "Specific message not available - already claimed or processed"
                        );
                    }
                    Err(e) => {
                        warn!(
                            processor_id = %self.processor_id,
                            queue = %msg_event.queue_name,
                            msg_id = %msg_event.msg_id,
                            error = %e,
                            "Failed to retrieve specific message after notification"
                        );
                        return Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                            "Failed to retrieve message: {}",
                            e
                        )));
                    }
                }
            }
            PgmqNotifyEvent::QueueCreated(queue_event) => {
                info!(
                    processor_id = %self.processor_id,
                    queue = %queue_event.queue_name,
                    namespace = %queue_event.namespace,
                    "Queue created event received"
                );
                // Could use this for dynamic namespace discovery
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
            processor_id = %self.processor_id,
            channel = %channel,
            payload = %payload,
            error = %error,
            "Failed to parse PGMQ notification"
        );
    }

    async fn handle_connection_error(&self, error: pgmq_notify::PgmqNotifyError) {
        error!(
            processor_id = %self.processor_id,
            error = %error,
            "PGMQ notification connection error"
        );
    }
}

/// Statistics for event-driven processing
#[derive(Debug, Clone)]
pub struct EventDrivenStats {
    pub processor_id: Uuid,
    pub supported_namespaces: Vec<String>,
    pub event_driven_enabled: bool,
    pub listener_connected: bool,
    pub messages_processed: u64,
    pub events_received: u64,
}
