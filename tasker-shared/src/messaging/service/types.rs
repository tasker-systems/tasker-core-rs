//! # Messaging Service Types
//!
//! Core types for the provider-agnostic messaging abstraction.

use std::time::Duration;

/// Unique identifier for a queued message
///
/// The format is provider-specific:
/// - PGMQ: i64 message ID as string
/// - RabbitMQ: delivery tag as string
/// - InMemory: UUID as string
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

impl MessageId {
    /// Create a new message ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for MessageId {
    fn from(id: i64) -> Self {
        Self(id.to_string())
    }
}

impl From<u64> for MessageId {
    fn from(id: u64) -> Self {
        Self(id.to_string())
    }
}

impl From<String> for MessageId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for MessageId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// Handle for acknowledging/extending a received message
///
/// The format is provider-specific:
/// - PGMQ: message_id as string (same as MessageId)
/// - RabbitMQ: delivery_tag as string
/// - InMemory: internal UUID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReceiptHandle(pub String);

impl ReceiptHandle {
    /// Create a new receipt handle
    pub fn new(handle: impl Into<String>) -> Self {
        Self(handle.into())
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Try to parse the receipt handle as an i64 (for PGMQ compatibility)
    pub fn as_i64(&self) -> Option<i64> {
        self.0.parse().ok()
    }
}

impl std::fmt::Display for ReceiptHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for ReceiptHandle {
    fn from(id: i64) -> Self {
        Self(id.to_string())
    }
}

impl From<u64> for ReceiptHandle {
    fn from(id: u64) -> Self {
        Self(id.to_string())
    }
}

impl From<String> for ReceiptHandle {
    fn from(handle: String) -> Self {
        Self(handle)
    }
}

impl From<&str> for ReceiptHandle {
    fn from(handle: &str) -> Self {
        Self(handle.to_string())
    }
}

/// A message received from a queue with metadata
#[derive(Debug, Clone)]
pub struct QueuedMessage<T> {
    /// Handle for acknowledging this message
    pub receipt_handle: ReceiptHandle,

    /// The deserialized message payload
    pub message: T,

    /// Number of times this message has been received
    ///
    /// Increments each time the message becomes visible after visibility timeout.
    /// Useful for implementing retry limits and DLQ logic.
    pub receive_count: u32,

    /// When the message was originally enqueued
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}

impl<T> QueuedMessage<T> {
    /// Create a new queued message
    pub fn new(
        receipt_handle: ReceiptHandle,
        message: T,
        receive_count: u32,
        enqueued_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            receipt_handle,
            message,
            receive_count,
            enqueued_at,
        }
    }

    /// Map the message to a different type
    pub fn map<U, F>(self, f: F) -> QueuedMessage<U>
    where
        F: FnOnce(T) -> U,
    {
        QueuedMessage {
            receipt_handle: self.receipt_handle,
            message: f(self.message),
            receive_count: self.receive_count,
            enqueued_at: self.enqueued_at,
        }
    }
}

/// Queue statistics for monitoring and backpressure decisions
#[derive(Debug, Clone)]
pub struct QueueStats {
    /// Queue name
    pub queue_name: String,

    /// Total number of messages in the queue (visible + invisible)
    pub message_count: u64,

    /// Number of messages currently being processed (invisible)
    ///
    /// Only available for providers that track this (PGMQ does, RabbitMQ doesn't directly).
    pub in_flight_count: Option<u64>,

    /// Age of the oldest message in the queue
    ///
    /// Useful for detecting stuck queues or processing delays.
    pub oldest_message_age: Option<Duration>,
}

impl QueueStats {
    /// Create new queue stats
    pub fn new(queue_name: impl Into<String>, message_count: u64) -> Self {
        Self {
            queue_name: queue_name.into(),
            message_count,
            in_flight_count: None,
            oldest_message_age: None,
        }
    }

    /// Set the in-flight count
    pub fn with_in_flight_count(mut self, count: u64) -> Self {
        self.in_flight_count = Some(count);
        self
    }

    /// Set the oldest message age
    pub fn with_oldest_message_age(mut self, age: Duration) -> Self {
        self.oldest_message_age = Some(age);
        self
    }
}

/// Health check result for queue verification
#[derive(Debug, Clone, Default)]
pub struct QueueHealthReport {
    /// Queues that exist and are accessible
    pub healthy: Vec<String>,

    /// Queues that don't exist
    pub missing: Vec<String>,

    /// Queues that exist but had errors during verification
    pub errors: Vec<(String, String)>,
}

impl QueueHealthReport {
    /// Create a new empty health report
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if all queues are healthy (none missing or errored)
    pub fn is_healthy(&self) -> bool {
        self.missing.is_empty() && self.errors.is_empty()
    }

    /// Add a healthy queue
    pub fn add_healthy(&mut self, queue_name: impl Into<String>) {
        self.healthy.push(queue_name.into());
    }

    /// Add a missing queue
    pub fn add_missing(&mut self, queue_name: impl Into<String>) {
        self.missing.push(queue_name.into());
    }

    /// Add a queue with an error
    pub fn add_error(&mut self, queue_name: impl Into<String>, error: impl Into<String>) {
        self.errors.push((queue_name.into(), error.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_conversions() {
        let id_from_i64 = MessageId::from(123_i64);
        assert_eq!(id_from_i64.as_str(), "123");

        let id_from_string = MessageId::from("abc-123".to_string());
        assert_eq!(id_from_string.as_str(), "abc-123");
    }

    #[test]
    fn test_receipt_handle_as_i64() {
        let handle = ReceiptHandle::from(456_i64);
        assert_eq!(handle.as_i64(), Some(456));

        let non_numeric = ReceiptHandle::from("not-a-number");
        assert_eq!(non_numeric.as_i64(), None);
    }

    #[test]
    fn test_queued_message_map() {
        let msg = QueuedMessage::new(
            ReceiptHandle::from("handle"),
            42_i32,
            1,
            chrono::Utc::now(),
        );

        let mapped = msg.map(|n| n.to_string());
        assert_eq!(mapped.message, "42");
        assert_eq!(mapped.receive_count, 1);
    }

    #[test]
    fn test_queue_health_report() {
        let mut report = QueueHealthReport::new();
        assert!(report.is_healthy());

        report.add_healthy("queue1");
        assert!(report.is_healthy());

        report.add_missing("queue2");
        assert!(!report.is_healthy());
    }
}
