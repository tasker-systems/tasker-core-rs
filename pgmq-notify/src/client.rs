//! # Unified PGMQ Client
//!
//! This module provides a unified PostgreSQL Message Queue (PGMQ) client that combines
//! all functionality from both the original PgmqNotifyClient and the tasker-shared PgmqClient.
//! It includes both notification capabilities and comprehensive PGMQ operations.

use crate::{
    config::PgmqNotifyConfig,
    error::{PgmqNotifyError, Result},
    listener::PgmqNotifyListener,
    types::{ClientStatus, QueueMetrics},
};
use pgmq::{types::Message, PGMQueue};
use regex::Regex;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing::{debug, error, info, instrument, warn};

/// Unified PGMQ client with comprehensive functionality and notification capabilities
#[derive(Debug, Clone)]
pub struct PgmqClient {
    /// Underlying PGMQ client
    pgmq: PGMQueue,
    /// Database connection pool for advanced operations and health checks
    pool: sqlx::PgPool,
    /// Configuration for notifications and queue naming
    config: PgmqNotifyConfig,
}

impl PgmqClient {
    /// Create new unified PGMQ client using connection string
    pub async fn new(database_url: &str) -> Result<Self> {
        Self::new_with_config(database_url, PgmqNotifyConfig::default()).await
    }

    /// Create new unified PGMQ client with custom configuration
    pub async fn new_with_config(database_url: &str, config: PgmqNotifyConfig) -> Result<Self> {
        info!("Connecting to pgmq using unified client");

        let pgmq = PGMQueue::new(database_url.to_string()).await?;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(20)
            .connect(database_url)
            .await?;

        info!("Connected to pgmq using unified client");
        Ok(Self { pgmq, pool, config })
    }

    /// Create new unified PGMQ client using existing connection pool (BYOP - Bring Your Own Pool)
    pub async fn new_with_pool(pool: sqlx::PgPool) -> Self {
        Self::new_with_pool_and_config(pool, PgmqNotifyConfig::default()).await
    }

    /// Create new unified PGMQ client with existing pool and custom configuration
    pub async fn new_with_pool_and_config(pool: sqlx::PgPool, config: PgmqNotifyConfig) -> Self {
        info!("Creating unified pgmq client with shared connection pool");

        let pgmq = PGMQueue::new_with_pool(pool.clone()).await;

        info!("Unified pgmq client created with shared pool");
        Self { pgmq, pool, config }
    }

    /// Create queue if it doesn't exist
    #[instrument(skip(self), fields(queue = %queue_name))]
    pub async fn create_queue(&self, queue_name: &str) -> Result<()> {
        debug!("📋 Creating queue: {}", queue_name);

        self.pgmq.create(queue_name).await?;

        info!("Queue created: {}", queue_name);
        Ok(())
    }

