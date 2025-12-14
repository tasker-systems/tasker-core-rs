//! Event dispatch FFI functions for Python worker
//!
//! TAS-72-P3: This module provides the step event polling and completion
//! system, enabling Python handlers to receive step execution events from
//! the Rust orchestration layer and submit completion results.
//!
//! ## Architecture
//!
//! Python handlers poll for events via `poll_step_events()`, receive
//! `FfiStepEvent` objects, execute the handler logic, and submit results
//! via `complete_step_event()`.

use crate::bridge::WORKER_SYSTEM;
use crate::conversions::{convert_ffi_step_event_to_python, convert_python_completion_to_step_result};
use crate::error::PythonFfiError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Poll for pending step execution events
///
/// This is a non-blocking call that returns the next available
/// step event or None if no events are pending.
///
/// Python handlers should call this in a polling loop with appropriate
/// sleep intervals (typically 10ms) between polls.
///
/// # Returns
///
/// A Python dict representing the `FfiStepEvent`, or `None` if no events
/// are available.
///
/// # Raises
///
/// - `RuntimeError` if the worker system is not running
#[pyfunction]
pub fn poll_step_events(py: Python<'_>) -> PyResult<Option<PyObject>> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    // Poll for next event via FfiDispatchChannel
    if let Some(event) = handle.ffi_dispatch_channel.poll() {
        debug!(
            event_id = %event.event_id,
            step_uuid = %event.step_uuid,
            "Polled FFI step event for Python processing"
        );

        // Convert the FfiStepEvent to Python format
        let python_event = convert_ffi_step_event_to_python(py, &event).map_err(|e| {
            error!("Failed to convert event to Python: {}", e);
            PythonFfiError::ConversionError(format!("Failed to convert event to Python: {}", e))
        })?;

        Ok(Some(python_event))
    } else {
        // Return None when no events are available
        Ok(None)
    }
}

/// Submit completion result for a step event
///
/// Called after a Python handler has completed execution.
/// This sends the result back to the completion channel and
/// triggers post-handler callbacks (domain event publishing).
///
/// # Arguments
///
/// * `event_id` - The event ID string from the `FfiStepEvent`
/// * `completion_data` - Python dict with completion result data
///
/// # Returns
///
/// `True` if the completion was successfully submitted, `False` otherwise.
///
/// # Raises
///
/// - `ValueError` if the event_id format is invalid
/// - `RuntimeError` if the worker system is not running
/// - `RuntimeError` if conversion fails
#[pyfunction]
pub fn complete_step_event(
    py: Python<'_>,
    event_id_str: String,
    completion_data: &Bound<'_, PyDict>,
) -> PyResult<bool> {
    // Parse event_id
    let event_id = Uuid::parse_str(&event_id_str).map_err(|e| {
        error!("Invalid event_id format: {}", e);
        PythonFfiError::InvalidArgument(format!("Invalid event_id format: {}", e))
    })?;

    // Convert Python completion to StepExecutionResult
    let result = convert_python_completion_to_step_result(py, completion_data).map_err(|e| {
        error!("Failed to convert completion data: {}", e);
        PythonFfiError::ConversionError(format!("Failed to convert completion data: {}", e))
    })?;

    // Get bridge handle and submit completion
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    // Submit completion via FfiDispatchChannel
    let success = handle.ffi_dispatch_channel.complete(event_id, result);

    if success {
        debug!(event_id = %event_id, "Step completion submitted successfully");
    } else {
        error!(event_id = %event_id, "Failed to submit step completion");
    }

    Ok(success)
}

/// Get metrics from the FFI dispatch channel
///
/// Returns metrics for monitoring and observability:
/// - `pending_count`: Number of events waiting for completion
/// - `oldest_pending_age_ms`: Age of the oldest pending event in milliseconds
/// - `newest_pending_age_ms`: Age of the newest pending event in milliseconds
/// - `oldest_event_id`: UUID of the oldest pending event (for debugging)
/// - `starvation_detected`: Boolean indicating if any events exceed starvation threshold
/// - `starving_event_count`: Number of events exceeding starvation threshold
///
/// # Returns
///
/// A Python dict with the metrics.
///
/// # Raises
///
/// - `RuntimeError` if the worker system is not running
#[pyfunction]
pub fn get_ffi_dispatch_metrics(py: Python<'_>) -> PyResult<PyObject> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    let metrics = handle.ffi_dispatch_channel.metrics();

    let dict = PyDict::new_bound(py);
    dict.set_item("pending_count", metrics.pending_count)?;
    dict.set_item(
        "oldest_pending_age_ms",
        metrics.oldest_pending_age_ms.map(|v| v as i64),
    )?;
    dict.set_item(
        "newest_pending_age_ms",
        metrics.newest_pending_age_ms.map(|v| v as i64),
    )?;
    dict.set_item(
        "oldest_event_id",
        metrics.oldest_event_id.map(|id| id.to_string()),
    )?;
    dict.set_item("starvation_detected", metrics.starvation_detected)?;
    dict.set_item("starving_event_count", metrics.starving_event_count)?;

    Ok(dict.into())
}

/// Check for starvation warnings and emit logs
///
/// This emits warning logs for any pending events that exceed the
/// starvation threshold. Call this periodically (e.g., every poll cycle)
/// for proactive monitoring.
///
/// # Returns
///
/// `True` after checking (always succeeds if worker is running).
///
/// # Raises
///
/// - `RuntimeError` if the worker system is not running
#[pyfunction]
pub fn check_starvation_warnings() -> PyResult<bool> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    handle.ffi_dispatch_channel.check_starvation_warnings();
    Ok(true)
}

/// Clean up timed-out pending events
///
/// Call this periodically to detect handlers that never completed.
/// Returns the number of events that timed out.
///
/// # Returns
///
/// The number of events that were cleaned up due to timeout.
///
/// # Raises
///
/// - `RuntimeError` if the worker system is not running
#[pyfunction]
pub fn cleanup_timeouts() -> PyResult<usize> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        PythonFfiError::LockError(e.to_string())
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or(PythonFfiError::WorkerNotInitialized)?;

    let count = handle.ffi_dispatch_channel.cleanup_timeouts();

    if count > 0 {
        warn!(count = count, "Cleaned up timed-out FFI events");
    }

    Ok(count)
}
