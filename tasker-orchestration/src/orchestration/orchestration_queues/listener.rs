//! pgmq-notify listener for TAS-43 orchestration queue events
//!
//! This module provides a robust pgmq-notify listener that handles orchestration
//! queue events, connection management, automatic reconnection, and event classification
//! using the structured approach from events.rs.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use pgmq_notify::{
    listener::PgmqEventHandler, PgmqNotifyConfig, PgmqNotifyEvent, PgmqNotifyListener,
};
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::{system_context::SystemContext, TaskerError, TaskerResult};

use super::events::OrchestrationQueueEvent;

/// pgmq-notify listener for orchestration queue notifications
///
/// Manages pgmq-notify connections with automatic reconnection, error handling,
/// and config-driven event classification. Provides a unified interface for
/// receiving all types of orchestration queue events.
pub struct OrchestrationQueueListener {
    /// Listener identifier
    listener_id: Uuid,
    /// Configuration
    config: OrchestrationListenerConfig,
    /// System context
    context: Arc<SystemContext>,
    /// Event sender channel
    event_sender: mpsc::Sender<OrchestrationNotification>,
    /// Channel monitor for observability (TAS-51)
    channel_monitor: ChannelMonitor,
    /// pgmq-notify listener (when connected)
    pgmq_listener: Option<PgmqNotifyListener>,
    /// Connection state
    is_connected: bool,
    /// Statistics
    stats: Arc<OrchestrationListenerStats>,
}

impl std::fmt::Debug for OrchestrationQueueListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrationQueueListener")
            .field("listener_id", &self.listener_id)
            .field("config", &self.config)
            .field("is_connected", &self.is_connected)
            .field("has_channel", &true)
            .finish()
    }
}

/// Configuration for orchestration queue listener
#[derive(Debug, Clone)]
pub struct OrchestrationListenerConfig {
    /// Namespace to listen for (e.g., "orchestration")
    pub namespace: String,
    /// Queue names to monitor
    pub monitored_queues: Vec<String>,
    /// Connection retry configuration
    pub retry_interval: Duration,
    /// Maximum retry attempts
    pub max_retry_attempts: u32,
    /// Event processing timeout
    pub event_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
}

impl Default for OrchestrationListenerConfig {
    fn default() -> Self {
        Self {
            namespace: "orchestration".to_string(),
            monitored_queues: vec![
                "orchestration_step_results".to_string(),
                "orchestration_task_requests".to_string(),
            ],
            retry_interval: Duration::from_secs(5),
            max_retry_attempts: 10,
            event_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
        }
    }
}

/// Unified notification enum for all orchestration queue events
///
/// Wraps the classified OrchestrationQueueEvent with connection error handling.
/// This provides a single channel interface for all event types and errors.
#[derive(Debug, Clone)]
pub enum OrchestrationNotification {
    /// Classified queue event using structured classification
    Event(OrchestrationQueueEvent),
    /// Connection error from pgmq-notify listener
    ConnectionError(String),
    /// Listener reconnected successfully
    Reconnected,
}

/// Runtime statistics for orchestration queue listener
#[derive(Debug, Default)]
pub struct OrchestrationListenerStats {
    /// Total events received
    pub events_received: AtomicU64,
    /// Step result events processed
    pub step_results_processed: AtomicU64,
    /// Task request events processed
    pub task_requests_processed: AtomicU64,
    /// Queue management events processed
    pub queue_management_processed: AtomicU64,
    /// Unknown events encountered
    pub unknown_events: AtomicU64,
    /// Connection errors encountered
    pub connection_errors: AtomicU64,
    /// Last event timestamp
    pub last_event_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
    /// Listener startup time
    pub started_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
}

