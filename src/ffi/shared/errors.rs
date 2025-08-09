//! # Shared FFI Error Types
//!
//! Common error definitions that can be mapped to language-specific
//! error types by the language bindings.

use std::fmt;

/// Shared FFI error type that can be converted to language-specific errors
#[derive(Debug, Clone)]
pub enum SharedFFIError {
    /// Orchestration system initialization failed
    OrchestrationInitializationFailed(String),
    
    /// Runtime creation failed
    RuntimeError(String),
    
    /// Initialization error
    InitializationError(String),

    /// Handle validation failed (expired, invalid, etc.)
    HandleValidationFailed(String),

    /// Database operation failed
    DatabaseError(String),

    /// Task creation failed
    TaskCreationFailed(String),

    /// Workflow step creation failed
    StepCreationFailed(String),

    /// Handler registration failed
    HandlerRegistrationFailed(String),

    /// Handler lookup failed
    HandlerLookupFailed(String),

    /// Event publishing failed
    EventPublishingFailed(String),

    /// Type conversion failed (language-specific data to shared types)
    TypeConversionFailed(String),

    /// Invalid input parameters
    InvalidInput(String),

    /// Internal error
    Internal(String),

    /// TCP executor operation failed
    TcpExecutorError(String),

    /// TCP executor not enabled/available
    TcpExecutorNotAvailable(String),

    /// Serialization/Deserialization failed
    SerializationError(String),

    /// Invalid batch data
    InvalidBatchData(String),

    /// Environment not safe for destructive operations
    EnvironmentNotSafe(String),

    /// Database migration failed
    MigrationFailed(String),

    /// Queue operation failed
    QueueOperationFailed(String),
}

impl fmt::Display for SharedFFIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SharedFFIError::OrchestrationInitializationFailed(msg) => {
                write!(f, "Orchestration initialization failed: {msg}")
            }
            SharedFFIError::RuntimeError(msg) => {
                write!(f, "Runtime error: {msg}")
            }
            SharedFFIError::InitializationError(msg) => {
                write!(f, "Initialization error: {msg}")
            }
            SharedFFIError::HandleValidationFailed(msg) => {
                write!(f, "Handle validation failed: {msg}")
            }
            SharedFFIError::DatabaseError(msg) => {
                write!(f, "Database error: {msg}")
            }
            SharedFFIError::TaskCreationFailed(msg) => {
                write!(f, "Task creation failed: {msg}")
            }
            SharedFFIError::StepCreationFailed(msg) => {
                write!(f, "Step creation failed: {msg}")
            }
            SharedFFIError::HandlerRegistrationFailed(msg) => {
                write!(f, "Handler registration failed: {msg}")
            }
            SharedFFIError::HandlerLookupFailed(msg) => {
                write!(f, "Handler lookup failed: {msg}")
            }
            SharedFFIError::EventPublishingFailed(msg) => {
                write!(f, "Event publishing failed: {msg}")
            }
            SharedFFIError::TypeConversionFailed(msg) => {
                write!(f, "Type conversion failed: {msg}")
            }
            SharedFFIError::InvalidInput(msg) => {
                write!(f, "Invalid input: {msg}")
            }
            SharedFFIError::Internal(msg) => {
                write!(f, "Internal error: {msg}")
            }
            SharedFFIError::TcpExecutorError(msg) => {
                write!(f, "TCP executor error: {msg}")
            }
            SharedFFIError::TcpExecutorNotAvailable(msg) => {
                write!(f, "TCP executor not available: {msg}")
            }
            SharedFFIError::SerializationError(msg) => {
                write!(f, "Serialization error: {msg}")
            }
            SharedFFIError::InvalidBatchData(msg) => {
                write!(f, "Invalid batch data: {msg}")
            }
            SharedFFIError::EnvironmentNotSafe(msg) => {
                write!(f, "Environment not safe: {msg}")
            }
            SharedFFIError::MigrationFailed(msg) => {
                write!(f, "Migration failed: {msg}")
            }
            SharedFFIError::QueueOperationFailed(msg) => {
                write!(f, "Queue operation failed: {msg}")
            }
        }
    }
}

impl std::error::Error for SharedFFIError {}

/// Result type for shared FFI operations
pub type SharedFFIResult<T> = Result<T, SharedFFIError>;
