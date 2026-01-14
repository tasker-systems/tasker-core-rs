//! # RabbitMQ Messaging Service (TAS-133d)
//!
//! RabbitMQ implementation of the `MessagingService` trait using the `lapin` crate.
//!
//! ## Features
//!
//! - **AMQP 0.9.1**: Standard RabbitMQ protocol support
//! - **Durable Queues**: Messages survive broker restarts
//! - **Dead Letter Exchanges**: Automatic DLQ routing for nack'd messages
//! - **Prefetch Control**: Backpressure via consumer prefetch limits
//!
//! ## Key Differences from PGMQ
//!
//! | Feature | PGMQ | RabbitMQ |
//! |---------|------|----------|
//! | Visibility timeout | Native `set_vt()` | Via consumer prefetch |
//! | Archive/DLQ | `a_{queue}` tables | Dead Letter Exchange |
//! | Message ID | `i64` from sequence | Delivery tag (u64) |
//! | Extend visibility | Supported | Not supported (logs warning) |
//!
//! ## Usage
//!
//! ```ignore
//! use tasker_shared::messaging::service::providers::RabbitMqMessagingService;
//! use tasker_shared::messaging::service::MessagingService;
//! use tasker_shared::config::tasker::RabbitmqConfig;
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = RabbitmqConfig::default();
//! let service = RabbitMqMessagingService::new(&config).await?;
//!
//! // Create a queue (with DLX)
//! service.ensure_queue("my_queue").await?;
//!
//! // Send a message
//! let msg_id = service.send_message("my_queue", &serde_json::json!({"key": "value"})).await?;
//!
//! // Receive messages
//! let messages = service.receive_messages::<serde_json::Value>(
//!     "my_queue",
//!     10,
//!     Duration::from_secs(30),
//! ).await?;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use lapin::options::{
    BasicAckOptions, BasicGetOptions, BasicNackOptions, BasicPublishOptions,
    ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::{AMQPValue, FieldTable};
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind};
use tokio::sync::RwLock;

use crate::config::tasker::{RabbitmqConfig, TaskerConfig};
use crate::messaging::service::traits::{MessagingService, QueueMessage};
use crate::messaging::service::types::{
    MessageId, QueueHealthReport, QueueStats, QueuedMessage, ReceiptHandle,
};
use crate::messaging::MessagingError;

/// Internal queue statistics tracking
#[derive(Debug, Default)]
struct QueueStatistics {
    total_sent: AtomicU64,
    total_received: AtomicU64,
    total_acked: AtomicU64,
    total_nacked: AtomicU64,
}

/// RabbitMQ-based messaging service implementation
///
/// Uses the `lapin` crate for AMQP 0.9.1 protocol support.
/// Implements all `MessagingService` trait methods with RabbitMQ semantics.
#[derive(Debug)]
pub struct RabbitMqMessagingService {
    /// RabbitMQ connection
    connection: Connection,
    /// Channel for queue operations
    channel: Channel,
    /// Configuration (from TaskerConfig TOML)
    config: RabbitmqConfig,
    /// Track which queues have been created (for DLX setup)
    created_queues: Arc<RwLock<std::collections::HashSet<String>>>,
    /// Per-queue statistics
    queue_stats: Arc<RwLock<HashMap<String, Arc<QueueStatistics>>>>,
}

impl RabbitMqMessagingService {
    /// Create a new RabbitMQ messaging service from configuration reference
    pub async fn new(config: &RabbitmqConfig) -> Result<Self, MessagingError> {
        Self::from_config(config.clone()).await
    }

