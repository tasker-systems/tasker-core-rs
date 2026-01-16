//! # PGMQ Messaging Service
//!
//! PostgreSQL Message Queue implementation via pgmq-notify crate.
//!
//! ## Features
//!
//! - **LISTEN/NOTIFY Support**: Event-driven message processing
//! - **Visibility Timeout**: Built-in PGMQ visibility semantics
//! - **Atomic Operations**: Database transaction guarantees
//! - **Full MessagingService Implementation**: Complete API compatibility
//! - **Push Notifications (TAS-133)**: Signal-only push via pg_notify

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use pgmq_notify::{PgmqClient, PgmqNotifyConfig};
use sqlx::PgPool;

use crate::messaging::service::traits::{
    MessagingService, NotificationStream, QueueMessage, SupportsPushNotifications,
};
use crate::messaging::service::types::{
    MessageHandle, MessageId, MessageMetadata, MessageNotification, QueueHealthReport, QueueStats,
    QueuedMessage, ReceiptHandle,
};
use crate::messaging::MessagingError;

/// PGMQ-based messaging service implementation
///
/// Wraps the `pgmq_notify::PgmqClient` to provide the `MessagingService` trait interface.
/// Supports all standard PGMQ operations plus LISTEN/NOTIFY for event-driven processing.
///
/// # Example
///
/// ```ignore
/// use tasker_shared::messaging::service::providers::PgmqMessagingService;
/// use tasker_shared::messaging::service::MessagingService;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let service = PgmqMessagingService::new("postgresql://localhost/tasker").await?;
///
/// // Create a queue
/// service.ensure_queue("my_queue").await?;
///
/// // Send a message
/// let msg_id = service.send_message("my_queue", &serde_json::json!({"key": "value"})).await?;
///
/// // Receive messages
/// let messages = service.receive_messages::<serde_json::Value>(
///     "my_queue",
///     10,
///     Duration::from_secs(30),
/// ).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct PgmqMessagingService {
    /// Underlying PGMQ client
    client: PgmqClient,
}

impl PgmqMessagingService {
    /// Create a new PGMQ messaging service from database URL
    ///
    /// Uses default `PgmqNotifyConfig`. For custom configuration, use `new_with_config`.
    pub async fn new(database_url: &str) -> Result<Self, MessagingError> {
        let client = PgmqClient::new(database_url)
            .await
            .map_err(|e| MessagingError::connection(e.to_string()))?;
        Ok(Self { client })
    }

    /// Create a new PGMQ messaging service with database URL and custom configuration
    ///
    /// Allows configuring notify behavior, queue naming patterns, and other options.
    pub async fn new_with_config(
        database_url: &str,
        config: PgmqNotifyConfig,
    ) -> Result<Self, MessagingError> {
        let client = PgmqClient::new_with_config(database_url, config)
            .await
            .map_err(|e| MessagingError::connection(e.to_string()))?;
        Ok(Self { client })
    }

    /// Create a new PGMQ messaging service with an existing connection pool
    ///
    /// Uses default `PgmqNotifyConfig`. For custom configuration, use `new_with_pool_and_config`.
    /// This is the preferred constructor when pool configuration is managed externally
    /// (e.g., from TOML config via SystemContext).
    pub async fn new_with_pool(pool: PgPool) -> Self {
        let client = PgmqClient::new_with_pool(pool).await;
        Self { client }
    }

    /// Create a new PGMQ messaging service with existing pool and custom configuration
    ///
    /// Combines externally-managed pool configuration with custom notify behavior.
    /// Use this when you need both pool tuning (from TOML) and notify configuration.
    pub async fn new_with_pool_and_config(pool: PgPool, config: PgmqNotifyConfig) -> Self {
        let client = PgmqClient::new_with_pool_and_config(pool, config).await;
        Self { client }
    }

