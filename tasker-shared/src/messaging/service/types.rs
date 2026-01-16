//! # Messaging Service Types
//!
//! Core types for the provider-agnostic messaging abstraction.
//!
//! ## Key Types
//!
//! - [`MessageHandle`]: Provider-specific handle for ack/nack operations (enum dispatch)
//! - [`MessageMetadata`]: Common metadata across all providers
//! - [`QueuedMessage`]: Provider-agnostic message wrapper combining payload + handle + metadata
//! - [`ReceiptHandle`]: String-based handle for backward compatibility
//! - [`MessageNotification`]: Push notification types for different delivery models (TAS-133)
//!
//! ## Design Rationale
//!
//! The `MessageHandle` enum provides explicit provider-specific information without
//! implicit string encoding. This enables:
//!
//! 1. **Type-safe provider dispatch** - match on enum variants for provider-specific ops
//! 2. **Common interface** - `queue_name()`, `as_receipt_handle()` work uniformly
//! 3. **No implicit/explicit gap** - single source of truth for provider info
//!
//! ## Push Notification Delivery Models (TAS-133)
//!
//! Different providers have different native delivery models:
//!
//! | Provider | Native Model | Notification Type | Fallback Needed |
//! |----------|--------------|-------------------|-----------------|
//! | PGMQ | Poll | Signal only (pg_notify) | Yes (catch-up) |
//! | RabbitMQ | **Push** | Full message (basic_consume) | **No** |
//!
//! The [`MessageNotification`] enum captures this distinction:
//! - `Available`: Signal that a message exists (PGMQ style, requires fetch)
//! - `Message`: Full message delivered (RabbitMQ style, ready to process)

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Unique identifier for a queued message
///
/// The format is provider-specific:
/// - PGMQ: i64 message ID as string
/// - RabbitMQ: delivery tag as string
/// - InMemory: UUID as string
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

// =============================================================================
// MessageHandle - Provider-specific handle for ack/nack operations
// =============================================================================

/// Provider-specific handle for message operations (ack, nack, extend visibility)
///
/// This enum provides explicit, type-safe dispatch for provider-specific operations.
/// Unlike the string-based [`ReceiptHandle`], `MessageHandle` carries all information
/// needed for message lifecycle operations without implicit encoding.
///
/// ## Design Rationale
///
/// The previous approach used `ReceiptHandle(String)` which implicitly encoded
/// provider info (e.g., PGMQ msg_id as string). This created an implicit/explicit
/// gap when we also needed provider context. `MessageHandle` makes the provider
/// explicit via enum variants, enabling:
///
/// - Pattern matching for provider-specific code paths
/// - Common interface methods for provider-agnostic code
/// - Backward compatibility via `as_receipt_handle()`
///
/// ## Example
///
/// ```rust
/// use tasker_shared::messaging::service::MessageHandle;
///
/// let handle = MessageHandle::Pgmq {
///     msg_id: 12345,
///     queue_name: "worker_payments_queue".to_string(),
/// };
///
/// // Provider-agnostic access
/// assert_eq!(handle.queue_name(), "worker_payments_queue");
/// assert_eq!(handle.provider_name(), "pgmq");
///
/// // Provider-specific access via pattern matching
/// if let MessageHandle::Pgmq { msg_id, .. } = &handle {
///     println!("PGMQ message ID: {}", msg_id);
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageHandle {
    /// PGMQ message handle
    ///
    /// PGMQ uses integer message IDs for all operations.
    Pgmq {
        /// The PGMQ message ID (i64)
        msg_id: i64,
        /// Queue name for operations
        queue_name: String,
    },

    /// RabbitMQ message handle
    ///
    /// RabbitMQ uses delivery tags for acknowledgment.
    RabbitMq {
        /// The delivery tag from basic_deliver
        delivery_tag: u64,
        /// Queue name for operations
        queue_name: String,
    },

    /// In-memory message handle (for testing)
    InMemory {
        /// Sequential message ID (u64)
        id: u64,
        /// Queue name for operations
        queue_name: String,
    },
}

impl MessageHandle {
    /// Get the queue name associated with this handle
    pub fn queue_name(&self) -> &str {
        match self {
            Self::Pgmq { queue_name, .. }
            | Self::RabbitMq { queue_name, .. }
            | Self::InMemory { queue_name, .. } => queue_name,
        }
    }