impl OrchestrationQueueListener {
    /// Create new orchestration queue listener
    pub async fn new(
        config: OrchestrationListenerConfig,
        context: Arc<SystemContext>,
        event_sender: mpsc::Sender<OrchestrationNotification>,
        channel_monitor: ChannelMonitor,
    ) -> TaskerResult<Self> {
        let listener_id = Uuid::new_v4();

        info!(
            listener_id = %listener_id,
            namespace = %config.namespace,
            monitored_queues = ?config.monitored_queues,
            channel_monitor = %channel_monitor.channel_name(),
            "Creating OrchestrationQueueListener with channel monitoring"
        );

        Ok(Self {
            listener_id,
            config,
            context,
            event_sender,
            channel_monitor,
            pgmq_listener: None,
            is_connected: false,
            stats: Arc::new(OrchestrationListenerStats::default()),
        })
    }

    /// Start the orchestration queue listener
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            listener_id = %self.listener_id,
            namespace = %self.config.namespace,
            "Starting OrchestrationQueueListener"
        );

        // Create pgmq-notify configuration from system context database URL
        // Updated for TAS-41 wrapper function integration: use extraction patterns that match
        // our wrapper functions' namespace extraction logic
        let pgmq_config = PgmqNotifyConfig::new()
            .with_queue_naming_pattern(r"(?P<namespace>\w+)_queue|orchestration.*")
            .with_default_namespace(&self.config.namespace);

        // Create pgmq-notify listener with bounded channel (TAS-51)
        // TAS-61 V2: Access mpsc_channels from orchestration context
        let buffer_size = self
            .context
            .tasker_config
            .orchestration
            .as_ref()
            .and_then(|o| Some(o.mpsc_channels.event_listeners.pgmq_event_buffer_size))
            .unwrap_or(10000);
        let mut listener = PgmqNotifyListener::new(
            self.context.database_pool().clone(),
            pgmq_config,
            buffer_size as usize,
        )
        .await
        .map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to create pgmq-notify listener: {}", e))
        })?;

        listener.connect().await.map_err(|e| {
            TaskerError::OrchestrationError(format!(
                "Failed to connect to pgmq-notify listener: {}",
                e
            ))
        })?;

        // Listen to message ready events for orchestration queues using TAS-41 wrapper function channels
        // Our wrapper functions send notifications to channels based on queue name patterns:
        // - "orchestration*" -> namespace "orchestration" -> channel "pgmq_message_ready.orchestration"
        // - "*_queue" -> namespace extracted from queue name -> channel "pgmq_message_ready.{namespace}"

        // Listen to orchestration namespace (covers orchestration, orchestration_priority, etc.)
        listener
            .listen_message_ready_for_namespace(&self.config.namespace)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to listen to orchestration namespace: {}",
                    e
                ))
            })?;

        // Also listen to the global channel for any messages we might miss
        listener.listen_message_ready_global().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to listen to global channel: {}", e))
        })?;

        // Create event handler with channel monitor
        let handler = OrchestrationEventHandler::new(
            self.config.clone(),
            self.context.clone(),
            self.event_sender.clone(),
            self.channel_monitor.clone(),
            self.listener_id,
            self.stats.clone(),
        );

        // Store the listener and start the listening task
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
        *self.stats.started_at.lock().await = Some(Instant::now());

        info!(
            listener_id = %self.listener_id,
            "OrchestrationQueueListener started successfully"
        );

        Ok(())
    }

    /// Stop the orchestration queue listener
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            listener_id = %self.listener_id,
            "Stopping OrchestrationQueueListener"
        );

        // Mark as disconnected
        self.is_connected = false;
        self.pgmq_listener = None;

        info!(
            listener_id = %self.listener_id,
            "OrchestrationQueueListener stopped successfully"
        );

        Ok(())
    }

    /// Check if listener is connected and healthy
    pub fn is_healthy(&self) -> bool {
        self.is_connected && self.pgmq_listener.is_some()
    }

    /// Get listener statistics
    pub async fn stats(&self) -> OrchestrationListenerStats {
        OrchestrationListenerStats {
            events_received: AtomicU64::new(self.stats.events_received.load(Ordering::Relaxed)),
            step_results_processed: AtomicU64::new(
                self.stats.step_results_processed.load(Ordering::Relaxed),
            ),
            task_requests_processed: AtomicU64::new(
                self.stats.task_requests_processed.load(Ordering::Relaxed),
            ),
            queue_management_processed: AtomicU64::new(
                self.stats
                    .queue_management_processed
                    .load(Ordering::Relaxed),
            ),
            unknown_events: AtomicU64::new(self.stats.unknown_events.load(Ordering::Relaxed)),
            connection_errors: AtomicU64::new(self.stats.connection_errors.load(Ordering::Relaxed)),
            last_event_at: Arc::new(tokio::sync::Mutex::new(
                *self.stats.last_event_at.lock().await,
            )),
            started_at: Arc::new(tokio::sync::Mutex::new(*self.stats.started_at.lock().await)),
        }
    }
}

