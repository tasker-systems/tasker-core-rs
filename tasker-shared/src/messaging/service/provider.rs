//! # Messaging Provider Enum
//!
//! Enum dispatch for messaging providers, avoiding trait object overhead.

use std::pin::Pin;
use std::time::Duration;

use futures::Stream;
use pgmq_notify::PgmqNotifyConfig;

use crate::config::tasker::RabbitmqConfig;
use super::providers::{InMemoryMessagingService, PgmqMessagingService, RabbitMqMessagingService};
use super::traits::{MessagingService, QueueMessage, SupportsPushNotifications};
use super::types::{MessageId, MessageNotification, QueueHealthReport, QueueStats, QueuedMessage, ReceiptHandle};
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
    /// Uses lapin crate for AMQP protocol support.
    /// Includes Dead Letter Exchange setup for failed message handling.
    /// Boxed to reduce enum size (RabbitMQ service has larger internal state).
    RabbitMq(Box<RabbitMqMessagingService>),

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

    /// Create a new PGMQ provider with database URL and custom configuration
    pub async fn new_pgmq_with_config(
        database_url: &str,
        config: PgmqNotifyConfig,
    ) -> Result<Self, MessagingError> {
        let service = PgmqMessagingService::new_with_config(database_url, config).await?;
        Ok(Self::Pgmq(service))
    }

    /// Create a new PGMQ provider with an existing connection pool
    ///
    /// Preferred when pool configuration is managed externally (e.g., from TOML).
    pub async fn new_pgmq_with_pool(pool: sqlx::PgPool) -> Self {
        let service = PgmqMessagingService::new_with_pool(pool).await;
        Self::Pgmq(service)
    }

    /// Create a new PGMQ provider with existing pool and custom configuration
    ///
    /// Use when you need both external pool config (from TOML) and notify configuration.
    pub async fn new_pgmq_with_pool_and_config(
        pool: sqlx::PgPool,
        config: PgmqNotifyConfig,
    ) -> Self {
        let service = PgmqMessagingService::new_with_pool_and_config(pool, config).await;
        Self::Pgmq(service)
    }

    /// Create a new RabbitMQ provider with configuration reference
    ///
    /// Clones the configuration internally.
    pub async fn new_rabbitmq(config: &RabbitmqConfig) -> Result<Self, MessagingError> {
        let service = RabbitMqMessagingService::new(config).await?;
        Ok(Self::RabbitMq(Box::new(service)))
    }

    /// Create a new RabbitMQ provider with owned configuration
    ///
    /// Takes ownership of the configuration without cloning.
    pub async fn new_rabbitmq_owned(config: RabbitmqConfig) -> Result<Self, MessagingError> {
        let service = RabbitMqMessagingService::from_config(config).await?;
        Ok(Self::RabbitMq(Box::new(service)))
    }

    /// Create a new RabbitMQ provider from environment variables
    ///
    /// Reads from: RABBITMQ_URL, RABBITMQ_PREFETCH_COUNT, RABBITMQ_HEARTBEAT_SECONDS
    pub async fn new_rabbitmq_from_env() -> Result<Self, MessagingError> {
        let service = RabbitMqMessagingService::from_env().await?;
        Ok(Self::RabbitMq(Box::new(service)))
    }

    /// Get the provider name for logging/metrics
    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::Pgmq(_) => "pgmq",
            Self::RabbitMq(_) => "rabbitmq",
            Self::InMemory(_) => "in_memory",
        }
    }

    /// Get the PGMQ service if this is a PGMQ provider
    ///
    /// Returns `Some(&PgmqMessagingService)` if this provider is PGMQ-based,
    /// otherwise returns `None`. Used for PGMQ-specific operations like
    /// event-driven processing with LISTEN/NOTIFY.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(pgmq) = provider.as_pgmq() {
    ///     // Use PGMQ-specific features like read_specific_message
    /// }
    /// ```
    pub fn as_pgmq(&self) -> Option<&PgmqMessagingService> {
        match self {
            Self::Pgmq(s) => Some(s),
            _ => None,
        }
    }

    /// Check if this is a PGMQ provider
    pub fn is_pgmq(&self) -> bool {
        matches!(self, Self::Pgmq(_))
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

    // =========================================================================
    // SupportsPushNotifications delegations (TAS-133)
    // =========================================================================

    /// Subscribe to push notifications for a queue
    ///
    /// TAS-133: Delegates to the underlying provider's `SupportsPushNotifications::subscribe()`.
    /// Returns a stream of `MessageNotification` items.
    ///
    /// # Notification Types
    ///
    /// - **PGMQ**: Returns `MessageNotification::Available` (signal only, fetch required)
    /// - **RabbitMQ**: Returns `MessageNotification::Message` (full message delivered)
    /// - **InMemory**: Returns `MessageNotification::Available` (signal only)
    pub fn subscribe(
        &self,
        queue_name: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError> {
        match self {
            Self::Pgmq(s) => s.subscribe(queue_name),
            Self::RabbitMq(s) => s.subscribe(queue_name),
            Self::InMemory(s) => s.subscribe(queue_name),
        }
    }

    /// Check if this provider requires fallback polling
    ///
    /// TAS-133: Some providers (like PGMQ) use signal-only notifications where
    /// messages must be fetched after receiving a signal. These providers need
    /// fallback polling to catch missed signals.
    ///
    /// Returns `true` for PGMQ and InMemory, `false` for RabbitMQ.
    pub fn requires_fallback_polling(&self) -> bool {
        match self {
            Self::Pgmq(s) => s.requires_fallback_polling(),
            Self::RabbitMq(s) => s.requires_fallback_polling(),
            Self::InMemory(s) => s.requires_fallback_polling(),
        }
    }

    /// Check if this provider supports fetching messages by ID after signal-only notifications
    ///
    /// TAS-133: PGMQ may send signal-only notifications for large messages (>7KB) where
    /// the notification contains only a message ID. Consumers must use
    /// `read_specific_message(msg_id)` to fetch the full message content.
    ///
    /// Code paths that depend on fetch-by-ID (like `ExecuteStepFromEventMessage`)
    /// should check this capability and fail loudly if the provider doesn't support it.
    ///
    /// Returns `true` for PGMQ, `false` for RabbitMQ and InMemory.
    pub fn supports_fetch_by_message_id(&self) -> bool {
        match self {
            Self::Pgmq(s) => s.supports_fetch_by_message_id(),
            Self::RabbitMq(s) => s.supports_fetch_by_message_id(),
            Self::InMemory(s) => s.supports_fetch_by_message_id(),
        }
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
