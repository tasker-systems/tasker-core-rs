//! pgmq-notify listener for TAS-43 worker namespace queue events
//!
//! This module provides a robust pgmq-notify listener that handles worker
//! namespace queue events, connection management, automatic reconnection, and event classification
//! using the structured approach from events.rs.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use uuid::Uuid;

use pgmq_notify::{
    error::PgmqNotifyError, listener::PgmqEventHandler, MessageReadyEvent, PgmqNotifyConfig,
    PgmqNotifyEvent, PgmqNotifyListener,
};
use tasker_shared::{system_context::SystemContext, TaskerError, TaskerResult};

use super::events::{WorkerNotification, WorkerQueueEvent};

/// pgmq-notify listener for worker namespace queue notifications
///
/// Manages pgmq-notify connections with automatic reconnection, error handling,
/// and config-driven event classification. Provides a unified interface for
/// receiving all types of worker namespace queue events.
pub struct WorkerQueueListener {
    /// Listener identifier
    listener_id: Uuid,
    /// Configuration
    config: WorkerListenerConfig,
    /// System context
    context: Arc<SystemContext>,
    /// Event sender channel
    event_sender: mpsc::Sender<WorkerNotification>,
    /// pgmq-notify listener (when connected)
    pgmq_listener: Option<PgmqNotifyListener>,
    /// Connection state
    is_connected: bool,
    /// Statistics
    stats: Arc<WorkerListenerStats>,
}

/// Configuration for worker queue listener
#[derive(Debug, Clone)]
pub struct WorkerListenerConfig {
    /// Supported namespaces for this worker (e.g., ["linear_workflow", "order_fulfillment"])
    pub supported_namespaces: Vec<String>,
    /// Connection retry configuration
    pub retry_interval: Duration,
    /// Maximum retry attempts
    pub max_retry_attempts: u32,
    /// Event processing timeout
    pub event_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Batch processing enabled
    pub batch_processing: bool,
    /// Connection timeout
    pub connection_timeout: Duration,
}

impl Default for WorkerListenerConfig {
    fn default() -> Self {
        Self {
            // Updated for TAS-41: Include all workflow namespaces supported by Rust workers
            supported_namespaces: vec![
                "linear_workflow".to_string(),
                "diamond_workflow".to_string(),
                "tree_workflow".to_string(),
                "mixed_dag_workflow".to_string(),
                "order_fulfillment".to_string(),
                // Also include the simplified "rust" namespace for generic worker tasks
                "rust".to_string(),
            ],
            retry_interval: Duration::from_secs(5),
            max_retry_attempts: 10,
            event_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
            batch_processing: true,
            connection_timeout: Duration::from_secs(10),
        }
    }
}

/// Runtime statistics for worker queue listener
#[derive(Debug, Default)]
pub struct WorkerListenerStats {
    /// Total events received
    pub events_received: AtomicU64,
    /// Step message events processed
    pub step_messages_processed: AtomicU64,
    /// Health check events processed
    pub health_checks_processed: AtomicU64,
    /// Configuration update events processed
    pub config_updates_processed: AtomicU64,
    /// Unknown events encountered
    pub unknown_events: AtomicU64,
    /// Connection errors encountered
    pub connection_errors: AtomicU64,
    /// Last event timestamp
    pub last_event_at: Arc<parking_lot::Mutex<Option<Instant>>>,
    /// Listener startup time
    pub started_at: Arc<parking_lot::Mutex<Option<Instant>>>,
}

impl WorkerQueueListener {
    /// Create new worker queue listener
    pub async fn new(
        config: WorkerListenerConfig,
        context: Arc<SystemContext>,
        event_sender: mpsc::Sender<WorkerNotification>,
    ) -> TaskerResult<Self> {
        let listener_id = Uuid::new_v4();

        info!(
            listener_id = %listener_id,
            supported_namespaces = ?config.supported_namespaces,
            "Creating WorkerQueueListener"
        );

        Ok(Self {
            listener_id,
            config,
            context,
            event_sender,
            pgmq_listener: None,
            is_connected: false,
            stats: Arc::new(WorkerListenerStats::default()),
        })
    }

