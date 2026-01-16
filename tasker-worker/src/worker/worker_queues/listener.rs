//! Provider-agnostic listener for TAS-43 worker namespace queue events
//!
//! This module provides a robust queue listener that handles worker
//! namespace queue events, connection management, and event classification using the
//! provider abstraction from TAS-133.
//!
//! ## Architecture (TAS-133)
//!
//! The listener uses `messaging_provider().subscribe()` instead of PGMQ-specific
//! `PgmqNotifyListener`. This enables both PGMQ and RabbitMQ backends:
//!
//! - **PGMQ**: Returns `MessageNotification::Available` (signal only, fetch required)
//! - **RabbitMQ**: Returns `MessageNotification::Message` (full message delivered)
//!
//! The listener handles both notification types and converts them to provider-agnostic
//! `MessageEvent` for classification into domain events.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use futures::StreamExt;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;

use tasker_shared::messaging::service::{MessageEvent, MessageNotification};
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::{system_context::SystemContext, TaskerError, TaskerResult};

use super::events::{WorkerNotification, WorkerQueueEvent};
use crate::worker::channels::WorkerNotificationSender;

/// Provider-agnostic listener for worker namespace queue notifications
///
/// TAS-133: Uses `messaging_provider().subscribe()` instead of PGMQ-specific
/// `PgmqNotifyListener`. Manages subscription streams with automatic error handling
/// and config-driven event classification. Provides a unified interface for
/// receiving all types of worker namespace queue events from any messaging backend.
pub(crate) struct WorkerQueueListener {
    /// Listener identifier
    listener_id: Uuid,
    /// Configuration
    config: WorkerListenerConfig,
    /// System context
    context: Arc<SystemContext>,
    /// Event sender channel (TAS-133: NewType wrapper for type safety)
    event_sender: WorkerNotificationSender,
    /// Channel monitor for observability (TAS-51)
    channel_monitor: ChannelMonitor,
    /// Subscription task handles (TAS-133: replaces pgmq_listener)
    subscription_handles: Vec<JoinHandle<()>>,
    /// Running state (TAS-133: replaces is_connected)
    is_running: Arc<AtomicBool>,
    /// Statistics
    stats: Arc<WorkerListenerStats>,
}

impl std::fmt::Debug for WorkerQueueListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerQueueListener")
            .field("listener_id", &self.listener_id)
            .field("config", &self.config)
            .field("is_running", &self.is_running.load(Ordering::Relaxed))
            .field("subscription_count", &self.subscription_handles.len())
            .finish()
    }
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
    pub last_event_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
    /// Listener startup time
    pub started_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
}

impl WorkerQueueListener {
    /// Create new worker queue listener
    ///
    /// TAS-133: Initializes queue classifier from config for provider-agnostic
    /// event classification.
    pub async fn new(
        config: WorkerListenerConfig,
        context: Arc<SystemContext>,
        event_sender: WorkerNotificationSender,
        channel_monitor: ChannelMonitor,
    ) -> TaskerResult<Self> {
        let listener_id = Uuid::new_v4();

        info!(
            listener_id = %listener_id,
            supported_namespaces = ?config.supported_namespaces,
            channel_monitor = %channel_monitor.channel_name(),
            provider = %context.messaging_provider().provider_name(),
            "Creating WorkerQueueListener with provider abstraction"
        );

        Ok(Self {
            listener_id,
            config,
            context,
            event_sender,
            channel_monitor,
            subscription_handles: Vec::new(),
            is_running: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(WorkerListenerStats::default()),
        })
    }

