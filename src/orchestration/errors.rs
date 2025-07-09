//! # Orchestration Errors
//!
//! Comprehensive error handling for the orchestration system.
//!
//! This module provides error types that cover all aspects of orchestration:
//! - Task execution errors
//! - Step execution errors
//! - Registry errors
//! - Configuration errors
//! - State management errors
//! - Event publishing errors

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

/// Result type for orchestration operations
pub type OrchestrationResult<T> = Result<T, OrchestrationError>;

/// Comprehensive orchestration error types
#[derive(Debug, Clone)]
pub enum OrchestrationError {
    /// Task execution failed
    TaskExecutionFailed {
        task_id: i64,
        reason: String,
        error_code: Option<String>,
    },

    /// Step execution failed
    StepExecutionFailed {
        step_id: i64,
        task_id: i64,
        reason: String,
        error_code: Option<String>,
        retry_after: Option<Duration>,
    },

    /// Database operation failed
    DatabaseError { operation: String, reason: String },

    /// State transition failed
    StateTransitionFailed {
        entity_type: String,
        entity_id: i64,
        reason: String,
    },

    /// Task is in invalid state for operation
    InvalidTaskState {
        task_id: i64,
        current_state: String,
        expected_states: Vec<String>,
    },

    /// Step is in invalid state for operation
    InvalidStepState {
        step_id: i64,
        current_state: String,
        expected_states: Vec<String>,
    },

    /// Registry operation failed
    RegistryError { operation: String, reason: String },

    /// Handler not found in registry
    HandlerNotFound {
        namespace: String,
        name: String,
        version: String,
    },

    /// Configuration error
    ConfigurationError { source: String, reason: String },

    /// YAML parsing error
    YamlParsingError { file_path: String, reason: String },

    /// Event publishing error
    EventPublishingError { event_type: String, reason: String },

    /// SQL function execution error
    SqlFunctionError {
        function_name: String,
        reason: String,
    },

    /// Step state machine not found
    StepStateMachineNotFound { step_id: i64 },

    /// Dependency resolution error
    DependencyResolutionError { task_id: i64, reason: String },

    /// Timeout error
    TimeoutError {
        operation: String,
        timeout_duration: Duration,
    },

    /// Validation error
    ValidationError { field: String, reason: String },

    /// FFI bridge error
    FfiBridgeError { operation: String, reason: String },

    /// Framework integration error
    FrameworkIntegrationError { framework: String, reason: String },

    /// Step execution error (from StepExecutor)
    ExecutionError(ExecutionError),
}

/// Step execution error types with proper classification
#[derive(Debug, Clone)]
pub enum StepExecutionError {
    /// Permanent error - should not be retried
    Permanent {
        message: String,
        error_code: Option<String>,
    },

    /// Retryable error - can be retried with backoff
    Retryable {
        message: String,
        retry_after: Option<u64>, // seconds
        skip_retry: bool,
        context: Option<HashMap<String, serde_json::Value>>,
    },

    /// Timeout error
    Timeout {
        message: String,
        timeout_duration: Duration,
    },

    /// Network error
    NetworkError {
        message: String,
        status_code: Option<u16>,
    },
}

/// Registry error types
#[derive(Debug, Clone)]
pub enum RegistryError {
    /// Handler not found
    NotFound(String),

    /// Registration conflict
    Conflict { key: String, reason: String },

    /// Validation error
    ValidationError {
        handler_class: String,
        reason: String,
    },

    /// Thread safety error
    ThreadSafetyError { operation: String, reason: String },
}

/// Configuration error types
#[derive(Debug, Clone)]
pub enum ConfigurationError {
    /// File not found
    FileNotFound(String),

    /// Parse error
    ParseError { file_path: String, reason: String },

    /// Schema validation error
    SchemaValidationError { schema_path: String, reason: String },

    /// Environment override error
    EnvironmentOverrideError { key: String, reason: String },
}

/// Event publishing error types
#[derive(Debug, Clone)]
pub enum EventError {
    /// Publishing failed
    PublishingFailed { event_type: String, reason: String },