    /// Get the provider name for logging/metrics
    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::Pgmq { .. } => "pgmq",
            Self::RabbitMq { .. } => "rabbitmq",
            Self::InMemory { .. } => "in_memory",
        }
    }

    /// Convert to a string-based ReceiptHandle for backward compatibility
    ///
    /// This enables using `MessageHandle` with existing `MessageClient` methods
    /// that accept `&ReceiptHandle`.
    pub fn as_receipt_handle(&self) -> ReceiptHandle {
        match self {
            Self::Pgmq { msg_id, .. } => ReceiptHandle::from(*msg_id),
            Self::RabbitMq { delivery_tag, .. } => ReceiptHandle::from(*delivery_tag),
            Self::InMemory { id, .. } => ReceiptHandle::from(*id),
        }
    }

    /// Try to get the PGMQ message ID (returns None for other providers)
    pub fn pgmq_msg_id(&self) -> Option<i64> {
        match self {
            Self::Pgmq { msg_id, .. } => Some(*msg_id),
            _ => None,
        }
    }

    /// Try to get the RabbitMQ delivery tag (returns None for other providers)
    pub fn rabbitmq_delivery_tag(&self) -> Option<u64> {
        match self {
            Self::RabbitMq { delivery_tag, .. } => Some(*delivery_tag),
            _ => None,
        }
    }

    /// Check if this is a PGMQ handle
    pub fn is_pgmq(&self) -> bool {
        matches!(self, Self::Pgmq { .. })
    }

    /// Check if this is a RabbitMQ handle
    pub fn is_rabbitmq(&self) -> bool {
        matches!(self, Self::RabbitMq { .. })
    }

    /// Check if this is an in-memory handle
    pub fn is_in_memory(&self) -> bool {
        matches!(self, Self::InMemory { .. })
    }
}

impl std::fmt::Display for MessageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pgmq { msg_id, queue_name } => {
                write!(f, "pgmq:{}:{}", queue_name, msg_id)
            }
            Self::RabbitMq {
                delivery_tag,
                queue_name,
            } => {
                write!(f, "rabbitmq:{}:{}", queue_name, delivery_tag)
            }
            Self::InMemory { id, queue_name } => {
                write!(f, "in_memory:{}:{}", queue_name, id)
            }
        }
    }
}

// =============================================================================
// MessageMetadata - Common metadata across all providers
// =============================================================================

/// Common metadata for received messages across all providers
///
/// This struct contains information that is available from all messaging providers,
/// extracted into a common format.
#[derive(Debug, Clone)]
pub struct MessageMetadata {
    /// Number of times this message has been received/delivered
    ///
    /// Increments each time the message becomes visible after visibility timeout
    /// or is redelivered. Useful for implementing retry limits and DLQ logic.
    pub receive_count: u32,

    /// When the message was originally enqueued
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}

impl MessageMetadata {
    /// Create new message metadata
    pub fn new(receive_count: u32, enqueued_at: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            receive_count,
            enqueued_at,
        }
    }

    /// Create metadata with default values (for testing)
    pub fn default_now() -> Self {
        Self {
            receive_count: 1,
            enqueued_at: chrono::Utc::now(),
        }
    }
}

// =============================================================================
// QueuedMessage - Provider-agnostic message wrapper
// =============================================================================

/// A message received from a queue with provider-agnostic wrapper
///
/// This struct combines:
/// - The deserialized domain payload (`message`)
/// - Provider-specific handle for ack/nack operations (`handle`)
/// - Common metadata across providers (`metadata`)
///
/// ## Design
///
/// `QueuedMessage` is the primary type for received messages in provider-agnostic
/// code paths. The `MessageHandle` enum enables provider-specific operations when
/// needed via pattern matching, while common operations use the uniform interface.
///
/// ## Example
///
/// ```rust,ignore
/// use tasker_shared::messaging::service::{QueuedMessage, MessageHandle, MessageMetadata};
///
/// // Receive a message (provider creates this)
/// let msg: QueuedMessage<MyPayload> = client.receive_messages(...).await?;
///
/// // Access payload
/// process(&msg.message);
///
/// // Provider-agnostic ack
/// client.ack_message(msg.handle.queue_name(), &msg.handle.as_receipt_handle()).await?;
///
/// // Or provider-specific handling
/// match &msg.handle {
///     MessageHandle::Pgmq { msg_id, .. } => { /* PGMQ-specific */ },
///     MessageHandle::RabbitMq { delivery_tag, .. } => { /* RabbitMQ-specific */ },
///     _ => { /* fallback */ }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct QueuedMessage<T> {
    /// The deserialized message payload
    pub message: T,

    /// Provider-specific handle for ack/nack/extend operations
    pub handle: MessageHandle,

    /// Common metadata (receive count, enqueue time)
    pub metadata: MessageMetadata,

    // Legacy field for backward compatibility during migration
    // TODO(TAS-133): Remove after all code migrates to `handle`
    /// Handle for acknowledging this message (legacy, use `handle` instead)
    pub receipt_handle: ReceiptHandle,
}

