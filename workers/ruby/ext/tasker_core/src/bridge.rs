//! # Ruby FFI Bridge with System Handle Management
//!
//! Provides lifecycle management for the worker system with status checking
//! and graceful shutdown capabilities, following patterns from embedded mode.

use crate::bootstrap::{
    bootstrap_worker, get_worker_status, stop_worker, transition_to_graceful_shutdown,
};
use crate::event_handler::{send_step_completion_event, RubyEventHandler};
use magnus::{function, prelude::*, Error, RModule};
use std::sync::{Arc, Mutex};
use tasker_shared::errors::TaskerResult;
use tasker_worker::{WorkerSystemHandle, WorkerSystemStatus};
use tracing::info;

/// Global handle to the worker system for Ruby FFI
pub static WORKER_SYSTEM: Mutex<Option<RubyBridgeHandle>> = Mutex::new(None);

/// Bridge handle that maintains worker system state
pub struct RubyBridgeHandle {
    /// Handle from tasker-worker bootstrap
    pub system_handle: WorkerSystemHandle,
    /// Ruby event handler for forwarding events
    pub event_handler: Arc<RubyEventHandler>,
    /// Keep runtime alive
    #[allow(dead_code)]
    pub runtime: tokio::runtime::Runtime,
}

impl RubyBridgeHandle {
    pub fn new(
        system_handle: WorkerSystemHandle,
        event_handler: Arc<RubyEventHandler>,
        runtime: tokio::runtime::Runtime,
    ) -> Self {
        Self {
            system_handle,
            event_handler,
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

    info!("✅ Ruby FFI bridge initialized");
    Ok(())
}