    /// Start the worker queue listener
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            listener_id = %self.listener_id,
            supported_namespaces = ?self.config.supported_namespaces,
            "Starting WorkerQueueListener"
        );

        // Create pgmq-notify configuration for worker namespace queues
        // Updated for TAS-41 wrapper function integration: use extraction patterns that match
        // our wrapper functions' namespace extraction logic from queue names
        // Pattern matches both "worker_{namespace}_queue" and "{namespace}_queue" formats
        let pgmq_config = PgmqNotifyConfig::new()
            .with_queue_naming_pattern(r"^(?:worker_)?(?P<namespace>\w+)_queue$")
            .with_default_namespace("default");

        // Create pgmq-notify listener
        let mut listener =
            PgmqNotifyListener::new(self.context.database_pool().clone(), pgmq_config)
                .await
                .map_err(|e| {
                    TaskerError::WorkerError(format!(
                        "Failed to create pgmq-notify listener: {}",
                        e
                    ))
                })?;

        listener.connect().await.map_err(|e| {
            TaskerError::WorkerError(format!("Failed to connect to pgmq-notify listener: {}", e))
        })?;

        // Listen to message ready events for all supported namespaces using TAS-41 wrapper function channels
        // Our wrapper functions send notifications to channels based on queue name patterns:
        // - "worker_{namespace}_queue" -> namespace "{namespace}" -> channel "pgmq_message_ready.{namespace}"
        // - "{namespace}_queue" -> namespace "{namespace}" -> channel "pgmq_message_ready.{namespace}"
        // So for supported_namespaces=["rust", "python"], we'll listen to:
        // - "pgmq_message_ready.rust" (gets notifications from worker_rust_queue)
        // - "pgmq_message_ready.python" (gets notifications from worker_python_queue)

        for namespace in &self.config.supported_namespaces {
            info!(
                listener_id = %self.listener_id,
                namespace = %namespace,
                "Subscribing to TAS-41 wrapper function notifications for worker namespace"
            );

            listener
                .listen_message_ready_for_namespace(namespace)
                .await
                .map_err(|e| {
                    TaskerError::WorkerError(format!(
                        "Failed to listen to namespace {}: {}",
                        namespace, e
                    ))
                })?;
        }

        // Create event handler
        let handler = WorkerEventHandler::new(
            self.config.clone(),
            self.context.clone(),
            self.event_sender.clone(),
            self.listener_id,
            self.stats.clone(),
        );

        // Start listening with the event handler in background task
        self.pgmq_listener = Some(listener);

        // Start listening with the event handler in background task
        let listener_id = self.listener_id;
        if let Some(mut listener) = self.pgmq_listener.take() {
            // Use the new background-task method that returns a JoinHandle
            match listener.start_listening_with_handler(handler).await {
                Ok(handle) => {
                    // Spawn a monitoring task for the background listener
                    tokio::spawn(async move {
                        match handle.await {
                            Ok(Ok(_)) => {
                                info!(listener_id = %listener_id, "pgmq-notify listener completed successfully");
                            }
                            Ok(Err(e)) => {
                                error!(listener_id = %listener_id, error = %e, "pgmq-notify listener failed");
                            }
                            Err(e) => {
                                error!(listener_id = %listener_id, error = %e, "pgmq-notify listener task panicked");
                            }
                        }
                    });
                }
                Err(e) => {
                    error!(listener_id = %listener_id, error = %e, "Failed to start pgmq-notify listener");
                    return Err(TaskerError::OrchestrationError(format!(
                        "Failed to start pgmq-notify listener: {}",
                        e
                    )));
                }
            }
        }

        self.is_connected = true;
        *self.stats.started_at.lock() = Some(Instant::now());

        info!(
            listener_id = %self.listener_id,
            supported_namespaces = ?self.config.supported_namespaces,
            "WorkerQueueListener started successfully"
        );

        Ok(())
    }

    /// Get listener statistics
    pub fn stats(&self) -> Arc<WorkerListenerStats> {
        self.stats.clone()
    }

    /// Check if listener is connected
    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    /// Get listener ID
    pub fn listener_id(&self) -> Uuid {
        self.listener_id
    }
}

