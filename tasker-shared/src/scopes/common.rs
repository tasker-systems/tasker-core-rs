//! # Common Scope Functionality
//!
//! Shared traits, helpers, and utilities used across all scope builders.

use crate::constants::status_groups;
use crate::state_machine::{TaskState, WorkflowStepState};
use sqlx::{types::Uuid, PgPool};

/// Base trait for all scope builders
pub trait ScopeBuilder<T> {
    /// Build the final query and execute it
    fn all(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<T>, sqlx::Error>> + Send;

    /// Get a single result (first match)
    fn first(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<T>, sqlx::Error>> + Send;

    /// Count the number of results
    fn count(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send;

    /// Check if any results exist
    fn exists(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<bool, sqlx::Error>> + Send;
}

/// Helper functions for generating state-based SQL conditions using validated constants
pub mod state_helpers {
    use super::*;

    /// Generate SQL condition for active (non-complete) workflow steps
    ///
    /// Uses validated constants to ensure consistency with state machine definitions
    pub fn active_step_condition() -> String {
        let excluded_states: Vec<String> = status_groups::UNREADY_WORKFLOW_STEP_STATUSES
            .iter()
            .map(|state| format!("'{state}'"))
            .collect();

        format!(
            "wst.most_recent = true AND wst.to_state NOT IN ({})",
            excluded_states.join(", ")
        )
    }

    /// Generate SQL condition for completed workflow steps
    pub fn completed_step_condition() -> String {
        let completion_states: Vec<String> = status_groups::VALID_STEP_COMPLETION_STATES
            .iter()
            .map(|state| format!("'{state}'"))
            .collect();

        format!(
            "tasker_workflow_step_transitions.most_recent = TRUE AND tasker_workflow_step_transitions.to_state IN ({})",
            completion_states.join(", ")
        )
    }

    /// Generate SQL condition for failed workflow steps
    pub fn failed_step_condition() -> String {
        format!(
            "tasker_workflow_step_transitions.most_recent = TRUE AND tasker_workflow_step_transitions.to_state = '{}'",
            WorkflowStepState::Error
        )
    }

    /// Generate SQL condition for completed task state
    pub fn completed_task_condition() -> String {
        format!("wst.to_state = '{}'", WorkflowStepState::Complete)
    }

    /// Generate SQL condition for failed task state
    pub fn failed_task_condition() -> String {
        format!("current_transitions.to_state = '{}'", TaskState::Error)
    }
}

/// Special query types that need custom execution
#[derive(Debug)]
pub enum SpecialQuery {
    SiblingsOf(Uuid),
    ProvidesToChildren(Uuid),
}
