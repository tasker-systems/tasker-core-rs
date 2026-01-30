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

use crate::orchestration::lifecycle::task_finalization::FinalizationError;
use tasker_shared::errors::OrchestrationError;

impl From<FinalizationError> for OrchestrationError {
    fn from(err: FinalizationError) -> Self {
        match err {
            FinalizationError::Database(sqlx_err) => OrchestrationError::DatabaseError {
                operation: "task_finalization".to_string(),
                reason: sqlx_err.to_string(),
            },
            FinalizationError::TaskNotFound { task_uuid } => OrchestrationError::InvalidTaskState {
                task_uuid,
                current_state: "not_found".to_string(),
                expected_states: vec!["exists".to_string()],
            },
            FinalizationError::StateMachine { task_uuid, error } => {
                OrchestrationError::StateTransitionFailed {
                    entity_type: "task".to_string(),
                    entity_uuid: task_uuid,
                    reason: error.to_string(),
                }
            }
            FinalizationError::InvalidTransition {
                transition,
                task_uuid,
            } => OrchestrationError::StateTransitionFailed {
                entity_type: "task".to_string(),
                entity_uuid: task_uuid,
                reason: format!("Invalid transition from state '{transition}'"),
            },
            FinalizationError::ContextUnavailable { task_uuid } => {
                OrchestrationError::ConfigurationError {
                    config_source: "task_finalizer".to_string(),
                    reason: format!("Context unavailable for task: {task_uuid}"),
                }
            }
            FinalizationError::EventPublishing(reason) => {
                OrchestrationError::EventPublishingError {
                    event_type: "task_finalization".to_string(),
                    reason,
                }
            }
            // Removed claim-related error variants - TAS-41 eliminates claims
            FinalizationError::General(reason) => OrchestrationError::ConfigurationError {
                config_source: "task_finalizer".to_string(),
                reason,
            },
        }
    }
}

