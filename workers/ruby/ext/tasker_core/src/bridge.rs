//! # Ruby FFI Bridge with System Handle Management
//!
//! Provides lifecycle management for the worker system with status checking
//! and graceful shutdown capabilities, following patterns from embedded mode.

use crate::bootstrap::{
    bootstrap_worker, get_worker_status, stop_worker, transition_to_graceful_shutdown,
};
use crate::conversions::{
    convert_ffi_step_event_to_ruby, convert_ruby_checkpoint_to_yield_data,
    convert_ruby_completion_to_step_result,
};
use crate::ffi_logging::{log_debug, log_error, log_info, log_trace, log_warn};
use magnus::{function, prelude::*, Error, ExceptionClass, RModule, Ruby, Value};
use std::sync::{Arc, Mutex};

/// Helper to get RuntimeError exception class (magnus 0.8 API)
fn runtime_error_class() -> ExceptionClass {
    Ruby::get()
        .expect("Ruby runtime should be available")
        .exception_runtime_error()
}

/// Helper to get ArgumentError exception class (magnus 0.8 API)
fn arg_error_class() -> ExceptionClass {
    Ruby::get()
        .expect("Ruby runtime should be available")
        .exception_arg_error()
}
use tasker_shared::errors::TaskerResult;
use tasker_shared::events::domain_events::DomainEvent;
use tasker_worker::worker::{FfiDispatchChannel, FfiDispatchMetrics, FfiStepEvent};
use tasker_worker::{WorkerSystemHandle, WorkerSystemStatus};
use tokio::sync::broadcast;
use tracing::{debug, error, info};
use uuid::Uuid;

/// Global handle to the worker system for Ruby FFI
pub static WORKER_SYSTEM: Mutex<Option<RubyBridgeHandle>> = Mutex::new(None);

/// Bridge handle that maintains worker system state
pub struct RubyBridgeHandle {
    /// Handle from tasker-worker bootstrap
    pub system_handle: WorkerSystemHandle,
    /// TAS-67: FFI dispatch channel for step execution events
    /// Ruby polls this to receive step events and submits completions
    pub ffi_dispatch_channel: Arc<FfiDispatchChannel>,
    /// TAS-65: Domain event publisher for Ruby handlers
    pub domain_event_publisher: Arc<tasker_shared::events::domain_events::DomainEventPublisher>,
    /// TAS-65 Phase 4.1: In-process event receiver for fast domain events
    /// Ruby can poll this channel to receive domain events with delivery_mode: fast
    pub in_process_event_receiver: Option<Arc<Mutex<broadcast::Receiver<DomainEvent>>>>,
    /// Keep runtime alive
    #[expect(dead_code, reason = "Runtime kept alive for async FFI task lifetime management")]
    pub runtime: tokio::runtime::Runtime,
}

impl RubyBridgeHandle {
    pub fn new(
        system_handle: WorkerSystemHandle,
        ffi_dispatch_channel: Arc<FfiDispatchChannel>,
        domain_event_publisher: Arc<tasker_shared::events::domain_events::DomainEventPublisher>,
        in_process_event_receiver: Option<broadcast::Receiver<DomainEvent>>,
        runtime: tokio::runtime::Runtime,
    ) -> Self {
        Self {
            system_handle,
            ffi_dispatch_channel,
            domain_event_publisher,
            in_process_event_receiver: in_process_event_receiver.map(|r| Arc::new(Mutex::new(r))),
            runtime,
        }
    }

    pub async fn status(&self) -> TaskerResult<WorkerSystemStatus> {
        self.system_handle.status().await
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.system_handle.stop().map_err(|e| e.to_string())
    }

    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        &self.system_handle.runtime_handle
    }

    /// TAS-67: Poll for the next step execution event via FfiDispatchChannel
    /// Returns None if no events are available (non-blocking)
    pub fn poll_step_event(&self) -> Option<FfiStepEvent> {
        self.ffi_dispatch_channel.poll()
    }

    /// TAS-67: Submit a completion result for an event
    /// Returns true if the completion was successfully submitted
    pub fn complete_step_event(
        &self,
        event_id: Uuid,
        result: tasker_shared::messaging::StepExecutionResult,
    ) -> bool {
        self.ffi_dispatch_channel.complete(event_id, result)
    }

    /// TAS-125: Submit a checkpoint yield for a batch processing step
    /// Returns true if the checkpoint was persisted and step re-dispatched
    pub fn checkpoint_yield_step_event(
        &self,
        event_id: Uuid,
        checkpoint_data: tasker_shared::models::batch_worker::CheckpointYieldData,
    ) -> bool {
        self.ffi_dispatch_channel
            .checkpoint_yield(event_id, checkpoint_data)
    }

    /// TAS-67 Phase 2: Get metrics about FFI dispatch channel health
    pub fn get_ffi_dispatch_metrics(&self) -> FfiDispatchMetrics {
        self.ffi_dispatch_channel.metrics()
    }

    /// TAS-67 Phase 2: Check for starvation warnings and emit logs
    pub fn check_starvation_warnings(&self) {
        self.ffi_dispatch_channel.check_starvation_warnings()
    }
}

