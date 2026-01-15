//! # In-Memory Messaging Service
//!
//! Thread-safe in-memory queue implementation for testing and development.
//!
//! ## Features
//!
//! - **Visibility Timeout**: Messages become invisible after receive, re-visible after timeout
//! - **Thread-Safe**: Uses `tokio::sync::RwLock` for concurrent access
//! - **Full MessagingService Implementation**: Complete API compatibility with PGMQ

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::messaging::service::traits::{MessagingService, QueueMessage};
use crate::messaging::service::types::{
    MessageHandle, MessageId, MessageMetadata, QueueHealthReport, QueueStats, QueuedMessage,
    ReceiptHandle,
};
use crate::messaging::MessagingError;

/// In-memory message with visibility tracking
#[derive(Debug, Clone)]
struct InMemoryQueuedMessage {
    /// Unique message ID
    id: u64,
    /// Serialized message payload
    payload: Vec<u8>,
    /// When the message was enqueued
    enqueued_at: DateTime<Utc>,
    /// When the message becomes visible again (None = visible now)
    visible_at: Option<DateTime<Utc>>,
    /// Number of times this message has been received
    receive_count: u32,
}

/// In-memory queue with message storage
#[derive(Debug, Default)]
struct InMemoryQueue {
    /// Messages in the queue (FIFO order)
    messages: VecDeque<InMemoryQueuedMessage>,
    /// Next message ID
    next_id: AtomicU64,
    /// Total messages sent to this queue
    total_sent: AtomicU64,
    /// Total messages received from this queue
    total_received: AtomicU64,
    /// Total messages acknowledged
    total_acked: AtomicU64,
    /// Total messages nacked
    total_nacked: AtomicU64,
}

impl InMemoryQueue {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            next_id: AtomicU64::new(1),
            total_sent: AtomicU64::new(0),
            total_received: AtomicU64::new(0),
            total_acked: AtomicU64::new(0),
            total_nacked: AtomicU64::new(0),
        }
    }
}

/// In-memory messaging service for testing
///
/// Provides a complete `MessagingService` implementation using in-memory data structures.
/// Messages are stored in `VecDeque` per queue with visibility timeout simulation.
///
/// # Example
///
/// ```rust
/// use tasker_shared::messaging::service::providers::InMemoryMessagingService;
/// use tasker_shared::messaging::service::MessagingService;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let service = InMemoryMessagingService::new();
///
/// // Create a queue
/// service.ensure_queue("test_queue").await?;
///
/// // Send a message
/// let msg_id = service.send_message("test_queue", &serde_json::json!({"key": "value"})).await?;
///
/// // Receive messages
/// let messages = service.receive_messages::<serde_json::Value>(
///     "test_queue",
///     10,
///     Duration::from_secs(30),
/// ).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct InMemoryMessagingService {
    /// Queue storage (queue_name -> queue)
    queues: RwLock<HashMap<String, InMemoryQueue>>,
}

impl Default for InMemoryMessagingService {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryMessagingService {
    /// Create a new in-memory messaging service
    pub fn new() -> Self {
        Self {
            queues: RwLock::new(HashMap::new()),
        }
    }

    /// Create with pre-initialized queues
    pub fn with_queues(queue_names: &[&str]) -> Self {
        let mut queues = HashMap::new();
        for name in queue_names {
            queues.insert(name.to_string(), InMemoryQueue::new());
        }
        Self {
            queues: RwLock::new(queues),
        }
    }

    /// Get the number of messages in a queue (for testing)
    pub async fn queue_length(&self, queue_name: &str) -> usize {
        let queues = self.queues.read().await;
        queues
            .get(queue_name)
            .map(|q| q.messages.len())
            .unwrap_or(0)
    }

    /// Clear all messages from a queue (for testing)
    pub async fn clear_queue(&self, queue_name: &str) {
        let mut queues = self.queues.write().await;
        if let Some(queue) = queues.get_mut(queue_name) {
            queue.messages.clear();
        }
    }

    /// Clear all queues (for testing)
    pub async fn clear_all(&self) {
        let mut queues = self.queues.write().await;
        queues.clear();
    }
}

#[async_trait]
impl MessagingService for InMemoryMessagingService {
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError> {
        let mut queues = self.queues.write().await;
        queues
            .entry(queue_name.to_string())
            .or_insert_with(InMemoryQueue::new);
        Ok(())
    }

