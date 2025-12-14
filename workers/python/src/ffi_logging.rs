//! FFI-specific logging module for Python
//!
//! Provides structured logging for FFI boundary debugging using the unified
//! logging macros that match patterns across Ruby and Python workers.
//!
//! ## Architecture
//!
//! ```text
//! Python Handler
//!     ↓
//! tasker_core.log_info("message", {"field": "value"})
//!     ↓
//! FFI Bridge (this module)
//!     ↓
//! tracing crate → OpenTelemetry (if enabled)
//! ```
//!
//! ## Log Levels
//!
//! - ERROR: Unrecoverable failures requiring intervention
//! - WARN: Degraded operation, retryable failures
//! - INFO: Lifecycle events, state transitions
//! - DEBUG: Detailed diagnostic information
//! - TRACE: Very verbose, hot-path entry/exit

use crate::conversions::python_dict_to_string_map;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tracing::{debug, error, info, trace, warn};

/// Initialize FFI logging using two-phase pattern for telemetry support
///
/// # Two-Phase Initialization Pattern
///
/// This function implements phase 1 of the FFI telemetry initialization pattern:
///
/// **Phase 1 (This function)**: Called during PyO3 module initialization (no Tokio runtime)
/// - If TELEMETRY_ENABLED=false: Initialize console-only logging (safe, no runtime needed)
/// - If TELEMETRY_ENABLED=true: Skip initialization (will be done in phase 2)
///
/// **Phase 2**: Called in `bootstrap_worker()` after Tokio runtime creation
/// - Always call `init_tracing()` in `runtime.block_on()` context
/// - If console already initialized: Returns early (no-op)
/// - If not initialized (telemetry case): Initializes with OpenTelemetry in Tokio context
///
/// # Why This Pattern?
///
/// OpenTelemetry batch exporter requires a Tokio runtime context for async I/O.
/// During PyO3 module initialization, no Tokio runtime exists yet, so we defer full
/// initialization until after the runtime is created in `bootstrap_worker()`.
pub fn init_ffi_logger() -> Result<(), Box<dyn std::error::Error>> {
    // Check if telemetry is enabled
    let telemetry_enabled = std::env::var("TELEMETRY_ENABLED")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    if telemetry_enabled {
        // Phase 1: Telemetry enabled - skip logging init
        // Will be initialized in bootstrap_worker() after runtime creation
        println!("📡 Telemetry enabled - deferring logging init to runtime context");
    } else {
        // Phase 1: Telemetry disabled - safe to initialize console-only logging
        tasker_shared::logging::init_console_only();

        // Use unified logging macro
        tasker_shared::log_ffi!(
            info,
            "FFI console logging initialized (no telemetry)",
            component: "python_ffi"
        );
    }

    Ok(())
}

/// Extract common logging fields from a Python dict
fn extract_log_fields(
    fields_map: &std::collections::HashMap<String, String>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
) {
    let correlation_id = fields_map.get("correlation_id").cloned();
    let task_uuid = fields_map.get("task_uuid").cloned();
    let step_uuid = fields_map.get("step_uuid").cloned();
    let namespace = fields_map.get("namespace").cloned();
    let operation = fields_map
        .get("operation")
        .cloned()
        .unwrap_or_else(|| "python_handler".to_string());

    (correlation_id, task_uuid, step_uuid, namespace, operation)
}

/// Log ERROR level message with structured fields (Python FFI)
///
/// # Python Usage
/// ```python
/// tasker_core.log_error("Task processing failed", {
///     "correlation_id": correlation_id,
///     "task_uuid": task_uuid,
///     "error_message": str(error)
/// })
/// ```
#[pyfunction]
#[pyo3(signature = (message, fields=None))]
pub fn log_error(message: String, fields: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let fields_map = fields
        .map(python_dict_to_string_map)
        .transpose()?
        .unwrap_or_default();

    let (correlation_id, task_uuid, step_uuid, namespace, operation) =
        extract_log_fields(&fields_map);

    error!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "python_ffi",
        "{}",
        message
    );

    Ok(())
}

/// Log WARN level message with structured fields (Python FFI)
#[pyfunction]
#[pyo3(signature = (message, fields=None))]
pub fn log_warn(message: String, fields: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let fields_map = fields
        .map(python_dict_to_string_map)
        .transpose()?
        .unwrap_or_default();

    let (correlation_id, task_uuid, step_uuid, namespace, operation) =
        extract_log_fields(&fields_map);

    warn!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "python_ffi",
        "{}",
        message
    );

    Ok(())
}

/// Log INFO level message with structured fields (Python FFI)
#[pyfunction]
#[pyo3(signature = (message, fields=None))]
pub fn log_info(message: String, fields: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let fields_map = fields
        .map(python_dict_to_string_map)
        .transpose()?
        .unwrap_or_default();

    let (correlation_id, task_uuid, step_uuid, namespace, operation) =
        extract_log_fields(&fields_map);

    info!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "python_ffi",
        "{}",
        message
    );

    Ok(())
}

/// Log DEBUG level message with structured fields (Python FFI)
#[pyfunction]
#[pyo3(signature = (message, fields=None))]
pub fn log_debug(message: String, fields: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let fields_map = fields
        .map(python_dict_to_string_map)
        .transpose()?
        .unwrap_or_default();

    let (correlation_id, task_uuid, step_uuid, namespace, operation) =
        extract_log_fields(&fields_map);

    debug!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "python_ffi",
        "{}",
        message
    );

    Ok(())
}

/// Log TRACE level message with structured fields (Python FFI)
#[pyfunction]
#[pyo3(signature = (message, fields=None))]
pub fn log_trace(message: String, fields: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let fields_map = fields
        .map(python_dict_to_string_map)
        .transpose()?
        .unwrap_or_default();

    let (correlation_id, task_uuid, step_uuid, namespace, operation) =
        extract_log_fields(&fields_map);

    trace!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "python_ffi",
        "{}",
        message
    );

    Ok(())
}