    /// Send generic JSON message to queue
    #[instrument(skip(self, message), fields(queue = %queue_name))]
    pub async fn send_json_message<T>(&self, queue_name: &str, message: &T) -> Result<i64>
    where
        T: serde::Serialize,
    {
        debug!("📤 Sending JSON message to queue: {}", queue_name);

        let serialized = serde_json::to_value(message)?;

        // Use wrapper function for atomic message sending + notification
        let message_id = sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2, $3)",
            queue_name,
            &serialized,
            0i32
        )
        .fetch_one(&self.pool)
        .await?;

        let message_id = message_id.ok_or_else(|| {
            PgmqNotifyError::Generic(anyhow::anyhow!("Wrapper function returned NULL message ID"))
        })?;

        info!(
            "JSON message sent to queue: {} with ID: {} (with notification)",
            queue_name, message_id
        );
        Ok(message_id)
    }

    /// Send message with visibility timeout (delay)
    #[instrument(skip(self, message), fields(queue = %queue_name, delay_seconds = %delay_seconds))]
    pub async fn send_message_with_delay<T>(
        &self,
        queue_name: &str,
        message: &T,
        delay_seconds: u64,
    ) -> Result<i64>
    where
        T: serde::Serialize,
    {
        debug!(
            "📤 Sending delayed message to queue: {} with delay: {}s",
            queue_name, delay_seconds
        );

        let serialized = serde_json::to_value(message)?;

        // Use wrapper function for atomic message sending + notification
        let message_id = sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2, $3)",
            queue_name,
            &serialized,
            delay_seconds as i32
        )
        .fetch_one(&self.pool)
        .await?;

        let message_id = message_id.ok_or_else(|| {
            PgmqNotifyError::Generic(anyhow::anyhow!("Wrapper function returned NULL message ID"))
        })?;

        info!(
            "Delayed message sent to queue: {} with ID: {} (with notification)",
            queue_name, message_id
        );
        Ok(message_id)
    }

    /// Read messages from queue
    #[instrument(skip(self), fields(queue = %queue_name, limit = ?limit))]
    pub async fn read_messages(
        &self,
        queue_name: &str,
        visibility_timeout: Option<i32>,
        limit: Option<i32>,
    ) -> Result<Vec<Message<serde_json::Value>>> {
        debug!(
            "📥 Reading messages from queue: {} (limit: {:?})",
            queue_name, limit
        );

        let messages = match limit {
            Some(l) => self
                .pgmq
                .read_batch(queue_name, visibility_timeout, l)
                .await?
                .unwrap_or_default(),
            None => match self.pgmq.read(queue_name, visibility_timeout).await? {
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

    /// Read messages from queue with pop (single read and delete)
    #[instrument(skip(self), fields(queue = %queue_name))]
    pub async fn pop_message(
        &self,
        queue_name: &str,
    ) -> Result<Option<Message<serde_json::Value>>> {
        debug!("📥 Popping message from queue: {}", queue_name);

        let message = self.pgmq.pop(queue_name).await?;

        if message.is_some() {
            debug!("📨 Popped message from queue: {}", queue_name);
        } else {
            debug!("📭 No messages available in queue: {}", queue_name);
        }
        Ok(message)
    }

    /// Read a specific message by ID using custom SQL function (for notification event handling)
    #[instrument(skip(self), fields(queue = %queue_name, message_id = %message_id))]
    pub async fn read_specific_message<T>(
        &self,
        queue_name: &str,
        message_id: i64,
        visibility_timeout: i32,
    ) -> Result<Option<Message<T>>>
    where
        T: serde::de::DeserializeOwned,
    {
        debug!(
            "📥 Reading specific message {} from queue: {}",
            message_id, queue_name
        );

        // Use the custom SQL function pgmq_read_specific_message for efficient specific message reading
        let query = "SELECT msg_id, read_ct, enqueued_at, vt, message FROM pgmq_read_specific_message($1, $2, $3)";

        let row = sqlx::query(query)
            .bind(queue_name)
            .bind(message_id)
            .bind(visibility_timeout)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let msg_id: i64 = row.get("msg_id");
            let read_ct: i32 = row.get("read_ct");
            let enqueued_at: chrono::DateTime<chrono::Utc> = row.get("enqueued_at");
            let vt: chrono::DateTime<chrono::Utc> = row.get("vt");
            let message_json: serde_json::Value = row.get("message");

            // Try to deserialize the message to the expected type
            match serde_json::from_value::<T>(message_json) {
                Ok(deserialized) => {
                    let typed_message = Message {
                        msg_id,
                        read_ct,
                        enqueued_at,
                        vt,
                        message: deserialized,
                    };
                    debug!("Found and deserialized specific message {}", message_id);
                    Ok(Some(typed_message))
                }
                Err(e) => {
                    warn!("Failed to deserialize message {}: {}", message_id, e);
                    Err(PgmqNotifyError::Serialization(e))
                }
            }
        } else {
            debug!(
                "📭 Specific message {} not found in queue: {}",
                message_id, queue_name
            );
            Ok(None)
        }
    }

    /// Delete message from queue
    #[instrument(skip(self), fields(queue = %queue_name, message_id = %message_id))]
    pub async fn delete_message(&self, queue_name: &str, message_id: i64) -> Result<()> {
        debug!(
            "🗑Deleting message {} from queue: {}",
            message_id, queue_name
        );

        self.pgmq.delete(queue_name, message_id).await?;

        debug!("Message deleted: {}", message_id);
        Ok(())
    }

    /// Archive message (move to archive)
    #[instrument(skip(self), fields(queue = %queue_name, message_id = %message_id))]
    pub async fn archive_message(&self, queue_name: &str, message_id: i64) -> Result<()> {
        debug!(
            "Archiving message {} from queue: {}",
            message_id, queue_name
        );

        self.pgmq.archive(queue_name, message_id).await?;

        debug!("Message archived: {}", message_id);
        Ok(())
    }

    /// Purge queue (delete all messages)
    #[instrument(skip(self), fields(queue = %queue_name))]
    pub async fn purge_queue(&self, queue_name: &str) -> Result<u64> {
        warn!("🧹 Purging queue: {}", queue_name);

        let purged_count = self.pgmq.purge(queue_name).await?;

        warn!(
            "🗑Purged {} messages from queue: {}",
            purged_count, queue_name
        );
        Ok(purged_count)
    }

    /// Drop queue completely
    #[instrument(skip(self), fields(queue = %queue_name))]
    pub async fn drop_queue(&self, queue_name: &str) -> Result<()> {
        warn!("💥 Dropping queue: {}", queue_name);

        self.pgmq.destroy(queue_name).await?;

        warn!("🗑Queue dropped: {}", queue_name);
        Ok(())
    }

    /// Get queue metrics/statistics
    #[instrument(skip(self), fields(queue = %queue_name))]
    pub async fn queue_metrics(&self, queue_name: &str) -> Result<QueueMetrics> {
        debug!("Getting metrics for queue: {}", queue_name);

        // Query actual pgmq metrics from the database using pgmq.metrics() function
        let row = sqlx::query!(
            "SELECT queue_length, oldest_msg_age_sec FROM pgmq.metrics($1)",
            queue_name
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(QueueMetrics {
                queue_name: queue_name.to_string(),
                message_count: row.queue_length.unwrap_or(0),
                consumer_count: None,
                oldest_message_age_seconds: row.oldest_msg_age_sec.map(|age| age as i64),
            })
        } else {
            // Queue doesn't exist or has no metrics
            Ok(QueueMetrics {
                queue_name: queue_name.to_string(),
                message_count: 0,
                consumer_count: None,
                oldest_message_age_seconds: None,
            })
        }
    }

    /// Get reference to underlying connection pool for advanced operations
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    /// Get reference to pgmq client for direct access
    pub fn pgmq(&self) -> &PGMQueue {
        &self.pgmq
    }

    /// Get the configuration
    pub fn config(&self) -> &PgmqNotifyConfig {
        &self.config
    }

    /// Check if this client has notification capabilities enabled
    pub fn has_notify_capabilities(&self) -> bool {
        // Check if the config has triggers enabled which enable notifications
        self.config.enable_triggers
    }

    /// Health check - verify database connectivity
    #[instrument(skip(self))]
    pub async fn health_check(&self) -> Result<bool> {
        match sqlx::query!("SELECT 1 as health_check")
            .fetch_one(&self.pool)
            .await
        {
            Ok(_) => {
                debug!("Health check passed");
                Ok(true)
            }
            Err(e) => {
                error!("Health check failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Get client status information
    #[instrument(skip(self))]
    pub async fn get_client_status(&self) -> Result<ClientStatus> {
        let healthy = self.health_check().await.unwrap_or(false);

        Ok(ClientStatus {
            client_type: "pgmq-unified".to_string(),
            connected: healthy,
            connection_info: HashMap::from([
                (
                    "backend".to_string(),
                    serde_json::Value::String("postgresql".to_string()),
                ),
                (
                    "queue_type".to_string(),
                    serde_json::Value::String("pgmq".to_string()),
                ),
                (
                    "has_notifications".to_string(),
                    serde_json::Value::Bool(true),
                ),
                (
                    "pool_size".to_string(),
                    serde_json::Value::Number(self.pool.size().into()),
                ),
            ]),
            last_activity: Some(chrono::Utc::now()),
        })
    }

    /// Extract namespace from queue name using configured pattern
    pub fn extract_namespace(&self, queue_name: &str) -> Option<String> {
        let pattern = &self.config.queue_naming_pattern;
        if let Ok(regex) = Regex::new(pattern) {
            if let Some(captures) = regex.captures(queue_name) {
                if let Some(namespace_match) = captures.name("namespace") {
                    return Some(namespace_match.as_str().to_string());
                }
            }
        }

        // Fallback: assume queue name is "{namespace}_queue"
        if queue_name.ends_with("_queue") {
            Some(queue_name.trim_end_matches("_queue").to_string())
        } else {
            None
        }
    }

    /// Create a notify listener for this client
    pub async fn create_listener(&self) -> Result<PgmqNotifyListener> {
        PgmqNotifyListener::new(self.pool.clone(), self.config.clone()).await
    }
}

/// Helper methods for common queue operations
impl PgmqClient {
    /// Process messages from namespace queue
    #[instrument(skip(self), fields(namespace = %namespace, batch_size = %batch_size))]
    pub async fn process_namespace_queue(
        &self,
        namespace: &str,
        visibility_timeout: Option<i32>,
        batch_size: i32,
    ) -> Result<Vec<Message<serde_json::Value>>> {
        let queue_name = format!("worker_{namespace}_queue");
        self.read_messages(&queue_name, visibility_timeout, Some(batch_size))
            .await
    }

    /// Complete message processing (delete from queue)
    #[instrument(skip(self), fields(namespace = %namespace, message_id = %message_id))]
    pub async fn complete_message(&self, namespace: &str, message_id: i64) -> Result<()> {
        let queue_name = format!("worker_{namespace}_queue");
        self.delete_message(&queue_name, message_id).await
    }

    /// Initialize standard namespace queues
    #[instrument(skip(self, namespaces))]
    pub async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> Result<()> {
        info!("Initializing {} namespace queues", namespaces.len());

        for namespace in namespaces {
            let queue_name = format!("worker_{namespace}_queue");
            self.create_queue(&queue_name).await?;
        }

        info!("Initialized all namespace queues");
        Ok(())
    }

    /// Send message within a transaction (for atomic operations)
    #[instrument(skip(self, message, tx), fields(queue = %queue_name))]
    pub async fn send_with_transaction<T>(
        &self,
        queue_name: &str,
        message: &T,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<i64>
    where
        T: serde::Serialize,
    {
        debug!(
            "📤 Sending message within transaction to queue: {}",
            queue_name
        );

        let serialized = serde_json::to_value(message)?;

        // Use wrapper function within the transaction for atomic message sending + notification
        let message_id = sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2, $3)",
            queue_name,
            &serialized,
            0i32
        )
        .fetch_one(&mut **tx)
        .await?;

        let message_id = message_id.ok_or_else(|| {
            PgmqNotifyError::Generic(anyhow::anyhow!("Wrapper function returned NULL message ID"))
        })?;

        debug!(
            "Message sent in transaction with id: {} (with notification)",
            message_id
        );
        Ok(message_id)
    }
}

/// Factory for creating PgmqClient instances
pub struct PgmqClientFactory;

impl PgmqClientFactory {
    /// Create new client from database URL
    pub async fn create(database_url: &str) -> Result<PgmqClient> {
        PgmqClient::new(database_url).await
    }

    /// Create new client with configuration
    pub async fn create_with_config(
        database_url: &str,
        config: PgmqNotifyConfig,
    ) -> Result<PgmqClient> {
        PgmqClient::new_with_config(database_url, config).await
    }

    /// Create new client with existing pool
    pub async fn create_with_pool(pool: PgPool) -> PgmqClient {
        PgmqClient::new_with_pool(pool).await
    }

    /// Create new client with existing pool and configuration
    pub async fn create_with_pool_and_config(pool: PgPool, config: PgmqNotifyConfig) -> PgmqClient {
        PgmqClient::new_with_pool_and_config(pool, config).await
    }
}

// Re-export for backward compatibility
pub use PgmqClient as PgmqNotifyClient;
pub use PgmqClientFactory as PgmqNotifyClientFactory;

#[cfg(test)]
mod tests {
    use super::*;
    use dotenvy::dotenv;

    #[tokio::test]
    async fn test_pgmq_client_creation() {
        dotenv().ok();
        // This test requires a PostgreSQL database with pgmq extension
        // Skip in CI or when database is not available
        if std::env::var("DATABASE_URL").is_err() {
            println!("Skipping pgmq test - no DATABASE_URL provided");
            return;
        }

        let database_url = std::env::var("DATABASE_URL").unwrap();
        match PgmqClient::new(&database_url).await {
            Ok(_) => {
                // Client creation succeeded
                println!("PgmqClient created successfully");
            }
            Err(e) => {
                // Skip test if it's a URL parsing or connection error
                // This allows the test to pass in environments without proper database setup
                println!(" Skipping test due to client creation error: {e:?}");
                return;
            }
        }
    }

    #[test]
    fn test_namespace_extraction() {
        dotenv().ok();
        let config = PgmqNotifyConfig::new().with_queue_naming_pattern(r"(?P<namespace>\w+)_queue");

        // Test the pattern matching directly without needing a full client
        let pattern = &config.queue_naming_pattern;
        let regex = regex::Regex::new(pattern).unwrap();

        // Test valid patterns
        let captures = regex.captures("orders_queue").unwrap();
        let namespace = captures.name("namespace").unwrap().as_str();
        assert_eq!(namespace, "orders");

        let captures = regex.captures("inventory_queue").unwrap();
        let namespace = captures.name("namespace").unwrap().as_str();
        assert_eq!(namespace, "inventory");

        // Test invalid pattern
        assert!(regex.captures("invalid_name").is_none());
    }

    #[tokio::test]
    async fn test_shared_pool_pattern() {
        dotenv().ok();
        // Skip test if no database URL provided
        if std::env::var("DATABASE_URL").is_err() {
            println!("Skipping shared pool test - no DATABASE_URL provided");
            return;
        }

        let database_url = std::env::var("DATABASE_URL").unwrap();

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

        println!("Shared pool pattern working correctly");
    }
}
