//! Worker-specific error types

use thiserror::Error;

/// Worker foundation error type
#[derive(Error, Debug)]
pub enum WorkerError {
    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Messaging error: {0}")]
    Messaging(String),

    #[error("Event system error: {0}")]
    EventSystem(String),

    #[error("Worker pool error: {0}")]
    WorkerPool(String),

    #[error("Step processing error: {0}")]
    StepProcessing(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error("Health check failed: {0}")]
    HealthCheck(String),

    #[error("Shutdown in progress")]
    ShutdownInProgress,

    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),

    #[error("FFI error: {0}")]
    Ffi(String),

    #[error("Shared component error: {0}")]
    Shared(#[from] tasker_shared::error::TaskerError),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

/// Result type alias for WorkerError
pub type Result<T> = std::result::Result<T, WorkerError>;