//! # Messaging Metrics (TAS-29 Phase 3.3)
//!
//! OpenTelemetry metrics for PGMQ message queue operations including:
//! - Message send/receive counters
//! - Message latency histograms
//! - Queue depth gauges
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::metrics::messaging::*;
//! use opentelemetry::KeyValue;
//! use std::time::Instant;
//!
//! // Record message send
//! let start = Instant::now();
//! // ... send message ...
//! let duration_ms = start.elapsed().as_millis() as f64;
//!
//! messages_sent_total().add(
//!     1,
//!     &[
//!         KeyValue::new("queue", "orchestration_task_requests"),
//!         KeyValue::new("message_type", "TaskRequestMessage"),
//!     ],
//! );
//!
//! message_send_duration().record(
//!     duration_ms,
//!     &[KeyValue::new("queue", "orchestration_task_requests")],
//! );
//! ```

use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter};
use std::sync::OnceLock;

/// Lazy-initialized meter for messaging metrics
static MESSAGING_METER: OnceLock<Meter> = OnceLock::new();

/// Get or initialize the messaging meter
fn meter() -> &'static Meter {
    MESSAGING_METER
        .get_or_init(|| opentelemetry::global::meter_provider().meter("tasker-messaging"))
}

// Counters

/// Total number of messages sent to queues
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
/// - message_type: TaskRequestMessage, StepExecutionResult, SimpleStepMessage
/// - correlation_id: Request correlation ID
pub fn messages_sent_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.messages.sent.total")
        .with_description("Total number of messages sent to queues")
        .build()
}

/// Total number of messages received from queues
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
/// - message_type: TaskRequestMessage, StepExecutionResult, SimpleStepMessage
/// - correlation_id: Request correlation ID
pub fn messages_received_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.messages.received.total")
        .with_description("Total number of messages received from queues")
        .build()
}

/// Total number of messages archived after processing
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
pub fn messages_archived_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.messages.archived.total")
        .with_description("Total number of messages archived after processing")
        .build()
}

/// Total number of message send failures
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
/// - error_type: queue_not_found, serialization_error, connection_error
pub fn message_send_failures_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.messages.send_failures.total")
        .with_description("Total number of message send failures")
        .build()
}

/// Total number of message receive failures
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
/// - error_type: deserialization_error, connection_error
pub fn message_receive_failures_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.messages.receive_failures.total")
        .with_description("Total number of message receive failures")
        .build()
}

// Histograms

/// Message end-to-end latency in milliseconds
///
/// Tracks time from message creation to processing completion.
/// Calculated as: processing_time - message_created_at
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
/// - message_type: TaskRequestMessage, StepExecutionResult, SimpleStepMessage
/// - correlation_id: Request correlation ID
pub fn message_latency() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.message.latency")
        .with_description("Message end-to-end latency in milliseconds")
        .with_unit("ms")
        .build()
}

/// Message send operation duration in milliseconds
///
/// Tracks time for pgmq_send operation to complete.
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
pub fn message_send_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.message.send.duration")
        .with_description("Message send operation duration in milliseconds")
        .with_unit("ms")
        .build()
}

/// Message receive operation duration in milliseconds
///
/// Tracks time for pgmq_read operation to complete.
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
pub fn message_receive_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.message.receive.duration")
        .with_description("Message receive operation duration in milliseconds")
        .with_unit("ms")
        .build()
}

/// Message archive operation duration in milliseconds
///
/// Tracks time for pgmq_archive operation to complete.
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
pub fn message_archive_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.message.archive.duration")
        .with_description("Message archive operation duration in milliseconds")
        .with_unit("ms")
        .build()
}

// Gauges

/// Current queue depth (number of unprocessed messages)
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
pub fn queue_depth() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.queue.depth")
        .with_description("Current queue depth (number of unprocessed messages)")
        .build()
}

/// Age of oldest message in queue (milliseconds)
///
/// Labels:
/// - queue: orchestration_task_requests, orchestration_step_results, {namespace}_queue
pub fn queue_oldest_message_age() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.queue.oldest_message.age")
        .with_description("Age of oldest message in queue (milliseconds)")
        .with_unit("ms")
        .build()
}

// Static instances for convenience

/// Static counter: messages_sent_total
pub static MESSAGES_SENT_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: messages_received_total
pub static MESSAGES_RECEIVED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: messages_archived_total
pub static MESSAGES_ARCHIVED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: message_send_failures_total
pub static MESSAGE_SEND_FAILURES_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: message_receive_failures_total
pub static MESSAGE_RECEIVE_FAILURES_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static histogram: message_latency
pub static MESSAGE_LATENCY: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: message_send_duration
pub static MESSAGE_SEND_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: message_receive_duration
pub static MESSAGE_RECEIVE_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: message_archive_duration
pub static MESSAGE_ARCHIVE_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static gauge: queue_depth
pub static QUEUE_DEPTH: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static gauge: queue_oldest_message_age
pub static QUEUE_OLDEST_MESSAGE_AGE: OnceLock<Gauge<u64>> = OnceLock::new();

/// Initialize all messaging metrics
///
/// This should be called during application startup after init_metrics().
pub fn init() {
    MESSAGES_SENT_TOTAL.get_or_init(messages_sent_total);
    MESSAGES_RECEIVED_TOTAL.get_or_init(messages_received_total);
    MESSAGES_ARCHIVED_TOTAL.get_or_init(messages_archived_total);
    MESSAGE_SEND_FAILURES_TOTAL.get_or_init(message_send_failures_total);
    MESSAGE_RECEIVE_FAILURES_TOTAL.get_or_init(message_receive_failures_total);
    MESSAGE_LATENCY.get_or_init(message_latency);
    MESSAGE_SEND_DURATION.get_or_init(message_send_duration);
    MESSAGE_RECEIVE_DURATION.get_or_init(message_receive_duration);
    MESSAGE_ARCHIVE_DURATION.get_or_init(message_archive_duration);
    QUEUE_DEPTH.get_or_init(queue_depth);
    QUEUE_OLDEST_MESSAGE_AGE.get_or_init(queue_oldest_message_age);
}
