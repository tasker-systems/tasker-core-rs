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
//! # Phase 1 (TAS-72-P1)
//!
//! This initial implementation provides minimal FFI to verify the
//! build pipeline works. Full functionality will be added in
//! subsequent phases.

use pyo3::prelude::*;

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
    // Version information
    m.add_function(wrap_pyfunction!(get_version, m)?)?;
    m.add_function(wrap_pyfunction!(get_rust_version, m)?)?;
    m.add_function(wrap_pyfunction!(health_check, m)?)?;

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