    /// Serialization error
    SerializationError { event_type: String, reason: String },

    /// FFI bridge error
    FfiBridgeError { reason: String },
}

/// State management error types
#[derive(Debug, Clone)]
pub enum StateError {
    /// Invalid transition
    InvalidTransition {
        entity_type: String,
        entity_id: i64,
        from_state: String,
        to_state: String,
    },

    /// State not found
    StateNotFound { entity_type: String, entity_id: i64 },

    /// Database error
    DatabaseError(String),

    /// Concurrent modification
    ConcurrentModification { entity_type: String, entity_id: i64 },
}

/// Discovery error types
#[derive(Debug, Clone)]
pub enum DiscoveryError {
    /// Database error
    DatabaseError(String),

    /// SQL function error
    SqlFunctionError {
        function_name: String,
        reason: String,
    },

    /// Dependency cycle detected
    DependencyCycle { task_id: i64, cycle_steps: Vec<i64> },
}

/// Step execution error types
#[derive(Debug, Clone)]
pub enum ExecutionError {
    /// Step execution failed
    StepExecutionFailed {
        step_id: i64,
        reason: String,
        error_code: Option<String>,
    },

    /// Invalid step state for execution
    InvalidStepState {
        step_id: i64,
        current_state: String,
        expected_state: String,
    },

    /// State transition error during execution
    StateTransitionError { step_id: i64, reason: String },

    /// Concurrency control error
    ConcurrencyError { step_id: i64, reason: String },

    /// No result returned from execution
    NoResultReturned { step_id: i64 },

    /// Execution timeout
    ExecutionTimeout {
        step_id: i64,
        timeout_duration: Duration,
    },

    /// Retry limit exceeded
    RetryLimitExceeded { step_id: i64, max_attempts: u32 },
}

// Implement Display for all error types
impl fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrchestrationError::TaskExecutionFailed {
                task_id,
                reason,
                error_code,
            } => {
                write!(
                    f,
                    "Task {task_id} execution failed: {reason} (code: {error_code:?})"
                )
            }
            OrchestrationError::StepExecutionFailed {
                step_id,
                task_id,
                reason,
                error_code,
                retry_after,
            } => {
                write!(
                    f,
                    "Step {step_id} in task {task_id} execution failed: {reason} (code: {error_code:?}, retry_after: {retry_after:?})"
                )
            }
            OrchestrationError::DatabaseError { operation, reason } => {
                write!(f, "Database operation '{operation}' failed: {reason}")
            }
            OrchestrationError::StateTransitionFailed {
                entity_type,
                entity_id,
                reason,
            } => {
                write!(
                    f,
                    "State transition failed for {entity_type} {entity_id}: {reason}"
                )
            }
            OrchestrationError::InvalidTaskState {
                task_id,
                current_state,
                expected_states,
            } => {
                write!(
                    f,
                    "Task {task_id} is in invalid state '{current_state}', expected one of: {expected_states:?}"
                )
            }
            OrchestrationError::InvalidStepState {
                step_id,
                current_state,
                expected_states,
            } => {
                write!(
                    f,
                    "Step {step_id} is in invalid state '{current_state}', expected one of: {expected_states:?}"
                )
            }
            OrchestrationError::RegistryError { operation, reason } => {
                write!(f, "Registry operation '{operation}' failed: {reason}")
            }
            OrchestrationError::HandlerNotFound {
                namespace,
                name,
                version,
            } => {
                write!(f, "Handler not found: {namespace}/{name}/{version}")
            }
            OrchestrationError::ConfigurationError { source, reason } => {
                write!(f, "Configuration error in '{source}': {reason}")
            }
            OrchestrationError::YamlParsingError { file_path, reason } => {
                write!(f, "YAML parsing error in '{file_path}': {reason}")
            }
            OrchestrationError::EventPublishingError { event_type, reason } => {
                write!(f, "Event publishing error for '{event_type}': {reason}")
            }
            OrchestrationError::SqlFunctionError {
                function_name,
                reason,
            } => {
                write!(f, "SQL function '{function_name}' error: {reason}")
            }
            OrchestrationError::StepStateMachineNotFound { step_id } => {
                write!(f, "Step state machine not found for step {step_id}")
            }
            OrchestrationError::DependencyResolutionError { task_id, reason } => {
                write!(
                    f,
                    "Dependency resolution error for task {task_id}: {reason}"
                )
            }
            OrchestrationError::TimeoutError {
                operation,
                timeout_duration,
            } => {
                write!(
                    f,
                    "Operation '{operation}' timed out after {timeout_duration:?}"
                )
            }
            OrchestrationError::ValidationError { field, reason } => {
                write!(f, "Validation error for field '{field}': {reason}")
            }
            OrchestrationError::FfiBridgeError { operation, reason } => {
                write!(f, "FFI bridge error during '{operation}': {reason}")
            }
            OrchestrationError::FrameworkIntegrationError { framework, reason } => {
                write!(
                    f,
                    "Framework integration error with '{framework}': {reason}"
                )
            }
            OrchestrationError::ExecutionError(err) => {
                write!(f, "Step execution error: {err}")
            }
        }
    }
}