    /// Create from an existing PgmqClient
    ///
    /// Escape hatch for when you need full control over client construction.
    pub fn from_client(client: PgmqClient) -> Self {
        Self { client }
    }

    /// Get a reference to the underlying PgmqClient
    pub fn client(&self) -> &PgmqClient {
        &self.client
    }

    /// Get a reference to the underlying connection pool
    pub fn pool(&self) -> &PgPool {
        self.client.pool()
    }

    /// Check if this service has LISTEN/NOTIFY capabilities
    pub fn has_notify_capabilities(&self) -> bool {
        self.client.has_notify_capabilities()
    }
}

#[async_trait]
impl MessagingService for PgmqMessagingService {
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError> {
        self.client
            .create_queue(queue_name)
            .await
            .map_err(|e| MessagingError::queue_creation(queue_name, e.to_string()))
    }

    async fn verify_queues(
        &self,
        queue_names: &[String],
    ) -> Result<QueueHealthReport, MessagingError> {
        let mut report = QueueHealthReport::new();

        for queue_name in queue_names {
            // Try to get metrics for the queue to verify it exists
            match self.client.queue_metrics(queue_name).await {
                Ok(_) => report.add_healthy(queue_name),
                Err(e) => {
                    // Check if it's a "queue not found" error or something else
                    let error_str = e.to_string();
                    if error_str.contains("does not exist") || error_str.contains("not found") {
                        report.add_missing(queue_name);
                    } else {
                        report.add_error(queue_name, error_str);
                    }
                }
            }
        }

        Ok(report)
    }

    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<MessageId, MessagingError> {
        // Serialize to JSON Value for PGMQ
        let json_value: serde_json::Value = serde_json::from_slice(&message.to_bytes()?)
            .map_err(|e| MessagingError::serialization(e.to_string()))?;

        let msg_id = self
            .client
            .send_json_message(queue_name, &json_value)
            .await
            .map_err(|e| MessagingError::send(queue_name, e.to_string()))?;

        Ok(MessageId::from(msg_id))
    }

    async fn send_batch<T: QueueMessage>(
        &self,
        queue_name: &str,
        messages: &[T],
    ) -> Result<Vec<MessageId>, MessagingError> {
        // PGMQ doesn't have a native batch send, so we send one by one
        // This could be optimized with a transaction in the future
        let mut ids = Vec::with_capacity(messages.len());

        for message in messages {
            let id = self.send_message(queue_name, message).await?;
            ids.push(id);
        }

        Ok(ids)
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        let vt_seconds = visibility_timeout.as_secs() as i32;

        let messages = self
            .client
            .read_messages(queue_name, Some(vt_seconds), Some(max_messages as i32))
            .await
            .map_err(|e| MessagingError::receive(queue_name, e.to_string()))?;

        let mut result = Vec::with_capacity(messages.len());

        for msg in messages {
            // Serialize the message to bytes then deserialize to T
            let bytes = serde_json::to_vec(&msg.message)
                .map_err(|e| MessagingError::serialization(e.to_string()))?;

            let deserialized = T::from_bytes(&bytes)?;

            // TAS-133: Use explicit MessageHandle::Pgmq for provider-agnostic message wrapper
            result.push(QueuedMessage::with_handle(
                deserialized,
                MessageHandle::Pgmq {
                    msg_id: msg.msg_id,
                    queue_name: queue_name.to_string(),
                },
                MessageMetadata::new(msg.read_ct as u32, msg.enqueued_at),
            ));
        }

        Ok(result)
    }

    async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError> {
        let message_id = receipt_handle
            .as_i64()
            .ok_or_else(|| MessagingError::invalid_receipt_handle(receipt_handle.as_str()))?;

        // Archive the message (PGMQ's equivalent of ack)
        self.client
            .archive_message(queue_name, message_id)
            .await
            .map_err(|e| MessagingError::ack(queue_name, message_id, e.to_string()))
    }

