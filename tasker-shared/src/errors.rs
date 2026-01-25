//! Error types for the Tasker system.
//!

use crate::config::ConfigurationError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum TaskerError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("State transition error: {0}")]
    StateTransitionError(String),
    #[error("Orchestration error: {0}")]
    OrchestrationError(String),
    #[error("Event error: {0}")]
    EventError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("FFI error: {0}")]
    FFIError(String),
    #[error("Messaging error: {0}")]
    MessagingError(String),
    #[error("Cache error: {0}")]
    CacheError(String),
    #[error("Worker error: {0}")]
    WorkerError(String),
    // Additional variants needed for executor system
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("Timeout error: {0}")]
    Timeout(String),
    #[error("Circuit breaker open: {0}")]
    CircuitBreakerOpen(String),
    #[error("Task initialization error: {message} for {name} in {namespace} version {version}")]
    TaskInitializationError {
        message: String,
        name: String,
        namespace: String,
        version: String,
    },
    #[error("State machine error: {0}")]
    StateMachineError(String),
    #[error("State machine action error: {0}")]
    StateMachineActionError(String),
    #[error("State machine guard error: {0}")]
    StateMachineGuardError(String),
    #[error("State machine persistence error: {0}")]
    StateMachinePersistenceError(String),
}

impl From<serde_json::Error> for TaskerError {
    fn from(error: serde_json::Error) -> Self {
        TaskerError::ValidationError(format!("JSON serialization error: {error}"))
    }
}

impl From<sqlx::Error> for TaskerError {
    fn from(err: sqlx::Error) -> Self {
        TaskerError::DatabaseError(err.to_string())
    }
}

impl From<crate::messaging::MessagingError> for TaskerError {
    fn from(error: crate::messaging::MessagingError) -> Self {
        TaskerError::MessagingError(error.to_string())
    }
}

pub type TaskerResult<T> = anyhow::Result<T, TaskerError>;
pub type OrchestrationResult<T> = anyhow::Result<T, OrchestrationError>;

/// Specific orchestration error types for detailed error handling
#[derive(Debug, Clone, PartialEq, Error)]
pub enum OrchestrationError {
    /// Database operation failed
    #[error("Database error: {operation} - {reason}")]
    DatabaseError { operation: String, reason: String },
    /// Task is in invalid state for operation
    #[error(
        "Task {task_uuid} is in invalid state {current_state}, expected one of {expected_states:?}"
    )]
    InvalidTaskState {
        task_uuid: Uuid,
        current_state: String,
        expected_states: Vec<String>,
    },
    /// Workflow step not found
    #[error("Workflow step {step_uuid} not found")]
    WorkflowStepNotFound { step_uuid: Uuid },
    /// Step state machine not found
    #[error("Step state machine not found for {step_uuid}")]
    StepStateMachineNotFound { step_uuid: Uuid },
    /// State verification failed
    #[error("State verification failed for {step_uuid}: {reason}")]
    StateVerificationFailed { step_uuid: Uuid, reason: String },
    /// Task execution delegation failed
    #[error("Task execution delegation failed for {task_uuid} to {framework}: {reason}")]
    DelegationFailed {
        task_uuid: Uuid,
        framework: String,
        reason: String,
    },
    /// Task execution failed
    #[error("Task execution failed for {task_uuid}: {reason}")]
    TaskExecutionFailed {
        task_uuid: Uuid,
        reason: String,
        error_code: Option<String>,
    },

    /// Step execution failed
    #[error("Step execution failed for {step_uuid}: {reason}")]
    StepExecutionFailed {
        step_uuid: Uuid,
        task_uuid: Option<Uuid>,
        reason: String,
        error_code: Option<String>,
        retry_after: Option<Duration>,
    },

    /// State transition failed
    #[error("State transition failed for {entity_type} {entity_uuid}: {reason}")]
    StateTransitionFailed {
        entity_type: String,
        entity_uuid: Uuid,
        reason: String,
    },

    /// Step is in invalid state for operation
    #[error("Step {step_uuid} is in invalid state {current_state} for operation")]
    InvalidStepState {
        step_uuid: Uuid,
        current_state: String,
        expected_states: Vec<String>,
    },

    /// Registry operation failed
    #[error("Registry operation failed for {operation}: {reason}")]
    RegistryError { operation: String, reason: String },

    /// Handler not found in registry
    #[error(
        "Handler not found in registry for namespace {namespace}, name {name}, version {version}"
    )]
    HandlerNotFound {
        namespace: String,
        name: String,
        version: String,
    },

    /// Step handler not found in registry
    #[error("Step handler not found in registry for step {step_uuid}, reason {reason}")]
    StepHandlerNotFound { step_uuid: Uuid, reason: String },

    /// Configuration error
    #[error("Configuration error for {config_source}: {reason}")]
    ConfigurationError {
        config_source: String,
        reason: String,
    },

    /// YAML parsing error
    #[error("YAML parsing error for file {file_path}: {reason}")]
    YamlParsingError { file_path: String, reason: String },

    /// Event publishing error
    #[error("Event publishing error for event type {event_type}: {reason}")]
    EventPublishingError { event_type: String, reason: String },

    /// SQL function execution error
    #[error("SQL function execution error for function {function_name}: {reason}")]
    SqlFunctionError {
        function_name: String,
        reason: String,
    },

    /// Dependency resolution error
    #[error("Dependency resolution error for task {task_uuid}: {reason}")]
    DependencyResolutionError { task_uuid: Uuid, reason: String },

    /// Timeout error
    #[error("Timeout error for operation {operation}: {timeout_duration:?}")]
    TimeoutError {
        operation: String,
        timeout_duration: Duration,
    },

    /// Validation error
    #[error("Validation error for field {field}: {reason}")]
    ValidationError { field: String, reason: String },

    /// FFI bridge error
    #[error("FFI bridge error for operation {operation}: {reason}")]
    FfiBridgeError { operation: String, reason: String },

    /// Framework integration error
    #[error("Framework integration error for {framework}: {reason}")]
    FrameworkIntegrationError { framework: String, reason: String },

    /// Step execution error (from StepExecutor)
    #[error("Step execution error: {error:?}")]
    ExecutionError { error: ExecutionError },
}

