use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskerError {
    DatabaseError(String),
    StateTransitionError(String),
    OrchestrationError(String),
    EventError(String),
    ValidationError(String),
    ConfigurationError(String),
    FFIError(String),
}

impl fmt::Display for TaskerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskerError::DatabaseError(msg) => write!(f, "Database error: {msg}"),
            TaskerError::StateTransitionError(msg) => write!(f, "State transition error: {msg}"),
            TaskerError::OrchestrationError(msg) => write!(f, "Orchestration error: {msg}"),
            TaskerError::EventError(msg) => write!(f, "Event error: {msg}"),
            TaskerError::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            TaskerError::ConfigurationError(msg) => write!(f, "Configuration error: {msg}"),
            TaskerError::FFIError(msg) => write!(f, "FFI error: {msg}"),
        }
    }
}

impl std::error::Error for TaskerError {}

pub type Result<T> = std::result::Result<T, TaskerError>;
