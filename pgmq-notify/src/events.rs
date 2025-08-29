//! Event types for PGMQ notifications

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Union of all possible PGMQ notification events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum PgmqNotifyEvent {
    /// Queue was created
    QueueCreated(QueueCreatedEvent),
    /// Message is ready for processing in a queue
    MessageReady(MessageReadyEvent),
}

/// Event emitted when a new PGMQ queue is created
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
    pub fn namespace(&self) -> &str {
        match self {
            PgmqNotifyEvent::QueueCreated(event) => &event.namespace,
            PgmqNotifyEvent::MessageReady(event) => &event.namespace,
        }
    }

    /// Get the queue name for any event type
    pub fn queue_name(&self) -> &str {
        match self {
            PgmqNotifyEvent::QueueCreated(event) => &event.queue_name,
            PgmqNotifyEvent::MessageReady(event) => &event.queue_name,
        }
    }

    /// Get the timestamp for any event type
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            PgmqNotifyEvent::QueueCreated(event) => event.created_at,
            PgmqNotifyEvent::MessageReady(event) => event.ready_at,
        }
    }

    /// Get metadata for any event type
    pub fn metadata(&self) -> &HashMap<String, String> {
        match self {
            PgmqNotifyEvent::QueueCreated(event) => &event.metadata,
            PgmqNotifyEvent::MessageReady(event) => &event.metadata,
        }
    }

    /// Check if event matches a specific namespace
    pub fn matches_namespace(&self, namespace: &str) -> bool {
        self.namespace() == namespace
    }

    /// Get the event type as a string
    pub fn event_type(&self) -> &'static str {
        match self {
            PgmqNotifyEvent::QueueCreated(_) => "queue_created",
            PgmqNotifyEvent::MessageReady(_) => "message_ready",
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
    pub fn with_visibility_timeout(mut self, timeout_seconds: i32) -> Self {
        self.visibility_timeout_seconds = Some(timeout_seconds);
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
