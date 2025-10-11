//! Task Finalization Module
//!
//! Atomic task finalization with focused, single-responsibility components.
//!
//! ## Architecture
//!
//! This module decomposes task finalization into distinct responsibilities:
//!
//! - **ExecutionContextProvider**: Fetches task execution context and checks blocked states
//! - **CompletionHandler**: Handles task completion and error state transitions
//! - **EventPublisher**: Publishes lifecycle events for observability
//! - **StateHandlers**: Handles different execution states (ready, waiting, processing, unclear)
//!
//! The main `TaskFinalizer` orchestrates these components to provide
//! a clean, state-driven task finalization interface.

use serde::{Deserialize, Serialize};
use tasker_shared::TaskerError;
use uuid::Uuid;

mod completion_handler;
mod event_publisher;
mod execution_context_provider;
mod service;
mod state_handlers;

pub use completion_handler::CompletionHandler;
pub use event_publisher::EventPublisher;
pub use execution_context_provider::ExecutionContextProvider;
pub use service::TaskFinalizer;
pub use state_handlers::StateHandlers;

/// Result of task finalization operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizationResult {
    /// Task ID that was finalized
    pub task_uuid: Uuid,
    /// Final action taken
    pub action: FinalizationAction,
    /// Completion percentage if completed
    pub completion_percentage: Option<f64>,
    /// Total number of steps in task
    pub total_steps: Option<i32>,
    /// Number of steps enqueued in this pass
    pub enqueued_steps: Option<i32>,
    /// Health status of the task
    pub health_status: Option<String>,
    /// Reason for the action (if applicable)
    pub reason: Option<String>,
}

/// Type of finalization action taken
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinalizationAction {
    /// Task was completed successfully
    Completed,
    /// Task was marked as failed due to errors
    Failed,
    /// Task was set to pending state
    Pending,
    /// Task was reenqueued for further processing
    Reenqueued,
    /// No action taken due to unclear state
    NoAction,
}

/// Errors that can occur during task finalization
#[derive(Debug, thiserror::Error)]
pub enum FinalizationError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Task not found: {task_uuid}")]
    TaskNotFound { task_uuid: Uuid },

    #[error("State machine error: {error}, for task {task_uuid}")]
    StateMachine { error: String, task_uuid: Uuid },

    #[error("Invalid state transition: {transition}, for task {task_uuid}")]
    InvalidTransition { transition: String, task_uuid: Uuid },

    #[error("Context unavailable for task: {task_uuid}")]
    ContextUnavailable { task_uuid: Uuid },

    #[error("Event publishing error: {0}")]
    EventPublishing(String),

    #[error("General error: {0}")]
    General(String),
}

impl From<TaskerError> for FinalizationError {
    fn from(error: TaskerError) -> Self {
        FinalizationError::General(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finalization_action_serialization() {
        let completed = FinalizationAction::Completed;
        assert_eq!(serde_json::to_string(&completed).unwrap(), "\"Completed\"");

        let failed = FinalizationAction::Failed;
        assert_eq!(serde_json::to_string(&failed).unwrap(), "\"Failed\"");

        let reenqueued = FinalizationAction::Reenqueued;
        assert_eq!(serde_json::to_string(&reenqueued).unwrap(), "\"Reenqueued\"");
    }

    #[test]
    fn test_finalization_result_creation() {
        let task_uuid = Uuid::new_v4();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::Completed,
            completion_percentage: Some(100.0),
            total_steps: Some(5),
            enqueued_steps: None,
            health_status: Some("healthy".to_string()),
            reason: None,
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::Completed));
        assert_eq!(result.completion_percentage, Some(100.0));
        assert_eq!(result.total_steps, Some(5));
        assert_eq!(result.health_status, Some("healthy".to_string()));
    }

    #[test]
    fn test_finalization_error_display() {
        let task_uuid = Uuid::new_v4();

        let error = FinalizationError::TaskNotFound { task_uuid };
        assert!(error.to_string().contains("Task not found"));
        assert!(error.to_string().contains(&task_uuid.to_string()));

        let error = FinalizationError::StateMachine {
            error: "invalid transition".to_string(),
            task_uuid,
        };
        assert!(error.to_string().contains("State machine error"));
        assert!(error.to_string().contains("invalid transition"));

        let error = FinalizationError::EventPublishing("failed to publish".to_string());
        assert_eq!(error.to_string(), "Event publishing error: failed to publish");
    }

    #[test]
    fn test_finalization_error_from_sqlx() {
        let sqlx_error = sqlx::Error::RowNotFound;
        let fin_error: FinalizationError = sqlx_error.into();

        match fin_error {
            FinalizationError::Database(err) => {
                // Verify it's the RowNotFound error
                assert!(err.to_string().contains("no rows returned"));
            }
            _ => panic!("Expected Database error"),
        }
    }

    #[test]
    fn test_finalization_error_from_tasker_error() {
        let tasker_error = TaskerError::OrchestrationError("test error".to_string());
        let fin_error: FinalizationError = tasker_error.into();

        match fin_error {
            FinalizationError::General(msg) => {
                assert!(msg.contains("test error"));
            }
            _ => panic!("Expected General error"),
        }
    }

    #[test]
    fn test_finalization_result_with_reason() {
        let task_uuid = Uuid::new_v4();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::Failed,
            completion_percentage: Some(75.0),
            total_steps: Some(10),
            enqueued_steps: Some(2),
            health_status: Some("degraded".to_string()),
            reason: Some("Steps in error state".to_string()),
        };

        assert_eq!(result.reason, Some("Steps in error state".to_string()));
        assert!(matches!(result.action, FinalizationAction::Failed));
    }
}
