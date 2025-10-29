//! # Workflow Step Scopes
//!
//! Query scopes for the WorkflowStep model with support for filtering by execution state,
//! step relationships, and task dependencies.

use super::common::{state_helpers, ScopeBuilder};
use crate::models::WorkflowStep;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

/// Query builder for WorkflowStep scopes
pub struct WorkflowStepScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
    has_workflow_step_transitions_join: bool,
    has_current_transitions_join: bool,
    has_tasks_join: bool,
}

crate::debug_with_query_builder!(WorkflowStepScope {
    query: QueryBuilder,
    has_conditions,
    has_workflow_step_transitions_join,
    has_current_transitions_join,
    has_tasks_join
});

impl WorkflowStep {
    /// Start building a scoped query
    pub fn scope() -> WorkflowStepScope {
        let query = QueryBuilder::new("SELECT tasker_workflow_steps.* FROM tasker_workflow_steps");
        WorkflowStepScope {
            query,
            has_conditions: false,
            has_workflow_step_transitions_join: false,
            has_current_transitions_join: false,
            has_tasks_join: false,
        }
    }
}

impl WorkflowStepScope {
    /// Add WHERE clause helper
    fn add_condition(&mut self, condition: &str) {
        if self.has_conditions {
            self.query.push(" AND ");
        } else {
            self.query.push(" WHERE ");
            self.has_conditions = true;
        }
        self.query.push(condition);
    }

    /// Ensure workflow step transitions join exists
    fn ensure_workflow_step_transitions_join(&mut self) {
        if !self.has_workflow_step_transitions_join {
            self.query.push(" INNER JOIN tasker_workflow_step_transitions ON tasker_workflow_step_transitions.workflow_step_uuid = tasker_workflow_steps.workflow_step_uuid");
            self.has_workflow_step_transitions_join = true;
        }
    }

    /// Ensure current transitions join exists (most recent transition per step)
    fn ensure_current_transitions_join(&mut self) {
        if !self.has_current_transitions_join {
            self.query.push(
                " INNER JOIN ( \
                SELECT DISTINCT ON (workflow_step_uuid) workflow_step_uuid, to_state, created_at \
                FROM tasker_workflow_step_transitions \
                ORDER BY workflow_step_uuid, sort_key DESC \
            ) current_wst ON current_wst.workflow_step_uuid = tasker_workflow_steps.workflow_step_uuid",
            );
            self.has_current_transitions_join = true;
        }
    }

    /// Ensure tasks join exists
    fn ensure_tasks_join(&mut self) {
        if !self.has_tasks_join {
            self.query.push(
                " INNER JOIN tasker_tasks ON tasker_tasks.task_uuid = tasker_workflow_steps.task_uuid",
            );
            self.has_tasks_join = true;
        }
    }

    /// Scope: for_task - Steps belonging to a specific task (Rails: scope :for_task)
    ///
    /// Filters workflow steps to those belonging to a specific task ID.
    /// Essential for task-scoped operations and workflow analysis.
    pub fn for_task(mut self, task_uuid: Uuid) -> Self {
        self.add_condition("tasker_workflow_steps.task_uuid = ");
        self.query.push_bind(task_uuid);
        self
    }

    /// Scope: for_named_step - Steps for a specific named step (Rails: scope :for_named_step)
    ///
    /// Filters to workflow steps created from a specific named step template.
    /// Useful for finding all instances of a particular step type.
    pub fn for_named_step(mut self, named_step_uuid: i32) -> Self {
        self.add_condition("tasker_workflow_steps.named_step_uuid = ");
        self.query.push_bind(named_step_uuid);
        self
    }

    /// Scope: pending - Steps in pending state (Rails: scope :pending)
    ///
    /// Finds workflow steps that are ready to be executed but haven't started yet.
    /// Uses state machine transitions to determine current state.
    pub fn pending(mut self) -> Self {
        self.ensure_current_transitions_join();
        self.add_condition("current_wst.to_state = 'pending'");
        self
    }

    /// Scope: in_progress - Steps currently executing (Rails: scope :in_progress)
    ///
    /// Finds workflow steps that are currently being processed.
    /// Essential for monitoring active workflow execution.
    pub fn in_progress(mut self) -> Self {
        self.ensure_current_transitions_join();
        self.add_condition("current_wst.to_state = 'in_progress'");
        self
    }

    /// Scope: completed - Successfully completed steps (Rails: scope :completed)
    ///
    /// Finds workflow steps that have completed successfully.
    /// Uses state machine validation constants for reliable filtering.
    pub fn completed(mut self) -> Self {
        self.ensure_workflow_step_transitions_join();
        let condition = state_helpers::completed_step_condition();
        self.add_condition(&condition);
        self
    }

