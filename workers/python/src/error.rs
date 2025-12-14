//! Error types for Python FFI bridge
//!
//! Provides custom Python exception types and error conversion utilities
//! for the tasker-core Python bindings.

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::PyErr;
use thiserror::Error;

/// Custom error types for Python FFI operations
#[derive(Error, Debug)]
pub enum PythonFfiError {
    #[error("Worker not initialized. Call bootstrap_worker() first.")]
    WorkerNotInitialized,

    #[error("Worker bootstrap failed: {0}")]
    BootstrapFailed(String),

    #[error("Worker already running")]
    WorkerAlreadyRunning,

    #[error("Lock acquisition failed: {0}")]
    LockError(String),

    #[error("Runtime error: {0}")]
    RuntimeError(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Conversion error: {0}")]
    ConversionError(String),

    #[error("FFI error: {0}")]
    FfiError(String),
}

impl From<PythonFfiError> for PyErr {
    fn from(err: PythonFfiError) -> PyErr {
        match err {
            PythonFfiError::WorkerNotInitialized => {
                PyRuntimeError::new_err(err.to_string())
            }
            PythonFfiError::BootstrapFailed(msg) => {
                PyRuntimeError::new_err(format!("Bootstrap failed: {}", msg))
            }
            PythonFfiError::WorkerAlreadyRunning => {
                PyRuntimeError::new_err(err.to_string())
            }
            PythonFfiError::LockError(msg) => {
                PyRuntimeError::new_err(format!("Lock error: {}", msg))
            }
            PythonFfiError::RuntimeError(msg) => {
                PyRuntimeError::new_err(msg)
            }
            PythonFfiError::InvalidArgument(msg) => {
                PyValueError::new_err(msg)
            }
            PythonFfiError::ConversionError(msg) => {
                PyValueError::new_err(format!("Conversion error: {}", msg))
            }
            PythonFfiError::FfiError(msg) => {
                PyRuntimeError::new_err(format!("FFI error: {}", msg))
            }
        }
    }
}

/// Result type for Python FFI operations
pub type PythonFfiResult<T> = Result<T, PythonFfiError>;