impl<T> QueuedMessage<T> {
    /// Create a new queued message with MessageHandle
    pub fn with_handle(message: T, handle: MessageHandle, metadata: MessageMetadata) -> Self {
        let receipt_handle = handle.as_receipt_handle();
        Self {
            message,
            handle,
            metadata,
            receipt_handle,
        }
    }

    /// Create a new queued message (legacy constructor for backward compatibility)
    ///
    /// Prefer `with_handle()` for new code.
    // TODO(TAS-133): Deprecate after migration
    pub fn new(
        receipt_handle: ReceiptHandle,
        message: T,
        receive_count: u32,
        enqueued_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        // Create a placeholder handle - callers should migrate to with_handle()
        // This maintains backward compat while we migrate
        let handle = MessageHandle::InMemory {
            id: 0,
            queue_name: "legacy".to_string(),
        };
        Self {
            message,
            handle,
            metadata: MessageMetadata::new(receive_count, enqueued_at),
            receipt_handle,
        }
    }

    /// Map the message to a different type
    pub fn map<U, F>(self, f: F) -> QueuedMessage<U>
    where
        F: FnOnce(T) -> U,
    {
        QueuedMessage {
            message: f(self.message),
            handle: self.handle,
            metadata: self.metadata,
            receipt_handle: self.receipt_handle,
        }
    }

    /// Get the queue name from the handle
    pub fn queue_name(&self) -> &str {
        self.handle.queue_name()
    }

