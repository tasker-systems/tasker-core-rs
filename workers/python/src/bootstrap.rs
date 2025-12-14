//! # Python Worker Bootstrap
//!
//! Provides bootstrap and lifecycle management for the Python worker system.
//! Follows the same patterns as the Ruby worker but adapted for PyO3.
//!
//! ## Architecture
//!
//! ```text
//! WorkerBootstrap → WorkerSystemHandle → take_dispatch_handles()
//!                                              │
//!                                              ▼
//!                                    FfiDispatchChannel
//!                                              │
//!                             ┌────────────────┴────────────────┐
//!                             ▼                                 ▼
//!                     poll_step_events()              complete_step_event()
//!                        (Python FFI)                    (Python FFI)
//! ```

use crate::bridge::{PythonBridgeHandle, WORKER_SYSTEM};
use crate::error::PythonFfiError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tasker_worker::worker::{
    DomainEventCallback, FfiDispatchChannel, FfiDispatchChannelConfig, StepEventPublisherRegistry,
};
use tasker_worker::WorkerBootstrap;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

/// Bootstrap the worker system for Python
///
/// This initializes the full tasker-worker system and creates an FfiDispatchChannel
/// for step event dispatch. The worker ID is auto-generated.
///
/// Returns a dict with bootstrap status:
/// - `handle_id`: Unique identifier for this worker instance
/// - `status`: "started" or "already_running"
/// - `message`: Human-readable status message
/// - `worker_id`: Full worker identifier string
#[pyfunction]
#[pyo3(signature = (config=None))]
pub fn bootstrap_worker(py: Python<'_>, config: Option<&Bound<'_, PyDict>>) -> PyResult<PyObject> {
    let worker_id = Uuid::new_v4();
    let worker_id_str = format!("python-worker-{}", worker_id);

    // Extract config options if provided
    let _namespace = if let Some(cfg) = config {
        cfg.get_item("namespace")?
            .map(|v| v.extract::<String>())
            .transpose()?
            .unwrap_or_else(|| "default".to_string())
    } else {
        "default".to_string()
    };

    // Check if already running
    let mut handle_guard = WORKER_SYSTEM
        .lock()
        .map_err(|e| PythonFfiError::LockError(e.to_string()))?;

    if handle_guard.is_some() {
        // Return existing handle info
        let result = PyDict::new_bound(py);
        result.set_item("handle_id", worker_id.to_string())?;
        result.set_item("status", "already_running")?;
        result.set_item("message", "Worker system already running")?;
        return Ok(result.into());
    }

    // Create tokio runtime
    let runtime = tokio::runtime::Runtime::new().map_err(|e| {
        error!("Failed to create tokio runtime: {}", e);
        PythonFfiError::RuntimeError(format!("Runtime creation failed: {}", e))
    })?;

    // Initialize tracing in Tokio runtime context
    runtime.block_on(async {
        tasker_shared::logging::init_tracing();
    });

    // Bootstrap the worker using tasker-worker foundation
    let mut system_handle = runtime
        .block_on(async { WorkerBootstrap::bootstrap().await })
        .map_err(|e| {
            error!("Failed to bootstrap worker system: {}", e);
            PythonFfiError::BootstrapFailed(e.to_string())
        })?;

    info!("✅ Worker system bootstrapped successfully");

    // Create domain event callback for step completion
    info!("🔔 Setting up step event publisher registry for domain events...");
    let (domain_event_publisher, domain_event_callback) = runtime.block_on(async {
        let worker_core = system_handle.worker_core.lock().await;

        // Get the message client for durable events
        let message_client = worker_core.context.message_client.clone();
        let publisher = Arc::new(
            tasker_shared::events::domain_events::DomainEventPublisher::new(message_client),
        );

        // Get EventRouter from WorkerCore for stats tracking
        let event_router = worker_core
            .event_router()
            .expect("EventRouter should be available from WorkerCore");

        // Create registry with EventRouter for dual-path delivery (durable + fast)
        let step_event_registry =
            StepEventPublisherRegistry::with_event_router(publisher.clone(), event_router);

        let registry = Arc::new(RwLock::new(step_event_registry));
        let callback = Arc::new(DomainEventCallback::new(registry));

        (publisher, callback)
    });
    info!("✅ Domain event callback created with EventRouter for stats tracking");

    // Take dispatch handles and create FfiDispatchChannel with callback
    let ffi_dispatch_channel = if let Some(dispatch_handles) = system_handle.take_dispatch_handles()
    {
        info!("🔗 Creating FfiDispatchChannel from dispatch handles...");

        // Create config with runtime handle for executing async callbacks from FFI threads
        let config = FfiDispatchChannelConfig::new(runtime.handle().clone())
            .with_service_id(worker_id_str.clone())
            .with_completion_timeout(std::time::Duration::from_secs(30));

        let channel = FfiDispatchChannel::new(
            dispatch_handles.dispatch_receiver,
            dispatch_handles.completion_sender,
            config,
            domain_event_callback,
        );

        info!("✅ FfiDispatchChannel created with domain event callback for Python step dispatch");
        Arc::new(channel)
    } else {
        error!("Failed to get dispatch handles from WorkerSystemHandle");
        return Err(PythonFfiError::BootstrapFailed(
            "Dispatch handles not available".to_string(),
        )
        .into());
    };

    // Get in-process event receiver from WorkerCore's event bus
    info!("⚡ Subscribing to WorkerCore's in-process event bus for fast domain events...");
    let in_process_event_receiver = runtime.block_on(async {
        let worker_core = system_handle.worker_core.lock().await;
        let bus = worker_core.in_process_event_bus();
        let bus_guard = bus.write().await;
        bus_guard.subscribe_ffi()
    });
    info!("✅ Subscribed to WorkerCore's in-process event bus for FFI domain events");

    // Store the bridge handle with FfiDispatchChannel
    *handle_guard = Some(PythonBridgeHandle::new(
        system_handle,
        ffi_dispatch_channel,
        domain_event_publisher,
        Some(in_process_event_receiver),
        runtime,
    ));

    // Return handle info to Python
    let result = PyDict::new_bound(py);
    result.set_item("handle_id", worker_id.to_string())?;
    result.set_item("status", "started")?;
    result.set_item("message", "Python worker system started successfully")?;
    result.set_item("worker_id", worker_id_str)?;

    Ok(result.into())
}

