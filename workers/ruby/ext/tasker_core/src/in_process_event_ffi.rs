//! # TAS-65 Phase 4.1: Ruby FFI Bindings for In-Process Domain Events
//!
//! Exposes the fast in-process event bus to Ruby for subscribing to domain events.
//! Ruby handlers can receive domain events with `delivery_mode: fast` for internal
//! processing like metrics, notifications, and logging integrations.
//!
//! ## Architecture
//!
//! ```text
//! EventRouter
//!     ↓ (delivery_mode: fast)
//! InProcessEventBus
//!     ↓
//! Broadcast Channel
//!     ↓
//! Ruby FFI poll_in_process_events()
//!     ↓
//! Ruby handlers (Sentry, DataDog, Slack, etc.)
//! ```
//!
//! ## Usage
//!
//! ```ruby
//! # Poll for fast domain events
//! loop do
//!   events = TaskerCore::FFI.poll_in_process_events(10)
//!   break if events.empty?
//!
//!   events.each do |event|
//!     puts "Received: #{event[:event_name]}"
//!     # Forward to integration (Sentry, DataDog, etc.)
//!   end
//! end
//! ```

use crate::bridge::WORKER_SYSTEM;
use chrono::{DateTime, Utc};
use magnus::{function, prelude::*, Error as MagnusError, RHash, RModule, Ruby, Value as RValue};
use tasker_shared::events::domain_events::DomainEvent;
use tokio::sync::broadcast;
use tracing::{debug, error, trace, warn};

/// Maximum events to return in a single poll (safety limit)
const MAX_POLL_BATCH_SIZE: i64 = 100;

/// Convert a DomainEvent to a Ruby hash
///
/// Transforms the Rust DomainEvent structure into a Ruby hash with all
/// relevant fields for Ruby-side processing.
fn domain_event_to_ruby_hash(ruby: &Ruby, event: &DomainEvent) -> Result<RHash, MagnusError> {
    let hash = ruby.hash_new();

    // Core event fields
    hash.aset("event_id", event.event_id.to_string())?;
    hash.aset("event_name", event.event_name.clone())?;
    hash.aset("event_version", event.event_version.clone())?;

    // Metadata
    let metadata = ruby.hash_new();
    metadata.aset("task_uuid", event.metadata.task_uuid.to_string())?;
    metadata.aset(
        "step_uuid",
        event.metadata.step_uuid.map(|u| u.to_string()),
    )?;
    metadata.aset("step_name", event.metadata.step_name.clone())?;
    metadata.aset("namespace", event.metadata.namespace.clone())?;
    metadata.aset("correlation_id", event.metadata.correlation_id.to_string())?;
    metadata.aset("fired_at", format_datetime(&event.metadata.fired_at))?;
    metadata.aset("fired_by", event.metadata.fired_by.clone())?;
    hash.aset("metadata", metadata)?;

    // Payload - business-specific data as JSON string for Ruby parsing
    // Using JSON string because Ruby can easily parse it with JSON.parse
    let payload_json = serde_json::to_string(&event.payload.payload).map_err(|e| {
        MagnusError::new(
            magnus::exception::runtime_error(),
            format!("Failed to serialize payload: {}", e),
        )
    })?;
    hash.aset("business_payload", payload_json)?;

    // Execution result summary (most commonly needed fields)
    let execution = ruby.hash_new();
    execution.aset("success", event.payload.execution_result.success)?;
    execution.aset("status", event.payload.execution_result.status.clone())?;
    execution.aset(
        "step_uuid",
        event.payload.execution_result.step_uuid.to_string(),
    )?;
    if let Some(ref error) = event.payload.execution_result.error {
        // Convert error to JSON string for Ruby consumption
        let error_json = serde_json::to_string(error).map_err(|e| {
            MagnusError::new(
                magnus::exception::runtime_error(),
                format!("Failed to serialize error: {}", e),
            )
        })?;
        execution.aset("error", error_json)?;
    }
    hash.aset("execution_result", execution)?;

    // Task/step context summary
    let context = ruby.hash_new();
    context.aset(
        "task_uuid",
        event
            .payload
            .task_sequence_step
            .task
            .task
            .task_uuid
            .to_string(),
    )?;
    context.aset(
        "task_name",
        event.payload.task_sequence_step.task.task_name.clone(),
    )?;
    context.aset(
        "namespace",
        event.payload.task_sequence_step.task.namespace_name.clone(),
    )?;
    context.aset(
        "step_name",
        event.payload.task_sequence_step.workflow_step.name.clone(),
    )?;
    hash.aset("context", context)?;

    Ok(hash)
}

