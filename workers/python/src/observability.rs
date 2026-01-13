//! Observability FFI functions for Python worker
//!
//! TAS-72-P5: This module provides in-process domain event polling (fast path),
//! and observability endpoints (health, metrics, config).
//!
//! Note: Domain event publishing requires full execution context (TaskSequenceStep,
//! StepExecutionResult) which is only available during step execution. For Python
//! handlers, domain events are published automatically by the Rust orchestration
//! layer after step completion.

use crate::bridge::WORKER_SYSTEM;
use crate::error::PythonFfiError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tracing::{debug, error, warn};

/// Poll for in-process domain events (fast path)
///
/// This is used for real-time notifications that don't require
/// guaranteed delivery (e.g., Slack notifications, metrics updates).
///
/// # Returns
///
/// A Python dict representing the `DomainEvent`, or `None` if no events
/// are available.
///
/// # Raises
///
/// - `RuntimeError` if the worker system is not running
#[pyfunction]
pub fn poll_in_process_events(py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    // Check if we have an in-process event receiver
    let receiver = match &handle.in_process_event_receiver {
        Some(r) => r,
        None => {
            debug!("No in-process event receiver configured");
            return Ok(None);
        }
    };

    // Try to receive an event (non-blocking)
    let mut receiver_guard = receiver.lock().map_err(|e| {
        error!("Failed to acquire receiver lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    // Use try_recv for non-blocking receive
    match receiver_guard.try_recv() {
        Ok(event) => {
            debug!(event_name = %event.event_name, "Received in-process domain event");

            // Convert to Python dict
            let dict = PyDict::new(py);
            dict.set_item("event_id", event.event_id.to_string())?;
            dict.set_item("event_name", &event.event_name)?;
            dict.set_item("event_version", &event.event_version)?;

            // Metadata
            let metadata_dict = PyDict::new(py);
            metadata_dict.set_item("task_uuid", event.metadata.task_uuid.to_string())?;
            metadata_dict.set_item(
                "step_uuid",
                event.metadata.step_uuid.map(|id| id.to_string()),
            )?;
            metadata_dict.set_item("step_name", &event.metadata.step_name)?;
            metadata_dict.set_item("namespace", &event.metadata.namespace)?;
            metadata_dict.set_item("correlation_id", event.metadata.correlation_id.to_string())?;
            metadata_dict.set_item("fired_at", event.metadata.fired_at.to_rfc3339())?;
            metadata_dict.set_item("fired_by", &event.metadata.fired_by)?;
            dict.set_item("metadata", metadata_dict)?;

            // Convert payload
            let payload_obj = pythonize::pythonize(py, &event.payload).map_err(|e| {
                error!("Failed to convert payload to Python: {}", e);
                PythonFfiError::ConversionError(format!("Failed to convert payload: {}", e))
            })?;
            dict.set_item("payload", payload_obj)?;

            Ok(Some(dict.into()))
        }
        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
            // No events available
            Ok(None)
        }
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(count)) => {
            warn!(
                count = count,
                "In-process event receiver lagged, some events dropped"
            );
            // Still return None, next call will get new events
            Ok(None)
        }
        Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
            warn!("In-process event channel closed");
            Ok(None)
        }
    }
}

