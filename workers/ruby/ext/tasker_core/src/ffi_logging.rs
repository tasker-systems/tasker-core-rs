//! FFI-specific logging module using unified logging patterns
//!
//! This module provides structured logging for FFI boundary debugging
//! using the unified logging macros that match Ruby patterns.

// All logging is now handled through the unified logging macros

/// Initialize FFI logging (now just ensures main logging is available)
pub fn init_ffi_logger() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize main structured logging if not already done
    tasker_shared::logging::init_tracing();

    // Use unified logging macro instead of file logging
    tasker_shared::log_ffi!(info, "FFI logging initialized", component: "ffi_boundary");
    Ok(())
}