/// Format datetime for Ruby
fn format_datetime(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

/// FFI function to poll for in-process domain events
///
/// Non-blocking poll that returns up to `max_events` domain events from the
/// fast in-process event bus. Returns empty array if no events are available.
///
/// # Arguments
///
/// * `max_events` - Maximum number of events to return (capped at 100)
///
/// # Returns
///
/// Array of Ruby hashes, each representing a domain event with:
/// - `event_id`: String UUID
/// - `event_name`: String (e.g., "payment.processed")
/// - `event_version`: String
/// - `metadata`: Hash with task_uuid, step_uuid, namespace, correlation_id, etc.
/// - `business_payload`: JSON string of business-specific data
/// - `execution_result`: Hash with success, status, step_uuid, error
/// - `context`: Hash with task_uuid, task_name, namespace, step_name
///
/// # Ruby Example
///
/// ```ruby
/// # Poll up to 10 events
/// events = TaskerCore::FFI.poll_in_process_events(10)
///
/// events.each do |event|
///   puts "Event: #{event[:event_name]}"
///   payload = JSON.parse(event[:business_payload])
///   # Process event...
/// end
/// ```
pub fn poll_in_process_events(max_events: i64) -> Result<RValue, MagnusError> {
    let ruby = Ruby::get().map_err(|e| {
        MagnusError::new(
            magnus::exception::runtime_error(),
            format!("Failed to get Ruby: {}", e),
        )
    })?;

    // Cap max_events for safety (1 to MAX_POLL_BATCH_SIZE)
    let max_events = max_events.clamp(1, MAX_POLL_BATCH_SIZE) as usize;

    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        MagnusError::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        MagnusError::new(
            magnus::exception::runtime_error(),
            "Worker system not running - call bootstrap_worker first",
        )
    })?;

    // Get the FFI receiver
    let ffi_receiver_guard = handle.in_process_event_receiver.as_ref().ok_or_else(|| {
        MagnusError::new(
            magnus::exception::runtime_error(),
            "In-process event bus not initialized",
        )
    })?;

    let mut receiver = ffi_receiver_guard.lock().map_err(|e| {
        error!("Failed to acquire event receiver lock: {}", e);
        MagnusError::new(
            magnus::exception::runtime_error(),
            "Event receiver lock failed",
        )
    })?;

    // Collect events using try_recv (non-blocking)
    let events = ruby.ary_new();
    let mut received_count = 0;

    while received_count < max_events {
        match receiver.try_recv() {
            Ok(event) => {
                trace!(
                    event_id = %event.event_id,
                    event_name = %event.event_name,
                    "Polled in-process domain event for Ruby"
                );

                match domain_event_to_ruby_hash(&ruby, &event) {
                    Ok(hash) => {
                        events.push(hash)?;
                        received_count += 1;
                    }
                    Err(e) => {
                        warn!(
                            event_id = %event.event_id,
                            error = %e,
                            "Failed to convert domain event to Ruby hash - skipping"
                        );
                        // Continue processing other events
                    }
                }
            }
            Err(broadcast::error::TryRecvError::Empty) => {
                // No more events available
                break;
            }
            Err(broadcast::error::TryRecvError::Closed) => {
                warn!("In-process event channel closed");
                break;
            }
            Err(broadcast::error::TryRecvError::Lagged(count)) => {
                warn!(
                    lagged_count = count,
                    "In-process event receiver lagged - some events were dropped"
                );
                // Continue receiving remaining events
            }
        }
    }

    if received_count > 0 {
        debug!(
            count = received_count,
            "Polled in-process domain events for Ruby"
        );
    }

    Ok(events.as_value())
}

/// FFI function to get in-process event bus statistics
///
/// Returns statistics about the in-process event bus including subscriber
/// counts and dispatch metrics.
///
/// # Returns
///
/// Ruby hash with:
/// - `enabled`: Boolean - whether in-process events are enabled
/// - `ffi_subscriber_count`: Integer - number of FFI subscribers
/// - Additional stats when available
pub fn get_in_process_event_stats() -> Result<RValue, MagnusError> {
    let ruby = Ruby::get().map_err(|e| {
        MagnusError::new(
            magnus::exception::runtime_error(),
            format!("Failed to get Ruby: {}", e),
        )
    })?;

    let hash = ruby.hash_new();

    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        MagnusError::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    match handle_guard.as_ref() {
        Some(handle) => {
            hash.aset("enabled", handle.in_process_event_receiver.is_some())?;

            // If we have access to the event bus stats through the handle,
            // add them here. For now, just report enabled status.
            if handle.in_process_event_receiver.is_some() {
                hash.aset("status", "active")?;
            } else {
                hash.aset("status", "not_initialized")?;
            }
        }
        None => {
            hash.aset("enabled", false)?;
            hash.aset("status", "worker_not_running")?;
        }
    }

    Ok(hash.as_value())
}

/// Initialize the in-process event FFI module
pub fn init_in_process_event_ffi(module: &RModule) -> Result<(), MagnusError> {
    module.define_singleton_method(
        "poll_in_process_events",
        function!(poll_in_process_events, 1),
    )?;
    module.define_singleton_method(
        "in_process_event_stats",
        function!(get_in_process_event_stats, 0),
    )?;
    Ok(())
}
