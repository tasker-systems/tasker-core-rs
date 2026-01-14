//! # Messaging Provider Enum
//!
//! Enum dispatch for messaging providers, avoiding trait object overhead.

use std::time::Duration;

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
/// let provider = MessagingProvider::Pgmq(PgmqMessagingService::new(pool).await?);
///
/// // Enum dispatch - no vtable lookup
/// let msg_id = provider.send_message("my_queue", &message).await?;
/// ```
#[derive(Debug)]
pub enum MessagingProvider {
    /// PGMQ provider (PostgreSQL-based message queue)
    ///
    /// Implemented in TAS-133b. Uses pgmq-notify crate for LISTEN/NOTIFY support.
    Pgmq(PgmqMessagingServiceStub),

    /// RabbitMQ provider (AMQP 0.9.1)
    ///
    /// Implemented in TAS-133d. Uses lapin crate.
    RabbitMq(RabbitMqMessagingServiceStub),

    /// In-memory provider for testing
    ///
    /// Implemented in TAS-133b. Thread-safe HashMap-based queues.
    InMemory(InMemoryMessagingServiceStub),
}

impl MessagingProvider {
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
// Stub implementations (replaced in TAS-133b and TAS-133d)
// =============================================================================

/// Stub for PGMQ messaging service
///
/// Replaced with real implementation in TAS-133b.
#[derive(Debug)]
pub struct PgmqMessagingServiceStub;

#[async_trait::async_trait]
impl MessagingService for PgmqMessagingServiceStub {
    async fn ensure_queue(&self, _queue_name: &str) -> Result<(), MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    async fn verify_queues(
        &self,
        _queue_names: &[String],
    ) -> Result<QueueHealthReport, MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    async fn send_message<T: QueueMessage>(
        &self,
        _queue_name: &str,
        _message: &T,
    ) -> Result<MessageId, MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        _queue_name: &str,
        _max_messages: usize,
        _visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    async fn ack_message(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    async fn nack_message(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
        _requeue: bool,
    ) -> Result<(), MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    async fn extend_visibility(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
        _extension: Duration,
    ) -> Result<(), MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    async fn queue_stats(&self, _queue_name: &str) -> Result<QueueStats, MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    async fn health_check(&self) -> Result<bool, MessagingError> {
        unimplemented!("PgmqMessagingService implemented in TAS-133b")
    }

    fn provider_name(&self) -> &'static str {
        "pgmq"
    }
}

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

/// Stub for in-memory messaging service (testing)
///
/// Replaced with real implementation in TAS-133b.
#[derive(Debug)]
pub struct InMemoryMessagingServiceStub;

#[async_trait::async_trait]
impl MessagingService for InMemoryMessagingServiceStub {
    async fn ensure_queue(&self, _queue_name: &str) -> Result<(), MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    async fn verify_queues(
        &self,
        _queue_names: &[String],
    ) -> Result<QueueHealthReport, MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    async fn send_message<T: QueueMessage>(
        &self,
        _queue_name: &str,
        _message: &T,
    ) -> Result<MessageId, MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        _queue_name: &str,
        _max_messages: usize,
        _visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    async fn ack_message(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    async fn nack_message(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
        _requeue: bool,
    ) -> Result<(), MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    async fn extend_visibility(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
        _extension: Duration,
    ) -> Result<(), MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    async fn queue_stats(&self, _queue_name: &str) -> Result<QueueStats, MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    async fn health_check(&self) -> Result<bool, MessagingError> {
        unimplemented!("InMemoryMessagingService implemented in TAS-133b")
    }

    fn provider_name(&self) -> &'static str {
        "in_memory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_names() {
        let pgmq = MessagingProvider::Pgmq(PgmqMessagingServiceStub);
        assert_eq!(pgmq.provider_name(), "pgmq");

        let rabbit = MessagingProvider::RabbitMq(RabbitMqMessagingServiceStub);
        assert_eq!(rabbit.provider_name(), "rabbitmq");

        let memory = MessagingProvider::InMemory(InMemoryMessagingServiceStub);
        assert_eq!(memory.provider_name(), "in_memory");
    }
}
