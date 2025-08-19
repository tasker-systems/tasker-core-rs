//! # Messaging Error Types
//!
//! Comprehensive error handling for the messaging system using thiserror
//! for structured error types instead of `Box<dyn Error>` patterns.

use crate::resilience::CircuitBreakerError;
use thiserror::Error;

/// Comprehensive messaging error types
#[derive(Error, Debug)]
pub enum MessagingError {
    #[error("Database connection error: {message}")]
    DatabaseConnection { message: String },

    #[error("Database query error: {operation}: {message}")]
    DatabaseQuery { operation: String, message: String },

    #[error("Queue operation failed: {queue_name}: {operation}: {message}")]
    QueueOperation {
        queue_name: String,
        operation: String,
        message: String,
    },

    #[error("Queue not found: {queue_name}")]
    QueueNotFound { queue_name: String },

    #[error("Message serialization error: {message}")]
    MessageSerialization { message: String },

    #[error("Message deserialization error: {message}")]
    MessageDeserialization { message: String },

    #[error("Circuit breaker is open for component: {component}")]
    CircuitBreakerOpen { component: String },

    #[error("Circuit breaker error: {message}")]
    CircuitBreakerError { message: String },

    #[error("Configuration error: {component}: {message}")]
    Configuration { component: String, message: String },

    #[error("Network timeout: operation {operation} timed out after {timeout_seconds}s")]
    Timeout {
        operation: String,
        timeout_seconds: u64,
    },

    #[error("Connection pool exhausted: {message}")]
    PoolExhausted { message: String },

    #[error("Protocol error: {message}")]
    Protocol { message: String },

    #[error("Invalid queue name: {queue_name}: {reason}")]
    InvalidQueueName { queue_name: String, reason: String },

    #[error("Message too large: {size_bytes} bytes exceeds limit of {limit_bytes} bytes")]
    MessageTooLarge {
        size_bytes: usize,
        limit_bytes: usize,
    },

    #[error("Queue capacity exceeded: {queue_name} has {current_count} messages, limit is {limit_count}")]
    QueueCapacityExceeded {
        queue_name: String,
        current_count: u64,
        limit_count: u64,
    },

    #[error("Authentication failed: {message}")]
    Authentication { message: String },

    #[error("Authorization failed for queue: {queue_name}: {message}")]
    Authorization { queue_name: String, message: String },

    #[error("Internal messaging error: {message}")]
    Internal { message: String },
}

impl MessagingError {
    /// Create a database connection error
    pub fn database_connection(message: impl Into<String>) -> Self {
        Self::DatabaseConnection {
            message: message.into(),
        }
    }

    /// Create a database query error
    pub fn database_query(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self::DatabaseQuery {
            operation: operation.into(),
            message: message.into(),
        }
    }

    /// Create a queue operation error
    pub fn queue_operation(
        queue_name: impl Into<String>,
        operation: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::QueueOperation {
            queue_name: queue_name.into(),
            operation: operation.into(),
            message: message.into(),
        }
    }

    /// Create a queue not found error
    pub fn queue_not_found(queue_name: impl Into<String>) -> Self {
        Self::QueueNotFound {
            queue_name: queue_name.into(),
        }
    }

    /// Create a message serialization error
    pub fn message_serialization(message: impl Into<String>) -> Self {
        Self::MessageSerialization {
            message: message.into(),
        }
    }

    /// Create a message deserialization error
    pub fn message_deserialization(message: impl Into<String>) -> Self {
        Self::MessageDeserialization {
            message: message.into(),
        }
    }

    /// Create a circuit breaker open error
    pub fn circuit_breaker_open(component: impl Into<String>) -> Self {
        Self::CircuitBreakerOpen {
            component: component.into(),
        }
    }

    /// Create a configuration error
    pub fn configuration(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Configuration {
            component: component.into(),
            message: message.into(),
        }
    }