    async fn nack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        requeue: bool,
    ) -> Result<(), MessagingError> {
        let message_id = receipt_handle
            .as_i64()
            .ok_or_else(|| MessagingError::invalid_receipt_handle(receipt_handle.as_str()))?;

        if requeue {
            // Make the message visible again by setting visibility timeout to 0
            // PGMQ doesn't have a direct "nack" operation, so we use set_visibility_timeout
            self.client
                .set_visibility_timeout(queue_name, message_id, 0)
                .await
                .map_err(|e| MessagingError::nack(queue_name, message_id, e.to_string()))?;
        } else {
            // Delete the message (dead-letter behavior)
            self.client
                .delete_message(queue_name, message_id)
                .await
                .map_err(|e| MessagingError::nack(queue_name, message_id, e.to_string()))?;
        }

        Ok(())
    }

    async fn extend_visibility(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        extension: Duration,
    ) -> Result<(), MessagingError> {
        let message_id = receipt_handle
            .as_i64()
            .ok_or_else(|| MessagingError::invalid_receipt_handle(receipt_handle.as_str()))?;

        let vt_seconds = extension.as_secs() as i32;

        self.client
            .set_visibility_timeout(queue_name, message_id, vt_seconds)
            .await
            .map_err(|e| {
                MessagingError::extend_visibility(queue_name, message_id, e.to_string())
            })?;

        Ok(())
    }

    async fn queue_stats(&self, queue_name: &str) -> Result<QueueStats, MessagingError> {
        let metrics = self
            .client
            .queue_metrics(queue_name)
            .await
            .map_err(|e| MessagingError::queue_stats(queue_name, e.to_string()))?;

        let mut stats = QueueStats::new(queue_name, metrics.message_count as u64);

        if let Some(age_seconds) = metrics.oldest_message_age_seconds {
            stats = stats.with_oldest_message_age_ms((age_seconds * 1000) as u64);
        }

        // PGMQ metrics don't include in-flight count directly, but we could query it
        // For now, we leave it as None

        Ok(stats)
    }

    async fn health_check(&self) -> Result<bool, MessagingError> {
        self.client
            .health_check()
            .await
            .map_err(|e| MessagingError::health_check(e.to_string()))
    }

    fn provider_name(&self) -> &'static str {
        "pgmq"
    }
}

// =============================================================================
// SupportsPushNotifications Implementation (TAS-133)
// =============================================================================

