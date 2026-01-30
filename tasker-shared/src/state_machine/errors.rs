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

#[cfg(test)]
mod tests {
    use super::*;

    // ── StateMachineError Display ─────────────────────────────────────

    #[test]
    fn test_state_machine_error_guard_failed_display() {
        let err = StateMachineError::GuardFailed {
            reason: "precondition unmet".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "Guard condition failed: precondition unmet"
        );
    }

    #[test]
    fn test_state_machine_error_invalid_transition_with_from_display() {
        let err = StateMachineError::InvalidTransition {
            from: Some("Pending".to_string()),
            to: "Complete".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("Pending"));
        assert!(display.contains("Complete"));
    }

    #[test]
    fn test_state_machine_error_invalid_transition_without_from_display() {
        let err = StateMachineError::InvalidTransition {
            from: None,
            to: "Complete".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("None"));
        assert!(display.contains("Complete"));
    }

    #[test]
    fn test_state_machine_error_action_failed_display() {
        let err = StateMachineError::ActionFailed {
            reason: "handler crashed".to_string(),
        };
        assert_eq!(format!("{err}"), "Action execution failed: handler crashed");
    }

    #[test]
    fn test_state_machine_error_persistence_failed_display() {
        let err = StateMachineError::PersistenceFailed {
            reason: "write conflict".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "Persistence operation failed: write conflict"
        );
    }

    #[test]
    fn test_state_machine_error_internal_display() {
        let err = StateMachineError::Internal("unexpected state".to_string());
        assert_eq!(format!("{err}"), "Internal error: unexpected state");
    }

    // ── GuardError Display ────────────────────────────────────────────

    #[test]
    fn test_guard_error_dependencies_not_met_display() {
        let err = GuardError::DependenciesNotMet {
            reason: "step 2 incomplete".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "Dependencies not satisfied: step 2 incomplete"
        );
    }

    #[test]
    fn test_guard_error_business_rule_violation_display() {
        let err = GuardError::BusinessRuleViolation {
            rule: "max retries exceeded".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "Business rule violation: max retries exceeded"
        );
    }

    #[test]
    fn test_guard_error_resource_unavailable_display() {
        let err = GuardError::ResourceUnavailable {
            resource: "worker pool".to_string(),
        };
        assert_eq!(format!("{err}"), "Resource not available: worker pool");
    }

    #[test]
    fn test_guard_error_invalid_state_display() {
        let err = GuardError::InvalidState {
            state: "Unknown".to_string(),
        };
        assert_eq!(format!("{err}"), "Invalid state for guard check: Unknown");
    }

    #[test]
    fn test_guard_error_timeout_display() {
        let err = GuardError::Timeout;
        assert_eq!(format!("{err}"), "Timeout while checking guard condition");
    }

    // ── ActionError Display ───────────────────────────────────────────

    #[test]
    fn test_action_error_event_publish_failed_display() {
        let err = ActionError::EventPublishFailed {
            event_name: "task.completed".to_string(),
        };
        assert_eq!(format!("{err}"), "Event publishing failed: task.completed");
    }

    #[test]
    fn test_action_error_database_update_failed_display() {
        let err = ActionError::DatabaseUpdateFailed {
            entity_type: "Task".to_string(),
            entity_id: "42".to_string(),
            reason: "row locked".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "Database update failed for Task 42: row locked"
        );
    }

    #[test]
    fn test_action_error_external_service_error_display() {
        let err = ActionError::ExternalServiceError {
            service: "payment-api".to_string(),
            reason: "503 unavailable".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "External service error: payment-api - 503 unavailable"
        );
    }

    #[test]
    fn test_action_error_timeout_display() {
        let err = ActionError::Timeout {
            action: "send_email".to_string(),
        };
        assert_eq!(format!("{err}"), "Action timeout: send_email");
    }

    #[test]
    fn test_action_error_configuration_display() {
        let err = ActionError::Configuration("missing key".to_string());
        assert_eq!(format!("{err}"), "Configuration error: missing key");
    }

    #[test]
    fn test_action_error_invalid_state_transition_display() {
        let err = ActionError::InvalidStateTransition {
            from_state: Some("Pending".to_string()),
            to_state: "Complete".to_string(),
            reason: "steps not done".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("Pending"));
        assert!(display.contains("Complete"));
        assert!(display.contains("steps not done"));
    }

    #[test]
    fn test_action_error_invalid_state_transition_none_from_display() {
        let err = ActionError::InvalidStateTransition {
            from_state: None,
            to_state: "InProgress".to_string(),
            reason: "no current state".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("None"));
        assert!(display.contains("InProgress"));
        assert!(display.contains("no current state"));
    }

    #[test]
    fn test_action_error_invalid_state_display() {
        let err = ActionError::InvalidState {
            state: "Cancelled".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "Invalid state for action execution: Cancelled"
        );
    }

    #[test]
    fn test_action_error_invalid_action_display() {
        let err = ActionError::InvalidAction {
            state: "Complete".to_string(),
        };
        assert_eq!(format!("{err}"), "Invalid action for state: Complete");
    }

    // ── PersistenceError Display ──────────────────────────────────────

    #[test]
    fn test_persistence_error_transition_save_failed_display() {
        let err = PersistenceError::TransitionSaveFailed {
            reason: "disk full".to_string(),
        };
        assert_eq!(format!("{err}"), "Failed to save transition: disk full");
    }