impl fmt::Display for StepExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepExecutionError::Permanent {
                message,
                error_code,
            } => {
                write!(f, "Permanent error: {message} (code: {error_code:?})")
            }
            StepExecutionError::Retryable {
                message,
                retry_after,
                skip_retry,
                context: _,
            } => {
                write!(
                    f,
                    "Retryable error: {message} (retry_after: {retry_after:?}s, skip_retry: {skip_retry})"
                )
            }
            StepExecutionError::Timeout {
                message,
                timeout_duration,
            } => {
                write!(
                    f,
                    "Timeout error: {message} (timeout: {timeout_duration:?})"
                )
            }
            StepExecutionError::NetworkError {
                message,
                status_code,
            } => {
                write!(f, "Network error: {message} (status: {status_code:?})")
            }
        }
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::NotFound(key) => write!(f, "Handler not found: {key}"),
            RegistryError::Conflict { key, reason } => {
                write!(f, "Registration conflict for '{key}': {reason}")
            }
            RegistryError::ValidationError {
                handler_class,
                reason,
            } => {
                write!(
                    f,
                    "Validation error for handler '{handler_class}': {reason}"
                )
            }
            RegistryError::ThreadSafetyError { operation, reason } => {
                write!(f, "Thread safety error in '{operation}': {reason}")
            }
        }
    }
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigurationError::FileNotFound(path) => {
                write!(f, "Configuration file not found: {path}")
            }
            ConfigurationError::ParseError { file_path, reason } => {
                write!(f, "Parse error in '{file_path}': {reason}")
            }
            ConfigurationError::SchemaValidationError {
                schema_path,
                reason,
            } => {
                write!(f, "Schema validation error in '{schema_path}': {reason}")
            }
            ConfigurationError::EnvironmentOverrideError { key, reason } => {
                write!(f, "Environment override error for '{key}': {reason}")
            }
        }
    }
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventError::PublishingFailed { event_type, reason } => {
                write!(f, "Event publishing failed for '{event_type}': {reason}")
            }
            EventError::SerializationError { event_type, reason } => {
                write!(f, "Event serialization error for '{event_type}': {reason}")
            }
            EventError::FfiBridgeError { reason } => {
                write!(f, "FFI bridge error: {reason}")
            }
        }
    }
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::InvalidTransition {
                entity_type,
                entity_id,
                from_state,
                to_state,
            } => {
                write!(
                    f,
                    "Invalid transition for {entity_type} {entity_id}: {from_state} -> {to_state}"
                )
            }
            StateError::StateNotFound {
                entity_type,
                entity_id,
            } => {
                write!(f, "State not found for {entity_type} {entity_id}")
            }
            StateError::DatabaseError(reason) => write!(f, "Database error: {reason}"),
            StateError::ConcurrentModification {
                entity_type,
                entity_id,
            } => {
                write!(
                    f,
                    "Concurrent modification detected for {entity_type} {entity_id}"
                )
            }
        }
    }
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscoveryError::DatabaseError(reason) => write!(f, "Database error: {reason}"),
            DiscoveryError::SqlFunctionError {
                function_name,
                reason,
            } => {
                write!(f, "SQL function '{function_name}' error: {reason}")
            }
            DiscoveryError::DependencyCycle {
                task_id,
                cycle_steps,
            } => {
                write!(
                    f,
                    "Dependency cycle detected in task {task_id}: steps {cycle_steps:?}"
                )
            }
        }
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionError::StepExecutionFailed {
                step_id,
                reason,
                error_code,
            } => {
                write!(
                    f,
                    "Step {step_id} execution failed: {reason} (code: {error_code:?})"
                )
            }
            ExecutionError::InvalidStepState {
                step_id,
                current_state,
                expected_state,
            } => {
                write!(
                    f,
                    "Step {step_id} in invalid state '{current_state}', expected '{expected_state}'"
                )
            }
            ExecutionError::StateTransitionError { step_id, reason } => {
                write!(f, "State transition failed for step {step_id}: {reason}")
            }
            ExecutionError::ConcurrencyError { step_id, reason } => {
                write!(f, "Concurrency error for step {step_id}: {reason}")
            }
            ExecutionError::NoResultReturned { step_id } => {
                write!(f, "No result returned from execution of step {step_id}")
            }
            ExecutionError::ExecutionTimeout {
                step_id,
                timeout_duration,
            } => {
                write!(
                    f,
                    "Step {step_id} execution timed out after {timeout_duration:?}"
                )
            }
            ExecutionError::RetryLimitExceeded {
                step_id,
                max_attempts,
            } => {
                write!(
                    f,
                    "Step {step_id} exceeded retry limit of {max_attempts} attempts"
                )
            }
        }
    }
}

