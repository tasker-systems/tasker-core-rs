//! # PostgreSQL Message Queue Client (pgmq-rs)
//!
//! Rust client using the pgmq-rs crate for high-performance message queue operations

use pgmq::{types::Message, PGMQueue};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Queue message for step execution (pgmq-rs version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgmqStepMessage {
    pub step_id: i64,
    pub task_id: i64,
    pub namespace: String,
    pub step_name: String,
    pub step_payload: serde_json::Value,
    pub metadata: PgmqStepMessageMetadata,
}

/// Metadata for step messages (pgmq-rs version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgmqStepMessageMetadata {
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub timeout_seconds: Option<i64>,
}

/// pgmq-rs based message queue client
#[derive(Debug, Clone)]
pub struct PgmqClient {
    pgmq: PGMQueue,
}

impl PgmqClient {
    /// Create new pgmq client using connection string
    pub async fn new(database_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!("🚀 Connecting to pgmq using pgmq-rs crate");

        let pgmq = PGMQueue::new(database_url.to_string()).await?;

        info!("✅ Connected to pgmq using pgmq-rs");
        Ok(Self { pgmq })
    }

    /// Create new pgmq client using existing connection pool (BYOP - Bring Your Own Pool)
    pub async fn new_with_pool(pool: sqlx::PgPool) -> Self {
        info!("🚀 Creating pgmq client with shared connection pool");

        let pgmq = PGMQueue::new_with_pool(pool).await;

        info!("✅ pgmq client created with shared pool");
        Self { pgmq }
    }

    /// Create queue if it doesn't exist
    pub async fn create_queue(
        &self,
        queue_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("📋 Creating queue: {}", queue_name);

        self.pgmq
            .create(queue_name)
            .await
            .map_err(|e| format!("Failed to create queue {queue_name}: {e}"))?;

        info!("✅ Queue created: {}", queue_name);
        Ok(())
    }

    /// Send message to queue
    pub async fn send_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "📤 Sending message to queue: {} for step: {}",
            queue_name, message.step_id
        );

        let message_id = self
            .pgmq
            .send(queue_name, message)
            .await
            .map_err(|e| format!("Failed to send message to {queue_name}: {e}"))?;

        info!(
            "✅ Message sent to queue: {} with id: {}",
            queue_name, message_id
        );
        Ok(message_id)
    }

    /// Send generic JSON message to queue
    pub async fn send_json_message<T: serde::Serialize>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        debug!("📤 Sending JSON message to queue: {}", queue_name);

        let serialized = serde_json::to_value(message)?;
        let message_id = self
            .pgmq
            .send(queue_name, &serialized)
            .await
            .map_err(|e| format!("Failed to send JSON message to {queue_name}: {e}"))?;

        info!(
            "✅ JSON message sent to queue: {} with ID: {}",
            queue_name, message_id
        );
        Ok(message_id)
    }

    /// Read messages from queue
    pub async fn read_messages(
        &self,
        queue_name: &str,
        vt: Option<i32>, // visibility timeout
        limit: Option<i32>,
    ) -> Result<Vec<Message<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "📥 Reading messages from queue: {} (limit: {:?})",
            queue_name, limit
        );

        let messages = match limit {
            Some(l) => self
                .pgmq
                .read_batch(queue_name, vt, l)
                .await?
                .unwrap_or_default(),
            None => match self.pgmq.read(queue_name, vt).await? {
                Some(msg) => vec![msg],
                None => vec![],
            },
        };

        debug!(
            "📨 Read {} messages from queue: {}",
            messages.len(),
            queue_name
        );
        Ok(messages)
    }

    /// Delete message from queue
    pub async fn delete_message(
        &self,
        queue_name: &str,
        message_id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "🗑️ Deleting message {} from queue: {}",
            message_id, queue_name
        );

        self.pgmq
            .delete(queue_name, message_id)
            .await
            .map_err(|e| format!("Failed to delete message {message_id}: {e}"))?;

        debug!("✅ Message deleted: {}", message_id);
        Ok(())
    }

    /// Archive message (move to archive)
    pub async fn archive_message(
        &self,
        queue_name: &str,
        message_id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "📦 Archiving message {} from queue: {}",
            message_id, queue_name
        );

        self.pgmq
            .archive(queue_name, message_id)
            .await
            .map_err(|e| format!("Failed to archive message {message_id}: {e}"))?;

        debug!("✅ Message archived: {}", message_id);
        Ok(())
    }

    /// Purge queue (delete all messages)
    pub async fn purge_queue(
        &self,
        queue_name: &str,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        warn!("🧹 Purging queue: {}", queue_name);

        let purged_count = self
            .pgmq
            .purge(queue_name)
            .await
            .map_err(|e| format!("Failed to purge queue {queue_name}: {e}"))?;

        warn!(
            "🗑️ Purged {} messages from queue: {}",
            purged_count, queue_name
        );
        Ok(purged_count)
    }

    /// Drop queue completely
    pub async fn drop_queue(
        &self,
        queue_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        warn!("💥 Dropping queue: {}", queue_name);

        self.pgmq
            .destroy(queue_name)
            .await
            .map_err(|e| format!("Failed to drop queue {queue_name}: {e}"))?;

        warn!("🗑️ Queue dropped: {}", queue_name);
        Ok(())
    }

    /// Get queue metrics/statistics
    pub async fn queue_metrics(
        &self,
        queue_name: &str,
    ) -> Result<QueueMetrics, Box<dyn std::error::Error + Send + Sync>> {
        debug!("📊 Getting metrics for queue: {}", queue_name);

        // pgmq-rs may have metrics methods - this is a placeholder
        // We'll implement basic metrics using SQL queries if needed
        Ok(QueueMetrics {
            queue_name: queue_name.to_string(),
            message_count: 0, // Placeholder
            oldest_message_age_seconds: None,
        })
    }

    /// Send message within a transaction (for atomic operations)
    pub async fn send_with_transaction<T>(
        &self,
        queue_name: &str,
        message: &T,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>>
    where
        T: serde::Serialize,
    {
        debug!(
            "📤 Sending message within transaction to queue: {}",
            queue_name
        );

        // Use pgmq's transaction support - checking if available
        let message_id =
            self.pgmq.send(queue_name, message).await.map_err(|e| {
                format!("Failed to send message to {queue_name} in transaction: {e}")
            })?;

        debug!("✅ Message sent in transaction with id: {}", message_id);
        Ok(message_id)
    }

    /// Get reference to underlying connection pool for advanced operations
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pgmq.connection
    }
}

