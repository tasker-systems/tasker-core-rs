//! # Ruby FFI Bridge with System Handle Management
//!
//! Provides lifecycle management for the worker system with status checking
//! and graceful shutdown capabilities, following patterns from embedded mode.

use crate::bootstrap::{
    bootstrap_worker, get_worker_status, stop_worker, transition_to_graceful_shutdown,
};
use crate::conversions::convert_step_execution_event_to_ruby;
use crate::event_handler::{send_step_completion_event, RubyEventHandler};
use crate::ffi_logging::{log_debug, log_error, log_info, log_trace, log_warn};
use magnus::{function, prelude::*, Error, RModule, Value};
use std::sync::{Arc, Mutex};
use tasker_shared::{errors::TaskerResult, types::StepExecutionEvent};
use tasker_worker::{WorkerSystemHandle, WorkerSystemStatus};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Global handle to the worker system for Ruby FFI
pub static WORKER_SYSTEM: Mutex<Option<RubyBridgeHandle>> = Mutex::new(None);

/// Bridge handle that maintains worker system state
pub struct RubyBridgeHandle {
    /// Handle from tasker-worker bootstrap
    pub system_handle: WorkerSystemHandle,
    /// Ruby event handler for forwarding events
    pub event_handler: Arc<RubyEventHandler>,
    /// Event receiver for polling step execution events
    pub event_receiver: Arc<Mutex<mpsc::UnboundedReceiver<StepExecutionEvent>>>,
    /// Keep runtime alive
    #[allow(dead_code)]
    pub runtime: tokio::runtime::Runtime,
}

impl RubyBridgeHandle {
    pub fn new(
        system_handle: WorkerSystemHandle,
        event_handler: Arc<RubyEventHandler>,
        event_receiver: mpsc::UnboundedReceiver<StepExecutionEvent>,
        runtime: tokio::runtime::Runtime,
    ) -> Self {
        Self {
            system_handle,
            event_handler,
            event_receiver: Arc::new(Mutex::new(event_receiver)),
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

    pub fn event_handler(&self) -> &Arc<RubyEventHandler> {
        &self.event_handler
    }

    /// Poll for the next step execution event
    /// Returns None if no events are available (non-blocking)
    pub fn poll_step_event(&self) -> Option<StepExecutionEvent> {
        let mut receiver = self.event_receiver.lock().ok()?;
        receiver.try_recv().ok()
    }
}

/// FFI function for Ruby to poll for step execution events
/// Returns a Ruby hash representation of the event or nil if none available
pub fn poll_step_events() -> Result<Value, Error> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        Error::new(
            magnus::exception::runtime_error(),
            "Worker system not running",
        )
    })?;

    // Poll for next event
    match handle.poll_step_event() {
        Some(event) => {
            debug!(
                event_id = %event.event_id,
                step_name = %event.payload.task_sequence_step.workflow_step.name,
                "Polled step execution event for Ruby processing"
            );

            // Convert the event to Ruby format
            let ruby_event = convert_step_execution_event_to_ruby(event).map_err(|e| {
                error!("Failed to convert event to Ruby: {}", e);
                Error::new(
                    magnus::exception::runtime_error(),
                    format!("Failed to convert event to Ruby: {}", e),
                )
            })?;

            Ok(ruby_event.as_value())
        }
        None => {
            // Return nil when no events are available
            let ruby = magnus::Ruby::get().map_err(|err| {
                Error::new(
                    magnus::exception::runtime_error(),
                    format!("Failed to get ruby system: {}", err),
                )
            })?;
            Ok(ruby.qnil().as_value())
        }
    }
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

    // Event handling
    module.define_singleton_method(
        "send_step_completion_event",
        function!(send_step_completion_event, 1),
    )?;
    module.define_singleton_method("poll_step_events", function!(poll_step_events, 0))?;

    // TAS-29 Phase 6: Unified structured logging via FFI
    module.define_singleton_method("log_error", function!(log_error, 2))?;
    module.define_singleton_method("log_warn", function!(log_warn, 2))?;
    module.define_singleton_method("log_info", function!(log_info, 2))?;
    module.define_singleton_method("log_debug", function!(log_debug, 2))?;
    module.define_singleton_method("log_trace", function!(log_trace, 2))?;

    info!("✅ Ruby FFI bridge initialized");
    Ok(())
}
