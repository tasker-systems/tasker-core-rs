use crate::errors::TaskerError;
use thiserror::Error;

/// Comprehensive error types for state machine operations
#[derive(Error, Debug)]
pub enum StateMachineError {
    #[error("Guard condition failed: {reason}")]
    GuardFailed { reason: String },

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition { from: Option<String>, to: String },

    #[error("Action execution failed: {reason}")]
    ActionFailed { reason: String },

    #[error("Persistence operation failed: {reason}")]
    PersistenceFailed { reason: String },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Specific error type for guard condition failures
#[derive(Error, Debug)]
pub enum GuardError {
    #[error("Dependencies not satisfied: {reason}")]
    DependenciesNotMet { reason: String },

    #[error("Business rule violation: {rule}")]
    BusinessRuleViolation { rule: String },

    #[error("Resource not available: {resource}")]
    ResourceUnavailable { resource: String },

    #[error("Database query failed: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Invalid state for guard check: {state}")]
    InvalidState { state: String },

    #[error("Timeout while checking guard condition")]
    Timeout,
}

/// Specific error type for action execution failures
#[derive(Error, Debug)]
pub enum ActionError {
    #[error("Event publishing failed: {event_name}")]
    EventPublishFailed { event_name: String },

    #[error("Database update failed for {entity_type} {entity_id}: {reason}")]
    DatabaseUpdateFailed {
        entity_type: String,
        entity_id: String,
        reason: String,
    },

    #[error("External service error: {service} - {reason}")]
    ExternalServiceError { service: String, reason: String },

    #[error("Action timeout: {action}")]
    Timeout { action: String },

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid state transition: {from_state:?} -> {to_state} - {reason}")]
    InvalidStateTransition {
        from_state: Option<String>,
        to_state: String,
        reason: String,
    },

    #[error("Invalid state for action execution: {state}")]
    InvalidState { state: String },

    #[error("Invalid action for state: {state}")]
    InvalidAction { state: String },
}

/// Specific error type for persistence operations
#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("Failed to save transition: {reason}")]
    TransitionSaveFailed { reason: String },

    #[error("Failed to resolve current state: {entity_id}")]
    StateResolutionFailed { entity_id: String },

    #[error("Concurrent modification detected for entity {entity_id}")]
    ConcurrentModification { entity_id: String },

    #[error("Invalid transition data: {field}")]
    InvalidTransitionData { field: String },

    #[error("Database constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<GuardError> for StateMachineError {
    fn from(err: GuardError) -> Self {
        Self::GuardFailed {
            reason: err.to_string(),
        }
    }
}

impl From<ActionError> for StateMachineError {
    fn from(err: ActionError) -> Self {
        Self::ActionFailed {
            reason: err.to_string(),
        }
    }
}

impl From<PersistenceError> for StateMachineError {
    fn from(err: PersistenceError) -> Self {
        Self::PersistenceFailed {
            reason: err.to_string(),
        }
    }
}

/// Result type alias for state machine operations
pub type StateMachineResult<T> = Result<T, StateMachineError>;
pub type GuardResult<T> = Result<T, GuardError>;
pub type ActionResult<T> = Result<T, ActionError>;
pub type PersistenceResult<T> = Result<T, PersistenceError>;

/// Helper function to create internal errors
pub fn internal_error(msg: impl Into<String>) -> StateMachineError {
    StateMachineError::Internal(msg.into())
}

/// Helper function to create guard dependency errors
pub fn dependencies_not_met(reason: impl Into<String>) -> GuardError {
    GuardError::DependenciesNotMet {
        reason: reason.into(),
    }
}

/// Helper function to create business rule violations
pub fn business_rule_violation(rule: impl Into<String>) -> GuardError {
    GuardError::BusinessRuleViolation { rule: rule.into() }
}

impl From<StateMachineError> for TaskerError {
    fn from(err: StateMachineError) -> Self {
        TaskerError::StateMachineError(format!("{err}"))
    }
}

impl From<GuardError> for TaskerError {
    fn from(err: GuardError) -> Self {
        TaskerError::StateMachineGuardError(format!("{err}"))
    }
}

impl From<ActionError> for TaskerError {
    fn from(err: ActionError) -> Self {
        TaskerError::StateMachineActionError(format!("{err}"))
    }
}

impl From<PersistenceError> for TaskerError {
    fn from(err: PersistenceError) -> Self {
        TaskerError::StateMachinePersistenceError(format!("{err}"))
    }
}