impl From<crate::orchestration::backoff_calculator::BackoffError> for OrchestrationError {
    fn from(err: crate::orchestration::backoff_calculator::BackoffError) -> Self {
        use crate::orchestration::backoff_calculator::BackoffError;
        match err {
            BackoffError::Database(sqlx_err) => OrchestrationError::DatabaseError {
                operation: "backoff_calculation".to_string(),
                reason: sqlx_err.to_string(),
            },
            BackoffError::InvalidConfig(reason) => OrchestrationError::ConfigurationError {
                config_source: "backoff_calculator".to_string(),
                reason,
            },
            BackoffError::StepNotFound(step_id) => OrchestrationError::StepHandlerNotFound {
                step_uuid: uuid::Uuid::now_v7(), // Default UUID since we only have the i64 id
                reason: format!("Step with ID {step_id} not found"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::backoff_calculator::BackoffError;
    use uuid::Uuid;

    // --- FinalizationError → OrchestrationError conversions ---

    #[test]
    fn test_finalization_database_error_converts_to_database_error() {
        let sqlx_err = sqlx::Error::ColumnNotFound("test_column".to_string());
        let finalization_err = FinalizationError::Database(sqlx_err);
        let orch_err: OrchestrationError = finalization_err.into();

        match orch_err {
            OrchestrationError::DatabaseError { operation, reason } => {
                assert_eq!(operation, "task_finalization");
                assert!(reason.contains("test_column"));
            }
            other => panic!("Expected DatabaseError, got: {other:?}"),
        }
    }

    #[test]
    fn test_finalization_task_not_found_converts_to_invalid_task_state() {
        let task_uuid = Uuid::now_v7();
        let finalization_err = FinalizationError::TaskNotFound { task_uuid };
        let orch_err: OrchestrationError = finalization_err.into();

        match orch_err {
            OrchestrationError::InvalidTaskState {
                task_uuid: err_uuid,
                current_state,
                expected_states,
            } => {
                assert_eq!(err_uuid, task_uuid);
                assert_eq!(current_state, "not_found");
                assert_eq!(expected_states, vec!["exists".to_string()]);
            }
            other => panic!("Expected InvalidTaskState, got: {other:?}"),
        }
    }

    #[test]
    fn test_finalization_state_machine_error_converts_to_state_transition_failed() {
        let task_uuid = Uuid::now_v7();
        let finalization_err = FinalizationError::StateMachine {
            task_uuid,
            error: "invalid guard".to_string(),
        };
        let orch_err: OrchestrationError = finalization_err.into();

        match orch_err {
            OrchestrationError::StateTransitionFailed {
                entity_type,
                entity_uuid,
                reason,
            } => {
                assert_eq!(entity_type, "task");
                assert_eq!(entity_uuid, task_uuid);
                assert!(reason.contains("invalid guard"));
            }
            other => panic!("Expected StateTransitionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_finalization_invalid_transition_converts_to_state_transition_failed() {
        let task_uuid = Uuid::now_v7();
        let finalization_err = FinalizationError::InvalidTransition {
            transition: "complete_to_pending".to_string(),
            task_uuid,
        };
        let orch_err: OrchestrationError = finalization_err.into();

        match orch_err {
            OrchestrationError::StateTransitionFailed {
                entity_type,
                entity_uuid,
                reason,
            } => {
                assert_eq!(entity_type, "task");
                assert_eq!(entity_uuid, task_uuid);
                assert!(reason.contains("complete_to_pending"));
            }
            other => panic!("Expected StateTransitionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_finalization_context_unavailable_converts_to_configuration_error() {
        let task_uuid = Uuid::now_v7();
        let finalization_err = FinalizationError::ContextUnavailable { task_uuid };
        let orch_err: OrchestrationError = finalization_err.into();

        match orch_err {
            OrchestrationError::ConfigurationError {
                config_source,
                reason,
            } => {
                assert_eq!(config_source, "task_finalizer");
                assert!(reason.contains(&task_uuid.to_string()));
            }
            other => panic!("Expected ConfigurationError, got: {other:?}"),
        }
    }

    #[test]
    fn test_finalization_event_publishing_converts_to_event_publishing_error() {
        let finalization_err = FinalizationError::EventPublishing("queue full".to_string());
        let orch_err: OrchestrationError = finalization_err.into();

        match orch_err {
            OrchestrationError::EventPublishingError { event_type, reason } => {
                assert_eq!(event_type, "task_finalization");
                assert_eq!(reason, "queue full");
            }
            other => panic!("Expected EventPublishingError, got: {other:?}"),
        }
    }

    #[test]
    fn test_finalization_general_converts_to_configuration_error() {
        let finalization_err = FinalizationError::General("unexpected failure".to_string());
        let orch_err: OrchestrationError = finalization_err.into();

        match orch_err {
            OrchestrationError::ConfigurationError {
                config_source,
                reason,
            } => {
                assert_eq!(config_source, "task_finalizer");
                assert_eq!(reason, "unexpected failure");
            }
            other => panic!("Expected ConfigurationError, got: {other:?}"),
        }
    }

    // --- BackoffError → OrchestrationError conversions ---

    #[test]
    fn test_backoff_database_error_converts_to_database_error() {
        let sqlx_err = sqlx::Error::ColumnNotFound("backoff_col".to_string());
        let backoff_err = BackoffError::Database(sqlx_err);
        let orch_err: OrchestrationError = backoff_err.into();

        match orch_err {
            OrchestrationError::DatabaseError { operation, reason } => {
                assert_eq!(operation, "backoff_calculation");
                assert!(reason.contains("backoff_col"));
            }
            other => panic!("Expected DatabaseError, got: {other:?}"),
        }
    }

    #[test]
    fn test_backoff_invalid_config_converts_to_configuration_error() {
        let backoff_err = BackoffError::InvalidConfig("negative delay".to_string());
        let orch_err: OrchestrationError = backoff_err.into();

        match orch_err {
            OrchestrationError::ConfigurationError {
                config_source,
                reason,
            } => {
                assert_eq!(config_source, "backoff_calculator");
                assert_eq!(reason, "negative delay");
            }
            other => panic!("Expected ConfigurationError, got: {other:?}"),
        }
    }

    #[test]
    fn test_backoff_step_not_found_converts_to_step_handler_not_found() {
        let backoff_err = BackoffError::StepNotFound(42);
        let orch_err: OrchestrationError = backoff_err.into();

        match orch_err {
            OrchestrationError::StepHandlerNotFound { reason, .. } => {
                assert!(reason.contains("42"));
            }
            other => panic!("Expected StepHandlerNotFound, got: {other:?}"),
        }
    }
}
