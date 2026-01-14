//! # Messaging Service Types
//!
//! Core types for the provider-agnostic messaging abstraction.

use std::sync::atomic::{AtomicU64, Ordering};
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
///
/// This is a snapshot struct returned by [`AtomicQueueStats::snapshot()`].
/// For lock-free updates on hot paths, use [`AtomicQueueStats`].
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

    /// Age of the oldest message in the queue (milliseconds)
    ///
    /// Useful for detecting stuck queues or processing delays.
    pub oldest_message_age_ms: Option<u64>,

    /// Total messages sent to this queue (lifetime counter)
    pub total_sent: u64,

    /// Total messages received from this queue (lifetime counter)
    pub total_received: u64,

    /// Total messages acknowledged (lifetime counter)
    pub total_acked: u64,

    /// Total messages nacked (lifetime counter)
    pub total_nacked: u64,
}

impl QueueStats {
    /// Create new queue stats
    pub fn new(queue_name: impl Into<String>, message_count: u64) -> Self {
        Self {
            queue_name: queue_name.into(),
            message_count,
            in_flight_count: None,
            oldest_message_age_ms: None,
            total_sent: 0,
            total_received: 0,
            total_acked: 0,
            total_nacked: 0,
        }
    }

    /// Set the in-flight count
    pub fn with_in_flight_count(mut self, count: u64) -> Self {
        self.in_flight_count = Some(count);
        self
    }

    /// Set the oldest message age in milliseconds
    pub fn with_oldest_message_age_ms(mut self, age_ms: u64) -> Self {
        self.oldest_message_age_ms = Some(age_ms);
        self
    }

    /// Set the oldest message age from Duration
    pub fn with_oldest_message_age(mut self, age: Duration) -> Self {
        self.oldest_message_age_ms = Some(age.as_millis() as u64);
        self
    }

    /// Set lifetime counters
    pub fn with_counters(
        mut self,
        total_sent: u64,
        total_received: u64,
        total_acked: u64,
        total_nacked: u64,
    ) -> Self {
        self.total_sent = total_sent;
        self.total_received = total_received;
        self.total_acked = total_acked;
        self.total_nacked = total_nacked;
        self
    }
}

/// Lock-free atomic queue statistics for hot-path updates
///
/// Follows the same pattern as `AtomicStepExecutionStats` in worker_status_actor.rs.
/// Uses `AtomicU64` for all counters to eliminate lock contention on send/receive paths.
///
/// # Usage
///
/// ```rust
/// use tasker_shared::messaging::service::AtomicQueueStats;
///
/// let stats = AtomicQueueStats::new("my_queue");
///
/// // Hot-path updates (lock-free)
/// stats.record_send();
/// stats.record_receive();
/// stats.record_ack();
///
/// // Update depth from provider poll
/// stats.update_depth(42, Some(5), Some(1500));
///
/// // Get snapshot for metrics/reporting
/// let snapshot = stats.snapshot();
/// assert_eq!(snapshot.total_sent, 1);
/// ```
#[derive(Debug)]
pub struct AtomicQueueStats {
    /// Queue name (immutable after creation)
    queue_name: String,

    /// Current queue depth (updated from provider polling)
    message_count: AtomicU64,

    /// Messages currently in-flight (updated from provider polling)
    in_flight_count: AtomicU64,

    /// Age of oldest message in milliseconds (updated from provider polling)
    oldest_message_age_ms: AtomicU64,

    /// Total messages sent (lifetime counter)
    total_sent: AtomicU64,

    /// Total messages received (lifetime counter)
    total_received: AtomicU64,

    /// Total messages acknowledged (lifetime counter)
    total_acked: AtomicU64,

    /// Total messages nacked (lifetime counter)
    total_nacked: AtomicU64,

    // Last recorded values for delta computation (for OpenTelemetry counter integration)
    last_recorded_sent: AtomicU64,
    last_recorded_received: AtomicU64,
    last_recorded_acked: AtomicU64,
    last_recorded_nacked: AtomicU64,
}

impl AtomicQueueStats {
    /// Create new atomic stats for a queue
    pub fn new(queue_name: impl Into<String>) -> Self {
        Self {
            queue_name: queue_name.into(),
            message_count: AtomicU64::new(0),
            in_flight_count: AtomicU64::new(0),
            oldest_message_age_ms: AtomicU64::new(0),
            total_sent: AtomicU64::new(0),
            total_received: AtomicU64::new(0),
            total_acked: AtomicU64::new(0),
            total_nacked: AtomicU64::new(0),
            last_recorded_sent: AtomicU64::new(0),
            last_recorded_received: AtomicU64::new(0),
            last_recorded_acked: AtomicU64::new(0),
            last_recorded_nacked: AtomicU64::new(0),
        }
    }

