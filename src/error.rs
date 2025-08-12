use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskerError {
    DatabaseError(String),
    StateTransitionError(String),
    OrchestrationError(String),
    EventError(String),
    ValidationError(String),
    InvalidInput(String),
    ConfigurationError(String),
    FFIError(String),
    MessagingError(String),
}

impl fmt::Display for TaskerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskerError::DatabaseError(msg) => write!(f, "Database error: {msg}"),
            TaskerError::StateTransitionError(msg) => write!(f, "State transition error: {msg}"),
            TaskerError::OrchestrationError(msg) => write!(f, "Orchestration error: {msg}"),
            TaskerError::EventError(msg) => write!(f, "Event error: {msg}"),
            TaskerError::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            TaskerError::InvalidInput(msg) => write!(f, "Invalid input: {msg}"),
            TaskerError::ConfigurationError(msg) => write!(f, "Configuration error: {msg}"),
            TaskerError::FFIError(msg) => write!(f, "FFI error: {msg}"),
            TaskerError::MessagingError(msg) => write!(f, "Messaging error: {msg}"),
        }
    }
}

impl std::error::Error for TaskerError {}

impl From<serde_json::Error> for TaskerError {
    fn from(error: serde_json::Error) -> Self {
        TaskerError::ValidationError(format!("JSON serialization error: {error}"))
    }
}

impl From<crate::orchestration::TaskInitializationError> for TaskerError {
    fn from(error: crate::orchestration::TaskInitializationError) -> Self {
        TaskerError::OrchestrationError(format!("Task initialization failed: {error}"))
    }
}

pub type Result<T> = std::result::Result<T, TaskerError>;

/// Specific orchestration error types for detailed error handling
#[derive(Debug, Clone, PartialEq)]
pub enum OrchestrationError {
    /// Database operation failed
    DatabaseError { operation: String, reason: String },
    /// State machine transition failed
    StateTransitionFailed {
        entity_type: String,
        entity_id: Uuid,
        reason: String,
    },
    /// Task is in invalid state for operation
    InvalidTaskState {
        task_uuid: Uuid,
        current_state: String,
        expected_states: Vec<String>,
    },
    /// Workflow step not found
    WorkflowStepNotFound { step_uuid: Uuid },
    /// Step state machine not found
    StepStateMachineNotFound { step_uuid: Uuid },
    /// State verification failed
    StateVerificationFailed { step_uuid: Uuid, reason: String },
    /// Task execution delegation failed
    DelegationFailed {
        task_uuid: Uuid,
        framework: String,
        reason: String,
    },
}

impl fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
                task_uuid,
                current_state,
                expected_states,
            } => {
                write!(
                    f,
                    "Task {task_uuid} is in invalid state '{current_state}', expected one of: {expected_states:?}"
                )
            }
            OrchestrationError::WorkflowStepNotFound { step_uuid } => {
                write!(f, "Workflow step {step_uuid} not found")
            }
            OrchestrationError::StepStateMachineNotFound { step_uuid } => {
                write!(f, "Step state machine for step {step_uuid} not found")
            }
            OrchestrationError::StateVerificationFailed { step_uuid, reason } => {
                write!(
                    f,
                    "State verification failed for step {step_uuid}: {reason}"
                )
            }
            OrchestrationError::DelegationFailed {
                task_uuid,
                framework,
                reason,
            } => {
                write!(
                    f,
                    "Delegation to {framework} failed for task {task_uuid}: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for OrchestrationError {}

/// Conversion from state machine errors to orchestration errors
impl From<crate::state_machine::errors::StateMachineError> for OrchestrationError {
    fn from(error: crate::state_machine::errors::StateMachineError) -> Self {
        OrchestrationError::StateTransitionFailed {
            entity_type: "Unknown".to_string(),
            entity_id: Uuid::now_v7(),
            reason: error.to_string(),
        }
    }
}

/// Result type for orchestration operations
pub type OrchestrationResult<T> = std::result::Result<T, OrchestrationError>;

// Note: No re-export needed since types are already public in this module