    async fn verify_queues(
        &self,
        queue_names: &[String],
    ) -> Result<QueueHealthReport, MessagingError> {
        let queues = self.queues.read().await;
        let mut report = QueueHealthReport::new();

        for name in queue_names {
            if queues.contains_key(name) {
                report.add_healthy(name);
            } else {
                report.add_missing(name);
            }
        }

        Ok(report)
    }

    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<MessageId, MessagingError> {
        let payload = message.to_bytes()?;

        let mut queues = self.queues.write().await;
        let queue = queues
            .get_mut(queue_name)
            .ok_or_else(|| MessagingError::queue_not_found(queue_name))?;

        let id = queue.next_id.fetch_add(1, Ordering::Relaxed);
        queue.total_sent.fetch_add(1, Ordering::Relaxed);

        let msg = InMemoryQueuedMessage {
            id,
            payload,
            enqueued_at: Utc::now(),
            visible_at: None,
            receive_count: 0,
        };

        queue.messages.push_back(msg);

        Ok(MessageId::from(id))
    }

    async fn send_batch<T: QueueMessage>(
        &self,
        queue_name: &str,
        messages: &[T],
    ) -> Result<Vec<MessageId>, MessagingError> {
        let mut ids = Vec::with_capacity(messages.len());

        // Serialize all messages first (outside the lock)
        let payloads: Vec<Vec<u8>> = messages
            .iter()
            .map(|m| m.to_bytes())
            .collect::<Result<_, _>>()?;

        let mut queues = self.queues.write().await;
        let queue = queues
            .get_mut(queue_name)
            .ok_or_else(|| MessagingError::queue_not_found(queue_name))?;

        let now = Utc::now();
        for payload in payloads {
            let id = queue.next_id.fetch_add(1, Ordering::Relaxed);
            queue.total_sent.fetch_add(1, Ordering::Relaxed);

            let msg = InMemoryQueuedMessage {
                id,
                payload,
                enqueued_at: now,
                visible_at: None,
                receive_count: 0,
            };

            queue.messages.push_back(msg);
            ids.push(MessageId::from(id));
        }

        Ok(ids)
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        let mut queues = self.queues.write().await;
        let queue = queues
            .get_mut(queue_name)
            .ok_or_else(|| MessagingError::queue_not_found(queue_name))?;

        let now = Utc::now();
        let visible_until = now + chrono::Duration::from_std(visibility_timeout).unwrap();
        let mut received = Vec::new();

        for msg in queue.messages.iter_mut() {
            if received.len() >= max_messages {
                break;
            }

            // Check if message is visible
            let is_visible = msg.visible_at.map(|vt| vt <= now).unwrap_or(true);

            if is_visible {
                // Deserialize the message
                let deserialized = T::from_bytes(&msg.payload)?;

                // Update visibility and receive count
                msg.visible_at = Some(visible_until);
                msg.receive_count += 1;
                queue.total_received.fetch_add(1, Ordering::Relaxed);

                // TAS-133: Use explicit MessageHandle::InMemory for provider-agnostic message wrapper
                received.push(QueuedMessage::with_handle(
                    deserialized,
                    MessageHandle::InMemory {
                        id: msg.id,
                        queue_name: queue_name.to_string(),
                    },
                    MessageMetadata::new(msg.receive_count, msg.enqueued_at),
                ));
            }
        }

        Ok(received)
    }

    async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError> {
        let message_id: u64 = receipt_handle
            .as_str()
            .parse()
            .map_err(|_| MessagingError::invalid_receipt_handle(receipt_handle.as_str()))?;

        let mut queues = self.queues.write().await;
        let queue = queues
            .get_mut(queue_name)
            .ok_or_else(|| MessagingError::queue_not_found(queue_name))?;

        // Find and remove the message
        if let Some(pos) = queue.messages.iter().position(|m| m.id == message_id) {
            queue.messages.remove(pos);
            queue.total_acked.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            Err(MessagingError::message_not_found(message_id.to_string()))
        }
    }