// Implement std::error::Error for all error types
impl std::error::Error for OrchestrationError {}
impl std::error::Error for StepExecutionError {}
impl std::error::Error for RegistryError {}
impl std::error::Error for ConfigurationError {}
impl std::error::Error for EventError {}
impl std::error::Error for StateError {}
impl std::error::Error for DiscoveryError {}
impl std::error::Error for ExecutionError {}

// Conversion implementations
impl From<sqlx::Error> for OrchestrationError {
    fn from(err: sqlx::Error) -> Self {
        OrchestrationError::DatabaseError {
            operation: "sqlx_operation".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for OrchestrationError {
    fn from(err: serde_json::Error) -> Self {
        OrchestrationError::ConfigurationError {
            source: "json_serialization".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<RegistryError> for OrchestrationError {
    fn from(err: RegistryError) -> Self {
        OrchestrationError::RegistryError {
            operation: "registry_operation".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<ConfigurationError> for OrchestrationError {
    fn from(err: ConfigurationError) -> Self {
        OrchestrationError::ConfigurationError {
            source: "configuration_manager".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<EventError> for OrchestrationError {
    fn from(err: EventError) -> Self {
        OrchestrationError::EventPublishingError {
            event_type: "unknown".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<StateError> for OrchestrationError {
    fn from(err: StateError) -> Self {
        OrchestrationError::StateTransitionFailed {
            entity_type: "unknown".to_string(),
            entity_id: 0,
            reason: err.to_string(),
        }
    }
}

impl From<DiscoveryError> for OrchestrationError {
    fn from(err: DiscoveryError) -> Self {
        OrchestrationError::DependencyResolutionError {
            task_id: 0,
            reason: err.to_string(),
        }
    }
}

impl From<crate::state_machine::errors::StateMachineError> for OrchestrationError {
    fn from(err: crate::state_machine::errors::StateMachineError) -> Self {
        OrchestrationError::StateTransitionFailed {
            entity_type: "state_machine".to_string(),
            entity_id: 0,
            reason: err.to_string(),
        }
    }
}

impl From<ExecutionError> for OrchestrationError {
    fn from(err: ExecutionError) -> Self {
        OrchestrationError::ExecutionError(err)
    }
}