    /// Scope: failed - Steps that failed during execution (Rails: scope :failed)
    ///
    /// Finds workflow steps that encountered errors during execution.
    /// Important for error handling and retry logic.
    pub fn failed(mut self) -> Self {
        self.ensure_workflow_step_transitions_join();
        let condition = state_helpers::failed_step_condition();
        self.add_condition(&condition);
        self
    }

    /// Scope: retryable - Steps eligible for retry (Rails: scope :retryable)
    ///
    /// Finds workflow steps that can be retried based on their retry settings.
    /// Combines retryable flag with attempt count validation.
    pub fn retryable(mut self) -> Self {
        self.add_condition("tasker_workflow_steps.retryable = true AND (tasker_workflow_steps.attempts IS NULL OR tasker_workflow_steps.attempts < tasker_workflow_steps.max_attempts)");
        self
    }

    /// Scope: processed - Steps marked as processed (Rails: scope :processed)
    ///
    /// Simple boolean flag check for processed status.
    /// Faster than state machine queries for basic processing flags.
    pub fn processed(mut self) -> Self {
        self.add_condition("tasker_workflow_steps.processed = true");
        self
    }

    /// Scope: unprocessed - Steps not yet processed (Rails: scope :unprocessed)
    ///
    /// Finds steps that haven't been marked as processed.
    /// Used for identifying pending work and workflow bottlenecks.
    pub fn unprocessed(mut self) -> Self {
        self.add_condition("tasker_workflow_steps.processed = false");
        self
    }

    /// Scope: active - Steps that are ready or in progress (Rails: scope :active)
    ///
    /// Combines multiple states to find "active" workflow steps that need attention.
    /// Uses state helper for consistent state machine integration.
    pub fn active(mut self) -> Self {
        self.ensure_current_transitions_join();
        let condition = state_helpers::active_step_condition();
        self.add_condition(&condition);
        self
    }

    /// Scope: for_tasks_since - Steps for tasks created since a specific time
    ///
    /// Useful for temporal analysis and filtering recent workflow activity.
    /// Requires JOIN with tasks table for creation timestamp access.
    pub fn for_tasks_since(mut self, since: DateTime<Utc>) -> Self {
        self.ensure_tasks_join();
        self.add_condition("tasker_tasks.created_at > ");
        self.query.push_bind(since.naive_utc());
        self
    }

    /// Scope: with_results - Steps that have execution results stored
    ///
    /// Filters to steps that completed with result data.
    /// Useful for analysis and result processing workflows.
    pub fn with_results(mut self) -> Self {
        self.add_condition("tasker_workflow_steps.results IS NOT NULL");
        self
    }

    /// Scope: latest_attempts - Steps ordered by most recent attempt
    ///
    /// Orders workflow steps by their last attempted timestamp.
    /// Useful for retry queue processing and debugging.
    pub fn latest_attempts(mut self) -> Self {
        self.query
            .push(" ORDER BY tasker_workflow_steps.last_attempted_at DESC NULLS LAST");
        self
    }

    /// Scope: by_current_state - Steps filtered by their current state
    pub fn by_current_state(mut self, state: crate::state_machine::WorkflowStepState) -> Self {
        self.ensure_current_transitions_join();
        self.add_condition("current_wst.to_state = ");
        self.query.push_bind(state.to_string());
        self
    }

    /// Scope: completed_since - Steps completed after a specific time
    pub fn completed_since(mut self, since: DateTime<Utc>) -> Self {
        self.ensure_workflow_step_transitions_join();
        self.add_condition(
            "tasker_workflow_step_transitions.most_recent = TRUE \
             AND tasker_workflow_step_transitions.to_state = 'complete' \
             AND tasker_workflow_step_transitions.created_at > ",
        );
        self.query.push_bind(since.naive_utc());
        self
    }

    /// Scope: failed_since - Steps that failed after a specific time
    pub fn failed_since(mut self, since: DateTime<Utc>) -> Self {
        self.ensure_workflow_step_transitions_join();
        self.add_condition(
            "tasker_workflow_step_transitions.most_recent = TRUE \
             AND tasker_workflow_step_transitions.to_state = 'error' \
             AND tasker_workflow_step_transitions.created_at > ",
        );
        self.query.push_bind(since.naive_utc());
        self
    }
}

impl ScopeBuilder<WorkflowStep> for WorkflowStepScope {
    async fn all(mut self, pool: &PgPool) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        let query = self.query.build_query_as::<WorkflowStep>();
        query.fetch_all(pool).await
    }

    async fn first(mut self, pool: &PgPool) -> Result<Option<WorkflowStep>, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<WorkflowStep>();
        query.fetch_optional(pool).await
    }

    async fn count(self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let count_query = self.query.sql().replace(
            "SELECT tasker_workflow_steps.* FROM tasker_workflow_steps",
            "SELECT COUNT(*) as count FROM tasker_workflow_steps",
        );

        let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
        Ok(row.0)
    }

    async fn exists(mut self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<WorkflowStep>();
        let result = query.fetch_optional(pool).await?;
        Ok(result.is_some())
    }
}
