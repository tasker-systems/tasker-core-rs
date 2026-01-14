//! # Messaging Service Traits
//!
//! Core trait definitions for provider-agnostic messaging.

use std::time::Duration;

use async_trait::async_trait;

use super::types::{MessageId, QueueHealthReport, QueueStats, QueuedMessage, ReceiptHandle};
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
