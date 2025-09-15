//! # Transition Scopes
//!
//! Query scopes for TaskTransition and WorkflowStepTransition models.
//! These scopes provide filtering and querying capabilities for state transition history.

use super::common::ScopeBuilder;
use crate::models::{TaskTransition, WorkflowStepTransition};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

/// Query builder for TaskTransition scopes
pub struct TaskTransitionScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
}

impl TaskTransition {
    /// Start building a scoped query
    pub fn scope() -> TaskTransitionScope {
        let query =
            QueryBuilder::new("SELECT tasker_task_transitions.* FROM tasker_task_transitions");
        TaskTransitionScope {
            query,
            has_conditions: false,
        }
    }
}

impl TaskTransitionScope {
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

    /// Scope: for_task - Transitions for a specific task (Rails: scope :for_task)
    ///
    /// Filters transitions to those belonging to a specific task.
    /// Essential for tracking task state history and audit trails.
    pub fn for_task(mut self, task_uuid: Uuid) -> Self {
        self.add_condition("tasker_task_transitions.task_uuid = ");
        self.query.push_bind(task_uuid);
        self
    }

    /// Scope: to_state - Transitions to a specific state (Rails: scope :to_state)
    ///
    /// Filters transitions by their target state.
    /// Useful for finding all instances of specific state changes.
    pub fn to_state(mut self, state: crate::state_machine::TaskState) -> Self {
        self.add_condition("tasker_task_transitions.to_state = ");
        self.query.push_bind(state.to_string());
        self
    }

    /// Scope: most_recent - Only the most recent transition per task (Rails: scope :most_recent)
    ///
    /// Filters to only the current state of each task.
    /// Critical for getting current task states efficiently.
    pub fn most_recent(mut self) -> Self {
        self.add_condition("tasker_task_transitions.most_recent = true");
        self
    }

    /// Scope: since - Transitions created after a specific time (Rails: scope :since)
    ///
    /// Temporal filtering for transition analysis and reporting.
    /// Useful for change tracking and activity monitoring.
    pub fn since(mut self, since: DateTime<Utc>) -> Self {
        self.add_condition("tasker_task_transitions.created_at > ");
        self.query.push_bind(since.naive_utc());
        self
    }

    /// Scope: with_metadata_key - Transitions containing specific metadata (Rails: scope :with_metadata_key)
    ///
    /// Filters transitions that contain a specific key in their metadata JSON.
    /// Enables rich querying of transition context and audit information.
    pub fn with_metadata_key(mut self, key: String) -> Self {
        self.add_condition("tasker_task_transitions.metadata ? ");
        self.query.push_bind(key);
        self
    }

    /// Scope: ordered_by_sort_key - Transitions in chronological order
    ///
    /// Orders transitions by their sort_key for proper chronological sequence.
    /// Essential for state machine analysis and history reconstruction.
    pub fn ordered_by_sort_key(mut self) -> Self {
        self.query
            .push(" ORDER BY tasker_task_transitions.sort_key ASC");
        self
    }

    /// Scope: recent - Order by most recent transitions first
    pub fn recent(mut self) -> Self {
        self.query
            .push(" ORDER BY tasker_task_transitions.sort_key DESC");
        self
    }
}

impl ScopeBuilder<TaskTransition> for TaskTransitionScope {
    async fn all(mut self, pool: &PgPool) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let query = self.query.build_query_as::<TaskTransition>();
        query.fetch_all(pool).await
    }

    async fn first(mut self, pool: &PgPool) -> Result<Option<TaskTransition>, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<TaskTransition>();
        query.fetch_optional(pool).await
    }

    async fn count(self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let count_query = self.query.sql().replace(
            "SELECT tasker_task_transitions.* FROM tasker_task_transitions",
            "SELECT COUNT(*) as count FROM tasker_task_transitions",
        );

        let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
        Ok(row.0)
    }

    async fn exists(mut self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<TaskTransition>();
        let result = query.fetch_optional(pool).await?;
        Ok(result.is_some())
    }
}

/// Query builder for WorkflowStepTransition scopes
pub struct WorkflowStepTransitionScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
    has_workflow_steps_join: bool,
}

impl WorkflowStepTransition {
    /// Start building a scoped query
    pub fn scope() -> WorkflowStepTransitionScope {
        let query = QueryBuilder::new(
            "SELECT tasker_workflow_step_transitions.* FROM tasker_workflow_step_transitions",
        );
        WorkflowStepTransitionScope {
            query,
            has_conditions: false,
            has_workflow_steps_join: false,
        }
    }
}

impl WorkflowStepTransitionScope {
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

    /// Ensure workflow steps join exists
    fn ensure_workflow_steps_join(&mut self) {
        if !self.has_workflow_steps_join {
            self.query.push(" INNER JOIN tasker_workflow_steps ON tasker_workflow_steps.workflow_step_uuid = tasker_workflow_step_transitions.workflow_step_uuid");
            self.has_workflow_steps_join = true;
        }
    }

    /// Scope: recent - Recent transitions ordered by creation time (Rails: scope :recent)
    ///
    /// Orders workflow step transitions by creation time, most recent first.
    /// Useful for monitoring recent activity and debugging workflow execution.
    pub fn recent(mut self) -> Self {
        self.query
            .push(" ORDER BY tasker_workflow_step_transitions.created_at DESC");
        self
    }

    /// Scope: to_state - Transitions to a specific state (Rails: scope :to_state)
    ///
    /// Filters workflow step transitions by their target state.
    /// Essential for tracking specific state changes across workflow steps.
    pub fn to_state(mut self, state: String) -> Self {
        self.add_condition("tasker_workflow_step_transitions.to_state = ");
        self.query.push_bind(state);
        self
    }

    /// Scope: with_metadata_key - Transitions with specific metadata (Rails: scope :with_metadata_key)
    ///
    /// Filters to transitions that contain a specific key in their metadata JSONB field.
    /// Enables rich querying of transition context, error details, and execution metadata.
    pub fn with_metadata_key(mut self, key: String) -> Self {
        self.add_condition("tasker_workflow_step_transitions.metadata ? ");
        self.query.push_bind(key);
        self
    }

    /// Scope: for_task - Transitions for steps belonging to a specific task (Rails: scope :for_task)
    ///
    /// Filters workflow step transitions to those belonging to steps of a specific task.
    /// Requires JOIN with workflow_steps table to access task_uuid.
    pub fn for_task(mut self, task_uuid: Uuid) -> Self {
        self.ensure_workflow_steps_join();
        self.add_condition("tasker_workflow_steps.task_uuid = ");
        self.query.push_bind(task_uuid);
        self
    }
}

impl ScopeBuilder<WorkflowStepTransition> for WorkflowStepTransitionScope {
    async fn all(mut self, pool: &PgPool) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let query = self.query.build_query_as::<WorkflowStepTransition>();
        query.fetch_all(pool).await
    }

    async fn first(mut self, pool: &PgPool) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<WorkflowStepTransition>();
        query.fetch_optional(pool).await
    }

    async fn count(self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        // Simple approach for WorkflowStepTransition count
        let results = self.all(pool).await?;
        Ok(results.len() as i64)
    }

    async fn exists(mut self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<WorkflowStepTransition>();
        let result = query.fetch_optional(pool).await?;
        Ok(result.is_some())
    }
}
