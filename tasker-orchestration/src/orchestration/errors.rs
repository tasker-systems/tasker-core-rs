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

use crate::orchestration::lifecycle::task_finalizer::FinalizationError;
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
