//! Provider-agnostic listener for TAS-43 orchestration queue events
//!
//! This module provides a robust queue listener that handles orchestration
//! queue events, connection management, and event classification using the
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

use tasker_shared::config::{ConfigDrivenMessageEvent, QueueClassifier};
use tasker_shared::messaging::service::{MessageEvent, MessageNotification};
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::{system_context::SystemContext, TaskerError, TaskerResult};

use super::events::OrchestrationQueueEvent;
use crate::orchestration::channels::OrchestrationNotificationSender;

/// Provider-agnostic listener for orchestration queue notifications
///
/// TAS-133: Uses `messaging_provider().subscribe()` instead of PGMQ-specific
/// `PgmqNotifyListener`. Manages subscription streams with automatic error handling
/// and config-driven event classification. Provides a unified interface for
/// receiving all types of orchestration queue events from any messaging backend.
pub struct OrchestrationQueueListener {
    /// Listener identifier
    listener_id: Uuid,
    /// Configuration
    config: OrchestrationListenerConfig,
    /// System context
    context: Arc<SystemContext>,
    /// Event sender channel (TAS-133: NewType wrapper for type safety)
    event_sender: OrchestrationNotificationSender,
    /// Channel monitor for observability (TAS-51)
    channel_monitor: ChannelMonitor,
    /// Config-driven queue classifier for event classification
    queue_classifier: QueueClassifier,
    /// Subscription task handles (TAS-133: replaces pgmq_listener)
    subscription_handles: Vec<JoinHandle<()>>,
    /// Running state (TAS-133: replaces is_connected)
    is_running: Arc<AtomicBool>,
    /// Statistics
    stats: Arc<OrchestrationListenerStats>,
}

