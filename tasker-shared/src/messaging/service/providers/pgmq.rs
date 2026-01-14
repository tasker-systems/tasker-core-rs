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

use std::time::Duration;

use async_trait::async_trait;
use pgmq_notify::PgmqClient;
use sqlx::PgPool;

use crate::messaging::service::traits::{MessagingService, QueueMessage};
use crate::messaging::service::types::{
    MessageId, QueueHealthReport, QueueStats, QueuedMessage, ReceiptHandle,
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
    pub async fn new(database_url: &str) -> Result<Self, MessagingError> {
        let client = PgmqClient::new(database_url)
            .await
            .map_err(|e| MessagingError::connection(e.to_string()))?;
        Ok(Self { client })
    }

    /// Create a new PGMQ messaging service with an existing connection pool
    pub async fn new_with_pool(pool: PgPool) -> Self {
        let client = PgmqClient::new_with_pool(pool).await;
        Self { client }
    }

    /// Create from an existing PgmqClient
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

            result.push(QueuedMessage::new(
                ReceiptHandle::from(msg.msg_id),
                deserialized,
                msg.read_ct as u32,
                msg.enqueued_at,
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
            .map_err(|e| MessagingError::extend_visibility(queue_name, message_id, e.to_string()))?;

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

#[cfg(test)]
mod tests {
    #[test]
    fn test_provider_name() {
        // We can't easily test the async methods without a database,
        // but we can test the sync methods
        // Note: This test would need a mock or real database to fully work
    }

    // Integration tests would go in a separate test file with database access
}