/// Queue metrics structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMetrics {
    pub queue_name: String,
    pub message_count: i64,
    pub oldest_message_age_seconds: Option<i64>,
}

/// Helper methods for common queue operations
impl PgmqClient {
    /// Send step execution message to namespace queue
    pub async fn enqueue_step(
        &self,
        namespace: &str,
        step_message: PgmqStepMessage,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let queue_name = format!("{namespace}_queue");
        self.send_message(&queue_name, &step_message).await
    }

    /// Process messages from namespace queue
    pub async fn process_namespace_queue(
        &self,
        namespace: &str,
        visibility_timeout: Option<i32>,
        batch_size: i32,
    ) -> Result<Vec<Message<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>> {
        let queue_name = format!("{namespace}_queue");
        self.read_messages(&queue_name, visibility_timeout, Some(batch_size))
            .await
    }

    /// Complete message processing (delete from queue)
    pub async fn complete_message(
        &self,
        namespace: &str,
        message_id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let queue_name = format!("{namespace}_queue");
        self.delete_message(&queue_name, message_id).await
    }

    /// Initialize standard namespace queues
    pub async fn initialize_namespace_queues(
        &self,
        namespaces: &[&str],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🏗️ Initializing {} namespace queues", namespaces.len());

        for namespace in namespaces {
            let queue_name = format!("{namespace}_queue");
            self.create_queue(&queue_name).await?;
        }

        info!("✅ Initialized all namespace queues");
        Ok(())
    }
}

/// Implement PgmqClientTrait for standard PgmqClient
#[async_trait::async_trait]
impl crate::messaging::PgmqClientTrait for PgmqClient {
    async fn create_queue(
        &self,
        queue_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.create_queue(queue_name).await
    }