    /// Create a timeout error
    pub fn timeout(operation: impl Into<String>, timeout_seconds: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            timeout_seconds,
        }
    }

    /// Create a pool exhausted error
    pub fn pool_exhausted(message: impl Into<String>) -> Self {
        Self::PoolExhausted {
            message: message.into(),
        }
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

/// Conversion from sqlx::Error to MessagingError
impl From<sqlx::Error> for MessagingError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => MessagingError::database_query("query", "No rows found"),
            sqlx::Error::Database(db_err) => {
                MessagingError::database_query("database", db_err.to_string())
            }
            sqlx::Error::PoolTimedOut => {
                MessagingError::timeout("database_pool", 30) // Default timeout
            }
            sqlx::Error::PoolClosed => MessagingError::pool_exhausted("Database pool is closed"),
            sqlx::Error::Configuration(config_err) => {
                MessagingError::configuration("database", config_err.to_string())
            }
            _ => MessagingError::database_connection(err.to_string()),
        }
    }
}

/// Conversion from serde_json::Error to MessagingError
impl From<serde_json::Error> for MessagingError {
    fn from(err: serde_json::Error) -> Self {
        if err.is_syntax() {
            MessagingError::message_deserialization(err.to_string())
        } else {
            MessagingError::message_serialization(err.to_string())
        }
    }
}

/// Conversion from pgmq::errors::PgmqError to MessagingError
impl From<pgmq::errors::PgmqError> for MessagingError {
    fn from(err: pgmq::errors::PgmqError) -> Self {
        MessagingError::queue_operation("unknown", "pgmq", err.to_string())
    }
}

/// Conversion from circuit breaker errors
impl From<CircuitBreakerError<MessagingError>> for MessagingError {
    fn from(err: CircuitBreakerError<MessagingError>) -> Self {
        match err {
            CircuitBreakerError::CircuitOpen { component } => {
                MessagingError::circuit_breaker_open(component)
            }
            CircuitBreakerError::OperationFailed(inner) => inner,
            CircuitBreakerError::ConfigurationError(msg) => {
                MessagingError::configuration("circuit_breaker", msg)
            }
        }
    }
}

/// Conversion from String to MessagingError
impl From<String> for MessagingError {
    fn from(message: String) -> Self {
        MessagingError::internal(message)
    }
}

/// Convert `Box<dyn Error>` patterns to MessagingError for legacy compatibility
impl From<Box<dyn std::error::Error + Send + Sync>> for MessagingError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        // Get the error string before attempting downcasts
        let error_string = err.to_string();

        // Try to downcast to known error types first
        if let Ok(sqlx_err) = err.downcast::<sqlx::Error>() {
            return (*sqlx_err).into();
        }

        // If downcast fails, use the error string we captured
        MessagingError::internal(error_string)
    }
}

/// Result type alias for messaging operations
pub type MessagingResult<T> = Result<T, MessagingError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_messaging_error_creation() {
        let db_err = MessagingError::database_connection("Connection failed");
        assert!(matches!(db_err, MessagingError::DatabaseConnection { .. }));

        let queue_err = MessagingError::queue_operation("test_queue", "send", "Failed to send");
        assert!(matches!(queue_err, MessagingError::QueueOperation { .. }));

        let timeout_err = MessagingError::timeout("operation", 30);
        assert!(matches!(timeout_err, MessagingError::Timeout { .. }));
    }

    #[test]
    fn test_error_conversions() {
        // Test sqlx::Error conversion
        let sqlx_err = sqlx::Error::PoolTimedOut;
        let messaging_err: MessagingError = sqlx_err.into();
        assert!(matches!(messaging_err, MessagingError::Timeout { .. }));

        // Test serde_json::Error conversion
        let json_str = "{invalid json";
        let json_err = serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let messaging_err: MessagingError = json_err.into();
        assert!(matches!(
            messaging_err,
            MessagingError::MessageDeserialization { .. }
        ));
    }

    #[test]
    fn test_error_display() {
        let db_err = MessagingError::database_connection("Test connection failed");
        let display_str = format!("{db_err}");
        assert!(display_str.contains("Database connection error"));
        assert!(display_str.contains("Test connection failed"));

        let queue_err = MessagingError::queue_operation("my_queue", "read", "Read failed");
        let display_str = format!("{queue_err}");
        assert!(display_str.contains("Queue operation failed"));
        assert!(display_str.contains("my_queue"));
        assert!(display_str.contains("read"));
        assert!(display_str.contains("Read failed"));
    }
}