    /// Get the provider name from the handle
    pub fn provider_name(&self) -> &'static str {
        self.handle.provider_name()
    }

    /// Get the receive count from metadata
    pub fn receive_count(&self) -> u32 {
        self.metadata.receive_count
    }

    /// Get the enqueued time from metadata
    pub fn enqueued_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.metadata.enqueued_at
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
            in_flight_count: if in_flight > 0 { Some(in_flight) } else { None },
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
        let last_sent = self
            .last_recorded_sent
            .swap(current_sent, Ordering::Relaxed);
        if current_sent > last_sent {
            messages_sent_total().add(current_sent - last_sent, labels);
        }

        let current_received = self.total_received.load(Ordering::Relaxed);
        let last_received = self
            .last_recorded_received
            .swap(current_received, Ordering::Relaxed);
        if current_received > last_received {
            messages_received_total().add(current_received - last_received, labels);
        }

        // Acked messages map to "archived" in PGMQ terminology
        let current_acked = self.total_acked.load(Ordering::Relaxed);
        let last_acked = self
            .last_recorded_acked
            .swap(current_acked, Ordering::Relaxed);
        if current_acked > last_acked {
            messages_archived_total().add(current_acked - last_acked, labels);
        }

        // Note: total_nacked doesn't have a direct counter in messaging.rs
        // It's tracked internally but could be added if needed
        let current_nacked = self.total_nacked.load(Ordering::Relaxed);
        let _last_nacked = self
            .last_recorded_nacked
            .swap(current_nacked, Ordering::Relaxed);
        // Future: add messages_nacked_total() counter if needed
    }

    /// Reset the delta tracking for metrics
    ///
    /// Call this if you need to reset the baseline for delta calculations,
    /// for example after a metrics system restart.
    pub fn reset_metrics_baseline(&self) {
        self.last_recorded_sent
            .store(self.total_sent.load(Ordering::Relaxed), Ordering::Relaxed);
        self.last_recorded_received.store(
            self.total_received.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
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

// =============================================================================
// MessageNotification - Push notification delivery models (TAS-133)
// =============================================================================

/// Push notification from a messaging provider
///
/// This enum represents the two distinct delivery models for push notifications:
///
/// ## PGMQ Notifications (TAS-133)
///
/// PGMQ uses PostgreSQL LISTEN/NOTIFY with two modes based on message size:
///
/// - **Small messages (< 7KB)**: Full payload included via `MessageWithPayload` event,
///   returned as `MessageNotification::Message` - process directly, no fetch needed
/// - **Large messages (>= 7KB)**: Signal-only via `MessageReady` event, returned as
///   `MessageNotification::Available` with `msg_id` - fetch via `read_specific_message()`
///
/// This enables RabbitMQ-style direct processing for most messages while falling back
/// to fetch-based processing for large payloads that exceed pg_notify's ~8KB limit.
///
/// ## RabbitMQ Style: Full Message (`Message`)
///
/// RabbitMQ's `basic_consume()` pushes complete messages to consumers. The full
/// message body is delivered via the AMQP channel, so no additional fetch is
/// needed. This model doesn't require fallback polling because message delivery
/// is protocol-guaranteed.
///
/// ## Example Usage
///
/// ```rust,ignore
/// match notification {
///     MessageNotification::Available { queue_name, msg_id } => {
///         // PGMQ large message: signal received with msg_id, fetch the specific message
///         if let Some(id) = msg_id {
///             let message = client.read_specific_message(&queue_name, id).await?;
///             process_message(message);
///         } else {
///             // Fallback: fetch any available message
///             let messages = client.receive_messages(&queue_name, 1, timeout).await?;
///             process_messages(messages);
///         }
///     }
///     MessageNotification::Message(queued_msg) => {
///         // RabbitMQ or PGMQ small message: full message already available
///         process_message(queued_msg);
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub enum MessageNotification {
    /// Signal that a message is available (PGMQ large message fallback)
    ///
    /// Used when the message payload exceeds the pg_notify 7KB threshold.
    /// The consumer must fetch the message separately using `read_specific_message()`
    /// with the provided `msg_id`.
    Available {
        /// The queue where the message is available
        queue_name: String,
        /// The specific message ID to fetch (TAS-133: for PGMQ large messages)
        ///
        /// When present, use `read_specific_message(queue_name, msg_id)` for efficient fetch.
        /// When None, fall back to `receive_messages()` polling.
        msg_id: Option<i64>,
    },

    /// Full message delivered (RabbitMQ or PGMQ small message)
    ///
    /// The complete message is included in the notification. This is used by:
    /// - RabbitMQ: `basic_consume()` delivers full messages
    /// - PGMQ: Small messages (< 7KB) include payload in pg_notify (TAS-133)
    ///
    /// The payload is raw bytes to allow for flexible deserialization.
    Message(QueuedMessage<Vec<u8>>),
}

impl MessageNotification {
    /// Create a new Available notification (PGMQ style) without msg_id
    ///
    /// Use this for fallback polling or when msg_id is unknown.
    pub fn available(queue_name: impl Into<String>) -> Self {
        Self::Available {
            queue_name: queue_name.into(),
            msg_id: None,
        }
    }

    /// Create a new Available notification with a specific msg_id (TAS-133)
    ///
    /// Use this for PGMQ large message signals where the msg_id is known.
    /// The consumer can then use `read_specific_message()` for efficient fetch.
    pub fn available_with_msg_id(queue_name: impl Into<String>, msg_id: i64) -> Self {
        Self::Available {
            queue_name: queue_name.into(),
            msg_id: Some(msg_id),
        }
    }

    /// Create a new Message notification (RabbitMQ style)
    pub fn message(queued_message: QueuedMessage<Vec<u8>>) -> Self {
        Self::Message(queued_message)
    }

    /// Get the queue name from the notification
    pub fn queue_name(&self) -> &str {
        match self {
            Self::Available { queue_name, .. } => queue_name,
            Self::Message(msg) => msg.queue_name(),
        }
    }

    /// Get the message ID if present (TAS-133: for PGMQ large messages)
    ///
    /// Returns `Some(msg_id)` for `Available` notifications with a known msg_id,
    /// or `None` for `Message` notifications or `Available` without msg_id.
    pub fn msg_id(&self) -> Option<i64> {
        match self {
            Self::Available { msg_id, .. } => *msg_id,
            Self::Message(_) => None,
        }
    }

    /// Check if this is an Available notification (requires fetch)
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }

    /// Check if this is a Message notification (full message included)
    pub fn is_message(&self) -> bool {
        matches!(self, Self::Message(_))
    }

    /// Try to extract the message if this is a Message variant
    pub fn into_message(self) -> Option<QueuedMessage<Vec<u8>>> {
        match self {
            Self::Message(msg) => Some(msg),
            Self::Available { .. } => None,
        }
    }
}

impl std::fmt::Display for MessageNotification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Available { queue_name, msg_id } => match msg_id {
                Some(id) => write!(
                    f,
                    "MessageNotification::Available({}, msg_id={})",
                    queue_name, id
                ),
                None => write!(f, "MessageNotification::Available({})", queue_name),
            },
            Self::Message(msg) => {
                write!(
                    f,
                    "MessageNotification::Message({}, {} bytes)",
                    msg.queue_name(),
                    msg.message.len()
                )
            }
        }
    }
}