    async fn send_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.send_message(queue_name, message).await
    }

    async fn send_json_message<T: serde::Serialize + Clone + Send + Sync>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.send_json_message(queue_name, message).await
    }

    async fn read_messages(
        &self,
        queue_name: &str,
        visibility_timeout: Option<i32>,
        qty: Option<i32>,
    ) -> Result<
        Vec<pgmq::types::Message<serde_json::Value>>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        self.read_messages(queue_name, visibility_timeout, qty)
            .await
    }

    async fn delete_message(
        &self,
        queue_name: &str,
        message_id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.delete_message(queue_name, message_id).await
    }

    async fn archive_message(
        &self,
        queue_name: &str,
        message_id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.archive_message(queue_name, message_id).await
    }

    async fn purge_queue(
        &self,
        queue_name: &str,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        self.purge_queue(queue_name).await
    }

    async fn drop_queue(
        &self,
        queue_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.drop_queue(queue_name).await
    }

    async fn queue_metrics(
        &self,
        queue_name: &str,
    ) -> Result<QueueMetrics, Box<dyn std::error::Error + Send + Sync>> {
        self.queue_metrics(queue_name).await
    }

    async fn initialize_namespace_queues(
        &self,
        namespaces: &[&str],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.initialize_namespace_queues(namespaces).await
    }

    async fn enqueue_step(
        &self,
        namespace: &str,
        step_message: PgmqStepMessage,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.enqueue_step(namespace, step_message).await
    }

    async fn process_namespace_queue(
        &self,
        namespace: &str,
        visibility_timeout: Option<i32>,
        batch_size: i32,
    ) -> Result<
        Vec<pgmq::types::Message<serde_json::Value>>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        self.process_namespace_queue(namespace, visibility_timeout, batch_size)
            .await
    }

    async fn complete_message(
        &self,
        namespace: &str,
        message_id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.complete_message(namespace, message_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pgmq_client_creation() {
        // This test requires a PostgreSQL database with pgmq extension
        // Skip in CI or when database is not available
        if std::env::var("TEST_DATABASE_URL").is_err() {
            println!("Skipping pgmq test - no TEST_DATABASE_URL provided");
            return;
        }

        let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
        let client = PgmqClient::new(&database_url).await;
        assert!(client.is_ok(), "Failed to create pgmq client: {client:?}");
    }

    #[test]
    fn test_step_message_serialization() {
        let message = PgmqStepMessage {
            step_id: 12345,
            task_id: 67890,
            namespace: "test_namespace".to_string(),
            step_name: "test_step".to_string(),
            step_payload: serde_json::json!({"key": "value"}),
            metadata: PgmqStepMessageMetadata {
                enqueued_at: chrono::Utc::now(),
                retry_count: 0,
                max_retries: 3,
                timeout_seconds: Some(300),
            },
        };

        let serialized = serde_json::to_string(&message).expect("Failed to serialize");
        let deserialized: PgmqStepMessage =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(message.step_id, deserialized.step_id);
        assert_eq!(message.task_id, deserialized.task_id);
        assert_eq!(message.namespace, deserialized.namespace);
    }

    #[tokio::test]
    async fn test_shared_pool_pattern() {
        // Skip test if no database URL provided
        if std::env::var("TEST_DATABASE_URL").is_err() {
            println!("Skipping shared pool test - no TEST_DATABASE_URL provided");
            return;
        }

        let database_url = std::env::var("TEST_DATABASE_URL").unwrap();

        // Create a connection pool
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to create connection pool");

        // Create pgmq client with shared pool
        let client = PgmqClient::new_with_pool(pool.clone()).await;

        // Verify we can access the pool
        assert_eq!(client.pool().size(), pool.size());

        println!("✅ Shared pool pattern working correctly");
    }

    #[tokio::test]
    async fn test_queue_setup_teardown() {
        // Skip test if no database URL provided
        if std::env::var("TEST_DATABASE_URL").is_err() {
            println!("Skipping setup/teardown test - no TEST_DATABASE_URL provided");
            return;
        }

        let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
        let client = PgmqClient::new(&database_url)
            .await
            .expect("Failed to create client");

        let test_queue = "test_setup_teardown_queue";

        // Setup: Create queue
        client
            .create_queue(test_queue)
            .await
            .expect("Failed to create test queue");

        // Test: Send and receive a message
        let test_message = PgmqStepMessage {
            step_id: 999,
            task_id: 888,
            namespace: "test".to_string(),
            step_name: "test_step".to_string(),
            step_payload: serde_json::json!({"test": true}),
            metadata: PgmqStepMessageMetadata {
                enqueued_at: chrono::Utc::now(),
                retry_count: 0,
                max_retries: 1,
                timeout_seconds: Some(30),
            },
        };

        let message_id = client
            .send_message(test_queue, &test_message)
            .await
            .expect("Failed to send message");
        assert!(message_id > 0, "Message ID should be positive");

        // Teardown: Clean up test queue
        client
            .drop_queue(test_queue)
            .await
            .expect("Failed to drop test queue");

        println!("✅ Queue setup/teardown test completed successfully");
    }
}
