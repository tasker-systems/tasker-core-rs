//! # Messaging Provider Enum
//!
//! Enum dispatch for messaging providers, avoiding trait object overhead.

use std::time::Duration;

use super::providers::{InMemoryMessagingService, PgmqMessagingService};
use super::traits::{MessagingService, QueueMessage};
use super::types::{MessageId, QueueHealthReport, QueueStats, QueuedMessage, ReceiptHandle};
use super::MessagingError;

/// Provider enum for zero-cost dispatch
///
/// Uses enum dispatch instead of `Arc<dyn MessagingService>` for hot-path performance:
/// - No vtable indirection on every send/receive
/// - Compiler can inline provider methods
/// - No generic proliferation (SystemContext stays non-generic)
///
/// # Variants
///
/// - `Pgmq` - PostgreSQL Message Queue via pgmq-notify (TAS-133b)
/// - `RabbitMq` - RabbitMQ via lapin crate (TAS-133d)
/// - `InMemory` - In-memory queue for testing (TAS-133b)
///
/// # Example
///
/// ```ignore
/// let provider = MessagingProvider::Pgmq(PgmqMessagingService::new("postgres://...").await?);
///
/// // Enum dispatch - no vtable lookup
/// let msg_id = provider.send_message("my_queue", &message).await?;
/// ```
#[derive(Debug)]
pub enum MessagingProvider {
    /// PGMQ provider (PostgreSQL-based message queue)
    ///
    /// Uses pgmq-notify crate for LISTEN/NOTIFY support.
    Pgmq(PgmqMessagingService),

    /// RabbitMQ provider (AMQP 0.9.1)
    ///
    /// Implemented in TAS-133d. Uses lapin crate.
    RabbitMq(RabbitMqMessagingServiceStub),

    /// In-memory provider for testing
    ///
    /// Thread-safe HashMap-based queues with visibility timeout simulation.
    InMemory(InMemoryMessagingService),
}

impl MessagingProvider {
    /// Create a new in-memory provider (for testing)
    pub fn new_in_memory() -> Self {
        Self::InMemory(InMemoryMessagingService::new())
    }

    /// Create a new PGMQ provider from database URL
    pub async fn new_pgmq(database_url: &str) -> Result<Self, MessagingError> {
        let service = PgmqMessagingService::new(database_url).await?;
        Ok(Self::Pgmq(service))
    }

    /// Create a new PGMQ provider with an existing connection pool
    pub async fn new_pgmq_with_pool(pool: sqlx::PgPool) -> Self {
        let service = PgmqMessagingService::new_with_pool(pool).await;
        Self::Pgmq(service)
    }

    /// Get the provider name for logging/metrics
    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::Pgmq(_) => "pgmq",
            Self::RabbitMq(_) => "rabbitmq",
            Self::InMemory(_) => "in_memory",
        }
    }

    /// Create a queue if it doesn't exist
    pub async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError> {
        match self {
            Self::Pgmq(s) => s.ensure_queue(queue_name).await,
            Self::RabbitMq(s) => s.ensure_queue(queue_name).await,
            Self::InMemory(s) => s.ensure_queue(queue_name).await,
        }
    }

    /// Bulk queue creation
    pub async fn ensure_queues(&self, queue_names: &[String]) -> Result<(), MessagingError> {
        match self {
            Self::Pgmq(s) => s.ensure_queues(queue_names).await,
            Self::RabbitMq(s) => s.ensure_queues(queue_names).await,
            Self::InMemory(s) => s.ensure_queues(queue_names).await,
        }
    }

    /// Verify queues exist
    pub async fn verify_queues(
        &self,
        queue_names: &[String],
    ) -> Result<QueueHealthReport, MessagingError> {
        match self {
            Self::Pgmq(s) => s.verify_queues(queue_names).await,
            Self::RabbitMq(s) => s.verify_queues(queue_names).await,
            Self::InMemory(s) => s.verify_queues(queue_names).await,
        }
    }

    /// Send a message to a queue
    pub async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<MessageId, MessagingError> {
        match self {
            Self::Pgmq(s) => s.send_message(queue_name, message).await,
            Self::RabbitMq(s) => s.send_message(queue_name, message).await,
            Self::InMemory(s) => s.send_message(queue_name, message).await,
        }
    }

    /// Send a batch of messages
    pub async fn send_batch<T: QueueMessage>(
        &self,
        queue_name: &str,
        messages: &[T],
    ) -> Result<Vec<MessageId>, MessagingError> {
        match self {
            Self::Pgmq(s) => s.send_batch(queue_name, messages).await,
            Self::RabbitMq(s) => s.send_batch(queue_name, messages).await,
            Self::InMemory(s) => s.send_batch(queue_name, messages).await,
        }
    }

    /// Receive messages with visibility timeout
    pub async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        match self {
            Self::Pgmq(s) => {
                s.receive_messages(queue_name, max_messages, visibility_timeout)
                    .await
            }
            Self::RabbitMq(s) => {
                s.receive_messages(queue_name, max_messages, visibility_timeout)
                    .await
            }
            Self::InMemory(s) => {
                s.receive_messages(queue_name, max_messages, visibility_timeout)
                    .await
            }
        }
    }

    /// Acknowledge a message
    pub async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError> {
        match self {
            Self::Pgmq(s) => s.ack_message(queue_name, receipt_handle).await,
            Self::RabbitMq(s) => s.ack_message(queue_name, receipt_handle).await,
            Self::InMemory(s) => s.ack_message(queue_name, receipt_handle).await,
        }
    }

    /// Negative acknowledge a message
    pub async fn nack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        requeue: bool,
    ) -> Result<(), MessagingError> {
        match self {
            Self::Pgmq(s) => s.nack_message(queue_name, receipt_handle, requeue).await,
            Self::RabbitMq(s) => s.nack_message(queue_name, receipt_handle, requeue).await,
            Self::InMemory(s) => s.nack_message(queue_name, receipt_handle, requeue).await,
        }
    }

    /// Extend visibility timeout
    pub async fn extend_visibility(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        extension: Duration,
    ) -> Result<(), MessagingError> {
        match self {
            Self::Pgmq(s) => {
                s.extend_visibility(queue_name, receipt_handle, extension)
                    .await
            }
            Self::RabbitMq(s) => {
                s.extend_visibility(queue_name, receipt_handle, extension)
                    .await
            }
            Self::InMemory(s) => {
                s.extend_visibility(queue_name, receipt_handle, extension)
                    .await
            }
        }
    }

    /// Get queue statistics
    pub async fn queue_stats(&self, queue_name: &str) -> Result<QueueStats, MessagingError> {
        match self {
            Self::Pgmq(s) => s.queue_stats(queue_name).await,
            Self::RabbitMq(s) => s.queue_stats(queue_name).await,
            Self::InMemory(s) => s.queue_stats(queue_name).await,
        }
    }

    /// Health check
    pub async fn health_check(&self) -> Result<bool, MessagingError> {
        match self {
            Self::Pgmq(s) => s.health_check().await,
            Self::RabbitMq(s) => s.health_check().await,
            Self::InMemory(s) => s.health_check().await,
        }
    }
}

