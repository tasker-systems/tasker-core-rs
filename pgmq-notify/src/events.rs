//! # Event types for PGMQ notifications
//!
//! This module defines the event types that are emitted by PGMQ operations
//! and can be listened to for real-time processing. Events are designed to be
//! lightweight and contain only essential information to minimize notification overhead.
//!
//! ## Event Types
//!
//! - [`QueueCreatedEvent`] - Emitted when a new queue is created
//! - [`MessageReadyEvent`] - Emitted when a message is enqueued and ready for processing
//!
//! ## Usage
//!
//! ```rust
//! use pgmq_notify::events::{PgmqNotifyEvent, MessageReadyEvent};
//! use chrono::Utc;
//!
//! // Create a message ready event
//! let event = PgmqNotifyEvent::MessageReady(MessageReadyEvent {
//!     queue_name: "tasks_queue".to_string(),
//!     namespace: "tasks".to_string(),
//!     msg_id: 12345,
//!     ready_at: Utc::now(),
//!     metadata: Default::default(),
//!     visibility_timeout_seconds: Some(30),
//! });
//!
//! // Serialize to JSON for notification
//! let json = serde_json::to_string(&event).unwrap();
//! assert!(json.contains("message_ready"));
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Union of all possible PGMQ notification events
///
/// This enum represents all event types that can be emitted by PGMQ operations.
/// Events are tagged for JSON serialization and can be pattern-matched for handling.
///
/// # Examples
///
/// ```rust
/// use pgmq_notify::events::{PgmqNotifyEvent, QueueCreatedEvent};
/// use chrono::Utc;
///
/// let event = PgmqNotifyEvent::QueueCreated(QueueCreatedEvent {
///     queue_name: "new_queue".to_string(),
///     namespace: "default".to_string(),
///     created_at: Utc::now(),
///     metadata: Default::default(),
/// });
///
/// match event {
///     PgmqNotifyEvent::QueueCreated(e) => {
///         println!("Queue created: {}", e.queue_name);
///     }
///     PgmqNotifyEvent::MessageReady(e) => {
///         println!("Message ready: {}", e.msg_id);
///     }
///     PgmqNotifyEvent::BatchReady(e) => {
///         println!("Batch ready: {} messages", e.message_count);
///     }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum PgmqNotifyEvent {
    /// Queue was created
    QueueCreated(QueueCreatedEvent),
    /// Message is ready for processing in a queue
    MessageReady(MessageReadyEvent),
    /// Batch of messages are ready for processing in a queue
    BatchReady(BatchReadyEvent),
}

/// Event emitted when a new PGMQ queue is created
///
/// This event is triggered when a new queue is created in the PGMQ system.
/// It includes the queue name, extracted namespace, and creation timestamp.
///
/// # Examples
///
/// ```rust
/// use pgmq_notify::events::QueueCreatedEvent;
/// use chrono::Utc;
/// use std::collections::HashMap;
///
/// let event = QueueCreatedEvent {
///     queue_name: "orders_queue".to_string(),
///     namespace: "orders".to_string(),
///     created_at: Utc::now(),
///     metadata: HashMap::new(),
/// };
///
/// assert_eq!(event.queue_name, "orders_queue");
/// assert_eq!(event.namespace, "orders");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueueCreatedEvent {
    /// Name of the queue that was created
    pub queue_name: String,
    /// Extracted namespace from the queue name
    pub namespace: String,
    /// When the queue was created
    pub created_at: DateTime<Utc>,
    /// Optional metadata about the queue
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Event emitted when a message is ready for processing
///
/// This event is triggered when a message is enqueued in PGMQ and becomes
/// available for processing by workers. It provides the message ID and queue
/// information needed to claim and process the message.
///
/// # Examples
///
/// ```rust
/// use pgmq_notify::events::MessageReadyEvent;
/// use chrono::Utc;
/// use std::collections::HashMap;
///
/// let event = MessageReadyEvent {
///     msg_id: 42,
///     queue_name: "tasks_queue".to_string(),
///     namespace: "tasks".to_string(),
///     ready_at: Utc::now(),
///     metadata: HashMap::new(),
///     visibility_timeout_seconds: Some(30),
/// };
///
/// assert_eq!(event.msg_id, 42);
/// assert_eq!(event.queue_name, "tasks_queue");
/// assert_eq!(event.namespace, "tasks");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageReadyEvent {
    /// ID of the message that's ready
    pub msg_id: i64,
    /// Queue where the message is available
    pub queue_name: String,
    /// Extracted namespace from the queue name
    pub namespace: String,
    /// When the message became ready
    pub ready_at: DateTime<Utc>,
    /// Optional message metadata (limited by payload size)
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Visibility timeout if applicable
    pub visibility_timeout_seconds: Option<i32>,
}

