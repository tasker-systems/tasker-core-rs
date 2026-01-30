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

    // TAS-133b: Additional helper methods for MessagingService providers

    /// Create a connection error
    pub fn connection(message: impl Into<String>) -> Self {
        Self::DatabaseConnection {
            message: message.into(),
        }
    }

    /// Create a serialization error (alias for message_serialization)
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::MessageSerialization {
            message: message.into(),
        }
    }

    /// Create a deserialization error (alias for message_deserialization)
    pub fn deserialization(message: impl Into<String>) -> Self {
        Self::MessageDeserialization {
            message: message.into(),
        }
    }

    /// Create a queue creation error
    pub fn queue_creation(queue_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::QueueOperation {
            queue_name: queue_name.into(),
            operation: "create".to_string(),
            message: message.into(),
        }
    }

    /// Create a send error
    pub fn send(queue_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::QueueOperation {
            queue_name: queue_name.into(),
            operation: "send".to_string(),
            message: message.into(),
        }
    }

    /// Create a receive error
    pub fn receive(queue_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::QueueOperation {
            queue_name: queue_name.into(),
            operation: "receive".to_string(),
            message: message.into(),
        }
    }

    /// Create an ack error
    pub fn ack(queue_name: impl Into<String>, message_id: i64, message: impl Into<String>) -> Self {
        Self::QueueOperation {
            queue_name: queue_name.into(),
            operation: format!("ack(msg_id={})", message_id),
            message: message.into(),
        }
    }

    /// Create a nack error
    pub fn nack(
        queue_name: impl Into<String>,
        message_id: i64,
        message: impl Into<String>,
    ) -> Self {
        Self::QueueOperation {
            queue_name: queue_name.into(),
            operation: format!("nack(msg_id={})", message_id),
            message: message.into(),
        }
    }

    /// Create an extend visibility error
    pub fn extend_visibility(
        queue_name: impl Into<String>,
        message_id: i64,
        message: impl Into<String>,
    ) -> Self {
        Self::QueueOperation {
            queue_name: queue_name.into(),
            operation: format!("extend_visibility(msg_id={})", message_id),
            message: message.into(),
        }
    }

    /// Create a queue stats error
    pub fn queue_stats(queue_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::QueueOperation {
            queue_name: queue_name.into(),
            operation: "queue_stats".to_string(),
            message: message.into(),
        }
    }

    /// Create a health check error
    pub fn health_check(message: impl Into<String>) -> Self {
        Self::Internal {
            message: format!("health_check: {}", message.into()),
        }
    }

    /// Create an invalid receipt handle error
    pub fn invalid_receipt_handle(handle: impl Into<String>) -> Self {
        Self::Internal {
            message: format!("Invalid receipt handle: {}", handle.into()),
        }
    }

    /// Create a message not found error
    pub fn message_not_found(message_id: impl Into<String>) -> Self {
        Self::Internal {
            message: format!("Message not found: {}", message_id.into()),
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

// Conversion from tasker-pgmq errors to our more detailed error types
impl From<tasker_pgmq::PgmqNotifyError> for MessagingError {
    fn from(err: tasker_pgmq::PgmqNotifyError) -> Self {
        match err {
            tasker_pgmq::PgmqNotifyError::Database(sqlx_err) => sqlx_err.into(),
            tasker_pgmq::PgmqNotifyError::Serialization(json_err) => json_err.into(),
            tasker_pgmq::PgmqNotifyError::Configuration { message } => {
                MessagingError::configuration("tasker-pgmq", message)
            }
            tasker_pgmq::PgmqNotifyError::InvalidChannel { channel } => {
                MessagingError::configuration(
                    "tasker-pgmq",
                    format!("Invalid channel: {}", channel),
                )
            }
            tasker_pgmq::PgmqNotifyError::InvalidPattern { pattern } => {
                MessagingError::configuration(
                    "tasker-pgmq",
                    format!("Invalid pattern: {}", pattern),
                )
            }
            tasker_pgmq::PgmqNotifyError::NotConnected => {
                MessagingError::database_connection("tasker-pgmq listener not connected")
            }
            tasker_pgmq::PgmqNotifyError::AlreadyListening { channel } => {
                MessagingError::configuration(
                    "tasker-pgmq",
                    format!("Already listening to channel: {}", channel),
                )
            }
            tasker_pgmq::PgmqNotifyError::Regex(regex_err) => {
                MessagingError::configuration("tasker-pgmq", format!("Regex error: {}", regex_err))
            }
            tasker_pgmq::PgmqNotifyError::Generic(err) => {
                MessagingError::internal(format!("tasker-pgmq error: {}", err))
            }
            tasker_pgmq::PgmqNotifyError::Messaging(msg_err) => {
                // This is a bit recursive, but should handle the conversion
                MessagingError::internal(format!("tasker-pgmq messaging error: {}", msg_err))
            }
            tasker_pgmq::PgmqNotifyError::Pgmq(pgmq_err) => {
                MessagingError::internal(format!("PGMQ error via tasker-pgmq: {}", pgmq_err))
            }
        }
    }
}

// Conversion from tasker-pgmq MessagingError to our MessagingError
impl From<tasker_pgmq::MessagingError> for MessagingError {
    fn from(err: tasker_pgmq::MessagingError) -> Self {
        match err {
            tasker_pgmq::MessagingError::Database(sqlx_err) => sqlx_err.into(),
            tasker_pgmq::MessagingError::Serialization(json_err) => json_err.into(),
            tasker_pgmq::MessagingError::Pgmq(msg) => {
                MessagingError::internal(format!("PGMQ error: {}", msg))
            }
            tasker_pgmq::MessagingError::Configuration(msg) => {
                MessagingError::configuration("pgmq", msg)
            }
            tasker_pgmq::MessagingError::Generic(msg) => {
                MessagingError::internal(format!("tasker-pgmq: {}", msg))
            }
        }
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

    // ---------------------------------------------------------------
    // Helper method tests: individual constructors
    // ---------------------------------------------------------------

    #[test]
    fn test_queue_not_found() {
        let err = MessagingError::queue_not_found("test_queue");
        assert!(matches!(
            err,
            MessagingError::QueueNotFound { ref queue_name } if queue_name == "test_queue"
        ));
        assert!(format!("{err}").contains("test_queue"));
    }

    #[test]
    fn test_message_serialization() {
        let err = MessagingError::message_serialization("bad data");
        assert!(matches!(
            err,
            MessagingError::MessageSerialization { ref message } if message == "bad data"
        ));
        assert!(format!("{err}").contains("bad data"));
    }

    #[test]
    fn test_message_deserialization() {
        let err = MessagingError::message_deserialization("bad data");
        assert!(matches!(
            err,
            MessagingError::MessageDeserialization { ref message } if message == "bad data"
        ));
        assert!(format!("{err}").contains("bad data"));
    }

    #[test]
    fn test_circuit_breaker_open() {
        let err = MessagingError::circuit_breaker_open("db");
        assert!(matches!(
            err,
            MessagingError::CircuitBreakerOpen { ref component } if component == "db"
        ));
        assert!(format!("{err}").contains("db"));
    }

    #[test]
    fn test_configuration() {
        let err = MessagingError::configuration("pgmq", "bad config");
        assert!(matches!(
            err,
            MessagingError::Configuration { ref component, ref message }
                if component == "pgmq" && message == "bad config"
        ));
        let display = format!("{err}");
        assert!(display.contains("pgmq"));
        assert!(display.contains("bad config"));
    }

    #[test]
    fn test_pool_exhausted() {
        let err = MessagingError::pool_exhausted("no pools");
        assert!(matches!(
            err,
            MessagingError::PoolExhausted { ref message } if message == "no pools"
        ));
        assert!(format!("{err}").contains("no pools"));
    }

    #[test]
    fn test_internal() {
        let err = MessagingError::internal("internal error");
        assert!(matches!(
            err,
            MessagingError::Internal { ref message } if message == "internal error"
        ));
        assert!(format!("{err}").contains("internal error"));
    }

    // ---------------------------------------------------------------
    // Alias helper methods
    // ---------------------------------------------------------------

    #[test]
    fn test_connection_alias() {
        let err = MessagingError::connection("conn failed");
        assert!(matches!(
            err,
            MessagingError::DatabaseConnection { ref message } if message == "conn failed"
        ));
        assert!(format!("{err}").contains("conn failed"));
    }

    #[test]
    fn test_serialization_alias() {
        let err = MessagingError::serialization("ser error");
        assert!(matches!(
            err,
            MessagingError::MessageSerialization { ref message } if message == "ser error"
        ));
        assert!(format!("{err}").contains("ser error"));
    }

    #[test]
    fn test_deserialization_alias() {
        let err = MessagingError::deserialization("deser error");
        assert!(matches!(
            err,
            MessagingError::MessageDeserialization { ref message } if message == "deser error"
        ));
        assert!(format!("{err}").contains("deser error"));
    }

    // ---------------------------------------------------------------
    // Queue operation helper methods
    // ---------------------------------------------------------------

    #[test]
    fn test_queue_creation() {
        let err = MessagingError::queue_creation("q1", "failed");
        assert!(matches!(
            err,
            MessagingError::QueueOperation {
                ref queue_name,
                ref operation,
                ref message
            } if queue_name == "q1" && operation == "create" && message == "failed"
        ));
    }

    #[test]
    fn test_send() {
        let err = MessagingError::send("q1", "send failed");
        assert!(matches!(
            err,
            MessagingError::QueueOperation {
                ref queue_name,
                ref operation,
                ref message
            } if queue_name == "q1" && operation == "send" && message == "send failed"
        ));
    }

    #[test]
    fn test_receive() {
        let err = MessagingError::receive("q1", "recv failed");
        assert!(matches!(
            err,
            MessagingError::QueueOperation {
                ref queue_name,
                ref operation,
                ref message
            } if queue_name == "q1" && operation == "receive" && message == "recv failed"
        ));
    }

    #[test]
    fn test_ack() {
        let err = MessagingError::ack("q1", 42, "ack failed");
        assert!(matches!(
            err,
            MessagingError::QueueOperation {
                ref queue_name,
                ref operation,
                ref message
            } if queue_name == "q1" && operation.contains("ack") && message == "ack failed"
        ));
        let display = format!("{err}");
        assert!(display.contains("42"));
    }

    #[test]
    fn test_nack() {
        let err = MessagingError::nack("q1", 42, "nack failed");
        assert!(matches!(
            err,
            MessagingError::QueueOperation {
                ref queue_name,
                ref operation,
                ref message
            } if queue_name == "q1" && operation.contains("nack") && message == "nack failed"
        ));
        let display = format!("{err}");
        assert!(display.contains("42"));
    }

    #[test]
    fn test_extend_visibility() {
        let err = MessagingError::extend_visibility("q1", 42, "failed");
        assert!(matches!(
            err,
            MessagingError::QueueOperation { ref queue_name, ref operation, .. }
                if queue_name == "q1" && operation.contains("extend_visibility")
        ));
        let display = format!("{err}");
        assert!(display.contains("42"));
    }

    #[test]
    fn test_queue_stats() {
        let err = MessagingError::queue_stats("q1", "stats failed");
        assert!(matches!(
            err,
            MessagingError::QueueOperation {
                ref queue_name,
                ref operation,
                ref message
            } if queue_name == "q1" && operation == "queue_stats" && message == "stats failed"
        ));
    }

    // ---------------------------------------------------------------
    // Internal-wrapping helper methods
    // ---------------------------------------------------------------

    #[test]
    fn test_health_check() {
        let err = MessagingError::health_check("check failed");
        assert!(matches!(err, MessagingError::Internal { .. }));
        let display = format!("{err}");
        assert!(display.contains("health_check:"));
        assert!(display.contains("check failed"));
    }

    #[test]
    fn test_invalid_receipt_handle() {
        let err = MessagingError::invalid_receipt_handle("handle123");
        assert!(matches!(err, MessagingError::Internal { .. }));
        let display = format!("{err}");
        assert!(display.contains("Invalid receipt handle"));
        assert!(display.contains("handle123"));
    }

    #[test]
    fn test_message_not_found() {
        let err = MessagingError::message_not_found("msg42");
        assert!(matches!(err, MessagingError::Internal { .. }));
        let display = format!("{err}");
        assert!(display.contains("Message not found"));
        assert!(display.contains("msg42"));
    }

    // ---------------------------------------------------------------
    // Conversion tests
    // ---------------------------------------------------------------

    #[test]
    fn test_serde_json_non_syntax_error_converts_to_serialization() {
        // Deserializing a JSON string into u32 produces a "data" error, not a syntax error
        let serde_err = serde_json::from_str::<u32>("\"not_a_number\"").unwrap_err();
        assert!(!serde_err.is_syntax(), "expected a non-syntax serde error");
        let messaging_err: MessagingError = serde_err.into();
        assert!(matches!(
            messaging_err,
            MessagingError::MessageSerialization { .. }
        ));
    }

    #[test]
    fn test_from_string_converts_to_internal() {
        let err: MessagingError = String::from("something went wrong").into();
        assert!(matches!(
            err,
            MessagingError::Internal { ref message } if message == "something went wrong"
        ));
    }

    #[test]
    fn test_from_circuit_breaker_error_circuit_open() {
        use crate::resilience::CircuitBreakerError;

        let cb_err: CircuitBreakerError<MessagingError> = CircuitBreakerError::CircuitOpen {
            component: "db_pool".to_string(),
        };
        let messaging_err: MessagingError = cb_err.into();
        assert!(matches!(
            messaging_err,
            MessagingError::CircuitBreakerOpen { ref component } if component == "db_pool"
        ));
    }

    #[test]
    fn test_from_circuit_breaker_error_operation_failed() {
        use crate::resilience::CircuitBreakerError;

        let inner = MessagingError::database_connection("pool lost");
        let cb_err: CircuitBreakerError<MessagingError> =
            CircuitBreakerError::OperationFailed(inner);
        let messaging_err: MessagingError = cb_err.into();
        assert!(matches!(
            messaging_err,
            MessagingError::DatabaseConnection { ref message } if message == "pool lost"
        ));
    }

    #[test]
    fn test_from_circuit_breaker_error_configuration_error() {
        use crate::resilience::CircuitBreakerError;

        let cb_err: CircuitBreakerError<MessagingError> =
            CircuitBreakerError::ConfigurationError("bad threshold".to_string());
        let messaging_err: MessagingError = cb_err.into();
        assert!(matches!(
            messaging_err,
            MessagingError::Configuration {
                ref component,
                ref message
            } if component == "circuit_breaker" && message == "bad threshold"
        ));
    }
}