/// Event handler for pgmq-notify events in orchestration queue listener
///
/// Handles pgmq-notify events and delegates them to orchestration operations via command pattern.
/// Uses config-driven classification to replace hardcoded string matching patterns.
struct OrchestrationEventHandler {
    /// Configuration
    config: OrchestrationListenerConfig,
    /// System context for database and messaging operations
    #[allow(dead_code)] // will be used in the future
    context: Arc<SystemContext>,
    /// Event sender for orchestration notifications
    event_sender: mpsc::Sender<OrchestrationNotification>,
    /// Channel monitor for observability (TAS-51)
    channel_monitor: ChannelMonitor,
    /// Listener identifier
    listener_id: Uuid,
    /// Statistics counters (shared with listener)
    stats: Arc<OrchestrationListenerStats>,
    /// Config-driven queue classifier for replacing hardcoded string matching
    queue_classifier: tasker_shared::config::QueueClassifier,
}

impl OrchestrationEventHandler {
    fn new(
        config: OrchestrationListenerConfig,
        context: Arc<SystemContext>,
        event_sender: mpsc::Sender<OrchestrationNotification>,
        channel_monitor: ChannelMonitor,
        listener_id: Uuid,
        stats: Arc<OrchestrationListenerStats>,
    ) -> Self {
        // TAS-61 V2: Access queues from common config
        let queue_config = context.tasker_config.common.queues.clone();
        let queue_classifier =
            tasker_shared::config::QueueClassifier::from_queues_config_v2(&queue_config);

        Self {
            config,
            context,
            event_sender,
            channel_monitor,
            listener_id,
            stats,
            queue_classifier,
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
                    listener_id = %self.listener_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    namespace = %msg_event.namespace,
                    "Received orchestration message ready event"
                );

                // Increment events received counter
                self.stats.events_received.fetch_add(1, Ordering::Relaxed);

                // Classify the message event using config-driven classification
                let queue_name = msg_event.queue_name.clone();
                let classified_event = tasker_shared::config::ConfigDrivenMessageEvent::classify(
                    msg_event.clone(),
                    &queue_name,
                    &self.queue_classifier,
                );

                debug!(
                    listener_id = %self.listener_id,
                    queue = %classified_event.inner().queue_name,
                    event_type = ?classified_event,
                    "Classified message event using config-driven enum-based dispatching"
                );

                // Convert to unified orchestration notification and send to event system
                let notification = match classified_event {
                    tasker_shared::config::ConfigDrivenMessageEvent::StepResults(event) => {
                        self.stats
                            .step_results_processed
                            .fetch_add(1, Ordering::Relaxed);

                        OrchestrationNotification::Event(OrchestrationQueueEvent::StepResult(event))
                    }
                    tasker_shared::config::ConfigDrivenMessageEvent::TaskRequests(event) => {
                        self.stats
                            .task_requests_processed
                            .fetch_add(1, Ordering::Relaxed);

                        OrchestrationNotification::Event(OrchestrationQueueEvent::TaskRequest(
                            event,
                        ))
                    }
                    tasker_shared::config::ConfigDrivenMessageEvent::TaskFinalizations(event) => {
                        self.stats
                            .queue_management_processed
                            .fetch_add(1, Ordering::Relaxed);

                        OrchestrationNotification::Event(OrchestrationQueueEvent::TaskFinalization(
                            event,
                        ))
                    }
                    tasker_shared::config::ConfigDrivenMessageEvent::WorkerNamespace {
                        namespace,
                        event,
                    } => {
                        // Log worker namespace messages for monitoring (these shouldn't normally be processed by orchestration)
                        debug!(
                            listener_id = %self.listener_id,
                            queue = %event.queue_name,
                            namespace = %namespace,
                            "Received worker namespace message in orchestration listener"
                        );
                        return Ok(());
                    }
                    tasker_shared::config::ConfigDrivenMessageEvent::Unknown(event) => {
                        self.stats.unknown_events.fetch_add(1, Ordering::Relaxed);
                        warn!(
                            listener_id = %self.listener_id,
                            queue = %event.queue_name,
                            msg_id = %event.msg_id,
                            "Received message event for unknown queue type"
                        );
                        OrchestrationNotification::Event(OrchestrationQueueEvent::Unknown {
                            queue_name: event.queue_name,
                            payload: format!("msg_id: {}", event.msg_id),
                        })
                    }
                };

                // Send notification to event system with channel monitoring (TAS-51)
                match self.event_sender.send(notification).await {
                    Ok(_) => {
                        // TAS-51: Record send and periodically check saturation (optimized)
                        if self.channel_monitor.record_send_success() {
                            self.channel_monitor
                                .check_and_warn_saturation(self.event_sender.capacity());
                        }
                    }
                    Err(e) => {
                        warn!(
                            listener_id = %self.listener_id,
                            error = %e,
                            "Failed to send orchestration notification to event system"
                        );
                    }
                }
            }
            PgmqNotifyEvent::QueueCreated(queue_event) => {
                info!(
                    listener_id = %self.listener_id,
                    queue = %queue_event.queue_name,
                    namespace = %queue_event.namespace,
                    "Queue created event received in orchestration listener"
                );

                // we are not currently doing anything with this event but we can later use it to trigger a workflow
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
            listener_id = %self.listener_id,
            channel = %channel,
            payload = %payload,
            error = %error,
            "Failed to parse PGMQ notification in orchestration listener"
        );