impl PgmqNotifyEvent {
    /// Get the namespace for any event type
    #[must_use]
    pub fn namespace(&self) -> &str {
        match self {
            PgmqNotifyEvent::QueueCreated(event) => &event.namespace,
            PgmqNotifyEvent::MessageReady(event) => &event.namespace,
            PgmqNotifyEvent::BatchReady(event) => &event.namespace,
        }
    }

    /// Get the queue name for any event type
    #[must_use]
    pub fn queue_name(&self) -> &str {
        match self {
            PgmqNotifyEvent::QueueCreated(event) => &event.queue_name,
            PgmqNotifyEvent::MessageReady(event) => &event.queue_name,
            PgmqNotifyEvent::BatchReady(event) => &event.queue_name,
        }
    }

    /// Get the timestamp for any event type
    #[must_use]
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            PgmqNotifyEvent::QueueCreated(event) => event.created_at,
            PgmqNotifyEvent::MessageReady(event) => event.ready_at,
            PgmqNotifyEvent::BatchReady(event) => event.ready_at,
        }
    }

    /// Get metadata for any event type
    #[must_use]
    pub fn metadata(&self) -> &HashMap<String, String> {
        match self {
            PgmqNotifyEvent::QueueCreated(event) => &event.metadata,
            PgmqNotifyEvent::MessageReady(event) => &event.metadata,
            PgmqNotifyEvent::BatchReady(event) => &event.metadata,
        }
    }

    /// Check if event matches a specific namespace
    #[must_use]
    pub fn matches_namespace(&self, namespace: &str) -> bool {
        self.namespace() == namespace
    }

    /// Get the event type as a string
    #[must_use]
    pub fn event_type(&self) -> &'static str {
        match self {
            PgmqNotifyEvent::QueueCreated(_) => "queue_created",
            PgmqNotifyEvent::MessageReady(_) => "message_ready",
            PgmqNotifyEvent::BatchReady(_) => "batch_ready",
        }
    }
}