// =============================================================================
// Stub for RabbitMQ (implemented in TAS-133d)
// =============================================================================

/// Stub for RabbitMQ messaging service
///
/// Replaced with real implementation in TAS-133d.
#[derive(Debug)]
pub struct RabbitMqMessagingServiceStub;

#[async_trait::async_trait]
impl MessagingService for RabbitMqMessagingServiceStub {
    async fn ensure_queue(&self, _queue_name: &str) -> Result<(), MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    async fn verify_queues(
        &self,
        _queue_names: &[String],
    ) -> Result<QueueHealthReport, MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    async fn send_message<T: QueueMessage>(
        &self,
        _queue_name: &str,
        _message: &T,
    ) -> Result<MessageId, MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        _queue_name: &str,
        _max_messages: usize,
        _visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    async fn ack_message(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    async fn nack_message(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
        _requeue: bool,
    ) -> Result<(), MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    async fn extend_visibility(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
        _extension: Duration,
    ) -> Result<(), MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    async fn queue_stats(&self, _queue_name: &str) -> Result<QueueStats, MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    async fn health_check(&self) -> Result<bool, MessagingError> {
        unimplemented!("RabbitMqMessagingService implemented in TAS-133d")
    }

    fn provider_name(&self) -> &'static str {
        "rabbitmq"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_provider_name() {
        let provider = MessagingProvider::new_in_memory();
        assert_eq!(provider.provider_name(), "in_memory");
    }

    #[tokio::test]
    async fn test_in_memory_provider_basic_ops() {
        let provider = MessagingProvider::new_in_memory();

        // Create queue
        provider.ensure_queue("test_queue").await.unwrap();

        // Health check
        assert!(provider.health_check().await.unwrap());

        // Verify queues
        let report = provider
            .verify_queues(&["test_queue".to_string()])
            .await
            .unwrap();
        assert!(report.is_healthy());
    }

    #[tokio::test]
    async fn test_in_memory_send_receive() {
        let provider = MessagingProvider::new_in_memory();
        provider.ensure_queue("test_queue").await.unwrap();

        // Send message
        let msg = serde_json::json!({"hello": "world"});
        let msg_id = provider.send_message("test_queue", &msg).await.unwrap();
        assert_eq!(msg_id.as_str(), "1");

        // Receive message
        let messages: Vec<QueuedMessage<serde_json::Value>> = provider
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message, msg);

        // Ack message
        provider
            .ack_message("test_queue", &messages[0].receipt_handle)
            .await
            .unwrap();

        // Should be empty now
        let messages2: Vec<QueuedMessage<serde_json::Value>> = provider
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(messages2.len(), 0);
    }
}