/// Stop the worker system
///
/// Gracefully shuts down the worker system and releases all resources.
///
/// Returns:
/// - `"Worker system stopped"` on success
/// - `"Worker system not running"` if not initialized
#[pyfunction]
pub fn stop_worker() -> PyResult<String> {
    let mut handle_guard = WORKER_SYSTEM
        .lock()
        .map_err(|e| PythonFfiError::LockError(e.to_string()))?;

    match handle_guard.as_mut() {
        Some(handle) => {
            handle.stop().map_err(|e| {
                error!("Failed to stop worker system: {}", e);
                PythonFfiError::RuntimeError(e)
            })?;
            *handle_guard = None;
            Ok("Worker system stopped".to_string())
        }
        None => Ok("Worker system not running".to_string()),
    }
}

/// Get worker system status
///
/// Returns a dict with current worker status:
/// - `running`: Boolean indicating if the worker is running
/// - `environment`: Current environment (test, development, production)
/// - `worker_core_status`: Internal worker core status
/// - `web_api_enabled`: Whether the web API is enabled
/// - `supported_namespaces`: List of supported task namespaces
/// - `database_pool_size`: Database connection pool size
/// - `database_pool_idle`: Number of idle database connections
#[pyfunction]
pub fn get_worker_status(py: Python<'_>) -> PyResult<PyObject> {
    let handle_guard = WORKER_SYSTEM
        .lock()
        .map_err(|e| PythonFfiError::LockError(e.to_string()))?;

    let result = PyDict::new_bound(py);

    if let Some(handle) = handle_guard.as_ref() {
        let runtime = handle.runtime_handle();
        let status = runtime
            .block_on(async { handle.status().await })
            .map_err(|err| {
                error!("Failed to get status from runtime: {err}");
                PythonFfiError::RuntimeError(format!("Failed to get status: {}", err))
            })?;

        result.set_item("running", status.running)?;
        result.set_item("environment", status.environment)?;
        result.set_item(
            "worker_core_status",
            format!("{:?}", status.worker_core_status),
        )?;
        result.set_item("web_api_enabled", status.web_api_enabled)?;
        result.set_item("supported_namespaces", status.supported_namespaces)?;
        result.set_item("database_pool_size", status.database_pool_size)?;
        result.set_item("database_pool_idle", status.database_pool_idle)?;
    } else {
        result.set_item("running", false)?;
        result.set_item("error", "Worker system not initialized")?;
    }

    Ok(result.into())
}

/// Transition to graceful shutdown
///
/// Initiates a graceful shutdown of the worker system, allowing in-flight
/// operations to complete before stopping.
///
/// Returns a status message.
#[pyfunction]
pub fn transition_to_graceful_shutdown() -> PyResult<String> {
    let handle_guard = WORKER_SYSTEM
        .lock()
        .map_err(|e| PythonFfiError::LockError(e.to_string()))?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    let runtime = handle.runtime_handle();
    runtime.block_on(async {
        let mut worker_core = handle.system_handle.worker_core.lock().await;
        worker_core.stop().await.map_err(|e| {
            error!("Failed to transition to graceful shutdown: {}", e);
            PythonFfiError::RuntimeError(format!("Graceful shutdown failed: {}", e))
        })
    })?;

    Ok("Worker system transitioned to graceful shutdown".to_string())
}

/// Check if worker is running
///
/// Returns True if the worker system is currently initialized and running.
#[pyfunction]
pub fn is_worker_running() -> PyResult<bool> {
    let handle_guard = WORKER_SYSTEM
        .lock()
        .map_err(|e| PythonFfiError::LockError(e.to_string()))?;

    Ok(handle_guard.is_some())
}