impl SupportsPushNotifications for PgmqMessagingService {
    /// Subscribe to push notifications for a PGMQ queue
    ///
    /// PGMQ uses PostgreSQL LISTEN/NOTIFY for push notifications with two modes (TAS-133):
    ///
    /// - **Small messages (< 7KB)**: Full payload included in notification via `MessageWithPayload`
    ///   event, returned as `MessageNotification::Message` - process directly without fetch
    /// - **Large messages (>= 7KB)**: Signal-only via `MessageReady` event, returned as
    ///   `MessageNotification::Available` with `msg_id` - fetch via `read_specific_message()`
    ///
    /// # Implementation Notes
    ///
    /// This creates a new `PgmqNotifyListener` for each subscription, which:
    /// 1. Opens a dedicated connection for LISTEN
    /// 2. Listens to the queue's notification channel
    /// 3. Returns `MessageNotification::Message` for small messages (< 7KB)
    /// 4. Returns `MessageNotification::Available` with `msg_id` for large messages
    ///
    /// # Fallback Polling
    ///
    /// **Important**: pg_notify is not guaranteed delivery. Notifications can be
    /// lost under load or if the listener disconnects. Consumers should always
    /// implement fallback polling (see `requires_fallback_polling()`).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use futures::StreamExt;
    ///
    /// let mut stream = service.subscribe("my_queue")?;
    /// while let Some(notification) = stream.next().await {
    ///     match notification {
    ///         MessageNotification::Message(queued_msg) => {
    ///             // TAS-133: Small message - full payload available, process directly
    ///             process(queued_msg);
    ///         }
    ///         MessageNotification::Available { queue_name, msg_id } => {
    ///             // Large message - fetch using msg_id
    ///             if let Some(id) = msg_id {
    ///                 let msg = service.read_specific_message(&queue_name, id).await?;
    ///                 process(msg);
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    fn subscribe(
        &self,
        queue_name: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError> {
        // TAS-133: Create a notification stream for this queue
        //
        // Note: This is a simplified implementation that creates a stream from
        // a spawned task with an mpsc channel. A production implementation would
        // likely use a shared listener pool or more sophisticated resource management.
        let pool = self.pool().clone();
        let queue_name = queue_name.to_string();
        let config = self.client.config().clone();

        // Create a channel to bridge the async listener to the stream
        let (tx, rx) = tokio::sync::mpsc::channel::<MessageNotification>(100);

        // Spawn a task to manage the listener lifecycle
        tokio::spawn(async move {
            use pgmq_notify::listener::PgmqEventHandler;
            use pgmq_notify::{PgmqNotifyEvent, PgmqNotifyListener};
            use tracing::{debug, error, warn};

            // Create and connect listener
            // TAS-51: Use default buffer size from config or fallback
            let buffer_size = 100; // Default buffer size
            let mut listener =
                match PgmqNotifyListener::new(pool, config.clone(), buffer_size).await {
                    Ok(l) => l,
                    Err(e) => {
                        error!("Failed to create PGMQ listener for {}: {}", queue_name, e);
                        return;
                    }
                };

            if let Err(e) = listener.connect().await {
                error!("Failed to connect PGMQ listener for {}: {}", queue_name, e);
                return;
            }

            // Extract namespace from queue name (convention: namespace_queue)
            let namespace = queue_name
                .rsplit('_')
                .next_back()
                .map(|s| s.to_string())
                .unwrap_or_else(|| queue_name.clone());

            // Listen to the message ready channel for this namespace
            let channel = config.message_ready_channel(&namespace);
            if let Err(e) = listener.listen_channel(&channel).await {
                error!(
                    "Failed to listen to channel {} for {}: {}",
                    channel, queue_name, e
                );
                return;
            }

            debug!(
                "PGMQ subscription started for queue: {} (channel: {})",
                queue_name, channel
            );

            // Create an event handler that sends notifications to the channel
            // TAS-133: Handles both small messages (full payload) and large messages (signal only)
            struct NotificationForwarder {
                tx: tokio::sync::mpsc::Sender<MessageNotification>,
                target_queue: String,
            }

            #[async_trait::async_trait]
            impl PgmqEventHandler for NotificationForwarder {
                async fn handle_event(
                    &self,
                    event: PgmqNotifyEvent,
                ) -> pgmq_notify::error::Result<()> {
                    // Only forward events for our target queue
                    if event.queue_name() != self.target_queue {
                        return Ok(());
                    }

                    // TAS-133: Convert event to appropriate MessageNotification variant
                    let notification = match &event {
                        PgmqNotifyEvent::MessageWithPayload(e) => {
                            // Small message (< 7KB): Full payload included
                            // Convert to MessageNotification::Message for direct processing
                            let handle = MessageHandle::Pgmq {
                                msg_id: e.msg_id,
                                queue_name: e.queue_name.clone(),
                            };
                            let metadata = MessageMetadata {
                                receive_count: 0, // First receive
                                enqueued_at: e.ready_at,
                            };
                            // Serialize the JSON payload to bytes
                            let payload_bytes =
                                serde_json::to_vec(&e.message).unwrap_or_else(|_| Vec::new());
                            let queued_msg =
                                QueuedMessage::with_handle(payload_bytes, handle, metadata);
                            MessageNotification::message(queued_msg)
                        }
                        PgmqNotifyEvent::MessageReady(e) => {
                            // Large message (>= 7KB): Signal only with msg_id
                            // Consumer must fetch via read_specific_message()
                            MessageNotification::available_with_msg_id(
                                e.queue_name.clone(),
                                e.msg_id,
                            )
                        }
                        PgmqNotifyEvent::QueueCreated(_) => {
                            // Queue creation events don't generate message notifications
                            debug!("Ignoring queue created event for notification stream");
                            return Ok(());
                        }
                        PgmqNotifyEvent::BatchReady(e) => {
                            // Batch events could be handled, but for now use Available
                            // The first msg_id in the batch if available
                            let msg_id = e.msg_ids.first().copied();
                            match msg_id {
                                Some(id) => MessageNotification::available_with_msg_id(
                                    e.queue_name.clone(),
                                    id,
                                ),
                                None => MessageNotification::available(e.queue_name.clone()),
                            }
                        }
                    };

                    if self.tx.send(notification).await.is_err() {
                        warn!("Notification receiver dropped");
                    }
                    Ok(())
                }
            }

            let handler = NotificationForwarder {
                tx,
                target_queue: queue_name.clone(),
            };

            // Run the listener loop - this blocks until disconnection
            if let Err(e) = listener.listen_with_handler(handler).await {
                error!("PGMQ listener error for {}: {}", queue_name, e);
            }

            debug!("PGMQ subscription ended for queue: {}", queue_name);
        });

        // Convert the mpsc receiver to a Stream using futures::stream::unfold
        let stream = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });

