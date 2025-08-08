//! FFI-specific logging module using unified logging patterns
//!
//! This module provides structured logging for FFI boundary debugging
//! using the unified logging macros that match Ruby patterns.

// All logging is now handled through the unified logging macros

/// Initialize FFI logging (now just ensures main logging is available)
pub fn init_ffi_logger() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize main structured logging if not already done
    tasker_core::logging::init_structured_logging();

    // Use unified logging macro instead of file logging
    tasker_core::log_ffi!(info, "FFI logging initialized", component: "ffi_boundary");
    Ok(())
}

/// Log a debug message using unified FFI logging
pub fn log_ffi_debug(component: &str, message: &str) {
    tasker_core::log_ffi!(debug, message, component: component);
}

/// Log task initialization data for debugging
pub fn log_task_init_data(stage: &str, data: &serde_json::Value) {
    let operation = format!("Task initialization - {stage}");
    let data_str =
        serde_json::to_string_pretty(data).unwrap_or_else(|_| "SERIALIZATION_ERROR".to_string());

    tasker_core::log_ffi!(debug, operation,
        component: "initialize_task",
        stage: stage,
        data_preview: &data_str[0..data_str.len().min(200)] // Limit preview size
    );
}

/// Log the result data before returning to Ruby
pub fn log_task_init_result(result: &crate::types::TaskHandlerInitializeResult) {
    tasker_core::log_ffi!(debug, "Task initialization result",
        component: "initialize_task",
        stage: "RESULT",
        task_id: result.task_id,
        step_count: result.step_count,
        handler_config_name: &result.handler_config_name
    );
}

/// Log Magnus value inspection
pub fn log_magnus_value(stage: &str, value: magnus::Value) {
    use magnus::value::ReprValue;

    let inspect_result: Result<String, _> = value.funcall("inspect", ());
    let class_name: Result<String, _> = value
        .funcall("class", ())
        .and_then(|c: magnus::Value| c.funcall("name", ()));

    let operation = format!("Magnus value at {stage}");
    tasker_core::log_ffi!(debug, operation,
        component: "magnus_inspection",
        stage: stage,
        class_name: &class_name.unwrap_or_else(|_| "UNKNOWN".to_string()),
        inspect_result: &inspect_result.unwrap_or_else(|_| "INSPECT_FAILED".to_string())
    );
}

/// Log raw FFI boundary crossing
pub fn log_ffi_boundary(direction: &str, component: &str, data: &str) {
    let operation = format!("FFI {direction} - {component}");
    let data_preview = if data.len() > 500 {
        &data[0..500]
    } else {
        data
    };

    tasker_core::log_ffi!(debug, operation,
        component: "ffi_boundary",
        direction: direction,
        boundary_component: component,
        data_preview: data_preview
    );
}
