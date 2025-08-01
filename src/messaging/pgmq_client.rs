//! # PostgreSQL Message Queue (pgmq) Client
//!
//! Rust integration layer for pgmq using sqlx. Provides SQS-like message queue
//! operations directly on PostgreSQL for workflow orchestration.

use crate::database::{DatabasePool, get_pool};
use crate::messaging::message::StepMessage;
use serde_json;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// PostgreSQL message queue client using sqlx
#[derive(Debug, Clone)]
pub struct PgmqClient {
    /// Database connection pool
    pool: PgPool,
}

/// Message from pgmq queue with metadata
#[derive(Debug, Clone)]
pub struct QueueMessage {
    /// Message ID assigned by pgmq
    pub msg_id: i64,
    /// Queue name
    pub queue_name: String,
    /// Message content as JSON
    pub message: serde_json::Value,
    /// Visibility timeout (when message becomes visible again if not deleted)
    pub vt: chrono::DateTime<chrono::Utc>,
    /// When message was enqueued
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
    /// Read count (how many times message has been read)
    pub read_ct: i32,
}

/// Queue statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    /// Queue name
    pub queue_name: String,
    /// Total messages in queue (visible + invisible)
    pub queue_length: i64,
    /// Number of messages currently visible (ready for processing)
    pub visible_messages: i64,
    /// Number of messages currently invisible (being processed)
    pub invisible_messages: i64,
    /// Oldest message age in seconds
    pub oldest_msg_age_seconds: Option<i64>,
    /// When stats were collected
    pub collected_at: chrono::DateTime<chrono::Utc>,
}

impl PgmqClient {
    /// Create a new pgmq client using the global database pool
    pub async fn new() -> Result<Self, sqlx::Error> {
        let pool = get_pool().await?;
        Ok(Self { pool })
    }

    /// Create a new pgmq client with a specific database pool
    pub fn new_with_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new queue
    /// 
    /// # Arguments
    /// * `queue_name` - Name of the queue to create
    /// 
    /// # Returns
    /// * `Ok(())` if queue created successfully or already exists
    /// * `Err(sqlx::Error)` if creation fails
    pub async fn create_queue(&self, queue_name: &str) -> Result<(), sqlx::Error> {
        debug!("📦 PGMQ: Creating queue: {}", queue_name);

        sqlx::query("SELECT pgmq_create($1)")
            .bind(queue_name)
            .execute(&self.pool)
            .await?;

        info!("✅ PGMQ: Queue created successfully: {}", queue_name);
        Ok(())
    }

    /// Drop/delete a queue and all its messages
    /// 
    /// # Arguments
    /// * `queue_name` - Name of the queue to drop
    pub async fn drop_queue(&self, queue_name: &str) -> Result<(), sqlx::Error> {
        debug!("🗑️ PGMQ: Dropping queue: {}", queue_name);

        sqlx::query("SELECT pgmq_drop($1)")
            .bind(queue_name)
            .execute(&self.pool)
            .await?;

        info!("✅ PGMQ: Queue dropped successfully: {}", queue_name);
        Ok(())
    }

    /// Send a message to a queue
    /// 
    /// # Arguments
    /// * `queue_name` - Name of the queue
    /// * `message` - JSON message content
    /// * `delay_seconds` - Optional delay before message becomes visible (default: 0)
    /// 
    /// # Returns
    /// * `Ok(msg_id)` - Message ID assigned by pgmq
    /// * `Err(sqlx::Error)` if send fails
    pub async fn send(
        &self,
        queue_name: &str,
        message: serde_json::Value,
        delay_seconds: Option<i32>,
    ) -> Result<i64, sqlx::Error> {
        let delay = delay_seconds.unwrap_or(0);
        
        debug!("📤 PGMQ: Sending message to queue: {} (delay: {}s)", queue_name, delay);

        let row = sqlx::query("SELECT pgmq_send($1, $2, $3) as msg_id")
            .bind(queue_name)
            .bind(message)
            .bind(delay)
            .fetch_one(&self.pool)
            .await?;

        let msg_id: i64 = row.get("msg_id");
        debug!("✅ PGMQ: Message sent successfully: {} -> msg_id: {}", queue_name, msg_id);
        
        Ok(msg_id)
    }