        self.stats.connection_errors.fetch_add(1, Ordering::Relaxed);

        let notification = OrchestrationNotification::ConnectionError(format!(
            "Parse error on channel {}: {}",
            channel, error
        ));

        // TAS-51: Channel monitoring for error notifications
        match self.event_sender.send(notification).await {
            Ok(_) => {
                if self.channel_monitor.record_send_success() {
                    self.channel_monitor
                        .check_and_warn_saturation(self.event_sender.capacity());
                }
            }
            Err(e) => {
                error!(
                    listener_id = %self.listener_id,
                    error = %e,
                    "Failed to send parse error notification to event system"
                );
            }
        }
    }

    async fn handle_connection_error(&self, error: pgmq_notify::PgmqNotifyError) {
        error!(
            listener_id = %self.listener_id,
            error = %error,
            "PGMQ notification connection error in orchestration listener"
        );

        self.stats.connection_errors.fetch_add(1, Ordering::Relaxed);

        let notification =
            OrchestrationNotification::ConnectionError(format!("Connection error: {}", error));

        // TAS-51: Channel monitoring for error notifications
        match self.event_sender.send(notification).await {
            Ok(_) => {
                if self.channel_monitor.record_send_success() {
                    self.channel_monitor
                        .check_and_warn_saturation(self.event_sender.capacity());
                }
            }
            Err(e) => {
                error!(
                    listener_id = %self.listener_id,
                    error = %e,
                    "Failed to send connection error notification to event system"
                );
            }
        }
    }
}
