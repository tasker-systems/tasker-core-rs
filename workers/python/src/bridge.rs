//! # Python FFI Bridge with System Handle Management
//!
//! Provides lifecycle management for the worker system with status checking
//! and graceful shutdown capabilities, adapted from the Ruby FFI patterns.
//!
//! ## Architecture
//!
//! ```text
//! Python Application
//!     ↓
//! tasker_core.bootstrap_worker()
//!     ↓
//! PythonBridgeHandle (global singleton)
//!     ↓
//! WorkerSystemHandle → FfiDispatchChannel
//!     │                       │
//!     ▼                       ▼
//! Orchestration          poll_step_events()
//!                        complete_step_event()
//! ```

use crate::error::{PythonFfiError, PythonFfiResult};
use std::sync::{Arc, Mutex};
use tasker_shared::errors::TaskerResult;
use tasker_shared::events::domain_events::DomainEvent;
use tasker_worker::worker::{FfiDispatchChannel, FfiDispatchMetrics, FfiStepEvent};
use tasker_worker::{WorkerSystemHandle, WorkerSystemStatus};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Global handle to the worker system for Python FFI
///
/// This follows the same singleton pattern as the Ruby worker.
/// The Mutex ensures thread-safe access across Python threads.
pub static WORKER_SYSTEM: Mutex<Option<PythonBridgeHandle>> = Mutex::new(None);

/// Bridge handle that maintains worker system state for Python FFI
///
/// This struct holds all the components needed to interact with the
/// tasker-worker system from Python code.
pub struct PythonBridgeHandle {
    /// Handle from tasker-worker bootstrap
    pub system_handle: WorkerSystemHandle,

    /// FFI dispatch channel for step execution events
    /// Python polls this to receive step events and submits completions
    pub ffi_dispatch_channel: Arc<FfiDispatchChannel>,

    /// Domain event publisher for Python handlers
    pub domain_event_publisher: Arc<tasker_shared::events::domain_events::DomainEventPublisher>,

    /// In-process event receiver for fast domain events
    /// Python can poll this channel to receive domain events with delivery_mode: fast
    pub in_process_event_receiver: Option<Arc<Mutex<broadcast::Receiver<DomainEvent>>>>,

    /// Tokio runtime - kept alive for async operations
    #[allow(dead_code)]
    pub runtime: tokio::runtime::Runtime,
}

impl PythonBridgeHandle {
    /// Create a new Python bridge handle
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

    /// Get the current worker system status
    pub async fn status(&self) -> TaskerResult<WorkerSystemStatus> {
        self.system_handle.status().await
    }

    /// Stop the worker system
    pub fn stop(&mut self) -> Result<(), String> {
        self.system_handle.stop().map_err(|e| e.to_string())
    }

    /// Get a reference to the tokio runtime handle
    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        &self.system_handle.runtime_handle
    }

    /// Poll for the next step execution event via FfiDispatchChannel
    ///
    /// Returns None if no events are available (non-blocking)
    pub fn poll_step_event(&self) -> Option<FfiStepEvent> {
        self.ffi_dispatch_channel.poll()
    }

    /// Submit a completion result for an event
    ///
    /// Returns true if the completion was successfully submitted
    pub fn complete_step_event(
        &self,
        event_id: Uuid,
        result: tasker_shared::messaging::StepExecutionResult,
    ) -> bool {
        self.ffi_dispatch_channel.complete(event_id, result)
    }

    /// Get metrics about FFI dispatch channel health
    pub fn get_ffi_dispatch_metrics(&self) -> FfiDispatchMetrics {
        self.ffi_dispatch_channel.metrics()
    }

    /// Check for starvation warnings and emit logs
    pub fn check_starvation_warnings(&self) {
        self.ffi_dispatch_channel.check_starvation_warnings()
    }
}

/// Helper to acquire the worker system lock
pub fn with_worker_system<F, T>(f: F) -> PythonFfiResult<T>
where
    F: FnOnce(&PythonBridgeHandle) -> PythonFfiResult<T>,
{
    let handle_guard = WORKER_SYSTEM
        .lock()
        .map_err(|e| PythonFfiError::LockError(e.to_string()))?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    f(handle)
}

/// Helper to acquire the worker system lock mutably
pub fn with_worker_system_mut<F, T>(f: F) -> PythonFfiResult<T>
where
    F: FnOnce(&mut PythonBridgeHandle) -> PythonFfiResult<T>,
{
    let mut handle_guard = WORKER_SYSTEM
        .lock()
        .map_err(|e| PythonFfiError::LockError(e.to_string()))?;

    let handle = handle_guard
        .as_mut()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    f(handle)
}
