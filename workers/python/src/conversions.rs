//! Type conversion utilities for Python FFI
//!
//! Provides conversion between Rust types and Python dicts using
//! the `pythonize` crate for seamless serde-based conversion.

use crate::error::PythonFfiError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pythonize::{depythonize, pythonize};
use tasker_shared::messaging::StepExecutionResult;
use tasker_worker::worker::FfiStepEvent;

/// Convert an FfiStepEvent to a Python dict
///
/// This converts the dispatch channel event format to Python.
/// Includes the event_id for correlation when completing the step.
pub fn convert_ffi_step_event_to_python(
    py: Python<'_>,
    event: &FfiStepEvent,
) -> PyResult<PyObject> {
    let dict = PyDict::new_bound(py);

    // Event ID is critical for completion correlation
    dict.set_item("event_id", event.event_id.to_string())?;
    dict.set_item("task_uuid", event.task_uuid.to_string())?;
    dict.set_item("step_uuid", event.step_uuid.to_string())?;
    dict.set_item("correlation_id", event.correlation_id.to_string())?;

    // Optional trace context
    if let Some(ref trace_id) = event.trace_id {
        dict.set_item("trace_id", trace_id.clone())?;
    }
    if let Some(ref span_id) = event.span_id {
        dict.set_item("span_id", span_id.clone())?;
    }

    // Convert the full execution event payload (contains TaskSequenceStep)
    let payload = &event.execution_event.payload;

    // Also expose correlation_id from task if different
    let task_correlation_id = payload.task_sequence_step.task.task.correlation_id;
    dict.set_item("task_correlation_id", task_correlation_id.to_string())?;

    // Parent correlation ID if present
    if let Some(parent_id) = payload.task_sequence_step.task.task.parent_correlation_id {
        dict.set_item("parent_correlation_id", parent_id.to_string())?;
    }

    // Convert TaskSequenceStep to Python dict using pythonize
    let task_sequence_step_py = pythonize(py, &payload.task_sequence_step).map_err(|e| {
        PythonFfiError::ConversionError(format!(
            "Failed to convert TaskSequenceStep to Python: {}",
            e
        ))
    })?;
    dict.set_item("task_sequence_step", task_sequence_step_py)?;

    Ok(dict.into())
}

/// Convert a Python dict to StepExecutionResult
///
/// This is used when Python handlers complete step execution and need to
/// send results back through the FFI channel.
pub fn convert_python_completion_to_step_result(
    _py: Python<'_>,
    value: &Bound<'_, PyDict>,
) -> PyResult<StepExecutionResult> {
    let result: StepExecutionResult = depythonize(value).map_err(|e| {
        PythonFfiError::ConversionError(format!(
            "Failed to convert Python dict to StepExecutionResult: {}",
            e
        ))
    })?;

    Ok(result)
}

/// Convert a Python dict to a Rust HashMap<String, String>
///
/// Used for logging context fields and other simple key-value mappings.
pub fn python_dict_to_string_map(
    dict: &Bound<'_, PyDict>,
) -> PyResult<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();

    for (key, value) in dict.iter() {
        let key_str: String = key.extract()?;
        let value_str: String = value.str()?.to_string();
        map.insert(key_str, value_str);
    }

    Ok(map)
}

// Note: Rust unit tests that use PyO3 APIs cannot run in a cdylib crate
// because they require linking against Python. Instead, test through Python:
// - See tests/test_phase2.py for conversion tests