/// TAS-67: FFI function for Ruby to poll for step execution events
/// Returns a Ruby hash representation of the FfiStepEvent or nil if none available
pub fn poll_step_events() -> Result<Value, Error> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or_else(|| Error::new(runtime_error_class(), "Worker system not running"))?;

    // Poll for next event via FfiDispatchChannel
    if let Some(event) = handle.poll_step_event() {
        debug!(
            event_id = %event.event_id,
            step_uuid = %event.step_uuid,
            "Polled FFI step event for Ruby processing"
        );

        // Convert the FfiStepEvent to Ruby format
        let ruby_event = convert_ffi_step_event_to_ruby(&event).map_err(|e| {
            error!("Failed to convert event to Ruby: {}", e);
            Error::new(
                runtime_error_class(),
                format!("Failed to convert event to Ruby: {}", e),
            )
        })?;

        Ok(ruby_event.as_value())
    } else {
        // Return nil when no events are available
        let ruby = magnus::Ruby::get().map_err(|err| {
            Error::new(
                runtime_error_class(),
                format!("Failed to get ruby system: {}", err),
            )
        })?;
        Ok(ruby.qnil().as_value())
    }
}

/// TAS-67: FFI function for Ruby to submit step completion
/// Called after a handler has completed execution
///
/// # Arguments
/// * `event_id` - The event ID string from the FfiStepEvent
/// * `completion_data` - Ruby hash with completion result data
pub fn complete_step_event(event_id_str: String, completion_data: Value) -> Result<bool, Error> {
    // Parse event_id
    let event_id = Uuid::parse_str(&event_id_str).map_err(|e| {
        error!("Invalid event_id format: {}", e);
        Error::new(arg_error_class(), format!("Invalid event_id format: {}", e))
    })?;

    // Convert Ruby completion to StepExecutionResult
    let result = convert_ruby_completion_to_step_result(completion_data).map_err(|e| {
        error!("Failed to convert completion data: {}", e);
        Error::new(
            runtime_error_class(),
            format!("Failed to convert completion data: {}", e),
        )
    })?;

    // Get bridge handle and submit completion
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or_else(|| Error::new(runtime_error_class(), "Worker system not running"))?;

    // Submit completion via FfiDispatchChannel
    let success = handle.complete_step_event(event_id, result);

    if success {
        debug!(event_id = %event_id, "Step completion submitted successfully");
    } else {
        error!(event_id = %event_id, "Failed to submit step completion");
    }

    Ok(success)
}

/// TAS-125: FFI function for Ruby to submit a checkpoint yield
///
/// Called from batch processing handlers when they want to persist progress
/// and be re-dispatched for continuation. Unlike complete_step_event, this
/// does NOT complete the step - instead it persists checkpoint data and
/// re-dispatches the step for continued processing.
///
/// # Arguments
/// * `event_id` - The event ID string from the FfiStepEvent
/// * `checkpoint_data` - Ruby hash with checkpoint data:
///   - step_uuid: String (UUID of the step)
///   - cursor: Any JSON value (position to resume from)
///   - items_processed: Integer (count of items processed so far)
///   - accumulated_results: Optional JSON value (partial results)
///
/// # Returns
/// `true` if the checkpoint was persisted and step re-dispatched,
/// `false` if checkpoint support is not configured or an error occurred.
pub fn checkpoint_yield_step_event(
    event_id_str: String,
    checkpoint_data: Value,
) -> Result<bool, Error> {
    // Parse event_id
    let event_id = Uuid::parse_str(&event_id_str).map_err(|e| {
        error!("Invalid event_id format: {}", e);
        Error::new(arg_error_class(), format!("Invalid event_id format: {}", e))
    })?;

    // Convert Ruby checkpoint data to CheckpointYieldData
    let checkpoint = convert_ruby_checkpoint_to_yield_data(checkpoint_data).map_err(|e| {
        error!("Failed to convert checkpoint data: {}", e);
        Error::new(
            runtime_error_class(),
            format!("Failed to convert checkpoint data: {}", e),
        )
    })?;

    // Get bridge handle and submit checkpoint yield
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or_else(|| Error::new(runtime_error_class(), "Worker system not running"))?;

    // Submit checkpoint yield via FfiDispatchChannel
    let success = handle.checkpoint_yield_step_event(event_id, checkpoint);

    if success {
        info!(
            event_id = %event_id,
            "Checkpoint yield submitted and step re-dispatched"
        );
    } else {
        error!(
            event_id = %event_id,
            "Failed to submit checkpoint yield (checkpoint support may not be configured)"
        );
    }

    Ok(success)
}