/// Conversion from state machine errors to orchestration errors
impl From<crate::state_machine::errors::StateMachineError> for OrchestrationError {
    fn from(error: crate::state_machine::errors::StateMachineError) -> Self {
        OrchestrationError::StateTransitionFailed {
            entity_type: "Unknown".to_string(),
            entity_uuid: Uuid::now_v7(),
            reason: error.to_string(),
        }
    }
}

/// Step execution error types with proper classification
#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub enum StepExecutionError {
    /// Permanent error - should not be retried
    #[error("Permanent error: {message}, error_code: {error_code:?}")]
    Permanent {
        message: String,
        error_code: Option<String>,
    },

    /// Retryable error - can be retried with backoff
    #[error("Retryable error: {message}, retry_after: {retry_after:?}, skip_retry: {skip_retry}, context: {context:?}")]
    Retryable {
        message: String,
        retry_after: Option<u64>, // seconds
        skip_retry: bool,
        context: Option<HashMap<String, serde_json::Value>>,
    },

    /// Timeout error
    #[error("Timeout error: {message}, timeout_duration: {timeout_duration:?}")]
    Timeout {
        message: String,
        timeout_duration: Duration,
    },

    /// Network error
    #[error("Network error: {message}, status_code: {status_code:?}")]
    NetworkError {
        message: String,
        status_code: Option<u16>,
    },
}

/// Registry error types
#[derive(Debug, Clone, Error)]
pub enum RegistryError {
    /// Handler not found
    #[error("Handler not found: {0}")]
    NotFound(String),

    /// Registration conflict
    #[error("Registration conflict: {key}, reason: {reason}")]
    Conflict { key: String, reason: String },

    /// Validation error
    #[error("Validation error: {handler_class}, reason: {reason}")]
    ValidationError {
        handler_class: String,
        reason: String,
    },

    /// Thread safety error
    #[error("Thread safety error: {operation}, reason: {reason}")]
    ThreadSafetyError { operation: String, reason: String },
}

/// Event publishing error types
#[derive(Debug, Clone, Error)]
pub enum EventError {
    /// Publishing failed
    #[error("Publishing failed: {event_type}, reason: {reason}")]
    PublishingFailed { event_type: String, reason: String },

    /// Serialization error
    #[error("Serialization error: {event_type}, reason: {reason}")]
    SerializationError { event_type: String, reason: String },

    /// FFI bridge error
    #[error("FFI bridge error: {reason}")]
    FfiBridgeError { reason: String },
}

/// State management error types
#[derive(Debug, Clone, Error)]
pub enum StateError {
    /// Invalid transition
    #[error("Invalid transition: {entity_type}, {entity_uuid}, {from_state}, {to_state}")]
    InvalidTransition {
        entity_type: String,
        entity_uuid: Uuid,
        from_state: String,
        to_state: String,
    },

    /// State not found
    #[error("State not found: {entity_type}, {entity_uuid}")]
    StateNotFound {
        entity_type: String,
        entity_uuid: Uuid,
    },

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Concurrent modification
    #[error("Concurrent modification: {entity_type}, {entity_id}")]
    ConcurrentModification { entity_type: String, entity_id: i64 },
}

/// Discovery error types
#[derive(Debug, Clone, Error)]
pub enum DiscoveryError {
    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// SQL function error
    #[error("SQL function error: {function_name}, {reason}")]
    SqlFunctionError {
        function_name: String,
        reason: String,
    },