    /// Create a new RabbitMQ messaging service from owned configuration
    ///
    /// This is the primary constructor that takes ownership of the config.
    pub async fn from_config(config: RabbitmqConfig) -> Result<Self, MessagingError> {
        let connection = Connection::connect(
            &config.url,
            ConnectionProperties::default()
                .with_connection_name("tasker-messaging".into()),
        )
        .await
        .map_err(|e| MessagingError::connection(format!("RabbitMQ connection failed: {}", e)))?;

        let channel = connection
            .create_channel()
            .await
            .map_err(|e| MessagingError::connection(format!("RabbitMQ channel creation failed: {}", e)))?;

        // Set prefetch for backpressure
        channel
            .basic_qos(config.prefetch_count, lapin::options::BasicQosOptions::default())
            .await
            .map_err(|e| MessagingError::configuration("rabbitmq", format!("Failed to set QoS: {}", e)))?;

        Ok(Self {
            connection,
            channel,
            config,
            created_queues: Arc::new(RwLock::new(std::collections::HashSet::new())),
            queue_stats: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create from TaskerConfig reference
    ///
    /// Extracts the RabbitMQ configuration from TaskerConfig, cloning it.
    /// Returns an error if RabbitMQ is not configured.
    pub async fn from_tasker_config(config: &TaskerConfig) -> Result<Self, MessagingError> {
        let rabbitmq_config = config
            .common
            .queues
            .rabbitmq
            .clone()
            .ok_or_else(|| {
                MessagingError::configuration(
                    "rabbitmq",
                    "RabbitMQ configuration not found in TaskerConfig".to_string(),
                )
            })?;

        Self::from_config(rabbitmq_config).await
    }

    /// Create from environment variables
    ///
    /// Reads from:
    /// - `RABBITMQ_URL` (default: "amqp://tasker:tasker@localhost:5672/%2F")
    /// - `RABBITMQ_PREFETCH_COUNT` (default: 10)
    /// - `RABBITMQ_HEARTBEAT_SECONDS` (default: 60)
    ///
    /// Useful for standalone testing without full TaskerConfig bootstrap.
    pub async fn from_env() -> Result<Self, MessagingError> {
        let config = RabbitmqConfig::builder()
            .url(
                std::env::var("RABBITMQ_URL")
                    .unwrap_or_else(|_| "amqp://tasker:tasker@localhost:5672/%2F".to_string()),
            )
            .prefetch_count(
                std::env::var("RABBITMQ_PREFETCH_COUNT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
            )
            .heartbeat_seconds(
                std::env::var("RABBITMQ_HEARTBEAT_SECONDS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60),
            )
            .connection_timeout_seconds(
                std::env::var("RABBITMQ_CONNECTION_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            )
            .build();

        Self::from_config(config).await
    }

    /// Get the configured prefetch count
    pub fn prefetch_count(&self) -> u16 {
        self.config.prefetch_count
    }

    /// Get connection URL (redacted for logging)
    pub fn connection_url_redacted(&self) -> &str {
        // Return just the scheme portion for logging (hide credentials)
        if self.config.url.contains('@') {
            if let Some(scheme_end) = self.config.url.find("://") {
                return &self.config.url[..scheme_end + 3];
            }
        }
        "amqp://..."
    }

    /// Get or create queue statistics tracker
    async fn get_or_create_stats(&self, queue_name: &str) -> Arc<QueueStatistics> {
        let stats = self.queue_stats.read().await;
        if let Some(s) = stats.get(queue_name) {
            return s.clone();
        }
        drop(stats);

        let mut stats = self.queue_stats.write().await;
        stats
            .entry(queue_name.to_string())
            .or_insert_with(|| Arc::new(QueueStatistics::default()))
            .clone()
    }

    /// Create Dead Letter Exchange and Queue for a queue
    async fn setup_dlx(&self, queue_name: &str) -> Result<(), MessagingError> {
        let dlx_name = format!("{}_dlx", queue_name);
        let dlq_name = format!("{}_dlq", queue_name);

        // Declare DLX (Direct exchange for routing by original queue name)
        self.channel
            .exchange_declare(
                &dlx_name,
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| {
                MessagingError::queue_creation(&dlx_name, format!("DLX creation failed: {}", e))
            })?;

        // Declare DLQ
        self.channel
            .queue_declare(
                &dlq_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| {
                MessagingError::queue_creation(&dlq_name, format!("DLQ creation failed: {}", e))
            })?;

        // Bind DLQ to DLX with routing key = original queue name
        self.channel
            .queue_bind(
                &dlq_name,
                &dlx_name,
                queue_name,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| {
                MessagingError::queue_creation(
                    &dlq_name,
                    format!("DLQ binding failed: {}", e),
                )
            })?;

        Ok(())
    }
}

#[async_trait]
impl MessagingService for RabbitMqMessagingService {
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError> {
        // Check if already created
        {
            let created = self.created_queues.read().await;
            if created.contains(queue_name) {
                return Ok(());
            }
        }

        // Setup DLX first
        self.setup_dlx(queue_name).await?;

        // Declare main queue with DLX configuration
        let dlx_name = format!("{}_dlx", queue_name);
        let mut args = FieldTable::default();
        args.insert(
            "x-dead-letter-exchange".into(),
            AMQPValue::LongString(dlx_name.into()),
        );
        args.insert(
            "x-dead-letter-routing-key".into(),
            AMQPValue::LongString(queue_name.into()),
        );

        self.channel
            .queue_declare(
                queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                args,
            )
            .await
            .map_err(|e| {
                MessagingError::queue_creation(queue_name, format!("Queue creation failed: {}", e))
            })?;

        // Mark as created
        {
            let mut created = self.created_queues.write().await;
            created.insert(queue_name.to_string());
        }

        Ok(())
    }

    async fn verify_queues(
        &self,
        queue_names: &[String],
    ) -> Result<QueueHealthReport, MessagingError> {
        let mut report = QueueHealthReport::new();

        for queue_name in queue_names {
            // Use passive declare to check if queue exists
            match self
                .channel
                .queue_declare(
                    queue_name,
                    QueueDeclareOptions {
                        passive: true, // Check only, don't create
                        ..Default::default()
                    },
                    FieldTable::default(),
                )
                .await
            {
                Ok(_) => report.add_healthy(queue_name),
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("NOT_FOUND") || error_str.contains("404") {
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
        let bytes = message.to_bytes()?;

        // Publish with persistent delivery mode
        let confirm = self
            .channel
            .basic_publish(
                "",         // Default exchange
                queue_name, // Routing key = queue name
                BasicPublishOptions::default(),
                &bytes,
                BasicProperties::default()
                    .with_delivery_mode(2) // Persistent
                    .with_content_type("application/json".into()),
            )
            .await
            .map_err(|e| MessagingError::send(queue_name, format!("Publish failed: {}", e)))?;

        // Wait for confirmation
        confirm
            .await
            .map_err(|e| MessagingError::send(queue_name, format!("Publish confirmation failed: {}", e)))?;

        // Track stats
        let stats = self.get_or_create_stats(queue_name).await;
        stats.total_sent.fetch_add(1, Ordering::Relaxed);

        // RabbitMQ doesn't return a message ID on publish; generate one
        // based on monotonic counter for this session
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let msg_id = COUNTER.fetch_add(1, Ordering::Relaxed);

        Ok(MessageId::from(msg_id))
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        _visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        // Note: RabbitMQ visibility is controlled by prefetch + ack, not a timeout
        // The visibility_timeout is used for compatibility but not directly applied

        let mut messages = Vec::with_capacity(max_messages);

        for _ in 0..max_messages {
            match self
                .channel
                .basic_get(queue_name, BasicGetOptions { no_ack: false })
                .await
            {
                Ok(Some(delivery)) => {
                    let deserialized = T::from_bytes(&delivery.data)?;

                    // Use delivery tag as receipt handle
                    let receipt_handle = ReceiptHandle::from(delivery.delivery.delivery_tag);

                    // RabbitMQ doesn't track receive count natively; we approximate
                    // from redelivered flag (0 or 1)
                    let receive_count = if delivery.delivery.redelivered { 2 } else { 1 };

                    // Use current time as enqueued_at (RabbitMQ basic_get doesn't include timestamp)
                    let enqueued_at = chrono::Utc::now();

                    messages.push(QueuedMessage::new(
                        receipt_handle,
                        deserialized,
                        receive_count,
                        enqueued_at,
                    ));
                }
                Ok(None) => {
                    // No more messages available
                    break;
                }
                Err(e) => {
                    return Err(MessagingError::receive(
                        queue_name,
                        format!("basic_get failed: {}", e),
                    ));
                }
            }
        }

        // Track stats
        if !messages.is_empty() {
            let stats = self.get_or_create_stats(queue_name).await;
            stats
                .total_received
                .fetch_add(messages.len() as u64, Ordering::Relaxed);
        }

        Ok(messages)
    }

    async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError> {
        let delivery_tag: u64 = receipt_handle
            .as_str()
            .parse()
            .map_err(|_| MessagingError::invalid_receipt_handle(receipt_handle.as_str()))?;

        self.channel
            .basic_ack(delivery_tag, BasicAckOptions::default())
            .await
            .map_err(|e| {
                MessagingError::ack(queue_name, delivery_tag as i64, format!("ack failed: {}", e))
            })?;

        // Track stats
        let stats = self.get_or_create_stats(queue_name).await;
        stats.total_acked.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    async fn nack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        requeue: bool,
    ) -> Result<(), MessagingError> {
        let delivery_tag: u64 = receipt_handle
            .as_str()
            .parse()
            .map_err(|_| MessagingError::invalid_receipt_handle(receipt_handle.as_str()))?;

        self.channel
            .basic_nack(
                delivery_tag,
                BasicNackOptions {
                    requeue,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| {
                MessagingError::nack(
                    queue_name,
                    delivery_tag as i64,
                    format!("nack failed: {}", e),
                )
            })?;

        // Track stats
        let stats = self.get_or_create_stats(queue_name).await;
        stats.total_nacked.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    async fn extend_visibility(
        &self,
        _queue_name: &str,
        _receipt_handle: &ReceiptHandle,
        _extension: Duration,
    ) -> Result<(), MessagingError> {
        // RabbitMQ doesn't support visibility timeout extension
        // Log warning and return Ok for compatibility
        tracing::warn!(
            "RabbitMQ does not support visibility timeout extension; \
             message may timeout if processing takes too long"
        );
        Ok(())
    }

    async fn queue_stats(&self, queue_name: &str) -> Result<QueueStats, MessagingError> {
        // Use passive declare to get message count
        let queue_state = self
            .channel
            .queue_declare(
                queue_name,
                QueueDeclareOptions {
                    passive: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| MessagingError::queue_stats(queue_name, format!("Queue query failed: {}", e)))?;

        let our_stats = self.get_or_create_stats(queue_name).await;

        let stats = QueueStats::new(queue_name, queue_state.message_count() as u64)
            .with_counters(
                our_stats.total_sent.load(Ordering::Relaxed),
                our_stats.total_received.load(Ordering::Relaxed),
                our_stats.total_acked.load(Ordering::Relaxed),
                our_stats.total_nacked.load(Ordering::Relaxed),
            );

        // Note: consumer_count available via queue_state.consumer_count()
        // but we don't have a field for it in QueueStats currently

        Ok(stats)
    }

    async fn health_check(&self) -> Result<bool, MessagingError> {
        // Check connection status
        if self.connection.status().connected() {
            Ok(true)
        } else {
            Err(MessagingError::health_check("RabbitMQ connection is not connected"))
        }
    }

    fn provider_name(&self) -> &'static str {
        "rabbitmq"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = RabbitmqConfig::default();
        assert!(config.url.contains("amqp://"));
        // Default prefetch is 100, heartbeat is 30 (per tasker.rs)
        assert_eq!(config.prefetch_count, 100);
        assert_eq!(config.heartbeat_seconds, 30);
    }

    #[test]
    fn test_config_builder() {
        let config = RabbitmqConfig::builder()
            .url("amqp://custom:pass@host:5672/".to_string())
            .prefetch_count(20)
            .build();

        assert!(config.url.contains("custom:pass@host"));
        assert_eq!(config.prefetch_count, 20);
    }

    // Integration tests require RabbitMQ to be running
    // Run with: docker compose -f docker/docker-compose.test.yml up -d rabbitmq
    // Then: cargo test --all-features --package tasker-shared rabbitmq -- --ignored

    #[tokio::test]
    #[ignore = "requires RabbitMQ running"]
    async fn test_rabbitmq_connection() {
        let service = RabbitMqMessagingService::from_env().await;
        assert!(service.is_ok(), "Should connect to RabbitMQ");

        let service = service.unwrap();
        assert_eq!(service.provider_name(), "rabbitmq");
    }

    #[tokio::test]
    #[ignore = "requires RabbitMQ running"]
    async fn test_rabbitmq_health_check() {
        let service = RabbitMqMessagingService::from_env().await.unwrap();

        let health = service.health_check().await;
        assert!(health.is_ok(), "Health check should succeed");
        assert!(health.unwrap(), "Health should be true");
    }

    #[tokio::test]
    #[ignore = "requires RabbitMQ running"]
    async fn test_rabbitmq_ensure_queue() {
        let service = RabbitMqMessagingService::from_env().await.unwrap();

        let queue_name = format!("test_ensure_{}", uuid::Uuid::new_v4());

        // Create queue
        let result = service.ensure_queue(&queue_name).await;
        assert!(result.is_ok(), "Should create queue: {:?}", result.err());

        // Idempotent
        let result = service.ensure_queue(&queue_name).await;
        assert!(result.is_ok(), "Should be idempotent");
    }

    #[tokio::test]
    #[ignore = "requires RabbitMQ running"]
    async fn test_rabbitmq_send_receive_roundtrip() {
        let service = RabbitMqMessagingService::from_env().await.unwrap();

        let queue_name = format!("test_roundtrip_{}", uuid::Uuid::new_v4());
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
    #[ignore = "requires RabbitMQ running"]
    async fn test_rabbitmq_nack_requeue() {
        let service = RabbitMqMessagingService::from_env().await.unwrap();

        let queue_name = format!("test_nack_{}", uuid::Uuid::new_v4());
        service.ensure_queue(&queue_name).await.unwrap();

        // Send message
        let msg = serde_json::json!({"action": "retry_me"});
        service.send_message(&queue_name, &msg).await.unwrap();

        // Receive
        let messages: Vec<QueuedMessage<serde_json::Value>> = service
            .receive_messages(&queue_name, 1, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);

        // Nack with requeue
        service
            .nack_message(&queue_name, &messages[0].receipt_handle, true)
            .await
            .unwrap();

        // Message should be available again
        let messages2: Vec<QueuedMessage<serde_json::Value>> = service
            .receive_messages(&queue_name, 1, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(messages2.len(), 1, "Message should be requeued");
    }

    #[tokio::test]
    #[ignore = "requires RabbitMQ running"]
    async fn test_rabbitmq_queue_stats() {
        let service = RabbitMqMessagingService::from_env().await.unwrap();

        let queue_name = format!("test_stats_{}", uuid::Uuid::new_v4());
        service.ensure_queue(&queue_name).await.unwrap();

        // Send a few messages
        for i in 0..3 {
            let msg = serde_json::json!({"index": i});
            service.send_message(&queue_name, &msg).await.unwrap();
        }

        // Brief delay for RabbitMQ queue state to stabilize
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Check stats
        let stats = service.queue_stats(&queue_name).await.unwrap();
        assert_eq!(stats.queue_name, queue_name);
        assert_eq!(stats.message_count, 3);
        assert_eq!(stats.total_sent, 3);
    }

    #[tokio::test]
    #[ignore = "requires RabbitMQ running"]
    async fn test_rabbitmq_verify_queues() {
        let service = RabbitMqMessagingService::from_env().await.unwrap();

        let existing_queue = format!("test_verify_exists_{}", uuid::Uuid::new_v4());
        let missing_queue = format!("test_verify_missing_{}", uuid::Uuid::new_v4());

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
        // Note: Missing queue detection depends on RabbitMQ error response
    }
}
