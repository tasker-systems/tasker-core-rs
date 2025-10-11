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