impl std::fmt::Debug for OrchestrationQueueListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrationQueueListener")
            .field("listener_id", &self.listener_id)
            .field("config", &self.config)
            .field("is_running", &self.is_running.load(Ordering::Relaxed))
            .field("subscription_count", &self.subscription_handles.len())
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
    /// Classified queue event using structured classification (signal-only, requires fetch)
    /// Used by PGMQ which sends LISTEN/NOTIFY signals without payload
    Event(OrchestrationQueueEvent),
    /// Full step result message with payload (no fetch required)
    /// Used by RabbitMQ which delivers complete messages via basic_consume
    /// TAS-133: Enables proper RabbitMQ push-based message processing
    StepResultWithPayload(tasker_shared::messaging::service::QueuedMessage<Vec<u8>>),
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
    ///
    /// TAS-133: Initializes queue classifier from config for provider-agnostic
    /// event classification.
    pub async fn new(
        config: OrchestrationListenerConfig,
        context: Arc<SystemContext>,
        event_sender: OrchestrationNotificationSender,
        channel_monitor: ChannelMonitor,
    ) -> TaskerResult<Self> {
        let listener_id = Uuid::new_v4();

        // TAS-133: Create queue classifier from config for provider-agnostic classification
        let queue_config = context.tasker_config.common.queues.clone();
        let queue_classifier = QueueClassifier::from_queues_config(&queue_config);

        info!(
            listener_id = %listener_id,
            namespace = %config.namespace,
            monitored_queues = ?config.monitored_queues,
            channel_monitor = %channel_monitor.channel_name(),
            provider = %context.messaging_provider().provider_name(),
            "Creating OrchestrationQueueListener with provider abstraction"
        );

        Ok(Self {
            listener_id,
            config,
            context,
            event_sender,
            channel_monitor,
            queue_classifier,
            subscription_handles: Vec::new(),
            is_running: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(OrchestrationListenerStats::default()),
        })
    }

    /// Start the orchestration queue listener
    ///
    /// TAS-133: Uses `messaging_provider().subscribe_many()` for efficient resource usage.
    /// For PGMQ, this uses a SINGLE PostgreSQL connection for all LISTEN channels,
    /// preventing connection pool exhaustion when monitoring multiple queues.
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            listener_id = %self.listener_id,
            namespace = %self.config.namespace,
            provider = %self.context.messaging_provider().provider_name(),
            "Starting OrchestrationQueueListener with provider abstraction (subscribe_many)"
        );

        // TAS-133: Subscribe to all monitored queues at once for connection efficiency
        let provider = self.context.messaging_provider();

        // Use subscribe_many for efficient connection sharing (especially PGMQ)
        let queue_name_refs: Vec<&str> = self
            .config
            .monitored_queues
            .iter()
            .map(|s| s.as_str())
            .collect();

        let subscriptions = provider.subscribe_many(&queue_name_refs).map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to subscribe to queues: {}", e))
        })?;

        info!(
            listener_id = %self.listener_id,
            subscription_count = subscriptions.len(),
            "subscribe_many returned {} subscriptions",
            subscriptions.len()
        );

        // Spawn a task for each subscription stream
        for (queue_name, stream) in subscriptions {
            let sender = self.event_sender.clone();
            let classifier = self.queue_classifier.clone();
            let stats = self.stats.clone();
            let namespace = self.config.namespace.clone();
            let monitor = self.channel_monitor.clone();
            let listener_id = self.listener_id;
            let is_running = self.is_running.clone();
            let queue_name_owned = queue_name.clone();

            // TAS-158: Named spawn for tokio-console visibility
            let handle = tasker_shared::spawn_named!("orchestration_queue_listener", async move {
                Self::process_subscription_stream(
                    stream,
                    sender,
                    classifier,
                    stats,
                    namespace,
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
            "OrchestrationQueueListener started successfully with shared connection"
        );

        Ok(())
    }

    /// Process a subscription stream, converting notifications to domain events
    ///
    /// TAS-133: This is the core processing loop that handles `MessageNotification`
    /// from any provider and converts them to `OrchestrationQueueEvent`.
    async fn process_subscription_stream(
        mut stream: std::pin::Pin<Box<dyn futures::Stream<Item = MessageNotification> + Send>>,
        sender: OrchestrationNotificationSender,
        classifier: QueueClassifier,
        stats: Arc<OrchestrationListenerStats>,
        namespace: String,
        monitor: ChannelMonitor,
        listener_id: Uuid,
        queue_name: String,
        is_running: Arc<AtomicBool>,
    ) {
        debug!(
            listener_id = %listener_id,
            queue = %queue_name,
            "Starting subscription stream processing"
        );

        while let Some(notification) = stream.next().await {
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
            let orchestration_notification = match notification {
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

                    // Classify and create domain event
                    let classified = ConfigDrivenMessageEvent::classify(
                        message_event.clone(),
                        &message_event.queue_name,
                        &classifier,
                    );

                    match classified {
                        ConfigDrivenMessageEvent::StepResults(event) => {
                            stats.step_results_processed.fetch_add(1, Ordering::Relaxed);
                            OrchestrationNotification::Event(OrchestrationQueueEvent::StepResult(
                                event,
                            ))
                        }
                        ConfigDrivenMessageEvent::TaskRequests(event) => {
                            stats
                                .task_requests_processed
                                .fetch_add(1, Ordering::Relaxed);
                            OrchestrationNotification::Event(OrchestrationQueueEvent::TaskRequest(
                                event,
                            ))
                        }
                        ConfigDrivenMessageEvent::TaskFinalizations(event) => {
                            stats
                                .queue_management_processed
                                .fetch_add(1, Ordering::Relaxed);
                            OrchestrationNotification::Event(
                                OrchestrationQueueEvent::TaskFinalization(event),
                            )
                        }
                        ConfigDrivenMessageEvent::WorkerNamespace {
                            namespace: worker_ns,
                            event,
                        } => {
                            debug!(
                                listener_id = %listener_id,
                                queue = %event.queue_name,
                                namespace = %worker_ns,
                                "Received worker namespace message in orchestration listener"
                            );
                            continue;
                        }
                        ConfigDrivenMessageEvent::Unknown(event) => {
                            stats.unknown_events.fetch_add(1, Ordering::Relaxed);
                            warn!(
                                listener_id = %listener_id,
                                queue = %event.queue_name,
                                "Unknown event type, skipping"
                            );
                            continue;
                        }
                    }
                }
                MessageNotification::Message(queued_msg) => {
                    // RabbitMQ style: full message available, process directly
                    // TAS-133: Use StepResultWithPayload to preserve the full message
                    let queue_name_str = queued_msg.queue_name().to_string();

                    info!(
                        listener_id = %listener_id,
                        queue = %queue_name_str,
                        namespace = %namespace,
                        provider = queued_msg.provider_name(),
                        "Full message received, routing to StepResultWithPayload"
                    );

                    // For now, assume step_results queue messages are step results
                    // The event system will deserialize and process the full payload
                    stats.step_results_processed.fetch_add(1, Ordering::Relaxed);
                    OrchestrationNotification::StepResultWithPayload(queued_msg)
                }
            };

            // Send the notification with channel monitoring
            match sender.send(orchestration_notification).await {
                Ok(()) => {
                    // TAS-51: Record send success and check saturation
                    if monitor.record_send_success() {
                        monitor.check_and_warn_saturation(sender.capacity());
                    }
                    debug!(
                        listener_id = %listener_id,
                        "Notification sent to event system"
                    );
                }
                Err(e) => {
                    warn!(
                        listener_id = %listener_id,
                        error = %e,
                        "Failed to send notification to event system"
                    );
                    stats.connection_errors.fetch_add(1, Ordering::Relaxed);
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

            let _ = sender
                .send(OrchestrationNotification::ConnectionError(format!(
                    "Subscription stream for '{}' ended unexpectedly",
                    queue_name
                )))
                .await;
        } else {
            debug!(
                listener_id = %listener_id,
                queue = %queue_name,
                "Subscription stream ended (listener stopped)"
            );
        }
    }

    /// Stop the orchestration queue listener
    ///
    /// TAS-133: Signals subscription tasks to stop and aborts them.
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            listener_id = %self.listener_id,
            subscription_count = %self.subscription_handles.len(),
            "Stopping OrchestrationQueueListener"
        );

        // Signal all subscription tasks to stop
        self.is_running.store(false, Ordering::SeqCst);

        // Abort all subscription handles
        for handle in self.subscription_handles.drain(..) {
            handle.abort();
        }

        info!(
            listener_id = %self.listener_id,
            "OrchestrationQueueListener stopped successfully"
        );

        Ok(())
    }

    /// Check if listener is running and healthy
    ///
    /// TAS-133: Checks running state instead of PGMQ-specific connection state.
    pub fn is_healthy(&self) -> bool {
        self.is_running.load(Ordering::Relaxed) && !self.subscription_handles.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::config::queues::OrchestrationOwnedQueues;
    use tasker_shared::messaging::service::MessageId;

    fn create_test_classifier() -> QueueClassifier {
        let orchestration_owned = OrchestrationOwnedQueues {
            step_results: "orchestration_step_results".to_string(),
            task_requests: "orchestration_task_requests".to_string(),
            task_finalizations: "orchestration_task_finalizations".to_string(),
        };
        QueueClassifier::new(
            orchestration_owned,
            "orchestration".to_string(),
            "worker".to_string(),
        )
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent classification for step results
    #[test]
    fn classify_event_step_results() {
        let classifier = create_test_classifier();
        let event = MessageEvent::new(
            "orchestration_step_results",
            "orchestration",
            MessageId::from(42i64),
        );

        let classified =
            ConfigDrivenMessageEvent::classify(event.clone(), &event.queue_name, &classifier);

        match classified {
            ConfigDrivenMessageEvent::StepResults(inner) => {
                assert_eq!(inner.queue_name, "orchestration_step_results");
                assert_eq!(inner.namespace, "orchestration");
            }
            other => panic!("Expected StepResults, got {:?}", other),
        }
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent classification for task requests
    #[test]
    fn classify_event_task_requests() {
        let classifier = create_test_classifier();
        let event = MessageEvent::new(
            "orchestration_task_requests",
            "orchestration",
            MessageId::from(1i64),
        );

        let classified =
            ConfigDrivenMessageEvent::classify(event.clone(), &event.queue_name, &classifier);

        match classified {
            ConfigDrivenMessageEvent::TaskRequests(inner) => {
                assert_eq!(inner.queue_name, "orchestration_task_requests");
            }
            other => panic!("Expected TaskRequests, got {:?}", other),
        }
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent classification for task finalizations
    #[test]
    fn classify_event_task_finalizations() {
        let classifier = create_test_classifier();
        let event = MessageEvent::new(
            "orchestration_task_finalizations",
            "orchestration",
            MessageId::from(1i64),
        );

        let classified =
            ConfigDrivenMessageEvent::classify(event.clone(), &event.queue_name, &classifier);

        match classified {
            ConfigDrivenMessageEvent::TaskFinalizations(inner) => {
                assert_eq!(inner.queue_name, "orchestration_task_finalizations");
            }
            other => panic!("Expected TaskFinalizations, got {:?}", other),
        }
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent classification for worker namespace
    #[test]
    fn classify_event_worker_namespace() {
        let classifier = create_test_classifier();
        let event = MessageEvent::new("worker_fulfillment_queue", "worker", MessageId::from(1i64));

        let classified =
            ConfigDrivenMessageEvent::classify(event.clone(), &event.queue_name, &classifier);

        match classified {
            ConfigDrivenMessageEvent::WorkerNamespace { namespace, event } => {
                assert_eq!(namespace, "fulfillment");
                assert_eq!(event.queue_name, "worker_fulfillment_queue");
            }
            other => panic!("Expected WorkerNamespace, got {:?}", other),
        }
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent from Available notification
    #[test]
    fn message_event_from_available_with_msg_id() {
        let event = MessageEvent::from_available("test_queue", Some(123), "orchestration");
        assert_eq!(event.queue_name, "test_queue");
        assert_eq!(event.namespace, "orchestration");
        assert_eq!(event.message_id.as_str(), "123");
    }

    /// TAS-133 Phase 2 Validation: Test MessageEvent from Available with None msg_id
    #[test]
    fn message_event_from_available_without_msg_id() {
        let event = MessageEvent::from_available("test_queue", None, "orchestration");
        assert_eq!(event.queue_name, "test_queue");
        assert_eq!(event.namespace, "orchestration");
        assert_eq!(event.message_id.as_str(), "unknown");
    }

    /// TAS-133 Phase 2 Validation: Test listener stats initialization
    #[test]
    fn listener_stats_default_values() {
        let stats = OrchestrationListenerStats::default();
        assert_eq!(stats.events_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.step_results_processed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.task_requests_processed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.queue_management_processed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_events.load(Ordering::Relaxed), 0);
        assert_eq!(stats.connection_errors.load(Ordering::Relaxed), 0);
    }

    /// TAS-133 Phase 2 Validation: Test listener config defaults
    #[test]
    fn listener_config_defaults() {
        let config = OrchestrationListenerConfig::default();
        assert_eq!(config.namespace, "orchestration");
        assert!(config
            .monitored_queues
            .contains(&"orchestration_step_results".to_string()));
        assert!(config
            .monitored_queues
            .contains(&"orchestration_task_requests".to_string()));
        assert_eq!(config.max_retry_attempts, 10);
    }

    /// TAS-133 Phase 2 Validation: Test notification enum variants
    #[test]
    fn notification_variants() {
        let event = OrchestrationQueueEvent::Unknown {
            queue_name: "test".to_string(),
            payload: "test".to_string(),
        };
        let notification = OrchestrationNotification::Event(event);
        assert!(matches!(notification, OrchestrationNotification::Event(_)));

        let error_notification =
            OrchestrationNotification::ConnectionError("test error".to_string());
        assert!(matches!(
            error_notification,
            OrchestrationNotification::ConnectionError(_)
        ));

        let reconnected = OrchestrationNotification::Reconnected;
        assert!(matches!(
            reconnected,
            OrchestrationNotification::Reconnected
        ));
    }
}
