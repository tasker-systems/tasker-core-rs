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
        assert_eq!(messages[0].receive_count, 1);

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
            messages2[0].receive_count, 2,
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