    /// Dependency cycle detected
    #[error("Dependency cycle detected: {task_uuid}")]
    DependencyCycle {
        task_uuid: Uuid,
        cycle_steps: Vec<i64>,
    },

    /// Task not found
    #[error("Task not found: {task_uuid}")]
    TaskNotFound { task_uuid: Uuid },

    /// Configuration error - task template or step template not found
    #[error("Configuration error - {entity_type}, {entity_id}: {reason}")]
    ConfigurationError {
        entity_type: String,
        entity_id: String,
        reason: String,
    },
}

/// Step execution error types
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ExecutionError {
    /// Step execution failed
    #[error("Step execution failed: {step_uuid}, {reason}, {error_code:?}")]
    StepExecutionFailed {
        step_uuid: Uuid,
        reason: String,
        error_code: Option<String>,
    },

    /// Invalid step state for execution
    #[error("Invalid step state for execution: {step_uuid}, current state: {current_state}, expected state: {expected_state}")]
    InvalidStepState {
        step_uuid: Uuid,
        current_state: String,
        expected_state: String,
    },

    /// State transition error during execution
    #[error("State transition error during execution: {step_uuid}, {reason}")]
    StateTransitionError { step_uuid: Uuid, reason: String },

    /// Concurrency control error
    #[error("Concurrency control error: {step_uuid}, {reason}")]
    ConcurrencyError { step_uuid: Uuid, reason: String },

    /// No result returned from execution
    #[error("No result returned from execution: {step_uuid}")]
    NoResultReturned { step_uuid: Uuid },

    /// Execution timeout
    #[error("Execution timeout: {step_uuid}, timeout duration: {timeout_duration:?}")]
    ExecutionTimeout {
        step_uuid: Uuid,
        timeout_duration: Duration,
    },

    /// Retry limit exceeded
    #[error("Retry limit exceeded: {step_uuid}, max attempts: {max_attempts}")]
    RetryLimitExceeded { step_uuid: Uuid, max_attempts: u32 },

    /// Batch creation failed during ZeroMQ publishing
    #[error(
        "Batch creation failed during ZeroMQ publishing: {batch_id}, {reason}, {error_code:?}"
    )]
    BatchCreationFailed {
        batch_id: String,
        reason: String,
        error_code: Option<String>,
    },
}

impl StepExecutionError {
    /// Get the error class name for event publishing
    pub fn error_class(&self) -> &'static str {
        match self {
            StepExecutionError::Permanent { .. } => "PermanentError",
            StepExecutionError::Retryable { .. } => "RetryableError",
            StepExecutionError::Timeout { .. } => "TimeoutError",
            StepExecutionError::NetworkError { .. } => "NetworkError",
        }
    }
}

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
            config_source: "json_serialization".to_string(),
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
            config_source: "configuration_manager".to_string(),
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
            entity_uuid: Uuid::now_v7(),
            reason: err.to_string(),
        }
    }
}

impl From<DiscoveryError> for OrchestrationError {
    fn from(err: DiscoveryError) -> Self {
        OrchestrationError::DependencyResolutionError {
            task_uuid: Uuid::now_v7(),
            reason: err.to_string(),
        }
    }
}

impl From<crate::events::PublishError> for OrchestrationError {
    fn from(err: crate::events::PublishError) -> Self {
        OrchestrationError::EventPublishingError {
            event_type: "unified_event".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<crate::event_system::deployment::DeploymentModeError> for TaskerError {
    fn from(error: crate::event_system::deployment::DeploymentModeError) -> Self {
        TaskerError::OrchestrationError(format!("DeploymentModeError: {}", error))
    }
}

/// Convert `Box<dyn Error>` patterns to OrchestrationError for legacy compatibility
impl From<Box<dyn std::error::Error + Send + Sync>> for OrchestrationError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        // Get the error string before attempting downcasts
        let error_string = err.to_string();

        // Try to downcast to known error types first
        if let Ok(sqlx_err) = err.downcast::<sqlx::Error>() {
            return (*sqlx_err).into();
        }

        // If downcast fails, create a generic database error
        OrchestrationError::DatabaseError {
            operation: "unknown_operation".to_string(),
            reason: error_string,
        }
    }
}

/// Convert from String to OrchestrationError
impl From<String> for OrchestrationError {
    fn from(message: String) -> Self {
        OrchestrationError::ConfigurationError {
            config_source: "string_conversion".to_string(),
            reason: message,
        }
    }
}

/// Convert from &str to OrchestrationError
impl From<&str> for OrchestrationError {
    fn from(message: &str) -> Self {
        OrchestrationError::ConfigurationError {
            config_source: "str_conversion".to_string(),
            reason: message.to_string(),
        }
    }
}