impl QueueCreatedEvent {
    /// Create a new queue created event
    pub fn new<S: Into<String>>(queue_name: S, namespace: S) -> Self {
        Self {
            queue_name: queue_name.into(),
            namespace: namespace.into(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create with custom timestamp
    pub fn with_timestamp<S: Into<String>>(
        queue_name: S,
        namespace: S,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            queue_name: queue_name.into(),
            namespace: namespace.into(),
            created_at,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the event
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Add a single metadata entry
    pub fn add_metadata<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

impl MessageReadyEvent {
    /// Create a new message ready event
    pub fn new<S: Into<String>>(msg_id: i64, queue_name: S, namespace: S) -> Self {
        Self {
            msg_id,
            queue_name: queue_name.into(),
            namespace: namespace.into(),
            ready_at: Utc::now(),
            metadata: HashMap::new(),
            visibility_timeout_seconds: None,
        }
    }

    /// Create with custom timestamp
    pub fn with_timestamp<S: Into<String>>(
        msg_id: i64,
        queue_name: S,
        namespace: S,
        ready_at: DateTime<Utc>,
    ) -> Self {
        Self {
            msg_id,
            queue_name: queue_name.into(),
            namespace: namespace.into(),
            ready_at,
            metadata: HashMap::new(),
            visibility_timeout_seconds: None,
        }
    }

    /// Add metadata to the event
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Add a single metadata entry
    pub fn add_metadata<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set visibility timeout
    #[must_use]
    pub fn with_visibility_timeout(mut self, timeout_seconds: i32) -> Self {
        self.visibility_timeout_seconds = Some(timeout_seconds);
        self
    }
}

/// Event emitted when a batch of messages is ready for processing
///
/// This event is triggered when multiple messages are enqueued in PGMQ via batch
/// operations and become available for processing. It provides the message IDs and
/// queue information needed to claim and process the batch.
///
/// # Examples
///
/// ```rust
/// use pgmq_notify::events::BatchReadyEvent;
/// use chrono::Utc;
/// use std::collections::HashMap;
///
/// let event = BatchReadyEvent {
///     msg_ids: vec![1, 2, 3],
///     queue_name: "tasks_queue".to_string(),
///     namespace: "tasks".to_string(),
///     message_count: 3,
///     ready_at: Utc::now(),
///     metadata: HashMap::new(),
///     delay_seconds: 0,
/// };
///
/// assert_eq!(event.msg_ids, vec![1, 2, 3]);
/// assert_eq!(event.message_count, 3);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchReadyEvent {
    /// IDs of the messages in the batch
    pub msg_ids: Vec<i64>,
    /// Queue where the messages are available
    pub queue_name: String,
    /// Extracted namespace from the queue name
    pub namespace: String,
    /// Number of messages in the batch
    pub message_count: i64,
    /// When the batch became ready
    pub ready_at: DateTime<Utc>,
    /// Optional message metadata (limited by payload size)
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Delay in seconds before messages become visible
    pub delay_seconds: i32,
}

impl BatchReadyEvent {
    /// Create a new batch ready event
    pub fn new<S: Into<String>>(msg_ids: Vec<i64>, queue_name: S, namespace: S) -> Self {
        let message_count = msg_ids.len() as i64;
        Self {
            msg_ids,
            queue_name: queue_name.into(),
            namespace: namespace.into(),
            message_count,
            ready_at: Utc::now(),
            metadata: HashMap::new(),
            delay_seconds: 0,
        }
    }

    /// Create with custom timestamp
    pub fn with_timestamp<S: Into<String>>(
        msg_ids: Vec<i64>,
        queue_name: S,
        namespace: S,
        ready_at: DateTime<Utc>,
    ) -> Self {
        let message_count = msg_ids.len() as i64;
        Self {
            msg_ids,
            queue_name: queue_name.into(),
            namespace: namespace.into(),
            message_count,
            ready_at,
            metadata: HashMap::new(),
            delay_seconds: 0,
        }
    }

    /// Add metadata to the event
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Add a single metadata entry
    pub fn add_metadata<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set delay seconds
    #[must_use]
    pub fn with_delay_seconds(mut self, delay_seconds: i32) -> Self {
        self.delay_seconds = delay_seconds;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_created_event() {
        let event = QueueCreatedEvent::new("orders_queue", "orders");
        assert_eq!(event.queue_name, "orders_queue");
        assert_eq!(event.namespace, "orders");
        assert!(event.metadata.is_empty());
    }

    #[test]
    fn test_message_ready_event() {
        let event = MessageReadyEvent::new(123, "inventory_queue", "inventory");
        assert_eq!(event.msg_id, 123);
        assert_eq!(event.queue_name, "inventory_queue");
        assert_eq!(event.namespace, "inventory");
        assert!(event.metadata.is_empty());
        assert_eq!(event.visibility_timeout_seconds, None);
    }

    #[test]
    fn test_event_common_methods() {
        let queue_event =
            PgmqNotifyEvent::QueueCreated(QueueCreatedEvent::new("test_queue", "test"));
        let message_event =
            PgmqNotifyEvent::MessageReady(MessageReadyEvent::new(456, "test_queue", "test"));

        assert_eq!(queue_event.namespace(), "test");
        assert_eq!(queue_event.queue_name(), "test_queue");
        assert_eq!(queue_event.event_type(), "queue_created");
        assert!(queue_event.matches_namespace("test"));
        assert!(!queue_event.matches_namespace("other"));

        assert_eq!(message_event.namespace(), "test");
        assert_eq!(message_event.queue_name(), "test_queue");
        assert_eq!(message_event.event_type(), "message_ready");
        assert!(message_event.matches_namespace("test"));
    }

    #[test]
    fn test_event_serialization() {
        let event = PgmqNotifyEvent::QueueCreated(
            QueueCreatedEvent::new("orders_queue", "orders").add_metadata("created_by", "system"),
        );

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: PgmqNotifyEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_metadata_builder() {
        let mut metadata = HashMap::new();
        metadata.insert("key1".to_string(), "value1".to_string());
        metadata.insert("key2".to_string(), "value2".to_string());

        let event = QueueCreatedEvent::new("test_queue", "test")
            .with_metadata(metadata)
            .add_metadata("single_key", "single_value");

        assert_eq!(
            event.metadata.get("single_key"),
            Some(&"single_value".to_string())
        );
        assert_eq!(event.metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(event.metadata.get("key2"), Some(&"value2".to_string()));
    }
}