/// TAS-67 Phase 2: FFI function for Ruby to get FFI dispatch channel metrics
/// Returns a Ruby hash with metrics for monitoring and observability
///
/// The returned hash contains:
/// - `pending_count`: Number of events waiting for completion
/// - `oldest_pending_age_ms`: Age of the oldest pending event in milliseconds
/// - `newest_pending_age_ms`: Age of the newest pending event in milliseconds
/// - `oldest_event_id`: UUID of the oldest pending event (for debugging)
/// - `starvation_detected`: Boolean indicating if any events exceed starvation threshold
/// - `starving_event_count`: Number of events exceeding starvation threshold
pub fn get_ffi_dispatch_metrics() -> Result<Value, Error> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or_else(|| Error::new(runtime_error_class(), "Worker system not running"))?;

    let metrics = handle.get_ffi_dispatch_metrics();

    let ruby = magnus::Ruby::get().map_err(|err| {
        Error::new(
            runtime_error_class(),
            format!("Failed to get ruby system: {}", err),
        )
    })?;

    let hash = ruby.hash_new();
    hash.aset("pending_count", metrics.pending_count)?;
    hash.aset(
        "oldest_pending_age_ms",
        metrics.oldest_pending_age_ms.map(|v| v as i64),
    )?;
    hash.aset(
        "newest_pending_age_ms",
        metrics.newest_pending_age_ms.map(|v| v as i64),
    )?;
    hash.aset(
        "oldest_event_id",
        metrics.oldest_event_id.map(|id| id.to_string()),
    )?;
    hash.aset("starvation_detected", metrics.starvation_detected)?;
    hash.aset("starving_event_count", metrics.starving_event_count)?;

    Ok(hash.as_value())
}

/// TAS-67 Phase 2: FFI function for Ruby to check for starvation warnings
/// This emits warning logs for any pending events that exceed the starvation threshold.
/// Call this periodically (e.g., every poll cycle) for proactive monitoring.
pub fn check_starvation_warnings() -> Result<bool, Error> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or_else(|| Error::new(runtime_error_class(), "Worker system not running"))?;

    handle.check_starvation_warnings();
    Ok(true)
}

/// Initialize the bridge module with all FFI functions
pub fn init_bridge(module: &RModule) -> Result<(), Error> {
    info!("🔌 Initializing Ruby FFI bridge");

    // Bootstrap and lifecycle
    module.define_singleton_method("bootstrap_worker", function!(bootstrap_worker, 0))?;
    module.define_singleton_method("stop_worker", function!(stop_worker, 0))?;
    module.define_singleton_method("worker_status", function!(get_worker_status, 0))?;
    module.define_singleton_method(
        "transition_to_graceful_shutdown",
        function!(transition_to_graceful_shutdown, 0),
    )?;

    // TAS-67: Event handling via FfiDispatchChannel
    module.define_singleton_method("poll_step_events", function!(poll_step_events, 0))?;
    module.define_singleton_method("complete_step_event", function!(complete_step_event, 2))?;
    // TAS-125: Checkpoint yield for batch processing handlers
    module.define_singleton_method(
        "checkpoint_yield_step_event",
        function!(checkpoint_yield_step_event, 2),
    )?;

    // TAS-67 Phase 2: Observability and metrics
    module.define_singleton_method(
        "get_ffi_dispatch_metrics",
        function!(get_ffi_dispatch_metrics, 0),
    )?;
    module.define_singleton_method(
        "check_starvation_warnings",
        function!(check_starvation_warnings, 0),
    )?;

    // TAS-29 Phase 6: Unified structured logging via FFI
    module.define_singleton_method("log_error", function!(log_error, 2))?;
    module.define_singleton_method("log_warn", function!(log_warn, 2))?;
    module.define_singleton_method("log_info", function!(log_info, 2))?;
    module.define_singleton_method("log_debug", function!(log_debug, 2))?;
    module.define_singleton_method("log_trace", function!(log_trace, 2))?;

    // TAS-65 Phase 2.4a: Domain event publishing (durable path)
    crate::event_publisher_ffi::init_event_publisher_ffi(module)?;

    // TAS-65 Phase 4.1: In-process event polling (fast path)
    crate::in_process_event_ffi::init_in_process_event_ffi(module)?;

    // TAS-77: Observability services FFI (health, metrics, templates, config)
    crate::observability_ffi::init_observability_ffi(module)?;

    info!("✅ Ruby FFI bridge initialized");
    Ok(())
}