    /// Start the worker queue listener
    ///
    /// TAS-133: Uses `messaging_provider().subscribe_many()` for efficient resource usage.
    /// For PGMQ, this uses a SINGLE PostgreSQL connection for all LISTEN channels,
    /// preventing connection pool exhaustion with many namespaces.
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            listener_id = %self.listener_id,
            supported_namespaces = ?self.config.supported_namespaces,
            provider = %self.context.messaging_provider().provider_name(),
            "Starting WorkerQueueListener with provider abstraction (subscribe_many)"
        );

        // TAS-133: Build queue names and subscribe to all at once for connection efficiency
        let provider = self.context.messaging_provider();
        let router = self.context.message_client.router();

        // Build queue names using router for consistency: worker_{namespace}_queue
        let queue_names: Vec<String> = self
            .config
            .supported_namespaces
            .iter()
            .map(|ns| router.step_queue(ns))
            .collect();

        // Build a map of queue_name -> namespace for later lookup
        let queue_to_namespace: std::collections::HashMap<String, String> = self
            .config
            .supported_namespaces
            .iter()
            .map(|ns| (router.step_queue(ns), ns.clone()))
            .collect();

        // Use subscribe_many for efficient connection sharing (especially PGMQ)
        let queue_name_refs: Vec<&str> = queue_names.iter().map(|s| s.as_str()).collect();
        let subscriptions = provider.subscribe_many(&queue_name_refs).map_err(|e| {
            TaskerError::WorkerError(format!("Failed to subscribe to queues: {}", e))
        })?;

        info!(
            listener_id = %self.listener_id,
            subscription_count = subscriptions.len(),
            "subscribe_many returned {} subscriptions",
            subscriptions.len()
        );

        // Spawn a task for each subscription stream
        for (queue_name, stream) in subscriptions {
            let namespace = queue_to_namespace
                .get(&queue_name)
                .cloned()
                .unwrap_or_else(|| queue_name.clone());

            let sender = self.event_sender.clone();
            let stats = self.stats.clone();
            let monitor = self.channel_monitor.clone();
            let listener_id = self.listener_id;
            let is_running = self.is_running.clone();
            let queue_name_owned = queue_name.clone();
            let namespace_owned = namespace.clone();

            let handle = tokio::spawn(async move {
                Self::process_subscription_stream(
                    stream,
                    sender,
                    stats,
                    namespace_owned,
                    monitor,
                    listener_id,
                    queue_name_owned,
                    is_running,
                )
                .await;
            });

            self.subscription_handles.push(handle);
        }

        self.is_running.store(true, Ordering::SeqCst);
        *self.stats.started_at.lock().await = Some(Instant::now());

        info!(
            listener_id = %self.listener_id,
            subscription_count = %self.subscription_handles.len(),
            "WorkerQueueListener started successfully with shared connection"
        );

        Ok(())
    }

    /// Process a subscription stream, converting notifications to domain events
    ///
    /// TAS-133: This is the core processing loop that handles `MessageNotification`
    /// from any provider and converts them to `WorkerQueueEvent`.
    async fn process_subscription_stream(
        mut stream: std::pin::Pin<Box<dyn futures::Stream<Item = MessageNotification> + Send>>,
        sender: WorkerNotificationSender,
        stats: Arc<WorkerListenerStats>,
        namespace: String,
        monitor: ChannelMonitor,
        listener_id: Uuid,
        queue_name: String,
        is_running: Arc<AtomicBool>,
    ) {
        info!(
            listener_id = %listener_id,
            queue = %queue_name,
            namespace = %namespace,
            "Starting subscription stream processing for worker namespace"
        );

        while let Some(notification) = stream.next().await {
            info!(
                listener_id = %listener_id,
                queue = %queue_name,
                namespace = %namespace,
                "Received notification from stream"
            );
            // Check if we should stop
            if !is_running.load(Ordering::Relaxed) {
                debug!(
                    listener_id = %listener_id,
                    queue = %queue_name,
                    "Subscription stream stopping (listener stopped)"
                );
                break;
            }

            stats.events_received.fetch_add(1, Ordering::Relaxed);

            // TAS-133: Route based on notification type
            // - Available (PGMQ): Signal-only, needs fetch by message ID
            // - Message (RabbitMQ): Full payload, can process directly
            let worker_notification = match notification {
                MessageNotification::Available {
                    queue_name: notif_queue,
                    msg_id,
                } => {
                    // PGMQ style: signal only, need to fetch message
                    let message_event =
                        MessageEvent::from_available(&notif_queue, msg_id, &namespace);

                    info!(
                        listener_id = %listener_id,
                        queue = %message_event.queue_name,
                        msg_id = %message_event.message_id,
                        namespace = %message_event.namespace,
                        "PGMQ signal-only notification, will fetch by msg_id"
                    );

                    let queue_event = Self::classify_event(&message_event);

                    // Update statistics
                    match &queue_event {
                        WorkerQueueEvent::StepMessage(_) => {
                            stats.step_messages_processed.fetch_add(1, Ordering::Relaxed);
                        }
                        WorkerQueueEvent::HealthCheck(_) => {
                            stats.health_checks_processed.fetch_add(1, Ordering::Relaxed);
                        }
                        WorkerQueueEvent::ConfigurationUpdate(_) => {
                            stats.config_updates_processed.fetch_add(1, Ordering::Relaxed);
                        }
                        WorkerQueueEvent::Unknown { .. } => {
                            stats.unknown_events.fetch_add(1, Ordering::Relaxed);
                        }
                    }

                    WorkerNotification::Event(queue_event)
                }
                MessageNotification::Message(queued_msg) => {
                    // RabbitMQ style: full message available, process directly
                    // TAS-133: Use StepMessageWithPayload to preserve the full message
                    // This avoids the lossy conversion to MessageEvent
                    let queue_name = queued_msg.queue_name().to_string();

                    info!(
                        listener_id = %listener_id,
                        queue = %queue_name,
                        namespace = %namespace,
                        provider = queued_msg.provider_name(),
                        "Full message received, routing to StepMessageWithPayload"
                    );

                    stats.step_messages_processed.fetch_add(1, Ordering::Relaxed);

                    WorkerNotification::StepMessageWithPayload(queued_msg)
                }
            };

            // Send the notification with channel monitoring (TAS-51)
            match sender.send(worker_notification).await {
                Ok(_) => {
                    // TAS-51: Record send and periodically check saturation (optimized)
                    if monitor.record_send_success() {
                        monitor.check_and_warn_saturation(sender.capacity());
                    }
                }
                Err(e) => {
                    warn!(
                        listener_id = %listener_id,
                        error = %e,
                        "Failed to send worker notification to event system"
                    );
                }
            }

            // Update last event timestamp
            *stats.last_event_at.lock().await = Some(Instant::now());
        }

        // Stream ended - could be intentional stop or connection loss
        if is_running.load(Ordering::Relaxed) {
            // Unexpected stream end
            warn!(
                listener_id = %listener_id,
                queue = %queue_name,
                "Subscription stream ended unexpectedly"
            );

            stats.connection_errors.fetch_add(1, Ordering::Relaxed);
        } else {
            debug!(
                listener_id = %listener_id,
                queue = %queue_name,
                "Subscription stream ended (listener stopped)"
            );
        }
    }

    /// Classify a message event into worker queue event types
    ///
    /// TAS-133: Uses provider-agnostic `MessageEvent` for classification
    fn classify_event(event: &MessageEvent) -> WorkerQueueEvent {
        let queue_name = &event.queue_name;

        // Most events in worker namespace queues will be step messages
        if queue_name.ends_with("_queue") {
            WorkerQueueEvent::StepMessage(event.clone())
        } else if queue_name.contains("_health") {
            WorkerQueueEvent::HealthCheck(event.clone())
        } else if queue_name.contains("_config") {
            WorkerQueueEvent::ConfigurationUpdate(event.clone())
        } else {
            WorkerQueueEvent::Unknown {
                queue_name: queue_name.clone(),
                payload: "Unclassified worker queue event".to_string(),
            }
        }
    }

    /// Stop the worker queue listener
    ///
    /// TAS-133: Signals subscription tasks to stop and aborts them.
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            listener_id = %self.listener_id,
            subscription_count = %self.subscription_handles.len(),
            "Stopping WorkerQueueListener"
        );

        // Signal all subscription tasks to stop
        self.is_running.store(false, Ordering::SeqCst);

        // Abort all subscription handles
        for handle in self.subscription_handles.drain(..) {
            handle.abort();
        }

        info!(
            listener_id = %self.listener_id,
            "WorkerQueueListener stopped successfully"
        );

        Ok(())
    }

    /// Check if listener is running and healthy
    ///
    /// TAS-133: Checks running state instead of PGMQ-specific connection state.
    #[expect(dead_code, reason = "Public API for health checks by event systems")]
    pub fn is_healthy(&self) -> bool {
        self.is_running.load(Ordering::Relaxed) && !self.subscription_handles.is_empty()
    }

    /// Get listener statistics
    #[expect(dead_code, reason = "Public API for monitoring listener statistics")]
    pub fn stats(&self) -> Arc<WorkerListenerStats> {
        self.stats.clone()
    }

    /// Get listener ID
    #[expect(dead_code, reason = "Public API for getting listener identifier")]
    pub fn listener_id(&self) -> Uuid {
        self.listener_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::messaging::service::MessageId;

    /// TAS-133 Phase 2 Validation: Test MessageEvent classification for step messages
    #[test]
    fn classify_event_step_message() {
        let event = MessageEvent::new("linear_workflow_queue", "linear_workflow", MessageId::from(42i64));
        let classified = WorkerQueueListener::classify_event(&event);

        match classified {
            WorkerQueueEvent::StepMessage(inner) => {
                assert_eq!(inner.queue_name, "linear_workflow_queue");
                assert_eq!(inner.namespace, "linear_workflow");
            }
            other => panic!("Expected StepMessage, got {:?}", other),
        }
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent classification for health check events
    #[test]
    fn classify_event_health_check() {
        let event = MessageEvent::new("worker_health", "worker", MessageId::from(1i64));
        let classified = WorkerQueueListener::classify_event(&event);

        match classified {
            WorkerQueueEvent::HealthCheck(inner) => {
                assert_eq!(inner.queue_name, "worker_health");
            }
            other => panic!("Expected HealthCheck, got {:?}", other),
        }
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent classification for config update events
    #[test]
    fn classify_event_config_update() {
        let event = MessageEvent::new("worker_config", "worker", MessageId::from(1i64));
        let classified = WorkerQueueListener::classify_event(&event);

        match classified {
            WorkerQueueEvent::ConfigurationUpdate(inner) => {
                assert_eq!(inner.queue_name, "worker_config");
            }
            other => panic!("Expected ConfigurationUpdate, got {:?}", other),
        }
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent classification for unknown events
    #[test]
    fn classify_event_unknown() {
        let event = MessageEvent::new("some_random_pattern", "unknown", MessageId::from(1i64));
        let classified = WorkerQueueListener::classify_event(&event);

        match classified {
            WorkerQueueEvent::Unknown { queue_name, payload } => {
                assert_eq!(queue_name, "some_random_pattern");
                assert!(payload.contains("Unclassified"));
            }
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent from Available notification
    #[test]
    fn message_event_from_available_with_msg_id() {
        let event = MessageEvent::from_available("test_queue", Some(42), "test_namespace");
        assert_eq!(event.queue_name, "test_queue");
        assert_eq!(event.namespace, "test_namespace");
        assert_eq!(event.message_id.as_str(), "42");
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent from Available with None msg_id
    #[test]
    fn message_event_from_available_without_msg_id() {
        let event = MessageEvent::from_available("test_queue", None, "test_namespace");
        assert_eq!(event.queue_name, "test_queue");
        assert_eq!(event.namespace, "test_namespace");
        assert_eq!(event.message_id.as_str(), "unknown");
    }

    /// TAS-133 Phase 2 Validation: Test listener stats initialization
    #[test]
    fn listener_stats_default_values() {
        let stats = WorkerListenerStats::default();
        assert_eq!(stats.events_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.step_messages_processed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.health_checks_processed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.config_updates_processed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_events.load(Ordering::Relaxed), 0);
        assert_eq!(stats.connection_errors.load(Ordering::Relaxed), 0);
    }

    /// TAS-133 Phase 2 Validation: Test listener config defaults
    #[test]
    fn listener_config_defaults() {
        let config = WorkerListenerConfig::default();
        assert!(config.supported_namespaces.contains(&"linear_workflow".to_string()));
        assert!(config.supported_namespaces.contains(&"diamond_workflow".to_string()));
        assert!(config.supported_namespaces.contains(&"order_fulfillment".to_string()));
        assert_eq!(config.max_retry_attempts, 10);
        assert!(config.batch_processing);
    }
}