    #[test]
    fn test_persistence_error_state_resolution_failed_display() {
        let err = PersistenceError::StateResolutionFailed {
            entity_id: "task-99".to_string(),
        };
        assert_eq!(format!("{err}"), "Failed to resolve current state: task-99");
    }

    #[test]
    fn test_persistence_error_concurrent_modification_display() {
        let err = PersistenceError::ConcurrentModification {
            entity_id: "step-7".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "Concurrent modification detected for entity step-7"
        );
    }

    #[test]
    fn test_persistence_error_invalid_transition_data_display() {
        let err = PersistenceError::InvalidTransitionData {
            field: "to_state".to_string(),
        };
        assert_eq!(format!("{err}"), "Invalid transition data: to_state");
    }

    #[test]
    fn test_persistence_error_constraint_violation_display() {
        let err = PersistenceError::ConstraintViolation {
            constraint: "unique_task_name".to_string(),
        };
        assert_eq!(
            format!("{err}"),
            "Database constraint violation: unique_task_name"
        );
    }

    // ── From<GuardError> for StateMachineError ────────────────────────

    #[test]
    fn test_guard_error_into_state_machine_error() {
        let guard_err = GuardError::DependenciesNotMet {
            reason: "step blocked".to_string(),
        };
        let sm_err: StateMachineError = guard_err.into();
        assert!(matches!(sm_err, StateMachineError::GuardFailed { .. }));
        let display = format!("{sm_err}");
        assert!(display.contains("step blocked"));
    }

    // ── From<ActionError> for StateMachineError ───────────────────────

    #[test]
    fn test_action_error_into_state_machine_error() {
        let action_err = ActionError::Timeout {
            action: "process_payment".to_string(),
        };
        let sm_err: StateMachineError = action_err.into();
        assert!(matches!(sm_err, StateMachineError::ActionFailed { .. }));
        let display = format!("{sm_err}");
        assert!(display.contains("process_payment"));
    }

    // ── From<PersistenceError> for StateMachineError ──────────────────

    #[test]
    fn test_persistence_error_into_state_machine_error() {
        let persist_err = PersistenceError::ConcurrentModification {
            entity_id: "task-1".to_string(),
        };
        let sm_err: StateMachineError = persist_err.into();
        assert!(matches!(
            sm_err,
            StateMachineError::PersistenceFailed { .. }
        ));
        let display = format!("{sm_err}");
        assert!(display.contains("task-1"));
    }

    // ── From<StateMachineError> for TaskerError ───────────────────────

    #[test]
    fn test_state_machine_error_into_tasker_error() {
        let sm_err = StateMachineError::Internal("oops".to_string());
        let tasker_err: TaskerError = sm_err.into();
        assert!(matches!(tasker_err, TaskerError::StateMachineError(_)));
        let display = format!("{tasker_err}");
        assert!(display.contains("oops"));
    }

    // ── From<GuardError> for TaskerError ──────────────────────────────

    #[test]
    fn test_guard_error_into_tasker_error() {
        let guard_err = GuardError::BusinessRuleViolation {
            rule: "no retry allowed".to_string(),
        };
        let tasker_err: TaskerError = guard_err.into();
        assert!(matches!(tasker_err, TaskerError::StateMachineGuardError(_)));
        let display = format!("{tasker_err}");
        assert!(display.contains("no retry allowed"));
    }

    // ── From<ActionError> for TaskerError ─────────────────────────────

    #[test]
    fn test_action_error_into_tasker_error() {
        let action_err = ActionError::Configuration("bad config".to_string());
        let tasker_err: TaskerError = action_err.into();
        assert!(matches!(
            tasker_err,
            TaskerError::StateMachineActionError(_)
        ));
        let display = format!("{tasker_err}");
        assert!(display.contains("bad config"));
    }

    // ── From<PersistenceError> for TaskerError ────────────────────────

    #[test]
    fn test_persistence_error_into_tasker_error() {
        let persist_err = PersistenceError::TransitionSaveFailed {
            reason: "network error".to_string(),
        };
        let tasker_err: TaskerError = persist_err.into();
        assert!(matches!(
            tasker_err,
            TaskerError::StateMachinePersistenceError(_)
        ));
        let display = format!("{tasker_err}");
        assert!(display.contains("network error"));
    }

    // ── Helper functions ──────────────────────────────────────────────

    #[test]
    fn test_internal_error_helper() {
        let err = internal_error("something went wrong");
        assert!(
            matches!(err, StateMachineError::Internal(ref msg) if msg == "something went wrong")
        );
        assert_eq!(format!("{err}"), "Internal error: something went wrong");
    }

    #[test]
    fn test_dependencies_not_met_helper() {
        let err = dependencies_not_met("upstream failed");
        assert!(matches!(
            err,
            GuardError::DependenciesNotMet { ref reason } if reason == "upstream failed"
        ));
        assert_eq!(
            format!("{err}"),
            "Dependencies not satisfied: upstream failed"
        );
    }

    #[test]
    fn test_business_rule_violation_helper() {
        let err = business_rule_violation("limit exceeded");
        assert!(matches!(
            err,
            GuardError::BusinessRuleViolation { ref rule } if rule == "limit exceeded"
        ));
        assert_eq!(format!("{err}"), "Business rule violation: limit exceeded");
    }
}
