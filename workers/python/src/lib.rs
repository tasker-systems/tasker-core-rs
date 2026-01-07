//! PyO3 bindings for tasker-core Python worker
//!
//! This module provides the FFI interface between Rust and Python,
//! exposing worker functionality through the `_tasker_core` module.
//!
//! # Module Structure
//!
//! The Python package is structured as:
//! - `tasker_core` - Public Python package
//! - `tasker_core._tasker_core` - Internal FFI module (this crate)
//!
//! # Phases
//!
//! - **Phase 1 (TAS-72-P1)**: Basic FFI scaffolding and build pipeline
//! - **Phase 2 (TAS-72-P2)**: Bootstrap, lifecycle, logging, type conversions
//! - **Phase 3 (TAS-72-P3)**: Event dispatch system (poll/complete)
//! - **Phase 5 (TAS-72-P5)**: Domain events and observability

// Allow dead code for functions that will be used in Phase 4+
#![allow(dead_code)]
// PyO3 0.22 macros generate code that triggers this lint in Rust 2024
#![allow(unsafe_op_in_unsafe_fn)]
// PyO3 macro expansion triggers false positives for useless_conversion
#![allow(clippy::useless_conversion)]

use pyo3::prelude::*;

mod bootstrap;
mod bridge;
mod conversions;
mod error;
mod event_dispatch;
mod ffi_logging;
mod observability;

/// Returns the version of the tasker-core-py package
#[pyfunction]
fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns the Rust library version for debugging
#[pyfunction]
fn get_rust_version() -> String {
    format!(
        "tasker-worker-py {} (rustc {})",
        env!("CARGO_PKG_VERSION"),
        env!("RUSTC_VERSION")
    )
}

/// Check if the FFI module is working correctly
///
/// Returns `True` if the FFI layer is functional.
/// This is useful for health checks and debugging.
#[pyfunction]
fn health_check() -> bool {
    true
}

/// The main Python module for tasker-core FFI
///
/// This is the low-level FFI module. The public API is exposed
/// through the `tasker_core` Python package, which re-exports
/// the necessary functions with proper docstrings and type hints.
///
/// # Module Registration
///
/// Functions are registered as module-level functions that can be
/// called from Python:
///
/// ```python
/// from tasker_core._tasker_core import get_version, health_check
/// print(get_version())
/// assert health_check()
/// ```
#[pymodule]
fn _tasker_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize FFI logging (Phase 1 of two-phase initialization)
    ffi_logging::init_ffi_logger().map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to initialize logging: {}", e))
    })?;

    // Version information (Phase 1)
    m.add_function(wrap_pyfunction!(get_version, m)?)?;
    m.add_function(wrap_pyfunction!(get_rust_version, m)?)?;
    m.add_function(wrap_pyfunction!(health_check, m)?)?;

    // Bootstrap and lifecycle (Phase 2)
    m.add_function(wrap_pyfunction!(bootstrap::bootstrap_worker, m)?)?;
    m.add_function(wrap_pyfunction!(bootstrap::stop_worker, m)?)?;
    m.add_function(wrap_pyfunction!(bootstrap::get_worker_status, m)?)?;
    m.add_function(wrap_pyfunction!(
        bootstrap::transition_to_graceful_shutdown,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(bootstrap::is_worker_running, m)?)?;

    // Logging functions (Phase 2)
    m.add_function(wrap_pyfunction!(ffi_logging::log_error, m)?)?;
    m.add_function(wrap_pyfunction!(ffi_logging::log_warn, m)?)?;
    m.add_function(wrap_pyfunction!(ffi_logging::log_info, m)?)?;
    m.add_function(wrap_pyfunction!(ffi_logging::log_debug, m)?)?;
    m.add_function(wrap_pyfunction!(ffi_logging::log_trace, m)?)?;

    // Event dispatch functions (Phase 3)
    m.add_function(wrap_pyfunction!(event_dispatch::poll_step_events, m)?)?;
    m.add_function(wrap_pyfunction!(event_dispatch::complete_step_event, m)?)?;
    m.add_function(wrap_pyfunction!(
        event_dispatch::get_ffi_dispatch_metrics,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(
        event_dispatch::check_starvation_warnings,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(event_dispatch::cleanup_timeouts, m)?)?;
    // TAS-125: Checkpoint yield for batch processing handlers
    m.add_function(wrap_pyfunction!(
        event_dispatch::checkpoint_yield_step_event,
        m
    )?)?;

    // Observability functions (Phase 5)
    m.add_function(wrap_pyfunction!(observability::poll_in_process_events, m)?)?;
    m.add_function(wrap_pyfunction!(observability::get_health_check, m)?)?;
    m.add_function(wrap_pyfunction!(observability::get_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(observability::get_worker_config, m)?)?;

    // Module metadata
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_version() {
        let version = get_version();
        assert!(!version.is_empty());
        assert!(version.contains('.'));
    }

    #[test]
    fn test_get_rust_version() {
        let version = get_rust_version();
        assert!(version.contains("tasker-worker-py"));
        assert!(version.contains("rustc"));
    }
}