// =============================================================================
// MessageEvent - Provider-agnostic event for routing and classification
// =============================================================================

/// Provider-agnostic message event for event routing and classification
///
/// This is the provider-agnostic equivalent of `pgmq_notify::MessageReadyEvent`.
/// Used as the inner type for classified domain events (`OrchestrationQueueEvent`,
/// `WorkerQueueEvent`) to enable provider-agnostic event processing.
///
/// ## Processing Pipeline
///
/// ```text
/// subscribe() → MessageNotification (delivery model)
///     → process → MessageEvent (routing info)
///         → classify → OrchestrationQueueEvent/WorkerQueueEvent (domain events)
/// ```
///
/// ## Distinction from MessageNotification
///
/// - [`MessageNotification`] answers: "Do I have the full message or just a signal?" (delivery model)
/// - [`MessageEvent`] answers: "What queue/namespace is this for?" (routing/classification)
///
/// These serve different stages in the processing pipeline.
///
/// ## Conversion
///
/// Created from [`MessageNotification`] during event processing:
/// - `Available { queue_name, msg_id }` → extract routing info + namespace
/// - `Message(QueuedMessage)` → extract from handle
///
/// Also convertible from `pgmq_notify::MessageReadyEvent` for backward compatibility.
///
/// ## Example
///
/// ```rust
/// use tasker_shared::messaging::service::{MessageEvent, MessageId};
///
/// // Create directly
/// let event = MessageEvent::new("orchestration_step_results", "orchestration", MessageId::from(42i64));
/// assert_eq!(event.queue_name, "orchestration_step_results");
/// assert_eq!(event.namespace, "orchestration");
///
/// // Display format: namespace:queue_name:message_id
/// assert_eq!(format!("{}", event), "orchestration:orchestration_step_results:42");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MessageEvent {
    /// Queue name where the message resides
    pub queue_name: String,

    /// Namespace for routing (e.g., "orchestration", "linear_workflow")
    ///
    /// Extracted from queue name patterns or explicitly provided.
    pub namespace: String,

    /// Provider-agnostic message identifier
    pub message_id: MessageId,
}

impl MessageEvent {
    /// Create a new message event
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::messaging::service::{MessageEvent, MessageId};
    ///
    /// let event = MessageEvent::new(
    ///     "worker_payments_queue",
    ///     "payments",
    ///     MessageId::from(123i64),
    /// );
    /// assert_eq!(event.queue_name, "worker_payments_queue");
    /// assert_eq!(event.namespace, "payments");
    /// ```
    pub fn new(
        queue_name: impl Into<String>,
        namespace: impl Into<String>,
        message_id: impl Into<MessageId>,
    ) -> Self {
        Self {
            queue_name: queue_name.into(),
            namespace: namespace.into(),
            message_id: message_id.into(),
        }
    }

