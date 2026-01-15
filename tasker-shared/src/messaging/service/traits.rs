//! # Messaging Service Traits
//!
//! Core trait definitions for provider-agnostic messaging.
//!
//! ## Core Traits
//!
//! - [`MessagingService`]: Core operations (send, receive, ack, nack)
//! - [`QueueMessage`]: Message serialization contract
//! - [`SupportsPushNotifications`]: Push-based notification subscriptions (TAS-133)
//!
//! ## Push Notification Architecture (TAS-133)
//!
//! The [`SupportsPushNotifications`] trait enables providers to offer push-based
//! message delivery. Different providers have different native delivery models:
//!
//! - **PGMQ**: Uses PostgreSQL LISTEN/NOTIFY for signal-only notifications
//! - **RabbitMQ**: Uses `basic_consume()` for full message push delivery
//!
//! See [`MessageNotification`] for the delivery model distinction.

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;

use super::types::{
    MessageId, MessageNotification, QueueHealthReport, QueueStats, QueuedMessage, ReceiptHandle,
};
use super::MessagingError;

/// Core messaging service trait - provider-agnostic operations
///
/// Implementations of this trait provide the actual messaging backend
/// (PGMQ, RabbitMQ, InMemory). The trait is designed to be implementable
/// by any message queue system that supports:
///
/// - Queue creation (idempotent)
/// - Message send/receive with visibility timeout
/// - Message acknowledgment (ack/nack)
/// - Queue statistics for monitoring
///
/// # Example Implementation
///
/// ```ignore
/// impl MessagingService for PgmqMessagingService {
///     async fn send_message<T: QueueMessage>(
///         &self,
///         queue_name: &str,
///         message: &T,
///     ) -> Result<MessageId, MessagingError> {
///         let bytes = message.to_bytes()?;
///         let json: serde_json::Value = serde_json::from_slice(&bytes)?;
///         let msg_id = self.client.send_json_message(queue_name, &json).await?;
///         Ok(MessageId::from(msg_id))
///     }
///     // ... other methods
/// }
/// ```
#[async_trait]
pub trait MessagingService: Send + Sync + 'static {
    /// Create a queue if it doesn't exist (idempotent)
    ///
    /// This operation should be safe to call multiple times.
    /// If the queue already exists, it should succeed silently.
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError>;

    /// Bulk queue creation (called during worker bootstrap)
    ///
    /// Default implementation calls `ensure_queue` for each name.
    /// Providers may override for batch optimization.
    async fn ensure_queues(&self, queue_names: &[String]) -> Result<(), MessagingError> {
        for queue_name in queue_names {
            self.ensure_queue(queue_name).await?;
        }
        Ok(())
    }

    /// Verify expected queues exist (startup health check)
    ///
    /// Returns a report indicating which queues are healthy, missing, or errored.
    /// This is called during worker/orchestration startup to fail fast if
    /// required queues don't exist.
    async fn verify_queues(
        &self,
        queue_names: &[String],
    ) -> Result<QueueHealthReport, MessagingError>;

    /// Send a message to a queue
    ///
    /// Returns the message ID assigned by the provider.
    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<MessageId, MessagingError>;

    /// Send a batch of messages (provider may optimize)
    ///
    /// Default implementation calls `send_message` for each message.
    /// Providers like RabbitMQ can override for transaction-based batching.
    async fn send_batch<T: QueueMessage>(
        &self,
        queue_name: &str,
        messages: &[T],
    ) -> Result<Vec<MessageId>, MessagingError> {
        let mut ids = Vec::with_capacity(messages.len());
        for message in messages {
            ids.push(self.send_message(queue_name, message).await?);
        }
        Ok(ids)
    }

    /// Receive messages with visibility timeout
    ///
    /// Messages become invisible to other consumers for `visibility_timeout`.
    /// If not acknowledged before timeout expires, they become visible again.
    ///
    /// # Arguments
    ///
    /// * `queue_name` - The queue to read from
    /// * `max_messages` - Maximum number of messages to receive
    /// * `visibility_timeout` - How long messages are invisible to other consumers
    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError>;

    /// Acknowledge successful processing (delete message)
    ///
    /// Call this after successfully processing a message to remove it from the queue.
    async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError>;

    /// Negative acknowledge (return to queue or move to DLQ)
    ///
    /// # Arguments
    ///
    /// * `queue_name` - The queue the message came from
    /// * `receipt_handle` - Handle from the received message
    /// * `requeue` - If true, message returns to queue. If false, moves to DLQ/archive.
    ///
    /// # Provider Behavior
    ///
    /// - **PGMQ**: `requeue=true` is a no-op (visibility timeout handles retry).
    ///   `requeue=false` archives to `a_{queue_name}` table.
    /// - **RabbitMQ**: Uses `basic_nack` with the requeue flag.
    async fn nack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        requeue: bool,
    ) -> Result<(), MessagingError>;

    /// Extend visibility timeout (heartbeat during long processing)
    ///
    /// Call this periodically during long-running message processing to
    /// prevent the message from becoming visible to other consumers.
    ///
    /// # Provider Support
    ///
    /// - **PGMQ**: Supported via `pgmq.set_vt()`
    /// - **RabbitMQ**: Not directly supported (logs warning, returns Ok)
    async fn extend_visibility(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        extension: Duration,
    ) -> Result<(), MessagingError>;

    /// Get queue statistics for backpressure decisions
    ///
    /// Returns message counts and optionally the oldest message age.
    async fn queue_stats(&self, queue_name: &str) -> Result<QueueStats, MessagingError>;

    /// Health check - verify the messaging backend is reachable
    ///
    /// Returns true if the backend is healthy, false otherwise.
    async fn health_check(&self) -> Result<bool, MessagingError>;

    /// Provider name for logging/metrics
    ///
    /// Returns a static string identifying the provider (e.g., "pgmq", "rabbitmq").
    fn provider_name(&self) -> &'static str;
}