    async fn nack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        requeue: bool,
    ) -> Result<(), MessagingError> {
        let message_id: u64 = receipt_handle
            .as_str()
            .parse()
            .map_err(|_| MessagingError::invalid_receipt_handle(receipt_handle.as_str()))?;

        let mut queues = self.queues.write().await;
        let queue = queues
            .get_mut(queue_name)
            .ok_or_else(|| MessagingError::queue_not_found(queue_name))?;

        if requeue {
            // Make message visible again immediately
            if let Some(msg) = queue.messages.iter_mut().find(|m| m.id == message_id) {
                msg.visible_at = None;
                queue.total_nacked.fetch_add(1, Ordering::Relaxed);
                Ok(())
            } else {
                Err(MessagingError::message_not_found(message_id.to_string()))
            }
        } else {
            // Remove the message (dead-letter)
            if let Some(pos) = queue.messages.iter().position(|m| m.id == message_id) {
                queue.messages.remove(pos);
                queue.total_nacked.fetch_add(1, Ordering::Relaxed);
                Ok(())
            } else {
                Err(MessagingError::message_not_found(message_id.to_string()))
            }
        }
    }

    async fn extend_visibility(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        extension: Duration,
    ) -> Result<(), MessagingError> {
        let message_id: u64 = receipt_handle
            .as_str()
            .parse()
            .map_err(|_| MessagingError::invalid_receipt_handle(receipt_handle.as_str()))?;

        let mut queues = self.queues.write().await;
        let queue = queues
            .get_mut(queue_name)
            .ok_or_else(|| MessagingError::queue_not_found(queue_name))?;

        if let Some(msg) = queue.messages.iter_mut().find(|m| m.id == message_id) {
            let extension_chrono = chrono::Duration::from_std(extension).unwrap();
            msg.visible_at = Some(msg.visible_at.unwrap_or_else(Utc::now) + extension_chrono);
            Ok(())
        } else {
            Err(MessagingError::message_not_found(message_id.to_string()))
        }
    }

    async fn queue_stats(&self, queue_name: &str) -> Result<QueueStats, MessagingError> {
        let queues = self.queues.read().await;
        let queue = queues
            .get(queue_name)
            .ok_or_else(|| MessagingError::queue_not_found(queue_name))?;

        let now = Utc::now();

        // Count in-flight messages
        let in_flight_count = queue
            .messages
            .iter()
            .filter(|m| m.visible_at.map(|vt| vt > now).unwrap_or(false))
            .count() as u64;

        // Get oldest message age
        let oldest_age_ms = queue.messages.front().map(|m| {
            let age = now - m.enqueued_at;
            age.num_milliseconds() as u64
        });

        let mut stats = QueueStats::new(queue_name, queue.messages.len() as u64)
            .with_in_flight_count(in_flight_count)
            .with_counters(
                queue.total_sent.load(Ordering::Relaxed),
                queue.total_received.load(Ordering::Relaxed),
                queue.total_acked.load(Ordering::Relaxed),
                queue.total_nacked.load(Ordering::Relaxed),
            );

        if let Some(age_ms) = oldest_age_ms {
            stats = stats.with_oldest_message_age_ms(age_ms);
        }

        // Note: visible_count is not directly stored in QueueStats but is useful info
        // The message_count includes both visible and in-flight

        Ok(stats)
    }

    async fn health_check(&self) -> Result<bool, MessagingError> {
        // In-memory service is always healthy
        Ok(true)
    }

    fn provider_name(&self) -> &'static str {
        "in_memory"
    }
}

// =============================================================================
// SupportsPushNotifications implementation (TAS-133)
// =============================================================================

use std::pin::Pin;
use futures::Stream;
use crate::messaging::service::traits::SupportsPushNotifications;
use crate::messaging::service::types::MessageNotification;