    /// Create from a `MessageNotification::Available` variant
    ///
    /// Requires namespace to be provided since `Available` doesn't contain it.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::messaging::service::MessageEvent;
    ///
    /// // With known msg_id (PGMQ large message signal)
    /// let event = MessageEvent::from_available("orchestration_step_results", Some(42), "orchestration");
    /// assert_eq!(event.queue_name, "orchestration_step_results");
    /// assert_eq!(event.namespace, "orchestration");
    ///
    /// // Without msg_id (fallback polling)
    /// let event = MessageEvent::from_available("worker_queue", None, "worker");
    /// assert_eq!(event.message_id.as_str(), "unknown");
    /// ```
    pub fn from_available(queue_name: &str, msg_id: Option<i64>, namespace: &str) -> Self {
        Self {
            queue_name: queue_name.to_string(),
            namespace: namespace.to_string(),
            message_id: msg_id
                .map(MessageId::from)
                .unwrap_or_else(|| MessageId::new("unknown")),
        }
    }

    /// Create from a `QueuedMessage` (for `MessageNotification::Message` variant)
    ///
    /// Extracts queue name and message ID from the message handle.
    /// Requires namespace to be provided since the handle doesn't contain it.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use tasker_shared::messaging::service::{MessageEvent, QueuedMessage, MessageHandle, MessageMetadata};
    ///
    /// let handle = MessageHandle::RabbitMq {
    ///     delivery_tag: 12345,
    ///     queue_name: "worker_payments_queue".to_string(),
    /// };
    /// let msg = QueuedMessage::with_handle(vec![1, 2, 3], handle, MessageMetadata::default_now());
    ///
    /// let event = MessageEvent::from_queued_message(&msg, "payments");
    /// assert_eq!(event.queue_name, "worker_payments_queue");
    /// assert_eq!(event.namespace, "payments");
    /// ```
    pub fn from_queued_message<T>(msg: &QueuedMessage<T>, namespace: &str) -> Self {
        Self {
            queue_name: msg.queue_name().to_string(),
            namespace: namespace.to_string(),
            message_id: MessageId::new(msg.handle.as_receipt_handle().as_str()),
        }
    }
}

impl std::fmt::Display for MessageEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.namespace, self.queue_name, self.message_id
        )
    }
}

/// Backward compatibility: Convert from `pgmq_notify::MessageReadyEvent`
///
/// This enables gradual migration from PGMQ-specific types to provider-agnostic types.
/// The conversion preserves all fields: `queue_name`, `namespace`, and `msg_id`.
///
/// # Example
///
/// ```rust,ignore
/// use tasker_shared::messaging::service::MessageEvent;
///
/// let pgmq_event = pgmq_notify::MessageReadyEvent {
///     msg_id: 123,
///     queue_name: "orchestration_step_results".to_string(),
///     namespace: "orchestration".to_string(),
/// };
///
/// let event: MessageEvent = pgmq_event.into();
/// assert_eq!(event.queue_name, "orchestration_step_results");
/// assert_eq!(event.namespace, "orchestration");
/// ```
impl From<pgmq_notify::MessageReadyEvent> for MessageEvent {
    fn from(event: pgmq_notify::MessageReadyEvent) -> Self {
        Self {
            queue_name: event.queue_name,
            namespace: event.namespace,
            message_id: MessageId::from(event.msg_id),
        }
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
        let msg = QueuedMessage::new(ReceiptHandle::from("handle"), 42_i32, 1, chrono::Utc::now());

        let mapped = msg.map(|n| n.to_string());
        assert_eq!(mapped.message, "42");
        assert_eq!(mapped.receive_count(), 1);
    }

    #[test]
    fn test_message_handle_pgmq() {
        let handle = MessageHandle::Pgmq {
            msg_id: 12345,
            queue_name: "test_queue".to_string(),
        };

        assert_eq!(handle.queue_name(), "test_queue");
        assert_eq!(handle.provider_name(), "pgmq");
        assert!(handle.is_pgmq());
        assert!(!handle.is_rabbitmq());
        assert_eq!(handle.pgmq_msg_id(), Some(12345));
        assert_eq!(handle.rabbitmq_delivery_tag(), None);

        let receipt = handle.as_receipt_handle();
        assert_eq!(receipt.as_i64(), Some(12345));
    }

    #[test]
    fn test_message_handle_rabbitmq() {
        let handle = MessageHandle::RabbitMq {
            delivery_tag: 67890,
            queue_name: "rabbit_queue".to_string(),
        };

        assert_eq!(handle.queue_name(), "rabbit_queue");
        assert_eq!(handle.provider_name(), "rabbitmq");
        assert!(!handle.is_pgmq());
        assert!(handle.is_rabbitmq());
        assert_eq!(handle.pgmq_msg_id(), None);
        assert_eq!(handle.rabbitmq_delivery_tag(), Some(67890));

        let receipt = handle.as_receipt_handle();
        assert_eq!(receipt.as_str(), "67890");
    }