        Ok(Box::pin(stream))
    }

    /// PGMQ requires fallback polling
    ///
    /// PostgreSQL LISTEN/NOTIFY is **not guaranteed delivery**:
    /// - Notifications can be lost under heavy load
    /// - Notifications are missed if the listener disconnects
    /// - Messages enqueued before subscription started won't trigger notifications
    ///
    /// Always combine PGMQ push notifications with periodic polling.
    fn requires_fallback_polling(&self) -> bool {
        true
    }

    /// Recommended fallback polling interval for PGMQ
    ///
    /// Returns a 5-second interval by default. This provides a good balance between:
    /// - Catching missed notifications quickly
    /// - Not overloading the database with frequent polls
    fn fallback_polling_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(5))
    }

    /// PGMQ supports fetching messages by ID after signal-only notifications
    ///
    /// Returns `true` because PGMQ's large message flow (>7KB) sends signal-only
    /// notifications containing only the message ID. Consumers must use
    /// `read_specific_message(msg_id)` to fetch the full message content.
    ///
    /// This is the flow handled by `ExecuteStepFromEventMessage` in the worker.
    fn supports_fetch_by_message_id(&self) -> bool {
        true
    }

    /// Subscribe to multiple queues using a SINGLE shared PostgreSQL connection
    ///
    /// This is a critical optimization for PGMQ. PostgreSQL LISTEN can listen to
    /// multiple channels on one connection. Without this, each `subscribe()` call
    /// creates a separate connection, quickly exhausting the pool.
    ///
    /// # Resource Efficiency
    ///
    /// - **Without `subscribe_many`**: N queues = N connections held permanently
    /// - **With `subscribe_many`**: N queues = 1 connection held permanently
    ///
    /// For workers with 20+ namespaces, this is the difference between pool
    /// exhaustion and healthy operation.
    fn subscribe_many(
        &self,
        queue_names: &[&str],
    ) -> Result<Vec<(String, NotificationStream)>, MessagingError> {
        use std::collections::HashMap;
        use std::sync::Arc;
        use tokio::sync::RwLock;

        if queue_names.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.pool().clone();
        let config = self.client.config().clone();

        // Create per-queue channels for demultiplexing
        // Map: queue_name -> sender
        let mut queue_senders: HashMap<String, tokio::sync::mpsc::Sender<MessageNotification>> =
            HashMap::new();
        let mut result_streams: Vec<(String, NotificationStream)> = Vec::new();

        for queue_name in queue_names {
            let (tx, rx) = tokio::sync::mpsc::channel::<MessageNotification>(100);
            queue_senders.insert(queue_name.to_string(), tx);

            // Convert receiver to stream
            let stream = futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|item| (item, rx))
            });
            result_streams.push((
                queue_name.to_string(),
                Box::pin(stream) as NotificationStream,
            ));
        }

        // Collect unique channels to listen to
        let channels_to_listen: Vec<String> = queue_names
            .iter()
            .map(|q| {
                // Extract namespace from queue name (convention: {namespace}_queue)
                let namespace = q
                    .rsplit('_')
                    .next_back()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| q.to_string());
                config.message_ready_channel(&namespace)
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Wrap senders in Arc<RwLock> for shared access in the spawned task
        let senders = Arc::new(RwLock::new(queue_senders));
        let queue_names_owned: Vec<String> = queue_names.iter().map(|s| s.to_string()).collect();

        // Spawn a SINGLE task with ONE listener for all queues
        tokio::spawn(async move {
            use pgmq_notify::listener::PgmqEventHandler;
            use pgmq_notify::{PgmqNotifyEvent, PgmqNotifyListener};
            use tracing::{debug, error, info};

            // Create and connect ONE listener
            let buffer_size = 100;
            let mut listener =
                match PgmqNotifyListener::new(pool, config.clone(), buffer_size).await {
                    Ok(l) => l,
                    Err(e) => {
                        error!(
                            "Failed to create shared PGMQ listener for {} queues: {}",
                            queue_names_owned.len(),
                            e
                        );
                        return;
                    }
                };

            if let Err(e) = listener.connect().await {
                error!(
                    "Failed to connect shared PGMQ listener for {} queues: {}",
                    queue_names_owned.len(),
                    e
                );
                return;
            }

            // Listen to ALL channels on this ONE connection
            for channel in &channels_to_listen {
                if let Err(e) = listener.listen_channel(channel).await {
                    error!("Failed to listen to channel {}: {}", channel, e);
                    // Continue - try to listen to other channels
                }
            }

            info!(
                queue_count = queue_names_owned.len(),
                channel_count = channels_to_listen.len(),
                "PGMQ subscribe_many started with SINGLE shared connection"
            );

            // Event handler that demultiplexes to per-queue senders
            struct DemultiplexingForwarder {
                senders:
                    Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<MessageNotification>>>>,
                queue_names: Vec<String>,
            }

            #[async_trait::async_trait]
            impl PgmqEventHandler for DemultiplexingForwarder {
                async fn handle_event(
                    &self,
                    event: PgmqNotifyEvent,
                ) -> pgmq_notify::error::Result<()> {
                    let event_queue_name = event.queue_name();

                    // Find matching queue(s) for this event
                    let senders = self.senders.read().await;

                    for queue_name in &self.queue_names {
                        // Match event's queue_name to our subscribed queues
                        if event_queue_name != queue_name.as_str() {
                            continue;
                        }

                        if let Some(tx) = senders.get(queue_name) {
                            // Convert event to MessageNotification
                            let notification = match &event {
                                PgmqNotifyEvent::MessageWithPayload(e) => {
                                    let handle = MessageHandle::Pgmq {
                                        msg_id: e.msg_id,
                                        queue_name: e.queue_name.clone(),
                                    };
                                    let metadata = MessageMetadata {
                                        receive_count: 0,
                                        enqueued_at: e.ready_at,
                                    };
                                    let payload_bytes = serde_json::to_vec(&e.message)
                                        .unwrap_or_else(|_| Vec::new());
                                    let queued_msg =
                                        QueuedMessage::with_handle(payload_bytes, handle, metadata);
                                    MessageNotification::message(queued_msg)
                                }
                                PgmqNotifyEvent::MessageReady(e) => {
                                    MessageNotification::available_with_msg_id(
                                        e.queue_name.clone(),
                                        e.msg_id,
                                    )
                                }
                                PgmqNotifyEvent::QueueCreated(_) => {
                                    continue; // Skip queue creation events
                                }
                                PgmqNotifyEvent::BatchReady(e) => {
                                    let msg_id = e.msg_ids.first().copied();
                                    match msg_id {
                                        Some(id) => MessageNotification::available_with_msg_id(
                                            e.queue_name.clone(),
                                            id,
                                        ),
                                        None => {
                                            MessageNotification::available(e.queue_name.clone())
                                        }
                                    }
                                }
                            };

                            if tx.send(notification).await.is_err() {
                                tracing::warn!(
                                    queue = %queue_name,
                                    "Notification receiver dropped for queue"
                                );
                            }
                        }
                    }

                    Ok(())
                }
            }

            let handler = DemultiplexingForwarder {
                senders,
                queue_names: queue_names_owned.clone(),
            };

            // Run the single listener loop
            if let Err(e) = listener.listen_with_handler(handler).await {
                error!(
                    "Shared PGMQ listener error for {} queues: {}",
                    queue_names_owned.len(),
                    e
                );
            }

            debug!(
                "PGMQ subscribe_many ended for {} queues",
                queue_names_owned.len()
            );
        });

        Ok(result_streams)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// Get database URL for tests, preferring PGMQ_DATABASE_URL for split-db mode
    fn get_test_database_url() -> String {
        std::env::var("PGMQ_DATABASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("DATABASE_URL").ok())
            .unwrap_or_else(|| {
                "postgresql://tasker:tasker@localhost:5432/tasker_rust_test".to_string()
            })
    }

    /// Generate unique queue name to avoid test conflicts
    fn unique_queue_name(prefix: &str) -> String {
        let test_id = &Uuid::new_v4().to_string()[..8];
        format!("{}_{}", prefix, test_id)
    }

    #[tokio::test]
    async fn test_pgmq_service_creation() {
        let database_url = get_test_database_url();
        let service = PgmqMessagingService::new(&database_url).await;
        assert!(service.is_ok(), "Should connect to database");

        let service = service.unwrap();
        assert_eq!(service.provider_name(), "pgmq");
        // Note: has_notify_capabilities() depends on connection mode configuration
        // It's informational, not a requirement for basic operations
        let _ = service.has_notify_capabilities();
    }

    #[tokio::test]
    async fn test_pgmq_health_check() {
        let database_url = get_test_database_url();
        let service = PgmqMessagingService::new(&database_url).await.unwrap();

        let health = service.health_check().await;
        assert!(health.is_ok(), "Health check should succeed");
        assert!(health.unwrap(), "Health check should return true");
    }

    #[tokio::test]
    async fn test_pgmq_ensure_queue() {
        let database_url = get_test_database_url();
        let service = PgmqMessagingService::new(&database_url).await.unwrap();
        let queue_name = unique_queue_name("test_ensure");

        // Create queue
        let result = service.ensure_queue(&queue_name).await;
        assert!(result.is_ok(), "Should create queue: {:?}", result.err());

        // Idempotent - creating again should succeed
        let result = service.ensure_queue(&queue_name).await;
        assert!(result.is_ok(), "Should be idempotent");
    }

    #[tokio::test]
    async fn test_pgmq_send_receive_roundtrip() {
        let database_url = get_test_database_url();
        let service = PgmqMessagingService::new(&database_url).await.unwrap();
        let queue_name = unique_queue_name("test_roundtrip");

        service.ensure_queue(&queue_name).await.unwrap();

        // Send message
        let msg = serde_json::json!({"test": "hello", "value": 42});
        let msg_id = service.send_message(&queue_name, &msg).await.unwrap();
        assert!(!msg_id.as_str().is_empty(), "Should return message ID");

        // Receive message
        let messages: Vec<QueuedMessage<serde_json::Value>> = service
            .receive_messages(&queue_name, 10, Duration::from_secs(30))
            .await
            .unwrap();

        assert_eq!(messages.len(), 1, "Should receive one message");
        assert_eq!(messages[0].message["test"], "hello");
        assert_eq!(messages[0].message["value"], 42);
        assert_eq!(messages[0].receive_count(), 1);

        // Ack message
        let ack_result = service
            .ack_message(&queue_name, &messages[0].receipt_handle)
            .await;
        assert!(ack_result.is_ok(), "Should ack message");

        // Should be empty now
        let messages2: Vec<QueuedMessage<serde_json::Value>> = service
            .receive_messages(&queue_name, 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(messages2.len(), 0, "Queue should be empty after ack");
    }

    #[tokio::test]
    async fn test_pgmq_nack_requeue() {
        let database_url = get_test_database_url();
        let service = PgmqMessagingService::new(&database_url).await.unwrap();
        let queue_name = unique_queue_name("test_nack");

        service.ensure_queue(&queue_name).await.unwrap();

        // Send message
        let msg = serde_json::json!({"action": "retry_me"});
        service.send_message(&queue_name, &msg).await.unwrap();

        // Receive with visibility timeout
        let messages: Vec<QueuedMessage<serde_json::Value>> = service
            .receive_messages(&queue_name, 1, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);

        // Nack with requeue (sets visibility to 0)
        service
            .nack_message(&queue_name, &messages[0].receipt_handle, true)
            .await
            .unwrap();

        // Message should be immediately visible again
        let messages2: Vec<QueuedMessage<serde_json::Value>> = service
            .receive_messages(&queue_name, 1, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(messages2.len(), 1, "Message should be requeued");
        assert_eq!(
            messages2[0].receive_count(),
            2,
            "Receive count should increment"
        );
    }

    #[tokio::test]
    async fn test_pgmq_queue_stats() {
        let database_url = get_test_database_url();
        let service = PgmqMessagingService::new(&database_url).await.unwrap();
        let queue_name = unique_queue_name("test_stats");

        service.ensure_queue(&queue_name).await.unwrap();

        // Send a few messages
        for i in 0..3 {
            let msg = serde_json::json!({"index": i});
            service.send_message(&queue_name, &msg).await.unwrap();
        }

        // Check stats
        let stats = service.queue_stats(&queue_name).await.unwrap();
        assert_eq!(stats.queue_name, queue_name);
        assert_eq!(stats.message_count, 3);
    }

    #[tokio::test]
    async fn test_pgmq_verify_queues() {
        let database_url = get_test_database_url();
        let service = PgmqMessagingService::new(&database_url).await.unwrap();

        let existing_queue = unique_queue_name("test_verify_exists");
        let missing_queue = unique_queue_name("test_verify_missing");

        // Create only the first queue
        service.ensure_queue(&existing_queue).await.unwrap();

        // Verify both
        let report = service
            .verify_queues(&[existing_queue.clone(), missing_queue.clone()])
            .await
            .unwrap();

        assert!(
            report.healthy.contains(&existing_queue),
            "Should find existing queue"
        );
        assert!(
            report.missing.contains(&missing_queue),
            "Should identify missing queue"
        );
    }

    #[tokio::test]
    async fn test_pgmq_send_batch() {
        let database_url = get_test_database_url();
        let service = PgmqMessagingService::new(&database_url).await.unwrap();
        let queue_name = unique_queue_name("test_batch");

        service.ensure_queue(&queue_name).await.unwrap();

        // Send batch
        let messages = vec![
            serde_json::json!({"batch": 1}),
            serde_json::json!({"batch": 2}),
            serde_json::json!({"batch": 3}),
        ];
        let ids = service.send_batch(&queue_name, &messages).await.unwrap();
        assert_eq!(ids.len(), 3, "Should return 3 message IDs");

        // Verify all messages are in queue
        let stats = service.queue_stats(&queue_name).await.unwrap();
        assert_eq!(stats.message_count, 3);
    }
}