    /// Send a step message to the appropriate namespace queue
    /// 
    /// # Arguments
    /// * `step_message` - Step message to send
    /// * `delay_seconds` - Optional delay before message becomes visible
    /// 
    /// # Returns
    /// * `Ok(msg_id)` - Message ID assigned by pgmq
    pub async fn send_step_message(
        &self, 
        step_message: &StepMessage,
        delay_seconds: Option<i32>,
    ) -> Result<i64, sqlx::Error> {
        let queue_name = step_message.queue_name();
        let message_json = step_message.to_json()
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize step message: {}", e).into()))?;

        debug!("📤 PGMQ: Sending step message - step_id: {}, task_id: {}, queue: {}", 
               step_message.step_id, step_message.task_id, queue_name);

        self.send(&queue_name, message_json, delay_seconds).await
    }

    /// Read messages from a queue
    /// 
    /// # Arguments
    /// * `queue_name` - Name of the queue
    /// * `vt_seconds` - Visibility timeout in seconds (how long message stays invisible)
    /// * `qty` - Number of messages to read (default: 1, max: 100)
    /// 
    /// # Returns
    /// * `Ok(Vec<QueueMessage>)` - List of messages read from queue
    pub async fn read(
        &self,
        queue_name: &str,
        vt_seconds: i32,
        qty: Option<i32>,
    ) -> Result<Vec<QueueMessage>, sqlx::Error> {
        let quantity = qty.unwrap_or(1).min(100); // pgmq has max limit
        
        debug!("📥 PGMQ: Reading {} messages from queue: {} (vt: {}s)", quantity, queue_name, vt_seconds);

        let rows = sqlx::query(
            "SELECT msg_id, read_ct, enqueued_at, vt, message 
             FROM pgmq_read($1, $2, $3)"
        )
        .bind(queue_name)
        .bind(vt_seconds)
        .bind(quantity)
        .fetch_all(&self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            let message = QueueMessage {
                msg_id: row.get("msg_id"),
                queue_name: queue_name.to_string(),
                message: row.get("message"),
                vt: row.get("vt"),
                enqueued_at: row.get("enqueued_at"),
                read_ct: row.get("read_ct"),
            };
            messages.push(message);
        }

        debug!("✅ PGMQ: Read {} messages from queue: {}", messages.len(), queue_name);
        Ok(messages)
    }

    /// Read step messages from a namespace queue
    /// 
    /// # Arguments
    /// * `namespace` - Namespace to read from (e.g., "fulfillment")
    /// * `vt_seconds` - Visibility timeout in seconds
    /// * `qty` - Number of messages to read
    /// 
    /// # Returns
    /// * `Ok(Vec<(QueueMessage, StepMessage)>)` - Queue messages with parsed step messages
    pub async fn read_step_messages(
        &self,
        namespace: &str,
        vt_seconds: i32,
        qty: Option<i32>,
    ) -> Result<Vec<(QueueMessage, StepMessage)>, sqlx::Error> {
        let queue_name = format!("{}_queue", namespace);
        let queue_messages = self.read(&queue_name, vt_seconds, qty).await?;

        let mut step_messages = Vec::new();
        for queue_msg in queue_messages {
            match StepMessage::from_json(queue_msg.message.clone()) {
                Ok(step_msg) => {
                    debug!("📋 PGMQ: Parsed step message - step_id: {}, task_id: {}", 
                           step_msg.step_id, step_msg.task_id);
                    step_messages.push((queue_msg, step_msg));
                }
                Err(e) => {
                    warn!("⚠️ PGMQ: Failed to parse step message from queue {}: {} - message: {:?}", 
                          queue_name, e, queue_msg.message);
                    // Continue processing other messages rather than failing entirely
                }
            }
        }

        Ok(step_messages)
    }

    /// Delete a message from the queue (acknowledge processing completion)
    /// 
    /// # Arguments
    /// * `queue_name` - Name of the queue
    /// * `msg_id` - Message ID to delete
    /// 
    /// # Returns
    /// * `Ok(true)` if message was deleted
    /// * `Ok(false)` if message was not found
    pub async fn delete(&self, queue_name: &str, msg_id: i64) -> Result<bool, sqlx::Error> {
        debug!("🗑️ PGMQ: Deleting message: {} from queue: {}", msg_id, queue_name);

        let row = sqlx::query("SELECT pgmq_delete($1, $2) as deleted")
            .bind(queue_name)
            .bind(msg_id)
            .fetch_one(&self.pool)
            .await?;

        let deleted: bool = row.get("deleted");
        if deleted {
            debug!("✅ PGMQ: Message deleted successfully: {} from {}", msg_id, queue_name);
        } else {
            warn!("⚠️ PGMQ: Message not found for deletion: {} from {}", msg_id, queue_name);
        }

        Ok(deleted)
    }