impl SupportsPushNotifications for InMemoryMessagingService {
    /// Subscribe to push notifications for a queue
    ///
    /// TAS-133: Returns an empty stream for in-memory testing. In real usage,
    /// tests should use polling rather than relying on push notifications.
    /// This implementation returns `MessageNotification::Available` style
    /// notifications (signal only, requires fetch) similar to PGMQ.
    fn subscribe(
        &self,
        queue_name: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError> {
        // For in-memory testing, return an empty stream.
        // Real tests should use polling-based receive_messages().
        // This satisfies the trait requirement without needing broadcast channels.
        let _queue_name = queue_name.to_string();
        let empty_stream = futures::stream::empty();
        Ok(Box::pin(empty_stream))
    }

    /// In-memory provider uses signal-only notifications (like PGMQ)
    ///
    /// Returns `true` because tests should use fallback polling rather than
    /// relying on push notifications from the in-memory provider.
    fn requires_fallback_polling(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestMessage {
        id: u32,
        content: String,
    }

    #[tokio::test]
    async fn test_ensure_queue() {
        let service = InMemoryMessagingService::new();

        service.ensure_queue("test_queue").await.unwrap();

        let report = service
            .verify_queues(&["test_queue".to_string()])
            .await
            .unwrap();
        assert!(report.is_healthy());
        assert_eq!(report.healthy.len(), 1);
    }

    #[tokio::test]
    async fn test_send_and_receive() {
        let service = InMemoryMessagingService::new();
        service.ensure_queue("test_queue").await.unwrap();

        let msg = TestMessage {
            id: 1,
            content: "Hello".to_string(),
        };

        let msg_id = service.send_message("test_queue", &msg).await.unwrap();
        assert_eq!(msg_id.as_str(), "1");

        let received: Vec<QueuedMessage<TestMessage>> = service
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();

        assert_eq!(received.len(), 1);
        assert_eq!(received[0].message, msg);
        assert_eq!(received[0].receive_count(), 1);
    }

    #[tokio::test]
    async fn test_visibility_timeout() {
        let service = InMemoryMessagingService::new();
        service.ensure_queue("test_queue").await.unwrap();

        let msg = TestMessage {
            id: 1,
            content: "Test".to_string(),
        };
        service.send_message("test_queue", &msg).await.unwrap();

        // First receive should get the message
        let received1: Vec<QueuedMessage<TestMessage>> = service
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(received1.len(), 1);

        // Immediate second receive should get nothing (message is invisible)
        let received2: Vec<QueuedMessage<TestMessage>> = service
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(received2.len(), 0);
    }

    #[tokio::test]
    async fn test_ack_message() {
        let service = InMemoryMessagingService::new();
        service.ensure_queue("test_queue").await.unwrap();

        let msg = TestMessage {
            id: 1,
            content: "Test".to_string(),
        };
        service.send_message("test_queue", &msg).await.unwrap();

        let received: Vec<QueuedMessage<TestMessage>> = service
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(received.len(), 1);

        // Ack the message
        service
            .ack_message("test_queue", &received[0].receipt_handle)
            .await
            .unwrap();

        // Queue should be empty
        assert_eq!(service.queue_length("test_queue").await, 0);
    }

    #[tokio::test]
    async fn test_nack_requeue() {
        let service = InMemoryMessagingService::new();
        service.ensure_queue("test_queue").await.unwrap();

        let msg = TestMessage {
            id: 1,
            content: "Test".to_string(),
        };
        service.send_message("test_queue", &msg).await.unwrap();

        let received: Vec<QueuedMessage<TestMessage>> = service
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();

        // Nack with requeue
        service
            .nack_message("test_queue", &received[0].receipt_handle, true)
            .await
            .unwrap();

        // Message should be visible again
        let received2: Vec<QueuedMessage<TestMessage>> = service
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(received2.len(), 1);
        assert_eq!(received2[0].receive_count(), 2); // Second receive
    }

    #[tokio::test]
    async fn test_send_batch() {
        let service = InMemoryMessagingService::new();
        service.ensure_queue("test_queue").await.unwrap();

        let messages = vec![
            TestMessage {
                id: 1,
                content: "One".to_string(),
            },
            TestMessage {
                id: 2,
                content: "Two".to_string(),
            },
            TestMessage {
                id: 3,
                content: "Three".to_string(),
            },
        ];

        let ids = service
            .send_batch("test_queue", &messages)
            .await
            .unwrap();
        assert_eq!(ids.len(), 3);

        let received: Vec<QueuedMessage<TestMessage>> = service
            .receive_messages("test_queue", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(received.len(), 3);
    }

    #[tokio::test]
    async fn test_queue_stats() {
        let service = InMemoryMessagingService::new();
        service.ensure_queue("test_queue").await.unwrap();

        // Send some messages
        for i in 0..5 {
            let msg = TestMessage {
                id: i,
                content: format!("Message {}", i),
            };
            service.send_message("test_queue", &msg).await.unwrap();
        }

        let stats = service.queue_stats("test_queue").await.unwrap();
        assert_eq!(stats.queue_name, "test_queue");
        assert_eq!(stats.message_count, 5);
        assert_eq!(stats.total_sent, 5);
    }

    #[tokio::test]
    async fn test_queue_not_found() {
        let service = InMemoryMessagingService::new();

        let msg = TestMessage {
            id: 1,
            content: "Test".to_string(),
        };

        let result = service.send_message("nonexistent", &msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_health_check() {
        let service = InMemoryMessagingService::new();
        assert!(service.health_check().await.unwrap());
    }
}
