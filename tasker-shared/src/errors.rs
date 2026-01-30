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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // TaskerError Display tests
    // =========================================================================

    #[test]
    fn test_tasker_error_display_database_error() {
        let err = TaskerError::DatabaseError("connection refused".to_string());
        assert!(err.to_string().contains("Database error"));
        assert!(err.to_string().contains("connection refused"));
    }

    #[test]
    fn test_tasker_error_display_state_transition_error() {
        let err = TaskerError::StateTransitionError("invalid transition".to_string());
        assert!(err.to_string().contains("State transition error"));
        assert!(err.to_string().contains("invalid transition"));
    }

    #[test]
    fn test_tasker_error_display_orchestration_error() {
        let err = TaskerError::OrchestrationError("task failed".to_string());
        assert!(err.to_string().contains("Orchestration error"));
        assert!(err.to_string().contains("task failed"));
    }

    #[test]
    fn test_tasker_error_display_event_error() {
        let err = TaskerError::EventError("publish failed".to_string());
        assert!(err.to_string().contains("Event error"));
        assert!(err.to_string().contains("publish failed"));
    }

    #[test]
    fn test_tasker_error_display_validation_error() {
        let err = TaskerError::ValidationError("bad input".to_string());
        assert!(err.to_string().contains("Validation error"));
        assert!(err.to_string().contains("bad input"));
    }

    #[test]
    fn test_tasker_error_display_invalid_input() {
        let err = TaskerError::InvalidInput("missing field".to_string());
        assert!(err.to_string().contains("Invalid input"));
        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn test_tasker_error_display_configuration_error() {
        let err = TaskerError::ConfigurationError("bad config".to_string());
        assert!(err.to_string().contains("Configuration error"));
        assert!(err.to_string().contains("bad config"));
    }

    #[test]
    fn test_tasker_error_display_invalid_configuration() {
        let err = TaskerError::InvalidConfiguration("wrong format".to_string());
        assert!(err.to_string().contains("Invalid configuration"));
        assert!(err.to_string().contains("wrong format"));
    }

    #[test]
    fn test_tasker_error_display_ffi_error() {
        let err = TaskerError::FFIError("ffi call failed".to_string());
        assert!(err.to_string().contains("FFI error"));
        assert!(err.to_string().contains("ffi call failed"));
    }

    #[test]
    fn test_tasker_error_display_messaging_error() {
        let err = TaskerError::MessagingError("queue full".to_string());
        assert!(err.to_string().contains("Messaging error"));
        assert!(err.to_string().contains("queue full"));
    }

    #[test]
    fn test_tasker_error_display_cache_error() {
        let err = TaskerError::CacheError("cache miss".to_string());
        assert!(err.to_string().contains("Cache error"));
        assert!(err.to_string().contains("cache miss"));
    }

    #[test]
    fn test_tasker_error_display_worker_error() {
        let err = TaskerError::WorkerError("worker crashed".to_string());
        assert!(err.to_string().contains("Worker error"));
        assert!(err.to_string().contains("worker crashed"));
    }

    #[test]
    fn test_tasker_error_display_internal() {
        let err = TaskerError::Internal("unexpected state".to_string());
        assert!(err.to_string().contains("Internal error"));
        assert!(err.to_string().contains("unexpected state"));
    }

    #[test]
    fn test_tasker_error_display_invalid_state() {
        let err = TaskerError::InvalidState("not ready".to_string());
        assert!(err.to_string().contains("Invalid state"));
        assert!(err.to_string().contains("not ready"));
    }

    #[test]
    fn test_tasker_error_display_invalid_parameter() {
        let err = TaskerError::InvalidParameter("bad param".to_string());
        assert!(err.to_string().contains("Invalid parameter"));
        assert!(err.to_string().contains("bad param"));
    }

    #[test]
    fn test_tasker_error_display_timeout() {
        let err = TaskerError::Timeout("operation timed out".to_string());
        assert!(err.to_string().contains("Timeout error"));
        assert!(err.to_string().contains("operation timed out"));
    }

    #[test]
    fn test_tasker_error_display_circuit_breaker_open() {
        let err = TaskerError::CircuitBreakerOpen("db circuit".to_string());
        assert!(err.to_string().contains("Circuit breaker open"));
        assert!(err.to_string().contains("db circuit"));
    }

    #[test]
    fn test_tasker_error_display_task_initialization_error() {
        let err = TaskerError::TaskInitializationError {
            message: "init failed".to_string(),
            name: "my_task".to_string(),
            namespace: "default".to_string(),
            version: "1.0".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Task initialization error"));
        assert!(display.contains("init failed"));
        assert!(display.contains("my_task"));
        assert!(display.contains("default"));
        assert!(display.contains("1.0"));
    }

    #[test]
    fn test_tasker_error_display_state_machine_error() {
        let err = TaskerError::StateMachineError("sm failure".to_string());
        assert!(err.to_string().contains("State machine error"));
        assert!(err.to_string().contains("sm failure"));
    }

    #[test]
    fn test_tasker_error_display_state_machine_action_error() {
        let err = TaskerError::StateMachineActionError("action failed".to_string());
        assert!(err.to_string().contains("State machine action error"));
        assert!(err.to_string().contains("action failed"));
    }

    #[test]
    fn test_tasker_error_display_state_machine_guard_error() {
        let err = TaskerError::StateMachineGuardError("guard failed".to_string());
        assert!(err.to_string().contains("State machine guard error"));
        assert!(err.to_string().contains("guard failed"));
    }

    #[test]
    fn test_tasker_error_display_state_machine_persistence_error() {
        let err = TaskerError::StateMachinePersistenceError("persist failed".to_string());
        assert!(err.to_string().contains("State machine persistence error"));
        assert!(err.to_string().contains("persist failed"));
    }

    // =========================================================================
    // TaskerError From conversions
    // =========================================================================

    #[test]
    fn test_tasker_error_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: TaskerError = json_err.into();
        match &err {
            TaskerError::ValidationError(msg) => {
                assert!(msg.contains("JSON serialization error"));
            }
            other => panic!("Expected ValidationError, got: {other:?}"),
        }
    }

    #[test]
    fn test_tasker_error_from_messaging_error() {
        let messaging_err = crate::messaging::MessagingError::internal("test messaging failure");
        let err: TaskerError = messaging_err.into();
        match &err {
            TaskerError::MessagingError(msg) => {
                assert!(msg.contains("test messaging failure"));
            }
            other => panic!("Expected MessagingError, got: {other:?}"),
        }
    }

    #[test]
    fn test_tasker_error_from_deployment_mode_error() {
        let deploy_err = crate::event_system::deployment::DeploymentModeError::ConfigurationError {
            message: "test deploy error".to_string(),
        };
        let err: TaskerError = deploy_err.into();
        match &err {
            TaskerError::OrchestrationError(msg) => {
                assert!(msg.contains("DeploymentModeError"));
                assert!(msg.contains("test deploy error"));
            }
            other => panic!("Expected OrchestrationError, got: {other:?}"),
        }
    }

    // =========================================================================
    // OrchestrationError Display tests
    // =========================================================================

    #[test]
    fn test_orchestration_error_display_database_error() {
        let err = OrchestrationError::DatabaseError {
            operation: "insert".to_string(),
            reason: "unique constraint".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Database error"));
        assert!(display.contains("insert"));
        assert!(display.contains("unique constraint"));
    }

    #[test]
    fn test_orchestration_error_display_invalid_task_state() {
        let err = OrchestrationError::InvalidTaskState {
            task_uuid: Uuid::now_v7(),
            current_state: "Pending".to_string(),
            expected_states: vec!["InProgress".to_string(), "Ready".to_string()],
        };
        let display = err.to_string();
        assert!(display.contains("invalid state"));
        assert!(display.contains("Pending"));
    }

    #[test]
    fn test_orchestration_error_display_workflow_step_not_found() {
        let uuid = Uuid::now_v7();
        let err = OrchestrationError::WorkflowStepNotFound { step_uuid: uuid };
        assert!(err.to_string().contains("Workflow step"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_orchestration_error_display_step_state_machine_not_found() {
        let uuid = Uuid::now_v7();
        let err = OrchestrationError::StepStateMachineNotFound { step_uuid: uuid };
        assert!(err.to_string().contains("Step state machine not found"));
    }

    #[test]
    fn test_orchestration_error_display_state_verification_failed() {
        let err = OrchestrationError::StateVerificationFailed {
            step_uuid: Uuid::now_v7(),
            reason: "mismatch".to_string(),
        };
        assert!(err.to_string().contains("State verification failed"));
        assert!(err.to_string().contains("mismatch"));
    }

    #[test]
    fn test_orchestration_error_display_delegation_failed() {
        let err = OrchestrationError::DelegationFailed {
            task_uuid: Uuid::now_v7(),
            framework: "ruby".to_string(),
            reason: "handler missing".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Task execution delegation failed"));
        assert!(display.contains("ruby"));
        assert!(display.contains("handler missing"));
    }

    #[test]
    fn test_orchestration_error_display_task_execution_failed() {
        let err = OrchestrationError::TaskExecutionFailed {
            task_uuid: Uuid::now_v7(),
            reason: "runtime error".to_string(),
            error_code: Some("E001".to_string()),
        };
        assert!(err.to_string().contains("Task execution failed"));
        assert!(err.to_string().contains("runtime error"));
    }

    #[test]
    fn test_orchestration_error_display_step_execution_failed() {
        let err = OrchestrationError::StepExecutionFailed {
            step_uuid: Uuid::now_v7(),
            task_uuid: None,
            reason: "handler error".to_string(),
            error_code: None,
            retry_after: Some(Duration::from_secs(5)),
        };
        assert!(err.to_string().contains("Step execution failed"));
        assert!(err.to_string().contains("handler error"));
    }

    #[test]
    fn test_orchestration_error_display_state_transition_failed() {
        let err = OrchestrationError::StateTransitionFailed {
            entity_type: "task".to_string(),
            entity_uuid: Uuid::now_v7(),
            reason: "illegal state".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("State transition failed"));
        assert!(display.contains("task"));
        assert!(display.contains("illegal state"));
    }

    #[test]
    fn test_orchestration_error_display_invalid_step_state() {
        let err = OrchestrationError::InvalidStepState {
            step_uuid: Uuid::now_v7(),
            current_state: "Complete".to_string(),
            expected_states: vec!["Pending".to_string()],
        };
        let display = err.to_string();
        assert!(display.contains("invalid state"));
        assert!(display.contains("Complete"));
    }

    #[test]
    fn test_orchestration_error_display_registry_error() {
        let err = OrchestrationError::RegistryError {
            operation: "lookup".to_string(),
            reason: "not registered".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Registry operation failed"));
        assert!(display.contains("lookup"));
    }

    #[test]
    fn test_orchestration_error_display_handler_not_found() {
        let err = OrchestrationError::HandlerNotFound {
            namespace: "default".to_string(),
            name: "my_task".to_string(),
            version: "1.0".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Handler not found"));
        assert!(display.contains("default"));
        assert!(display.contains("my_task"));
        assert!(display.contains("1.0"));
    }

    #[test]
    fn test_orchestration_error_display_step_handler_not_found() {
        let err = OrchestrationError::StepHandlerNotFound {
            step_uuid: Uuid::now_v7(),
            reason: "no handler registered".to_string(),
        };
        assert!(err.to_string().contains("Step handler not found"));
        assert!(err.to_string().contains("no handler registered"));
    }

    #[test]
    fn test_orchestration_error_display_configuration_error() {
        let err = OrchestrationError::ConfigurationError {
            config_source: "toml".to_string(),
            reason: "parse failure".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Configuration error"));
        assert!(display.contains("toml"));
        assert!(display.contains("parse failure"));
    }

    #[test]
    fn test_orchestration_error_display_yaml_parsing_error() {
        let err = OrchestrationError::YamlParsingError {
            file_path: "/etc/config.yaml".to_string(),
            reason: "bad syntax".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("YAML parsing error"));
        assert!(display.contains("/etc/config.yaml"));
    }

    #[test]
    fn test_orchestration_error_display_event_publishing_error() {
        let err = OrchestrationError::EventPublishingError {
            event_type: "task_complete".to_string(),
            reason: "channel closed".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Event publishing error"));
        assert!(display.contains("task_complete"));
    }

    #[test]
    fn test_orchestration_error_display_sql_function_error() {
        let err = OrchestrationError::SqlFunctionError {
            function_name: "get_ready_steps".to_string(),
            reason: "timeout".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("SQL function execution error"));
        assert!(display.contains("get_ready_steps"));
    }

    #[test]
    fn test_orchestration_error_display_dependency_resolution_error() {
        let err = OrchestrationError::DependencyResolutionError {
            task_uuid: Uuid::now_v7(),
            reason: "circular dependency".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Dependency resolution error"));
        assert!(display.contains("circular dependency"));
    }

    #[test]
    fn test_orchestration_error_display_timeout_error() {
        let err = OrchestrationError::TimeoutError {
            operation: "step execution".to_string(),
            timeout_duration: Duration::from_secs(30),
        };
        let display = err.to_string();
        assert!(display.contains("Timeout error"));
        assert!(display.contains("step execution"));
    }

    #[test]
    fn test_orchestration_error_display_validation_error() {
        let err = OrchestrationError::ValidationError {
            field: "name".to_string(),
            reason: "cannot be empty".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Validation error"));
        assert!(display.contains("name"));
        assert!(display.contains("cannot be empty"));
    }

    #[test]
    fn test_orchestration_error_display_ffi_bridge_error() {
        let err = OrchestrationError::FfiBridgeError {
            operation: "call_ruby".to_string(),
            reason: "segfault".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("FFI bridge error"));
        assert!(display.contains("call_ruby"));
    }

    #[test]
    fn test_orchestration_error_display_framework_integration_error() {
        let err = OrchestrationError::FrameworkIntegrationError {
            framework: "python".to_string(),
            reason: "import error".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Framework integration error"));
        assert!(display.contains("python"));
    }

    #[test]
    fn test_orchestration_error_display_execution_error() {
        let uuid = Uuid::now_v7();
        let err = OrchestrationError::ExecutionError {
            error: ExecutionError::StepExecutionFailed {
                step_uuid: uuid,
                reason: "handler panicked".to_string(),
                error_code: None,
            },
        };
        let display = err.to_string();
        assert!(display.contains("Step execution error"));
    }

    // =========================================================================
    // OrchestrationError From conversions
    // =========================================================================

    #[test]
    fn test_orchestration_error_from_string() {
        let err: OrchestrationError = "something went wrong".to_string().into();
        match &err {
            OrchestrationError::ConfigurationError {
                config_source,
                reason,
            } => {
                assert_eq!(config_source, "string_conversion");
                assert_eq!(reason, "something went wrong");
            }
            other => panic!("Expected ConfigurationError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_str() {
        let err: OrchestrationError = "a static error message".into();
        match &err {
            OrchestrationError::ConfigurationError {
                config_source,
                reason,
            } => {
                assert_eq!(config_source, "str_conversion");
                assert_eq!(reason, "a static error message");
            }
            other => panic!("Expected ConfigurationError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: OrchestrationError = json_err.into();
        match &err {
            OrchestrationError::ConfigurationError {
                config_source,
                reason,
            } => {
                assert_eq!(config_source, "json_serialization");
                assert!(!reason.is_empty());
            }
            other => panic!("Expected ConfigurationError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_registry_error() {
        let reg_err = RegistryError::NotFound("my_handler".to_string());
        let err: OrchestrationError = reg_err.into();
        match &err {
            OrchestrationError::RegistryError {
                operation, reason, ..
            } => {
                assert_eq!(operation, "registry_operation");
                assert!(reason.contains("my_handler"));
            }
            other => panic!("Expected RegistryError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_registry_error_conflict() {
        let reg_err = RegistryError::Conflict {
            key: "handler_key".to_string(),
            reason: "duplicate".to_string(),
        };
        let err: OrchestrationError = reg_err.into();
        match &err {
            OrchestrationError::RegistryError { reason, .. } => {
                assert!(reason.contains("handler_key"));
                assert!(reason.contains("duplicate"));
            }
            other => panic!("Expected RegistryError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_registry_error_validation() {
        let reg_err = RegistryError::ValidationError {
            handler_class: "MyHandler".to_string(),
            reason: "missing trait impl".to_string(),
        };
        let err: OrchestrationError = reg_err.into();
        match &err {
            OrchestrationError::RegistryError { reason, .. } => {
                assert!(reason.contains("MyHandler"));
            }
            other => panic!("Expected RegistryError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_registry_error_thread_safety() {
        let reg_err = RegistryError::ThreadSafetyError {
            operation: "register".to_string(),
            reason: "lock poisoned".to_string(),
        };
        let err: OrchestrationError = reg_err.into();
        match &err {
            OrchestrationError::RegistryError { reason, .. } => {
                assert!(reason.contains("lock poisoned"));
            }
            other => panic!("Expected RegistryError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_event_error_publishing_failed() {
        let event_err = EventError::PublishingFailed {
            event_type: "task_complete".to_string(),
            reason: "channel full".to_string(),
        };
        let err: OrchestrationError = event_err.into();
        match &err {
            OrchestrationError::EventPublishingError { reason, .. } => {
                assert!(reason.contains("task_complete"));
                assert!(reason.contains("channel full"));
            }
            other => panic!("Expected EventPublishingError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_event_error_serialization() {
        let event_err = EventError::SerializationError {
            event_type: "step_result".to_string(),
            reason: "invalid utf8".to_string(),
        };
        let err: OrchestrationError = event_err.into();
        match &err {
            OrchestrationError::EventPublishingError { reason, .. } => {
                assert!(reason.contains("step_result"));
            }
            other => panic!("Expected EventPublishingError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_event_error_ffi_bridge() {
        let event_err = EventError::FfiBridgeError {
            reason: "bridge down".to_string(),
        };
        let err: OrchestrationError = event_err.into();
        match &err {
            OrchestrationError::EventPublishingError { reason, .. } => {
                assert!(reason.contains("bridge down"));
            }
            other => panic!("Expected EventPublishingError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_state_error_invalid_transition() {
        let uuid = Uuid::now_v7();
        let state_err = StateError::InvalidTransition {
            entity_type: "task".to_string(),
            entity_uuid: uuid,
            from_state: "Pending".to_string(),
            to_state: "Complete".to_string(),
        };
        let err: OrchestrationError = state_err.into();
        match &err {
            OrchestrationError::StateTransitionFailed { reason, .. } => {
                assert!(reason.contains("Invalid transition"));
                assert!(reason.contains("Pending"));
                assert!(reason.contains("Complete"));
            }
            other => panic!("Expected StateTransitionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_state_error_state_not_found() {
        let state_err = StateError::StateNotFound {
            entity_type: "step".to_string(),
            entity_uuid: Uuid::now_v7(),
        };
        let err: OrchestrationError = state_err.into();
        assert!(matches!(
            err,
            OrchestrationError::StateTransitionFailed { .. }
        ));
    }

    #[test]
    fn test_orchestration_error_from_state_error_database() {
        let state_err = StateError::DatabaseError("pool timeout".to_string());
        let err: OrchestrationError = state_err.into();
        match &err {
            OrchestrationError::StateTransitionFailed { reason, .. } => {
                assert!(reason.contains("pool timeout"));
            }
            other => panic!("Expected StateTransitionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_state_error_concurrent_modification() {
        let state_err = StateError::ConcurrentModification {
            entity_type: "task".to_string(),
            entity_id: 42,
        };
        let err: OrchestrationError = state_err.into();
        match &err {
            OrchestrationError::StateTransitionFailed { reason, .. } => {
                assert!(reason.contains("Concurrent modification"));
            }
            other => panic!("Expected StateTransitionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_discovery_error_database() {
        let disc_err = DiscoveryError::DatabaseError("connection lost".to_string());
        let err: OrchestrationError = disc_err.into();
        match &err {
            OrchestrationError::DependencyResolutionError { reason, .. } => {
                assert!(reason.contains("connection lost"));
            }
            other => panic!("Expected DependencyResolutionError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_discovery_error_sql_function() {
        let disc_err = DiscoveryError::SqlFunctionError {
            function_name: "find_ready".to_string(),
            reason: "syntax error".to_string(),
        };
        let err: OrchestrationError = disc_err.into();
        match &err {
            OrchestrationError::DependencyResolutionError { reason, .. } => {
                assert!(reason.contains("find_ready"));
            }
            other => panic!("Expected DependencyResolutionError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_discovery_error_dependency_cycle() {
        let disc_err = DiscoveryError::DependencyCycle {
            task_uuid: Uuid::now_v7(),
            cycle_steps: vec![1, 2, 3],
        };
        let err: OrchestrationError = disc_err.into();
        assert!(matches!(
            err,
            OrchestrationError::DependencyResolutionError { .. }
        ));
    }

    #[test]
    fn test_orchestration_error_from_discovery_error_task_not_found() {
        let disc_err = DiscoveryError::TaskNotFound {
            task_uuid: Uuid::now_v7(),
        };
        let err: OrchestrationError = disc_err.into();
        match &err {
            OrchestrationError::DependencyResolutionError { reason, .. } => {
                assert!(reason.contains("Task not found"));
            }
            other => panic!("Expected DependencyResolutionError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_discovery_error_configuration() {
        let disc_err = DiscoveryError::ConfigurationError {
            entity_type: "step_template".to_string(),
            entity_id: "42".to_string(),
            reason: "not found".to_string(),
        };
        let err: OrchestrationError = disc_err.into();
        match &err {
            OrchestrationError::DependencyResolutionError { reason, .. } => {
                assert!(reason.contains("step_template"));
            }
            other => panic!("Expected DependencyResolutionError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_state_machine_error() {
        let sm_err =
            crate::state_machine::errors::StateMachineError::Internal("test failure".to_string());
        let err: OrchestrationError = sm_err.into();
        match &err {
            OrchestrationError::StateTransitionFailed {
                entity_type,
                reason,
                ..
            } => {
                assert_eq!(entity_type, "Unknown");
                assert!(reason.contains("test failure"));
            }
            other => panic!("Expected StateTransitionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_state_machine_error_guard_failed() {
        let sm_err = crate::state_machine::errors::StateMachineError::GuardFailed {
            reason: "deps not met".to_string(),
        };
        let err: OrchestrationError = sm_err.into();
        match &err {
            OrchestrationError::StateTransitionFailed { reason, .. } => {
                assert!(reason.contains("Guard condition failed"));
                assert!(reason.contains("deps not met"));
            }
            other => panic!("Expected StateTransitionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_state_machine_error_invalid_transition() {
        let sm_err = crate::state_machine::errors::StateMachineError::InvalidTransition {
            from: Some("Pending".to_string()),
            to: "Complete".to_string(),
        };
        let err: OrchestrationError = sm_err.into();
        match &err {
            OrchestrationError::StateTransitionFailed { reason, .. } => {
                assert!(reason.contains("Invalid state transition"));
            }
            other => panic!("Expected StateTransitionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_configuration_error() {
        let config_err = ConfigurationError::validation_error("pool size too small");
        let err: OrchestrationError = config_err.into();
        match &err {
            OrchestrationError::ConfigurationError {
                config_source,
                reason,
            } => {
                assert_eq!(config_source, "configuration_manager");
                assert!(reason.contains("pool size too small"));
            }
            other => panic!("Expected ConfigurationError, got: {other:?}"),
        }
    }

    #[test]
    fn test_orchestration_error_from_publish_error() {
        let pub_err = crate::events::PublishError::ChannelClosed;
        let err: OrchestrationError = pub_err.into();
        match &err {
            OrchestrationError::EventPublishingError { event_type, reason } => {
                assert_eq!(event_type, "unified_event");
                assert!(reason.contains("closed"));
            }
            other => panic!("Expected EventPublishingError, got: {other:?}"),
        }
    }

    // =========================================================================
    // StepExecutionError error_class() tests
    // =========================================================================

    #[test]
    fn test_step_execution_error_class_permanent() {
        let err = StepExecutionError::Permanent {
            message: "fatal".to_string(),
            error_code: Some("E001".to_string()),
        };
        assert_eq!(err.error_class(), "PermanentError");
    }

    #[test]
    fn test_step_execution_error_class_retryable() {
        let err = StepExecutionError::Retryable {
            message: "transient".to_string(),
            retry_after: Some(5),
            skip_retry: false,
            context: None,
        };
        assert_eq!(err.error_class(), "RetryableError");
    }

    #[test]
    fn test_step_execution_error_class_timeout() {
        let err = StepExecutionError::Timeout {
            message: "took too long".to_string(),
            timeout_duration: Duration::from_secs(30),
        };
        assert_eq!(err.error_class(), "TimeoutError");
    }

    #[test]
    fn test_step_execution_error_class_network() {
        let err = StepExecutionError::NetworkError {
            message: "connection reset".to_string(),
            status_code: Some(503),
        };
        assert_eq!(err.error_class(), "NetworkError");
    }

    // =========================================================================
    // StepExecutionError Display tests
    // =========================================================================

    #[test]
    fn test_step_execution_error_display_permanent() {
        let err = StepExecutionError::Permanent {
            message: "unrecoverable".to_string(),
            error_code: Some("PERM01".to_string()),
        };
        let display = err.to_string();
        assert!(display.contains("Permanent error"));
        assert!(display.contains("unrecoverable"));
        assert!(display.contains("PERM01"));
    }

    #[test]
    fn test_step_execution_error_display_retryable() {
        let err = StepExecutionError::Retryable {
            message: "try again".to_string(),
            retry_after: Some(10),
            skip_retry: false,
            context: None,
        };
        let display = err.to_string();
        assert!(display.contains("Retryable error"));
        assert!(display.contains("try again"));
    }

    #[test]
    fn test_step_execution_error_display_timeout() {
        let err = StepExecutionError::Timeout {
            message: "deadline exceeded".to_string(),
            timeout_duration: Duration::from_secs(60),
        };
        let display = err.to_string();
        assert!(display.contains("Timeout error"));
        assert!(display.contains("deadline exceeded"));
    }

    #[test]
    fn test_step_execution_error_display_network() {
        let err = StepExecutionError::NetworkError {
            message: "dns lookup failed".to_string(),
            status_code: None,
        };
        let display = err.to_string();
        assert!(display.contains("Network error"));
        assert!(display.contains("dns lookup failed"));
    }

    // =========================================================================
    // ExecutionError Display tests
    // =========================================================================

    #[test]
    fn test_execution_error_display_step_execution_failed() {
        let uuid = Uuid::now_v7();
        let err = ExecutionError::StepExecutionFailed {
            step_uuid: uuid,
            reason: "handler returned error".to_string(),
            error_code: Some("H001".to_string()),
        };
        let display = err.to_string();
        assert!(display.contains("Step execution failed"));
        assert!(display.contains("handler returned error"));
    }

    #[test]
    fn test_execution_error_display_invalid_step_state() {
        let uuid = Uuid::now_v7();
        let err = ExecutionError::InvalidStepState {
            step_uuid: uuid,
            current_state: "Complete".to_string(),
            expected_state: "InProgress".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Invalid step state"));
        assert!(display.contains("Complete"));
        assert!(display.contains("InProgress"));
    }

    #[test]
    fn test_execution_error_display_state_transition_error() {
        let uuid = Uuid::now_v7();
        let err = ExecutionError::StateTransitionError {
            step_uuid: uuid,
            reason: "concurrent update".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("State transition error"));
        assert!(display.contains("concurrent update"));
    }

    #[test]
    fn test_execution_error_display_concurrency_error() {
        let uuid = Uuid::now_v7();
        let err = ExecutionError::ConcurrencyError {
            step_uuid: uuid,
            reason: "lock contention".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Concurrency control error"));
        assert!(display.contains("lock contention"));
    }

    #[test]
    fn test_execution_error_display_no_result_returned() {
        let uuid = Uuid::now_v7();
        let err = ExecutionError::NoResultReturned { step_uuid: uuid };
        assert!(err.to_string().contains("No result returned"));
    }

    #[test]
    fn test_execution_error_display_execution_timeout() {
        let uuid = Uuid::now_v7();
        let err = ExecutionError::ExecutionTimeout {
            step_uuid: uuid,
            timeout_duration: Duration::from_secs(120),
        };
        let display = err.to_string();
        assert!(display.contains("Execution timeout"));
    }

    #[test]
    fn test_execution_error_display_retry_limit_exceeded() {
        let uuid = Uuid::now_v7();
        let err = ExecutionError::RetryLimitExceeded {
            step_uuid: uuid,
            max_attempts: 3,
        };
        let display = err.to_string();
        assert!(display.contains("Retry limit exceeded"));
        assert!(display.contains("3"));
    }

    #[test]
    fn test_execution_error_display_batch_creation_failed() {
        let err = ExecutionError::BatchCreationFailed {
            batch_id: "batch-123".to_string(),
            reason: "zmq publish error".to_string(),
            error_code: Some("ZMQ01".to_string()),
        };
        let display = err.to_string();
        assert!(display.contains("Batch creation failed"));
        assert!(display.contains("batch-123"));
        assert!(display.contains("zmq publish error"));
    }

    // =========================================================================
    // RegistryError Display tests
    // =========================================================================

    #[test]
    fn test_registry_error_display_not_found() {
        let err = RegistryError::NotFound("missing_handler".to_string());
        assert!(err.to_string().contains("Handler not found"));
        assert!(err.to_string().contains("missing_handler"));
    }

    #[test]
    fn test_registry_error_display_conflict() {
        let err = RegistryError::Conflict {
            key: "handler_key".to_string(),
            reason: "already registered".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Registration conflict"));
        assert!(display.contains("handler_key"));
    }

    #[test]
    fn test_registry_error_display_validation() {
        let err = RegistryError::ValidationError {
            handler_class: "MyHandler".to_string(),
            reason: "missing method".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Validation error"));
        assert!(display.contains("MyHandler"));
    }

    #[test]
    fn test_registry_error_display_thread_safety() {
        let err = RegistryError::ThreadSafetyError {
            operation: "write".to_string(),
            reason: "poisoned lock".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Thread safety error"));
        assert!(display.contains("poisoned lock"));
    }

    // =========================================================================
    // EventError Display tests
    // =========================================================================

    #[test]
    fn test_event_error_display_publishing_failed() {
        let err = EventError::PublishingFailed {
            event_type: "step_result".to_string(),
            reason: "broker unavailable".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Publishing failed"));
        assert!(display.contains("step_result"));
    }

    #[test]
    fn test_event_error_display_serialization() {
        let err = EventError::SerializationError {
            event_type: "task_event".to_string(),
            reason: "invalid bytes".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Serialization error"));
        assert!(display.contains("task_event"));
    }

    #[test]
    fn test_event_error_display_ffi_bridge() {
        let err = EventError::FfiBridgeError {
            reason: "bridge disconnected".to_string(),
        };
        assert!(err.to_string().contains("FFI bridge error"));
        assert!(err.to_string().contains("bridge disconnected"));
    }

    // =========================================================================
    // StateError Display tests
    // =========================================================================

    #[test]
    fn test_state_error_display_invalid_transition() {
        let err = StateError::InvalidTransition {
            entity_type: "task".to_string(),
            entity_uuid: Uuid::now_v7(),
            from_state: "Pending".to_string(),
            to_state: "Complete".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Invalid transition"));
        assert!(display.contains("Pending"));
        assert!(display.contains("Complete"));
    }

    #[test]
    fn test_state_error_display_state_not_found() {
        let err = StateError::StateNotFound {
            entity_type: "step".to_string(),
            entity_uuid: Uuid::now_v7(),
        };
        assert!(err.to_string().contains("State not found"));
        assert!(err.to_string().contains("step"));
    }

    #[test]
    fn test_state_error_display_database_error() {
        let err = StateError::DatabaseError("pool closed".to_string());
        assert!(err.to_string().contains("Database error"));
        assert!(err.to_string().contains("pool closed"));
    }

    #[test]
    fn test_state_error_display_concurrent_modification() {
        let err = StateError::ConcurrentModification {
            entity_type: "task".to_string(),
            entity_id: 99,
        };
        let display = err.to_string();
        assert!(display.contains("Concurrent modification"));
        assert!(display.contains("task"));
        assert!(display.contains("99"));
    }

    // =========================================================================
    // DiscoveryError Display tests
    // =========================================================================

    #[test]
    fn test_discovery_error_display_database_error() {
        let err = DiscoveryError::DatabaseError("query timeout".to_string());
        assert!(err.to_string().contains("Database error"));
        assert!(err.to_string().contains("query timeout"));
    }

    #[test]
    fn test_discovery_error_display_sql_function_error() {
        let err = DiscoveryError::SqlFunctionError {
            function_name: "resolve_deps".to_string(),
            reason: "invalid sql".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("SQL function error"));
        assert!(display.contains("resolve_deps"));
    }

    #[test]
    fn test_discovery_error_display_dependency_cycle() {
        let err = DiscoveryError::DependencyCycle {
            task_uuid: Uuid::now_v7(),
            cycle_steps: vec![1, 2, 3],
        };
        assert!(err.to_string().contains("Dependency cycle detected"));
    }

    #[test]
    fn test_discovery_error_display_task_not_found() {
        let err = DiscoveryError::TaskNotFound {
            task_uuid: Uuid::now_v7(),
        };
        assert!(err.to_string().contains("Task not found"));
    }

    #[test]
    fn test_discovery_error_display_configuration_error() {
        let err = DiscoveryError::ConfigurationError {
            entity_type: "step_template".to_string(),
            entity_id: "5".to_string(),
            reason: "missing definition".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("Configuration error"));
        assert!(display.contains("step_template"));
        assert!(display.contains("missing definition"));
    }
}