    /// Archive a message (move to archive table for retention)
    /// 
    /// # Arguments
    /// * `queue_name` - Name of the queue
    /// * `msg_id` - Message ID to archive
    pub async fn archive(&self, queue_name: &str, msg_id: i64) -> Result<bool, sqlx::Error> {
        debug!("📦 PGMQ: Archiving message: {} from queue: {}", msg_id, queue_name);

        let row = sqlx::query("SELECT pgmq_archive($1, $2) as archived")
            .bind(queue_name)
            .bind(msg_id)
            .fetch_one(&self.pool)
            .await?;

        let archived: bool = row.get("archived");
        if archived {
            debug!("✅ PGMQ: Message archived successfully: {} from {}", msg_id, queue_name);
        } else {
            warn!("⚠️ PGMQ: Message not found for archiving: {} from {}", msg_id, queue_name);
        }

        Ok(archived)
    }

    /// Get queue statistics
    /// 
    /// # Arguments
    /// * `queue_name` - Name of the queue
    /// 
    /// # Returns
    /// * `Ok(QueueStats)` - Queue statistics
    pub async fn get_queue_stats(&self, queue_name: &str) -> Result<QueueStats, sqlx::Error> {
        debug!("📊 PGMQ: Getting stats for queue: {}", queue_name);

        let row = sqlx::query(
            "SELECT queue_length, newest_msg_age_sec, oldest_msg_age_sec, total_messages
             FROM pgmq_metrics($1)"
        )
        .bind(queue_name)
        .fetch_one(&self.pool)
        .await?;

        // Get visible/invisible message counts
        let visible_row = sqlx::query(
            "SELECT COUNT(*) as visible_count FROM pgmq_read($1, 0, 1000)"
        )
        .bind(queue_name)
        .fetch_one(&self.pool)
        .await?;

        let queue_length: i64 = row.get("queue_length");
        let oldest_msg_age_seconds: Option<i64> = row.get("oldest_msg_age_sec");
        let visible_messages: i64 = visible_row.get("visible_count");
        let invisible_messages = queue_length - visible_messages;

        let stats = QueueStats {
            queue_name: queue_name.to_string(),
            queue_length,
            visible_messages,
            invisible_messages,
            oldest_msg_age_seconds,
            collected_at: chrono::Utc::now(),
        };

        debug!("✅ PGMQ: Queue stats - {}: {} total, {} visible, {} invisible", 
               queue_name, queue_length, visible_messages, invisible_messages);

        Ok(stats)
    }

    /// List all available queues
    /// 
    /// # Returns
    /// * `Ok(Vec<String>)` - List of queue names
    pub async fn list_queues(&self) -> Result<Vec<String>, sqlx::Error> {
        debug!("📋 PGMQ: Listing all queues");

        let rows = sqlx::query("SELECT queue_name FROM pgmq_list_queues()")
            .fetch_all(&self.pool)
            .await?;

        let queue_names: Vec<String> = rows
            .into_iter()
            .map(|row| row.get("queue_name"))
            .collect();

        debug!("✅ PGMQ: Found {} queues: {:?}", queue_names.len(), queue_names);
        Ok(queue_names)
    }

    /// Purge all messages from a queue
    /// 
    /// # Arguments
    /// * `queue_name` - Name of the queue to purge
    /// 
    /// # Returns
    /// * `Ok(purged_count)` - Number of messages purged
    pub async fn purge_queue(&self, queue_name: &str) -> Result<i64, sqlx::Error> {
        debug!("🧹 PGMQ: Purging all messages from queue: {}", queue_name);

        let row = sqlx::query("SELECT pgmq_purge($1) as purged_count")
            .bind(queue_name)
            .fetch_one(&self.pool)
            .await?;

        let purged_count: i64 = row.get("purged_count");
        info!("✅ PGMQ: Purged {} messages from queue: {}", purged_count, queue_name);

        Ok(purged_count)
    }

    /// Ensure namespace queues exist for workflow processing
    /// 
    /// # Arguments
    /// * `namespaces` - List of namespaces to ensure queues for
    pub async fn ensure_namespace_queues(&self, namespaces: &[&str]) -> Result<(), sqlx::Error> {
        info!("🚀 PGMQ: Ensuring namespace queues exist: {:?}", namespaces);

        for namespace in namespaces {
            let queue_name = format!("{}_queue", namespace);
            if let Err(e) = self.create_queue(&queue_name).await {
                // pgmq_create is idempotent - it won't fail if queue already exists
                // But we should log any actual errors
                error!("❌ PGMQ: Failed to create queue {}: {}", queue_name, e);
                return Err(e);
            }
        }

        info!("✅ PGMQ: All namespace queues ensured");
        Ok(())
    }

