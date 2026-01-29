//! # Client Error Types
//!
//! Unified error handling for tasker-client library and CLI operations.

use anyhow::Result;
use tasker_shared::errors::TaskerError;
use thiserror::Error;

/// Client operation result type
pub type ClientResult<T> = Result<T, ClientError>;

/// Comprehensive error types for client operations
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON serialization/deserialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },

    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Task not found: {task_id}")]
    TaskNotFound { task_id: String },

    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: String },

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Service unavailable: {service} - {reason}")]
    ServiceUnavailable { service: String, reason: String },

    #[error("Timeout waiting for operation: {operation}")]
    Timeout { operation: String },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("UUID parsing error: {0}")]
    UuidError(#[from] uuid::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid response: {field} - {reason}")]
    InvalidResponse { field: String, reason: String },

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Tasker system error: {0}")]
    TaskerError(#[from] TaskerError),
}

impl ClientError {
    /// Create an API error from HTTP response
    pub fn api_error(status: u16, message: impl Into<String>) -> Self {
        Self::ApiError {
            status,
            message: message.into(),
        }
    }

    /// Create a configuration error
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError(message.into())
    }

    /// Create a service unavailable error
    pub fn service_unavailable(service: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ServiceUnavailable {
            service: service.into(),
            reason: reason.into(),
        }
    }

    /// Create an invalid response error for protocol violations
    ///
    /// Use this when a gRPC response is missing required fields or contains
    /// malformed data. This indicates a protocol violation that should not
    /// be silently defaulted.
    pub fn invalid_response(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidResponse {
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// Check if error is recoverable (worth retrying)
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        match self {
            ClientError::HttpError(e) => e.is_timeout() || e.is_connect(),
            ClientError::ServiceUnavailable { .. } => true,
            ClientError::Timeout { .. } => true,
            ClientError::ApiError { status, .. } => *status >= 500,
            // Protocol violations are not recoverable - the server is broken
            ClientError::InvalidResponse { .. } => false,
            _ => false,
        }
    }
}