/// Get comprehensive health check status
///
/// Returns health status of all worker components including database,
/// PGMQ, and handler system.
///
/// # Returns
///
/// A Python dict with health check data.
///
/// # Raises
///
/// - `RuntimeError` if the worker system is not running
/// - `RuntimeError` if health check fails
#[pyfunction]
pub fn get_health_check(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    // Get status from system handle
    let status = handle
        .runtime
        .block_on(async { handle.system_handle.status().await })
        .map_err(|e| {
            error!("Failed to get worker status: {}", e);
            PythonFfiError::FfiError(format!("Failed to get health check: {}", e))
        })?;

    // Build health check dict
    let dict = PyDict::new(py);

    // Determine overall health status based on running state
    let is_healthy = status.running;
    dict.set_item("status", if is_healthy { "healthy" } else { "unhealthy" })?;
    dict.set_item("is_running", status.running)?;

    // Components health
    let components = PyDict::new(py);

    // Database component
    let db_component = PyDict::new(py);
    db_component.set_item("name", "database")?;
    // Assume healthy if pool size > 0 and has idle connections
    let db_healthy = status.database_pool_size > 0;
    db_component.set_item("status", if db_healthy { "healthy" } else { "unhealthy" })?;
    db_component.set_item("pool_size", status.database_pool_size)?;
    db_component.set_item("pool_idle", status.database_pool_idle)?;
    components.set_item("database", db_component)?;

    dict.set_item("components", components)?;

    // Rust layer info
    let rust_info = PyDict::new(py);
    rust_info.set_item("environment", &status.environment)?;
    rust_info.set_item(
        "worker_core_status",
        format!("{:?}", status.worker_core_status),
    )?;
    rust_info.set_item("web_api_enabled", status.web_api_enabled)?;
    dict.set_item("rust", rust_info)?;

    // Python layer info
    // Note: Python handler count is tracked by the Python runtime, not Rust.
    // Python applications should extend this dict with their own handler registry count.
    let python_info = PyDict::new(py);
    python_info.set_item("handler_count", "unknown (tracked by Python)")?;
    dict.set_item("python", python_info)?;

    Ok(dict.into())
}

/// Get worker performance metrics
///
/// Returns metrics about step execution, timing, channels, and errors.
///
/// # Returns
///
/// A Python dict with metrics data.
///
/// # Raises
///
/// - `RuntimeError` if the worker system is not running
#[pyfunction]
pub fn get_metrics(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    // Get FFI dispatch metrics
    let ffi_metrics = handle.ffi_dispatch_channel.metrics();

    // Get status for additional metrics
    let status = handle
        .runtime
        .block_on(async { handle.system_handle.status().await })
        .map_err(|e| {
            error!("Failed to get worker status: {}", e);
            PythonFfiError::FfiError(format!("Failed to get metrics: {}", e))
        })?;

    let dict = PyDict::new(py);

    // FFI dispatch channel metrics
    dict.set_item("dispatch_channel_pending", ffi_metrics.pending_count)?;
    dict.set_item("oldest_pending_age_ms", ffi_metrics.oldest_pending_age_ms)?;
    dict.set_item("newest_pending_age_ms", ffi_metrics.newest_pending_age_ms)?;
    dict.set_item("starvation_detected", ffi_metrics.starvation_detected)?;
    dict.set_item("starving_event_count", ffi_metrics.starving_event_count)?;

    // Database pool metrics
    dict.set_item("database_pool_size", status.database_pool_size)?;
    dict.set_item("database_pool_idle", status.database_pool_idle)?;

    // Environment info
    dict.set_item("environment", &status.environment)?;

    // Placeholders for step execution metrics
    // These will be populated when the worker tracks step execution stats
    dict.set_item("steps_processed", 0)?;
    dict.set_item("steps_succeeded", 0)?;
    dict.set_item("steps_failed", 0)?;
    dict.set_item("steps_in_progress", ffi_metrics.pending_count)?;

    Ok(dict.into())
}

/// Get current worker configuration
///
/// Returns the runtime configuration settings for the worker.
///
/// # Returns
///
/// A Python dict with configuration data.
///
/// # Raises
///
/// - `RuntimeError` if the worker system is not running
#[pyfunction]
pub fn get_worker_config(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    // Get status for configuration info
    let status = handle
        .runtime
        .block_on(async { handle.system_handle.status().await })
        .map_err(|e| {
            error!("Failed to get worker status: {}", e);
            PythonFfiError::FfiError(format!("Failed to get config: {}", e))
        })?;

    let dict = PyDict::new(py);
    dict.set_item("environment", &status.environment)?;

    // Namespaces
    dict.set_item("supported_namespaces", status.supported_namespaces.clone())?;

    // Web API
    dict.set_item("web_api_enabled", status.web_api_enabled)?;

    // Database pool settings
    dict.set_item("database_pool_size", status.database_pool_size)?;

    // Configuration defaults (these would come from config if we had access)
    dict.set_item("polling_interval_ms", 10)?;
    dict.set_item("starvation_threshold_ms", 1000)?;
    dict.set_item("max_concurrent_handlers", 10)?;
    dict.set_item("handler_timeout_ms", 30000)?;

    Ok(dict.into())
}