    /// Get comprehensive metrics for all queues
    pub async fn get_all_queue_metrics(&self) -> Result<HashMap<String, QueueStats>, sqlx::Error> {
        let queue_names = self.list_queues().await?;
        let mut metrics = HashMap::new();

        for queue_name in queue_names {
            match self.get_queue_stats(&queue_name).await {
                Ok(stats) => {
                    metrics.insert(queue_name, stats);
                }
                Err(e) => {
                    warn!("⚠️ PGMQ: Failed to get stats for queue {}: {}", queue_name, e);
                }
            }
        }

        Ok(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::message::StepMessageMetadata;

    // Note: These tests require a running PostgreSQL instance with pgmq extension
    // They are marked as ignored by default - run with `cargo test -- --ignored`

    #[tokio::test]
    #[ignore]
    async fn test_pgmq_basic_operations() {
        let client = PgmqClient::new().await.expect("Failed to create pgmq client");
        let test_queue = "test_queue";

        // Clean up any existing test queue
        let _ = client.drop_queue(test_queue).await;

        // Create queue
        client.create_queue(test_queue).await.expect("Failed to create queue");

        // Send message
        let test_message = serde_json::json!({"test": "data", "timestamp": chrono::Utc::now()});
        let msg_id = client.send(test_queue, test_message.clone(), None).await
            .expect("Failed to send message");

        assert!(msg_id > 0);

        // Read message
        let messages = client.read(test_queue, 30, Some(1)).await
            .expect("Failed to read messages");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].msg_id, msg_id);
        assert_eq!(messages[0].message["test"], "data");

        // Delete message
        let deleted = client.delete(test_queue, msg_id).await
            .expect("Failed to delete message");
        assert!(deleted);

        // Clean up
        client.drop_queue(test_queue).await.expect("Failed to drop test queue");
    }

    #[tokio::test]
    #[ignore]
    async fn test_step_message_operations() {
        let client = PgmqClient::new().await.expect("Failed to create pgmq client");
        
        // Create step message
        let step_message = StepMessage::new(
            12345,
            67890,
            "test_fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            "validate_order".to_string(),
            serde_json::json!({"order_id": 1001}),
        );

        let queue_name = step_message.queue_name();
        
        // Clean up any existing test queue
        let _ = client.drop_queue(&queue_name).await;
        
        // Create queue
        client.create_queue(&queue_name).await.expect("Failed to create queue");

        // Send step message
        let msg_id = client.send_step_message(&step_message, None).await
            .expect("Failed to send step message");

        // Read step messages
        let messages = client.read_step_messages("test_fulfillment", 30, Some(1)).await
            .expect("Failed to read step messages");

        assert_eq!(messages.len(), 1);
        let (queue_msg, parsed_step_msg) = &messages[0];
        assert_eq!(queue_msg.msg_id, msg_id);
        assert_eq!(parsed_step_msg.step_id, 12345);
        assert_eq!(parsed_step_msg.task_id, 67890);

        // Clean up
        client.delete(&queue_name, msg_id).await.expect("Failed to delete message");
        client.drop_queue(&queue_name).await.expect("Failed to drop test queue");
    }

    #[tokio::test]
    #[ignore]
    async fn test_queue_statistics() {
        let client = PgmqClient::new().await.expect("Failed to create pgmq client");
        let test_queue = "test_stats_queue";

        // Clean up and create queue
        let _ = client.drop_queue(test_queue).await;
        client.create_queue(test_queue).await.expect("Failed to create queue");

        // Send some test messages
        for i in 0..5 {
            let msg = serde_json::json!({"test_message": i});
            client.send(test_queue, msg, None).await.expect("Failed to send message");
        }

        // Get stats
        let stats = client.get_queue_stats(test_queue).await
            .expect("Failed to get queue stats");

        assert_eq!(stats.queue_name, test_queue);
        assert_eq!(stats.queue_length, 5);
        assert_eq!(stats.visible_messages, 5);
        assert_eq!(stats.invisible_messages, 0);

        // Clean up
        client.drop_queue(test_queue).await.expect("Failed to drop test queue");
    }
}