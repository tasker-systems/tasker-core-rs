//! Error types for TypeScript FFI bindings.

use thiserror::Error;

/// Errors that can occur in the TypeScript FFI layer.
#[derive(Error, Debug)]
pub enum TypeScriptFfiError {
    /// Worker has not been initialized (bootstrap not called)
    #[error("Worker not initialized. Call bootstrap_worker first.")]
    WorkerNotInitialized,

    /// Bootstrap failed
    #[error("Bootstrap failed: {0}")]
    BootstrapFailed(String),

    /// Worker is already running
    #[error("Worker is already running")]
    WorkerAlreadyRunning,

    /// Failed to acquire lock
    #[error("Failed to acquire lock on worker state")]
    LockError,

    /// Runtime error
    #[error("Runtime error: {0}")]
    RuntimeError(String),

    /// Invalid argument provided
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Type conversion error
    #[error("Conversion error: {0}")]
    ConversionError(String),

    /// FFI-specific error
    #[error("FFI error: {0}")]
    FfiError(String),
}