    #[test]
    fn test_message_handle_in_memory() {
        let id = 42u64;
        let handle = MessageHandle::InMemory {
            id,
            queue_name: "mem_queue".to_string(),
        };

        assert_eq!(handle.queue_name(), "mem_queue");
        assert_eq!(handle.provider_name(), "in_memory");
        assert!(handle.is_in_memory());

        let receipt = handle.as_receipt_handle();
        assert_eq!(receipt.as_str(), "42");
    }

    #[test]
    fn test_message_handle_display() {
        let pgmq = MessageHandle::Pgmq {
            msg_id: 123,
            queue_name: "q1".to_string(),
        };
        assert_eq!(format!("{}", pgmq), "pgmq:q1:123");

        let rabbit = MessageHandle::RabbitMq {
            delivery_tag: 456,
            queue_name: "q2".to_string(),
        };
        assert_eq!(format!("{}", rabbit), "rabbitmq:q2:456");
    }

    #[test]
    fn test_message_metadata() {
        let now = chrono::Utc::now();
        let metadata = MessageMetadata::new(3, now);

        assert_eq!(metadata.receive_count, 3);
        assert_eq!(metadata.enqueued_at, now);
    }

    #[test]
    fn test_queued_message_with_handle() {
        let handle = MessageHandle::Pgmq {
            msg_id: 999,
            queue_name: "test_queue".to_string(),
        };
        let metadata = MessageMetadata::default_now();

        let msg = QueuedMessage::with_handle("payload", handle, metadata);

        assert_eq!(msg.message, "payload");
        assert_eq!(msg.queue_name(), "test_queue");
        assert_eq!(msg.provider_name(), "pgmq");
        assert!(msg.handle.is_pgmq());
        assert_eq!(msg.receipt_handle.as_i64(), Some(999));
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

    // =========================================================================
    // MessageNotification tests (TAS-133)
    // =========================================================================

    #[test]
    fn test_message_notification_available() {
        let notification = MessageNotification::available("test_queue");

        assert!(notification.is_available());
        assert!(!notification.is_message());
        assert_eq!(notification.queue_name(), "test_queue");
        assert!(notification.into_message().is_none());
    }

    #[test]
    fn test_message_notification_message() {
        let handle = MessageHandle::RabbitMq {
            delivery_tag: 12345,
            queue_name: "rabbit_queue".to_string(),
        };
        let metadata = MessageMetadata::default_now();
        let queued_msg = QueuedMessage::with_handle(vec![1, 2, 3, 4], handle, metadata);

        let notification = MessageNotification::message(queued_msg);

        assert!(!notification.is_available());
        assert!(notification.is_message());
        assert_eq!(notification.queue_name(), "rabbit_queue");

        let extracted = notification.into_message();
        assert!(extracted.is_some());
        let msg = extracted.unwrap();
        assert_eq!(msg.message, vec![1, 2, 3, 4]);
        assert!(msg.handle.is_rabbitmq());
    }

    #[test]
    fn test_message_notification_display() {
        let available = MessageNotification::available("my_queue");
        assert_eq!(
            format!("{}", available),
            "MessageNotification::Available(my_queue)"
        );

        let handle = MessageHandle::Pgmq {
            msg_id: 100,
            queue_name: "pgmq_queue".to_string(),
        };
        let metadata = MessageMetadata::default_now();
        let queued_msg = QueuedMessage::with_handle(vec![0; 50], handle, metadata);
        let message = MessageNotification::message(queued_msg);
        assert_eq!(
            format!("{}", message),
            "MessageNotification::Message(pgmq_queue, 50 bytes)"
        );
    }

    #[test]
    fn test_message_notification_available_with_msg_id() {
        // TAS-133: Test Available notification with msg_id (for PGMQ large messages)
        let notification = MessageNotification::available_with_msg_id("test_queue", 42);

        assert!(notification.is_available());
        assert!(!notification.is_message());
        assert_eq!(notification.queue_name(), "test_queue");
        assert_eq!(notification.msg_id(), Some(42));
        assert!(notification.into_message().is_none());
    }

    #[test]
    fn test_message_notification_msg_id() {
        // TAS-133: Test msg_id() accessor for different variants
        let available_no_id = MessageNotification::available("queue1");
        assert_eq!(available_no_id.msg_id(), None);

        let available_with_id = MessageNotification::available_with_msg_id("queue2", 123);
        assert_eq!(available_with_id.msg_id(), Some(123));

        let handle = MessageHandle::Pgmq {
            msg_id: 999,
            queue_name: "pgmq_queue".to_string(),
        };
        let metadata = MessageMetadata::default_now();
        let queued_msg = QueuedMessage::with_handle(vec![1, 2, 3], handle, metadata);
        let message = MessageNotification::message(queued_msg);
        // Message variant returns None for msg_id() since the id is in the handle
        assert_eq!(message.msg_id(), None);
    }

    #[test]
    fn test_message_notification_display_with_msg_id() {
        // TAS-133: Test Display for Available with msg_id
        let available_with_id = MessageNotification::available_with_msg_id("my_queue", 42);
        assert_eq!(
            format!("{}", available_with_id),
            "MessageNotification::Available(my_queue, msg_id=42)"
        );
    }

    // =========================================================================
    // MessageEvent tests (TAS-133e)
    // =========================================================================

    #[test]
    fn test_message_event_new() {
        let event = MessageEvent::new("test_queue", "orchestration", 123_i64);

        assert_eq!(event.queue_name, "test_queue");
        assert_eq!(event.namespace, "orchestration");
        assert_eq!(event.message_id.as_str(), "123");
    }

    #[test]
    fn test_message_event_from_available_with_msg_id() {
        let event = MessageEvent::from_available("step_results", Some(456), "worker");

        assert_eq!(event.queue_name, "step_results");
        assert_eq!(event.namespace, "worker");
        assert_eq!(event.message_id.as_str(), "456");
    }

    #[test]
    fn test_message_event_from_available_without_msg_id() {
        let event = MessageEvent::from_available("step_results", None, "worker");

        assert_eq!(event.queue_name, "step_results");
        assert_eq!(event.namespace, "worker");
        assert_eq!(event.message_id.as_str(), "unknown"); // "unknown" for fallback polling without msg_id
    }

    #[test]
    fn test_message_event_from_queued_message() {
        let handle = MessageHandle::Pgmq {
            msg_id: 789,
            queue_name: "orchestration_commands".to_string(),
        };
        let metadata = MessageMetadata::default_now();
        let queued_msg = QueuedMessage::with_handle(vec![1, 2, 3], handle, metadata);

        let event = MessageEvent::from_queued_message(&queued_msg, "orchestration");

        assert_eq!(event.queue_name, "orchestration_commands");
        assert_eq!(event.namespace, "orchestration");
        // msg_id comes from receipt_handle
        assert_eq!(event.message_id.as_str(), "789");
    }

    #[test]
    fn test_message_event_display() {
        let event = MessageEvent::new("my_queue", "my_namespace", 42_i64);
        // Display format is "namespace:queue_name:msg_id" for compact logging
        assert_eq!(format!("{}", event), "my_namespace:my_queue:42");
    }

    #[test]
    fn test_message_event_from_pgmq_notify() {
        use std::collections::HashMap;

        let pgmq_event = pgmq_notify::MessageReadyEvent {
            msg_id: 999,
            queue_name: "orchestration_step_results".to_string(),
            namespace: "orchestration".to_string(),
            ready_at: chrono::Utc::now(),
            metadata: HashMap::new(),
            visibility_timeout_seconds: None,
        };

        let event: MessageEvent = pgmq_event.into();

        assert_eq!(event.queue_name, "orchestration_step_results");
        assert_eq!(event.namespace, "orchestration");
        assert_eq!(event.message_id.as_str(), "999");
    }

    #[test]
    fn test_message_event_serde_roundtrip() {
        let event = MessageEvent::new("test_queue", "test_namespace", 123_i64);

        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: MessageEvent = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized, event);
    }

    #[test]
    fn test_message_event_equality() {
        let event1 = MessageEvent::new("queue", "ns", 1_i64);
        let event2 = MessageEvent::new("queue", "ns", 1_i64);
        let event3 = MessageEvent::new("queue", "ns", 2_i64);

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }
}
