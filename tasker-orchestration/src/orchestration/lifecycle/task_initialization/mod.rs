//! Task Initialization Module
//!
//! Atomic task creation with focused, single-responsibility components.
//!
//! ## Architecture
//!
//! This module decomposes task initialization into distinct responsibilities:
//!
//! - **NamespaceResolver**: Finds or creates task namespaces and named tasks
//! - **TemplateLoader**: Loads task templates from the registry
//! - **WorkflowStepBuilder**: Creates workflow steps and dependencies (DAG)
//! - **StateInitializer**: Initializes state machines for tasks and steps
//!
//! The main `TaskInitializer` orchestrates these components to provide
//! a clean, transaction-safe task creation interface.

use tasker_shared::TaskerError;

mod namespace_resolver;
mod service;
mod state_initializer;
mod template_loader;
mod workflow_step_builder;

pub use namespace_resolver::NamespaceResolver;
pub use service::{TaskInitializationResult, TaskInitializer};
pub use state_initializer::StateInitializer;
pub use template_loader::TemplateLoader;
pub use workflow_step_builder::WorkflowStepBuilder;

/// Errors that can occur during task initialization
#[derive(Debug, thiserror::Error)]
pub enum TaskInitializationError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Configuration not found for task: {0}")]
    ConfigurationNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("State machine error: {0}")]
    StateMachine(String),

    #[error("Event publishing error: {0}")]
    EventPublishing(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Step enqueuing error: {0}")]
    StepEnqueuing(String),
}

impl TaskInitializationError {
    /// Determine if this error is a client error (4xx) or server error (5xx)
    ///
    /// Client errors should NOT trip circuit breakers:
    /// - ConfigurationNotFound: Template doesn't exist (404)
    /// - InvalidConfiguration: Bad template data (400)
    ///
    /// Server errors SHOULD trip circuit breakers:
    /// - Database: Connection/query failures (500)
    /// - TransactionFailed: Database transaction errors (500)
    /// - StateMachine: Internal state management errors (500)
    /// - EventPublishing: Internal event system errors (500)
    /// - StepEnqueuing: Internal queue errors (500)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            TaskInitializationError::ConfigurationNotFound(_)
                | TaskInitializationError::InvalidConfiguration(_)
        )
    }
}

impl From<TaskInitializationError> for TaskerError {
    fn from(error: TaskInitializationError) -> Self {
        TaskerError::OrchestrationError(format!("Task initialization failed: {error}"))
    }
}

impl From<sqlx::Error> for TaskInitializationError {
    fn from(error: sqlx::Error) -> Self {
        TaskInitializationError::Database(error.to_string())
    }
}