/// Event handler for worker pgmq-notify events
struct WorkerEventHandler {
    config: WorkerListenerConfig,
    context: Arc<SystemContext>,
    event_sender: mpsc::Sender<WorkerNotification>,
    listener_id: Uuid,
    stats: Arc<WorkerListenerStats>,
}

impl WorkerEventHandler {
    fn new(
        config: WorkerListenerConfig,
        context: Arc<SystemContext>,
        event_sender: mpsc::Sender<WorkerNotification>,
        listener_id: Uuid,
        stats: Arc<WorkerListenerStats>,
    ) -> Self {
        Self {
            config,
            context,
            event_sender,
            listener_id,
            stats,
        }
    }

    /// Classify message ready event into worker queue event types
    fn classify_event(&self, event: MessageReadyEvent) -> WorkerQueueEvent {
        let queue_name = &event.queue_name;

        // Most events in worker namespace queues will be step messages
        if queue_name.ends_with("_queue") {
            WorkerQueueEvent::StepMessage(event)
        } else if queue_name.contains("_health") {
            WorkerQueueEvent::HealthCheck(event)
        } else if queue_name.contains("_config") {
            WorkerQueueEvent::ConfigurationUpdate(event)
        } else {
            WorkerQueueEvent::Unknown {
                queue_name: queue_name.clone(),
                payload: format!("Unclassified worker queue event"),
            }
        }
    }
}

#[async_trait::async_trait]
impl PgmqEventHandler for WorkerEventHandler {
    async fn handle_event(&self, event: PgmqNotifyEvent) -> Result<(), PgmqNotifyError> {
        match event {
            PgmqNotifyEvent::MessageReady(msg_event) => {
                debug!(
                    listener_id = %self.listener_id,
                    queue_name = %msg_event.queue_name,
                    namespace = %msg_event.namespace,
                    "Received worker message ready event"
                );

                // Update statistics
                self.stats.events_received.fetch_add(1, Ordering::Relaxed);
                *self.stats.last_event_at.lock() = Some(Instant::now());

                // Classify the event
                let queue_event = self.classify_event(msg_event);

                // Update specific event type statistics
                match &queue_event {
                    WorkerQueueEvent::StepMessage(_) => {
                        self.stats
                            .step_messages_processed
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    WorkerQueueEvent::HealthCheck(_) => {
                        self.stats
                            .health_checks_processed
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    WorkerQueueEvent::ConfigurationUpdate(_) => {
                        self.stats
                            .config_updates_processed
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    WorkerQueueEvent::Unknown { .. } => {
                        self.stats.unknown_events.fetch_add(1, Ordering::Relaxed);
                    }
                }

                // Send the classified event
                let notification = WorkerNotification::Event(queue_event);
                if let Err(e) = self.event_sender.send(notification).await {
                    error!(
                        listener_id = %self.listener_id,
                        error = %e,
                        "Failed to send worker queue event"
                    );
                    return Err(PgmqNotifyError::Generic(anyhow::anyhow!(
                        "Failed to send event: {}",
                        e
                    )));
                }

                Ok(())
            }
            PgmqNotifyEvent::QueueCreated(queue_event) => {
                debug!(
                    listener_id = %self.listener_id,
                    queue_name = %queue_event.queue_name,
                    namespace = %queue_event.namespace,
                    "Worker queue listener received queue created event"
                );

                // We don't need to handle queue creation events for workers,
                // but we can log them for debugging
                Ok(())
            }
        }
    }
}