    /// Get the queue name
    pub fn queue_name(&self) -> &str {
        &self.queue_name
    }

    /// Record a message send (lock-free)
    #[inline]
    pub fn record_send(&self) {
        self.total_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a batch of messages sent (lock-free)
    #[inline]
    pub fn record_send_batch(&self, count: u64) {
        self.total_sent.fetch_add(count, Ordering::Relaxed);
    }

    /// Record a message receive (lock-free)
    #[inline]
    pub fn record_receive(&self) {
        self.total_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a batch of messages received (lock-free)
    #[inline]
    pub fn record_receive_batch(&self, count: u64) {
        self.total_received.fetch_add(count, Ordering::Relaxed);
    }

    /// Record a message acknowledgment (lock-free)
    #[inline]
    pub fn record_ack(&self) {
        self.total_acked.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a message negative acknowledgment (lock-free)
    #[inline]
    pub fn record_nack(&self) {
        self.total_nacked.fetch_add(1, Ordering::Relaxed);
    }

    /// Update queue depth from provider polling
    ///
    /// Called periodically when fetching queue stats from the provider.
    /// Uses atomic stores (not add) since these are absolute values.
    pub fn update_depth(
        &self,
        message_count: u64,
        in_flight_count: Option<u64>,
        oldest_message_age_ms: Option<u64>,
    ) {
        self.message_count.store(message_count, Ordering::Relaxed);
        if let Some(count) = in_flight_count {
            self.in_flight_count.store(count, Ordering::Relaxed);
        }
        if let Some(age) = oldest_message_age_ms {
            self.oldest_message_age_ms.store(age, Ordering::Relaxed);
        }
    }

    /// Get a snapshot of current statistics
    ///
    /// Note: Individual reads are atomic but the snapshot is not
    /// transactionally consistent (acceptable for monitoring).
    pub fn snapshot(&self) -> QueueStats {
        let message_count = self.message_count.load(Ordering::Relaxed);
        let in_flight = self.in_flight_count.load(Ordering::Relaxed);
        let oldest_age = self.oldest_message_age_ms.load(Ordering::Relaxed);

        QueueStats {
            queue_name: self.queue_name.clone(),
            message_count,
            in_flight_count: if in_flight > 0 {
                Some(in_flight)
            } else {
                None
            },
            oldest_message_age_ms: if oldest_age > 0 {
                Some(oldest_age)
            } else {
                None
            },
            total_sent: self.total_sent.load(Ordering::Relaxed),
            total_received: self.total_received.load(Ordering::Relaxed),
            total_acked: self.total_acked.load(Ordering::Relaxed),
            total_nacked: self.total_nacked.load(Ordering::Relaxed),
        }
    }

    /// Record stats to OpenTelemetry metrics
    ///
    /// Call this periodically (e.g., after [`update_depth()`]) to push current values to metrics.
    /// Records both gauges (queue depth, oldest message age) and counter deltas (sent, received, etc.).
    ///
    /// Uses the gauges and counters from [`crate::metrics::messaging`].
    ///
    /// # Counter Delta Calculation
    ///
    /// For counters, this method calculates deltas since the last call to avoid double-counting.
    /// This is safe for periodic metric recording but should not be called concurrently.
    pub fn record_to_metrics(&self) {
        use crate::metrics::messaging::{
            messages_archived_total, messages_received_total, messages_sent_total, queue_depth,
            queue_oldest_message_age,
        };
        use opentelemetry::KeyValue;

        let queue_label = KeyValue::new("queue", self.queue_name.clone());
        let labels = std::slice::from_ref(&queue_label);

        // Record gauges (absolute values)
        queue_depth().record(self.message_count.load(Ordering::Relaxed), labels);

        let oldest_age = self.oldest_message_age_ms.load(Ordering::Relaxed);
        if oldest_age > 0 {
            queue_oldest_message_age().record(oldest_age, labels);
        }

        // Record counter deltas
        // We swap the last_recorded value with the current total and compute the delta
        let current_sent = self.total_sent.load(Ordering::Relaxed);
        let last_sent = self.last_recorded_sent.swap(current_sent, Ordering::Relaxed);
        if current_sent > last_sent {
            messages_sent_total().add(current_sent - last_sent, labels);
        }

        let current_received = self.total_received.load(Ordering::Relaxed);
        let last_received = self.last_recorded_received.swap(current_received, Ordering::Relaxed);
        if current_received > last_received {
            messages_received_total().add(current_received - last_received, labels);
        }

        // Acked messages map to "archived" in PGMQ terminology
        let current_acked = self.total_acked.load(Ordering::Relaxed);
        let last_acked = self.last_recorded_acked.swap(current_acked, Ordering::Relaxed);
        if current_acked > last_acked {
            messages_archived_total().add(current_acked - last_acked, labels);
        }

        // Note: total_nacked doesn't have a direct counter in messaging.rs
        // It's tracked internally but could be added if needed
        let current_nacked = self.total_nacked.load(Ordering::Relaxed);
        let _last_nacked = self.last_recorded_nacked.swap(current_nacked, Ordering::Relaxed);
        // Future: add messages_nacked_total() counter if needed
    }

    /// Reset the delta tracking for metrics
    ///
    /// Call this if you need to reset the baseline for delta calculations,
    /// for example after a metrics system restart.
    pub fn reset_metrics_baseline(&self) {
        self.last_recorded_sent
            .store(self.total_sent.load(Ordering::Relaxed), Ordering::Relaxed);
        self.last_recorded_received
            .store(self.total_received.load(Ordering::Relaxed), Ordering::Relaxed);
        self.last_recorded_acked
            .store(self.total_acked.load(Ordering::Relaxed), Ordering::Relaxed);
        self.last_recorded_nacked
            .store(self.total_nacked.load(Ordering::Relaxed), Ordering::Relaxed);
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
    fn test_queue_stats_builder() {
        let stats = QueueStats::new("test_queue", 100)
            .with_in_flight_count(10)
            .with_oldest_message_age_ms(5000)
            .with_counters(1000, 900, 850, 50);

        assert_eq!(stats.queue_name, "test_queue");
        assert_eq!(stats.message_count, 100);
        assert_eq!(stats.in_flight_count, Some(10));
        assert_eq!(stats.oldest_message_age_ms, Some(5000));
        assert_eq!(stats.total_sent, 1000);
        assert_eq!(stats.total_received, 900);
        assert_eq!(stats.total_acked, 850);
        assert_eq!(stats.total_nacked, 50);
    }

    #[test]
    fn test_atomic_queue_stats_lock_free() {
        let stats = AtomicQueueStats::new("my_queue");

        // Record operations (lock-free)
        stats.record_send();
        stats.record_send();
        stats.record_send_batch(3);
        stats.record_receive();
        stats.record_receive_batch(2);
        stats.record_ack();
        stats.record_nack();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.queue_name, "my_queue");
        assert_eq!(snapshot.total_sent, 5); // 1 + 1 + 3
        assert_eq!(snapshot.total_received, 3); // 1 + 2
        assert_eq!(snapshot.total_acked, 1);
        assert_eq!(snapshot.total_nacked, 1);
    }

    #[test]
    fn test_atomic_queue_stats_update_depth() {
        let stats = AtomicQueueStats::new("test_queue");

        // Initial state
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.message_count, 0);
        assert!(snapshot.in_flight_count.is_none());
        assert!(snapshot.oldest_message_age_ms.is_none());

        // Update from provider poll
        stats.update_depth(42, Some(5), Some(1500));

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.message_count, 42);
        assert_eq!(snapshot.in_flight_count, Some(5));
        assert_eq!(snapshot.oldest_message_age_ms, Some(1500));
    }

    #[test]
    fn test_atomic_queue_stats_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AtomicQueueStats>();
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

    #[test]
    fn test_atomic_queue_stats_reset_metrics_baseline() {
        let stats = AtomicQueueStats::new("test_queue");

        // Record some operations
        stats.record_send_batch(100);
        stats.record_receive_batch(80);
        stats.record_ack();
        stats.record_ack();

        // Verify counters
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_sent, 100);
        assert_eq!(snapshot.total_received, 80);
        assert_eq!(snapshot.total_acked, 2);

        // Reset baseline (simulating what happens after record_to_metrics)
        stats.reset_metrics_baseline();

        // Record more operations
        stats.record_send_batch(50);
        stats.record_receive_batch(40);
        stats.record_ack();

        // Verify totals include all operations
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_sent, 150); // 100 + 50
        assert_eq!(snapshot.total_received, 120); // 80 + 40
        assert_eq!(snapshot.total_acked, 3); // 2 + 1

        // The baseline tracking ensures deltas would be calculated correctly
        // (actual delta calculation tested implicitly via record_to_metrics)
    }
}