/// Message serialization contract
///
/// Types implementing this trait can be sent through the messaging system.
/// The trait abstracts serialization format, allowing:
/// - JSON (current default via serde_json)
/// - MessagePack (future, for performance-critical paths)
/// - Protocol Buffers (future, for cross-language compatibility)
///
/// # Example Implementation
///
/// ```ignore
/// impl QueueMessage for StepMessage {
///     fn to_bytes(&self) -> Result<Vec<u8>, MessagingError> {
///         serde_json::to_vec(self)
///             .map_err(|e| MessagingError::message_serialization(e.to_string()))
///     }
///
///     fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError> {
///         serde_json::from_slice(bytes)
///             .map_err(|e| MessagingError::message_deserialization(e.to_string()))
///     }
/// }
/// ```
pub trait QueueMessage: Send + Sync + Clone + 'static {
    /// Serialize the message to bytes
    fn to_bytes(&self) -> Result<Vec<u8>, MessagingError>;

    /// Deserialize the message from bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError>
    where
        Self: Sized;
}

/// Blanket implementation for types that implement Serialize + DeserializeOwned
///
/// This provides JSON serialization by default for any serde-compatible type.
impl<T> QueueMessage for T
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + Clone + 'static,
{
    fn to_bytes(&self) -> Result<Vec<u8>, MessagingError> {
        serde_json::to_vec(self).map_err(|e| MessagingError::message_serialization(e.to_string()))
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError> {
        serde_json::from_slice(bytes)
            .map_err(|e| MessagingError::message_deserialization(e.to_string()))
    }
}

// =============================================================================
// SupportsPushNotifications - Push-based notification subscriptions (TAS-133)
// =============================================================================

/// Push notification subscription capability (TAS-133)
///
/// This trait enables messaging providers to offer push-based message delivery
/// instead of requiring polling. Different providers implement push differently:
///
/// ## PGMQ (Signal-Only Push)
///
/// PGMQ uses PostgreSQL's LISTEN/NOTIFY for notifications. When a message is
/// enqueued, `pg_notify()` sends a signal to subscribed listeners. However,
/// LISTEN/NOTIFY is not guaranteed (signals can be lost under load), so PGMQ
/// consumers need a fallback polling mechanism.
///
/// The `subscribe()` implementation for PGMQ returns `MessageNotification::Available`
/// variants, indicating that a message exists but must be fetched separately.
///
/// ## RabbitMQ (Full Message Push)
///
/// RabbitMQ uses `basic_consume()` for native push delivery. The AMQP protocol
/// guarantees message delivery, so no fallback polling is needed. Messages are
/// delivered in full via the consumer channel.
///
/// The `subscribe()` implementation for RabbitMQ returns `MessageNotification::Message`
/// variants containing the complete message ready for processing.
///
/// ## InMemory (Signal-Only for Testing)
///
/// The in-memory provider uses a broadcast channel to simulate notifications,
/// returning `MessageNotification::Available` similar to PGMQ.
///
/// # Example
///
/// ```rust,ignore
/// use futures::StreamExt;
/// use tasker_shared::messaging::service::{
///     SupportsPushNotifications, MessageNotification, MessagingService,
/// };
///
/// async fn consume_with_push<P: SupportsPushNotifications>(
///     provider: &P,
///     queue_name: &str,
/// ) -> Result<(), MessagingError> {
///     let mut stream = provider.subscribe(queue_name)?;
///
///     while let Some(notification) = stream.next().await {
///         match notification {
///             MessageNotification::Available { queue_name } => {
///                 // PGMQ style: need to fetch the message
///                 let messages = provider.receive_messages::<serde_json::Value>(
///                     &queue_name, 1, Duration::from_secs(30)
///                 ).await?;
///                 for msg in messages {
///                     process(msg);
///                 }
///             }
///             MessageNotification::Message(queued_msg) => {
///                 // RabbitMQ style: message already available
///                 process_bytes(queued_msg);
///             }
///         }
///     }
///     Ok(())
/// }
/// ```
///
/// # Fallback Polling
///
/// For providers that return `MessageNotification::Available` (like PGMQ),
/// consumers should implement a fallback polling mechanism to handle:
///
/// 1. Missed signals (pg_notify is not guaranteed delivery)
/// 2. Messages enqueued before subscription started
/// 3. Recovery after connection interruptions
///
/// The `HybridConsumer` pattern (see TAS-133 architecture docs) combines
/// push notifications with periodic polling for reliability.
pub trait SupportsPushNotifications: MessagingService {
    /// Subscribe to push notifications for a queue
    ///
    /// Returns a stream of [`MessageNotification`] items. The notification type
    /// depends on the provider's native delivery model:
    ///
    /// - **PGMQ**: Returns `MessageNotification::Available` (signal only)
    /// - **RabbitMQ**: Returns `MessageNotification::Message` (full message)
    /// - **InMemory**: Returns `MessageNotification::Available` (signal only)
    ///
    /// # Errors
    ///
    /// Returns `MessagingError` if:
    /// - The queue doesn't exist
    /// - The provider doesn't support push notifications
    /// - Connection/subscription setup fails
    fn subscribe(
        &self,
        queue_name: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;

    /// Check if this provider requires fallback polling
    ///
    /// Returns `true` for providers that use signal-only notifications (like PGMQ),
    /// where messages must be fetched after receiving a signal. These providers
    /// need fallback polling to catch missed signals.
    ///
    /// Returns `false` for providers that deliver full messages (like RabbitMQ),
    /// where the protocol guarantees delivery and no polling is needed.
    fn requires_fallback_polling(&self) -> bool;

    /// Get the recommended fallback polling interval
    ///
    /// For providers that require fallback polling, returns the recommended
    /// interval between polls. Returns `None` if fallback polling is not needed.
    ///
    /// Default implementation returns `Some(Duration::from_secs(5))` if
    /// `requires_fallback_polling()` returns true, `None` otherwise.
    fn fallback_polling_interval(&self) -> Option<Duration> {
        if self.requires_fallback_polling() {
            Some(Duration::from_secs(5))
        } else {
            None
        }
    }

    /// Check if this provider supports fetching messages by ID after signal-only notifications
    ///
    /// Some messaging providers (like PGMQ) may send signal-only notifications for large
    /// messages where the notification contains only a message ID. The consumer must then
    /// fetch the full message content using a provider-specific `read_by_message_id()` method.
    ///
    /// ## Provider Behavior
    ///
    /// - **PGMQ**: Returns `true`. Large messages (>7KB) trigger signal-only notifications
    ///   via pg_notify. The notification contains the queue name and message ID.
    ///   Consumers must call `read_specific_message(msg_id)` to fetch the content.
    ///   This is the flow handled by `ExecuteStepFromEventMessage` in the worker.
    ///
    /// - **RabbitMQ**: Returns `false`. AMQP always delivers complete messages via
    ///   `basic_consume()`. There is no signal-only notification flow, so code paths
    ///   expecting to fetch by message ID will fail.
    ///
    /// - **InMemory**: Returns `false`. Uses broadcast channel for testing but does
    ///   not support the fetch-by-ID pattern.
    ///
    /// ## Usage
    ///
    /// Guard code paths that depend on fetch-by-ID capability:
    ///
    /// ```rust,ignore
    /// // In step executor actor, ExecuteStepFromEventMessage handler
    /// if !provider.supports_fetch_by_message_id() {
    ///     tracing::error!(
    ///         provider = provider.provider_name(),
    ///         "Signal-only notification received but provider does not support fetch-by-ID. \
    ///          This is a configuration error - use ExecuteStepFromQueuedMessage for this provider."
    ///     );
    ///     return Err(/* ... */);
    /// }
    /// ```
    fn supports_fetch_by_message_id(&self) -> bool;

    /// Subscribe to push notifications for multiple queues efficiently
    ///
    /// This method allows subscribing to multiple queues with resource efficiency.
    /// The default implementation calls `subscribe()` for each queue, but providers
    /// can override this to share connections across subscriptions.
    ///
    /// # Resource Efficiency
    ///
    /// - **PGMQ**: Overrides to use a single PostgreSQL connection for all LISTEN channels,
    ///   dramatically reducing connection pool usage (1 connection vs N connections).
    /// - **RabbitMQ**: Uses default implementation (separate consumers per queue).
    /// - **InMemory**: Uses default implementation.
    ///
    /// # Returns
    ///
    /// A vector of `(queue_name, stream)` tuples, one per requested queue.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let queues = vec!["queue_a", "queue_b", "queue_c"];
    /// let subscriptions = provider.subscribe_many(&queues)?;
    ///
    /// for (queue_name, stream) in subscriptions {
    ///     tokio::spawn(async move {
    ///         while let Some(notification) = stream.next().await {
    ///             handle_notification(notification).await;
    ///         }
    ///     });
    /// }
    /// ```
    fn subscribe_many(
        &self,
        queue_names: &[&str],
    ) -> Result<Vec<(String, NotificationStream)>, MessagingError> {
        // Default implementation: call subscribe() for each queue
        // Providers can override for efficiency (e.g., PGMQ shares one connection)
        queue_names
            .iter()
            .map(|queue_name| {
                let stream = self.subscribe(queue_name)?;
                Ok((queue_name.to_string(), stream))
            })
            .collect()
    }
}

/// Type alias for the notification stream returned by `SupportsPushNotifications::subscribe()`
pub type NotificationStream = Pin<Box<dyn Stream<Item = MessageNotification> + Send>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct TestMessage {
        id: u64,
        data: String,
    }

    #[test]
    fn test_queue_message_roundtrip() {
        let msg = TestMessage {
            id: 42,
            data: "hello".to_string(),
        };

        let bytes = msg.to_bytes().expect("serialization should succeed");
        let decoded =
            TestMessage::from_bytes(&bytes).expect("deserialization should succeed");

        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_queue_message_invalid_bytes() {
        let result = TestMessage::from_bytes(b"not valid json");
        assert!(result.is_err());
    }
}
