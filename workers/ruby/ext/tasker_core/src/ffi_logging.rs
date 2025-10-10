//! FFI-specific logging module using unified logging patterns
//!
//! This module provides structured logging for FFI boundary debugging
//! using the unified logging macros that match Ruby patterns.
//!
//! ## TAS-29 Phase 6: Ruby FFI Logging Bridge
//!
//! This module exposes Rust's tracing infrastructure to Ruby via FFI, enabling
//! unified structured logging across both Ruby and Rust components.
//!
//! ### Architecture
//!
//! ```text
//! Ruby Handler
//!     ↓
//! TaskerCore::Tracing.info("message", fields: {...})
//!     ↓
//! FFI Bridge (this module)
//!     ↓
//! tasker_shared::log_ffi! macro
//!     ↓
//! tracing crate → OpenTelemetry (if enabled)
//! ```
//!
//! ### Log Levels
//!
//! - ERROR: Unrecoverable failures requiring intervention
//! - WARN: Degraded operation, retryable failures
//! - INFO: Lifecycle events, state transitions
//! - DEBUG: Detailed diagnostic information
//! - TRACE: Very verbose, hot-path entry/exit

use magnus::{value::ReprValue, Error, RHash, Value};
use std::collections::HashMap;
use tracing::{debug, error, info, trace, warn};

/// Initialize FFI logging (now just ensures main logging is available)
pub fn init_ffi_logger() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize main structured logging if not already done
    tasker_shared::logging::init_tracing();

    // Use unified logging macro instead of file logging
    tasker_shared::log_ffi!(info, "FFI logging initialized", component: "ffi_boundary");
    Ok(())
}

/// Convert Ruby hash to Rust HashMap for structured fields
fn ruby_hash_to_map(hash: RHash) -> Result<HashMap<String, String>, Error> {
    let mut map = HashMap::new();

    hash.foreach(|key: Value, value: Value| {
        let key_str = key.to_r_string()?.to_string()?;
        let value_str = value.to_r_string()?.to_string()?;
        map.insert(key_str, value_str);
        Ok(magnus::r_hash::ForEach::Continue)
    })?;

    Ok(map)
}

/// Log ERROR level message with structured fields (Ruby FFI)
///
/// # Ruby Usage
/// ```ruby
/// TaskerCore.log_error("Task processing failed", {
///   correlation_id: correlation_id,
///   task_uuid: task_uuid,
///   error_message: error.message
/// })
/// ```
pub fn log_error(message: String, fields: RHash) -> Result<(), Error> {
    let fields_map = ruby_hash_to_map(fields)?;

    // Extract common fields for structured logging
    let correlation_id = fields_map.get("correlation_id").cloned();
    let task_uuid = fields_map.get("task_uuid").cloned();
    let step_uuid = fields_map.get("step_uuid").cloned();
    let namespace = fields_map.get("namespace").cloned();
    let operation = fields_map
        .get("operation")
        .cloned()
        .unwrap_or_else(|| "ruby_handler".to_string());

    // Log with structured fields
    error!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "ruby_ffi",
        "{}",
        message
    );

    Ok(())
}

/// Log WARN level message with structured fields (Ruby FFI)
pub fn log_warn(message: String, fields: RHash) -> Result<(), Error> {
    let fields_map = ruby_hash_to_map(fields)?;

    let correlation_id = fields_map.get("correlation_id").cloned();
    let task_uuid = fields_map.get("task_uuid").cloned();
    let step_uuid = fields_map.get("step_uuid").cloned();
    let namespace = fields_map.get("namespace").cloned();
    let operation = fields_map
        .get("operation")
        .cloned()
        .unwrap_or_else(|| "ruby_handler".to_string());

    warn!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "ruby_ffi",
        "{}",
        message
    );

    Ok(())
}

/// Log INFO level message with structured fields (Ruby FFI)
pub fn log_info(message: String, fields: RHash) -> Result<(), Error> {
    let fields_map = ruby_hash_to_map(fields)?;

    let correlation_id = fields_map.get("correlation_id").cloned();
    let task_uuid = fields_map.get("task_uuid").cloned();
    let step_uuid = fields_map.get("step_uuid").cloned();
    let namespace = fields_map.get("namespace").cloned();
    let operation = fields_map
        .get("operation")
        .cloned()
        .unwrap_or_else(|| "ruby_handler".to_string());

    info!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "ruby_ffi",
        "{}",
        message
    );

    Ok(())
}

/// Log DEBUG level message with structured fields (Ruby FFI)
pub fn log_debug(message: String, fields: RHash) -> Result<(), Error> {
    let fields_map = ruby_hash_to_map(fields)?;

    let correlation_id = fields_map.get("correlation_id").cloned();
    let task_uuid = fields_map.get("task_uuid").cloned();
    let step_uuid = fields_map.get("step_uuid").cloned();
    let namespace = fields_map.get("namespace").cloned();
    let operation = fields_map
        .get("operation")
        .cloned()
        .unwrap_or_else(|| "ruby_handler".to_string());

    debug!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "ruby_ffi",
        "{}",
        message
    );

    Ok(())
}

/// Log TRACE level message with structured fields (Ruby FFI)
pub fn log_trace(message: String, fields: RHash) -> Result<(), Error> {
    let fields_map = ruby_hash_to_map(fields)?;

    let correlation_id = fields_map.get("correlation_id").cloned();
    let task_uuid = fields_map.get("task_uuid").cloned();
    let step_uuid = fields_map.get("step_uuid").cloned();
    let namespace = fields_map.get("namespace").cloned();
    let operation = fields_map
        .get("operation")
        .cloned()
        .unwrap_or_else(|| "ruby_handler".to_string());

    trace!(
        correlation_id = correlation_id.as_deref(),
        task_uuid = task_uuid.as_deref(),
        step_uuid = step_uuid.as_deref(),
        namespace = namespace.as_deref(),
        operation = %operation,
        component = "ruby_ffi",
        "{}",
        message
    );

    Ok(())
}
