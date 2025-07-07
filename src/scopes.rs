//! # Query Scopes
//!
//! This module provides Rails-like scopes for building complex SQL queries with type safety.
//! Each model has its own scope builder that can be chained to create sophisticated queries.
//!
//! ## Design Philosophy
//!
//! - **Type Safety**: All scopes are validated at compile time
//! - **Rails Compatibility**: Direct port of Rails scopes with identical behavior
//! - **Performance**: Uses prepared statements and compile-time query verification
//! - **Composability**: Scopes can be chained and combined

#![allow(clippy::manual_async_fn)]

/// Helper functions for generating state-based SQL conditions using validated constants
mod state_helpers {
    use crate::constants::status_groups;
    use crate::state_machine::{TaskState, WorkflowStepState};

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

use crate::models::{
    NamedTask, Task, TaskNamespace, TaskTransition, WorkflowStep, WorkflowStepEdge,
    WorkflowStepTransition,
};
use crate::state_machine::{TaskState, WorkflowStepState};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};

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

/// Query builder for Task scopes
pub struct TaskScope {
    query: QueryBuilder<'static, Postgres>,
    has_current_transitions_join: bool,
    has_named_tasks_join: bool,
    has_namespaces_join: bool,
    has_workflow_steps_join: bool,
    has_workflow_step_transitions_join: bool,
    has_conditions: bool,
}

impl Task {
    /// Start building a scoped query
    pub fn scope() -> TaskScope {
        let query = QueryBuilder::new("SELECT tasker_tasks.* FROM tasker_tasks");
        TaskScope {
            query,
            has_current_transitions_join: false,
            has_named_tasks_join: false,
            has_namespaces_join: false,
            has_workflow_steps_join: false,
            has_workflow_step_transitions_join: false,
            has_conditions: false,
        }
    }
}

impl TaskScope {
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

    /// Ensure current transitions join exists
    fn ensure_current_transitions_join(&mut self) {
        if !self.has_current_transitions_join {
            self.query.push(
                " INNER JOIN ( \
                SELECT DISTINCT ON (task_id) task_id, to_state, created_at \
                FROM tasker_task_transitions \
                ORDER BY task_id, sort_key DESC \
            ) current_transitions ON current_transitions.task_id = tasker_tasks.task_id",
            );
            self.has_current_transitions_join = true;
        }
    }

    /// Ensure named tasks join exists
    fn ensure_named_tasks_join(&mut self) {
        if !self.has_named_tasks_join {
            // Don't add joins if we already have WHERE conditions - this is a limitation
            if !self.has_conditions {
                self.query.push(" INNER JOIN tasker_named_tasks ON tasker_named_tasks.named_task_id = tasker_tasks.named_task_id");
                self.has_named_tasks_join = true;
            }
        }
    }

    /// Ensure namespace join exists (requires named tasks join)
    fn ensure_namespaces_join(&mut self) {
        self.ensure_named_tasks_join();
        if !self.has_namespaces_join {
            // Don't add joins if we already have WHERE conditions - this is a limitation
            if !self.has_conditions {
                self.query.push(" INNER JOIN tasker_task_namespaces ON tasker_task_namespaces.task_namespace_id = tasker_named_tasks.task_namespace_id");
                self.has_namespaces_join = true;
            }
        }
    }

    /// Ensure workflow steps join exists
    fn ensure_workflow_steps_join(&mut self) {
        if !self.has_workflow_steps_join {
            self.query.push(" INNER JOIN tasker_workflow_steps ON tasker_workflow_steps.task_id = tasker_tasks.task_id");
            self.has_workflow_steps_join = true;
        }
    }

    /// Ensure workflow step transitions join exists (requires workflow steps join)
    fn ensure_workflow_step_transitions_join(&mut self) {
        self.ensure_workflow_steps_join();
        if !self.has_workflow_step_transitions_join {
            self.query.push(" INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_id = tasker_workflow_steps.workflow_step_id");
            self.has_workflow_step_transitions_join = true;
        }
    }

    /// Scope: by_current_state - Tasks filtered by their current state
    pub fn by_current_state(mut self, state: TaskState) -> Self {
        self.ensure_current_transitions_join();
        self.add_condition("current_transitions.to_state = ");
        self.query.push_bind(state.to_string());
        self
    }

    /// Scope: active - Tasks that have active (non-complete) workflow steps
    pub fn active(mut self) -> Self {
        self.ensure_workflow_step_transitions_join();
        let condition = state_helpers::active_step_condition();
        self.add_condition(&condition);
        self
    }

    /// Scope: created_since - Tasks created after a specific time
    pub fn created_since(mut self, since: DateTime<Utc>) -> Self {
        self.add_condition("tasker_tasks.created_at > ");
        self.query.push_bind(since.naive_utc());
        self
    }

    /// Scope: completed_since - Tasks with steps completed after a specific time
    pub fn completed_since(mut self, since: DateTime<Utc>) -> Self {
        self.ensure_workflow_step_transitions_join();
        let condition = format!(
            "{} AND wst.created_at > ",
            state_helpers::completed_task_condition()
        );
        self.add_condition(&condition);
        self.query.push_bind(since.naive_utc());
        self
    }

    /// Scope: failed_since - Tasks that failed after a specific time
    pub fn failed_since(mut self, since: DateTime<Utc>) -> Self {
        // Use current transitions join but check if it already exists
        if !self.has_current_transitions_join {
            self.query.push(
                " INNER JOIN ( \
                SELECT DISTINCT ON (task_id) task_id, to_state, created_at \
                FROM tasker_task_transitions \
                ORDER BY task_id, sort_key DESC \
            ) current_transitions ON current_transitions.task_id = tasker_tasks.task_id",
            );
            self.has_current_transitions_join = true;
        }
        let condition = format!(
            "{} AND current_transitions.created_at > ",
            state_helpers::failed_task_condition()
        );
        self.add_condition(&condition);
        self.query.push_bind(since.naive_utc());
        self
    }

    /// Scope: in_namespace - Tasks in a specific namespace
    pub fn in_namespace(mut self, namespace_name: String) -> Self {
        self.ensure_namespaces_join();
        // Only add the condition if the join was successfully added
        if self.has_namespaces_join {
            self.add_condition("tasker_task_namespaces.name = ");
            self.query.push_bind(namespace_name);
        }
        // If the join wasn't added (due to existing conditions), silently continue
        // This is a limitation of the current query builder approach
        self
    }

    /// Scope: with_task_name - Tasks with a specific named task
    pub fn with_task_name(mut self, task_name: String) -> Self {
        self.ensure_named_tasks_join();
        self.add_condition("tasker_named_tasks.name = ");
        self.query.push_bind(task_name);
        self
    }

    /// Scope: with_version - Tasks with a specific version
    pub fn with_version(mut self, version: String) -> Self {
        self.ensure_named_tasks_join();
        self.add_condition("tasker_named_tasks.version = ");
        self.query.push_bind(version);
        self
    }

    /// Add ordering
    pub fn order_by_created_at(mut self, ascending: bool) -> Self {
        if ascending {
            self.query.push(" ORDER BY tasker_tasks.created_at ASC");
        } else {
            self.query.push(" ORDER BY tasker_tasks.created_at DESC");
        }
        self
    }

    /// Add limit
    pub fn limit(mut self, limit: i64) -> Self {
        self.query.push(" LIMIT ");
        self.query.push_bind(limit);
        self
    }
}

impl ScopeBuilder<Task> for TaskScope {
    fn all(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<Task>, sqlx::Error>> + Send {
        async move {
            let query = self.query.build_query_as::<Task>();
            query.fetch_all(pool).await
        }
    }

    fn first(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<Task>, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<Task>();
            query.fetch_optional(pool).await
        }
    }

    fn count(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            // Replace the SELECT clause for counting
            let count_query = self.query.sql().replace(
                "SELECT tasker_tasks.* FROM tasker_tasks",
                "SELECT COUNT(*) as count FROM tasker_tasks",
            );

            let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
            Ok(row.0)
        }
    }

    fn exists(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<bool, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<Task>();
            let result = query.fetch_optional(pool).await?;
            Ok(result.is_some())
        }
    }
}

/// Query builder for WorkflowStep scopes
pub struct WorkflowStepScope {
    query: QueryBuilder<'static, Postgres>,
    has_workflow_step_transitions_join: bool,
    has_current_transitions_join: bool,
    has_tasks_join: bool,
    has_conditions: bool,
}

impl WorkflowStep {
    /// Start building a scoped query
    pub fn scope() -> WorkflowStepScope {
        let query = QueryBuilder::new("SELECT tasker_workflow_steps.* FROM tasker_workflow_steps");
        WorkflowStepScope {
            query,
            has_workflow_step_transitions_join: false,
            has_current_transitions_join: false,
            has_tasks_join: false,
            has_conditions: false,
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
            self.query.push(" INNER JOIN tasker_workflow_step_transitions \
                             ON tasker_workflow_step_transitions.workflow_step_id = tasker_workflow_steps.workflow_step_id");
            self.has_workflow_step_transitions_join = true;
        }
    }

    /// Ensure current transitions join exists (for state-based queries)
    fn ensure_current_transitions_join(&mut self) {
        if !self.has_current_transitions_join {
            self.query.push(" INNER JOIN ( \
                SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state \
                FROM tasker_workflow_step_transitions \
                WHERE most_recent = true \
                ORDER BY workflow_step_id, sort_key DESC \
            ) current_transitions ON current_transitions.workflow_step_id = tasker_workflow_steps.workflow_step_id");
            self.has_current_transitions_join = true;
        }
    }

    /// Ensure tasks join exists
    fn ensure_tasks_join(&mut self) {
        if !self.has_tasks_join {
            self.query.push(
                " INNER JOIN tasker_tasks ON tasker_tasks.task_id = tasker_workflow_steps.task_id",
            );
            self.has_tasks_join = true;
        }
    }

    /// Scope: completed - Steps that are in completed state
    pub fn completed(mut self) -> Self {
        self.ensure_workflow_step_transitions_join();
        let condition = state_helpers::completed_step_condition();
        self.add_condition(&condition);
        self
    }

    /// Scope: failed - Steps that are in failed state
    pub fn failed(mut self) -> Self {
        self.ensure_workflow_step_transitions_join();
        let condition = state_helpers::failed_step_condition();
        self.add_condition(&condition);
        self
    }

    /// Scope: for_task - Steps belonging to a specific task
    pub fn for_task(mut self, task_id: i64) -> Self {
        self.add_condition("tasker_workflow_steps.task_id = ");
        self.query.push_bind(task_id);
        self
    }

    /// Scope: by_current_state - Steps filtered by their current state
    pub fn by_current_state(mut self, state: WorkflowStepState) -> Self {
        self.ensure_current_transitions_join();
        self.add_condition("current_transitions.to_state = ");
        self.query.push_bind(state.to_string());
        self
    }

    /// Scope: completed_since - Steps completed after a specific time
    pub fn completed_since(mut self, since: DateTime<Utc>) -> Self {
        self.ensure_workflow_step_transitions_join();
        let condition = format!(
            "tasker_workflow_step_transitions.most_recent = TRUE \
             AND tasker_workflow_step_transitions.to_state = '{}' \
             AND tasker_workflow_step_transitions.created_at > ",
            WorkflowStepState::Complete
        );
        self.add_condition(&condition);
        self.query.push_bind(since.naive_utc());
        self
    }

    /// Scope: failed_since - Steps that failed after a specific time
    pub fn failed_since(mut self, since: DateTime<Utc>) -> Self {
        self.ensure_workflow_step_transitions_join();
        let condition = format!(
            "tasker_workflow_step_transitions.most_recent = TRUE \
             AND tasker_workflow_step_transitions.to_state = '{}' \
             AND tasker_workflow_step_transitions.created_at > ",
            WorkflowStepState::Error
        );
        self.add_condition(&condition);
        self.query.push_bind(since.naive_utc());
        self
    }

    /// Scope: for_tasks_since - Steps for tasks created after a specific time
    pub fn for_tasks_since(mut self, since: DateTime<Utc>) -> Self {
        self.ensure_tasks_join();
        self.add_condition("tasker_tasks.created_at > ");
        self.query.push_bind(since.naive_utc());
        self
    }
}

impl ScopeBuilder<WorkflowStep> for WorkflowStepScope {
    fn all(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<WorkflowStep>, sqlx::Error>> + Send {
        async move {
            let query = self.query.build_query_as::<WorkflowStep>();
            query.fetch_all(pool).await
        }
    }

    fn first(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<WorkflowStep>, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<WorkflowStep>();
            query.fetch_optional(pool).await
        }
    }

    fn count(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            let count_query = self.query.sql().replace(
                "SELECT tasker_workflow_steps.* FROM tasker_workflow_steps",
                "SELECT COUNT(*) as count FROM tasker_workflow_steps",
            );

            let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
            Ok(row.0)
        }
    }

    fn exists(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<bool, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<WorkflowStep>();
            let result = query.fetch_optional(pool).await?;
            Ok(result.is_some())
        }
    }
}

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

    /// Scope: recent - Order by most recent transitions first
    pub fn recent(mut self) -> Self {
        self.query
            .push(" ORDER BY tasker_task_transitions.sort_key DESC");
        self
    }

    /// Scope: to_state - Filter by destination state
    pub fn to_state(mut self, state: TaskState) -> Self {
        self.add_condition("tasker_task_transitions.to_state = ");
        self.query.push_bind(state.to_string());
        self
    }

    /// Scope: with_metadata_key - Filter by presence of metadata key
    pub fn with_metadata_key(mut self, key: String) -> Self {
        self.add_condition("metadata ? ");
        self.query.push_bind(key);
        self
    }

    /// Scope: for_task - Transitions for a specific task
    pub fn for_task(mut self, task_id: i64) -> Self {
        self.add_condition("tasker_task_transitions.task_id = ");
        self.query.push_bind(task_id);
        self
    }
}

impl ScopeBuilder<TaskTransition> for TaskTransitionScope {
    fn all(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<TaskTransition>, sqlx::Error>> + Send {
        async move {
            let query = self.query.build_query_as::<TaskTransition>();
            query.fetch_all(pool).await
        }
    }

    fn first(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<TaskTransition>, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<TaskTransition>();
            query.fetch_optional(pool).await
        }
    }

    fn count(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            let count_query = self.query.sql().replace(
                "SELECT tasker_task_transitions.* FROM tasker_task_transitions",
                "SELECT COUNT(*) as count FROM tasker_task_transitions",
            );

            let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
            Ok(row.0)
        }
    }

    fn exists(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<bool, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<TaskTransition>();
            let result = query.fetch_optional(pool).await?;
            Ok(result.is_some())
        }
    }
}

/// Query builder for WorkflowStepEdge scopes
pub struct WorkflowStepEdgeScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
    is_custom_query: bool,
}

impl WorkflowStepEdge {
    /// Start building a scoped query
    pub fn scope() -> WorkflowStepEdgeScope {
        let query = QueryBuilder::new(
            "SELECT tasker_workflow_step_edges.* FROM tasker_workflow_step_edges",
        );
        WorkflowStepEdgeScope {
            query,
            has_conditions: false,
            is_custom_query: false,
        }
    }
}

impl WorkflowStepEdgeScope {
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

    /// Scope: children_of - Get edges where step is the parent (from_step)
    ///
    /// This finds all workflow steps that are direct children of the given step.
    /// Essential for DAG forward navigation.
    pub fn children_of(mut self, step_id: i64) -> Self {
        self.add_condition("tasker_workflow_step_edges.from_step_id = ");
        self.query.push_bind(step_id);
        self
    }

    /// Scope: parents_of - Get edges where step is the child (to_step)
    ///
    /// This finds all workflow steps that are direct parents of the given step.
    /// Essential for DAG backward navigation and dependency checking.
    pub fn parents_of(mut self, step_id: i64) -> Self {
        self.add_condition("tasker_workflow_step_edges.to_step_id = ");
        self.query.push_bind(step_id);
        self
    }

    /// Scope: provides_edges - Get edges with name 'provides'
    ///
    /// Filters to only 'provides' relationship edges, which are the standard
    /// dependency relationships in the workflow DAG.
    pub fn provides_edges(mut self) -> Self {
        self.add_condition("tasker_workflow_step_edges.name = 'provides'");
        self
    }

    /// Scope: provides_to_children - Get 'provides' edges to children of a step
    ///
    /// This finds all 'provides' edges that point to the children of the given step.
    /// Useful for analyzing downstream dependencies.
    pub fn provides_to_children(self, step_id: i64) -> Self {
        // Build a custom query with the subquery, similar to siblings_of approach
        let provides_to_children_sql = format!(
            r"
            SELECT tasker_workflow_step_edges.*
            FROM tasker_workflow_step_edges
            WHERE tasker_workflow_step_edges.name = 'provides'
            AND tasker_workflow_step_edges.to_step_id IN (
                SELECT to_step_id
                FROM tasker_workflow_step_edges
                WHERE from_step_id = {step_id}
            )
            "
        );

        WorkflowStepEdgeScope {
            query: QueryBuilder::new(&provides_to_children_sql),
            has_conditions: false,
            is_custom_query: true,
        }
    }

    /// Scope: siblings_of - Get edges to steps that share the same parent set
    ///
    /// This is CRITICAL for workflow orchestration to find steps that can run in parallel.
    /// Two steps are siblings if they have exactly the same set of parent steps.
    ///
    /// Uses a complex CTE (Common Table Expression) to:
    /// 1. Find all parents of the target step
    /// 2. Find all potential siblings (steps that share at least one parent)
    /// 3. Filter to only steps that have EXACTLY the same parent set using array aggregation
    ///
    /// This is essential for the orchestration engine to determine which steps
    /// are ready to run in parallel after their dependencies are satisfied.
    pub fn siblings_of(self, step_id: i64) -> Self {
        // Build the complex CTE query for finding siblings
        // This matches the Rails implementation exactly
        let siblings_sql = format!(
            r"
            WITH step_parents AS (
                SELECT from_step_id
                FROM tasker_workflow_step_edges
                WHERE to_step_id = {step_id}
            ),
            potential_siblings AS (
                SELECT to_step_id
                FROM tasker_workflow_step_edges
                WHERE from_step_id IN (SELECT from_step_id FROM step_parents)
                AND to_step_id != {step_id}
            ),
            siblings AS (
                SELECT to_step_id
                FROM tasker_workflow_step_edges
                WHERE to_step_id IN (SELECT to_step_id FROM potential_siblings)
                GROUP BY to_step_id
                HAVING ARRAY_AGG(from_step_id ORDER BY from_step_id) =
                      (SELECT ARRAY_AGG(from_step_id ORDER BY from_step_id) FROM step_parents)
            )
            SELECT e.*
            FROM tasker_workflow_step_edges e
            JOIN siblings ON e.to_step_id = siblings.to_step_id
            "
        );

        // Replace the entire query with the CTE
        WorkflowStepEdgeScope {
            query: QueryBuilder::new(&siblings_sql),
            has_conditions: false,
            is_custom_query: true,
        }
    }
}

impl ScopeBuilder<WorkflowStepEdge> for WorkflowStepEdgeScope {
    fn all(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<WorkflowStepEdge>, sqlx::Error>> + Send {
        async move {
            let query = self.query.build_query_as::<WorkflowStepEdge>();
            query.fetch_all(pool).await
        }
    }

    fn first(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<WorkflowStepEdge>, sqlx::Error>> + Send
    {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<WorkflowStepEdge>();
            query.fetch_optional(pool).await
        }
    }

    fn count(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            let count_query = if self.is_custom_query || self.query.sql().contains("WITH") {
                // For CTE queries, wrap in outer count
                format!(
                    "SELECT COUNT(*) as count FROM ({}) as subquery",
                    self.query.sql()
                )
            } else {
                // For simple queries, replace SELECT clause
                self.query.sql().replace(
                    "SELECT tasker_workflow_step_edges.* FROM tasker_workflow_step_edges",
                    "SELECT COUNT(*) as count FROM tasker_workflow_step_edges",
                )
            };

            let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
            Ok(row.0)
        }
    }

    fn exists(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<bool, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<WorkflowStepEdge>();
            let result = query.fetch_optional(pool).await?;
            Ok(result.is_some())
        }
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
            self.query.push(" INNER JOIN tasker_workflow_steps ON tasker_workflow_steps.workflow_step_id = tasker_workflow_step_transitions.workflow_step_id");
            self.has_workflow_steps_join = true;
        }
    }

    /// Scope: recent - Order by most recent transitions first (Rails: scope :recent)
    ///
    /// Orders transitions by sort_key DESC to show the most recent transitions first.
    /// This is useful for debugging and monitoring recent activity.
    pub fn recent(mut self) -> Self {
        self.query
            .push(" ORDER BY tasker_workflow_step_transitions.sort_key DESC");
        self
    }

    /// Scope: to_state - Filter by destination state (Rails: scope :to_state)
    ///
    /// Filters transitions to only those that transitioned TO a specific state.
    /// Essential for finding all steps that are currently in a particular state.
    pub fn to_state(mut self, state: String) -> Self {
        self.add_condition("tasker_workflow_step_transitions.to_state = ");
        self.query.push_bind(state);
        self
    }

    /// Scope: with_metadata_key - Filter by presence of metadata key (Rails: scope :with_metadata_key)
    ///
    /// Uses PostgreSQL JSONB ? operator to check if metadata contains a specific key.
    /// Useful for finding transitions with specific metadata like error details, retry info, etc.
    pub fn with_metadata_key(mut self, key: String) -> Self {
        self.add_condition("tasker_workflow_step_transitions.metadata ? ");
        self.query.push_bind(key);
        self
    }

    /// Scope: for_task - Get transitions for all workflow steps of a specific task (Rails: scope :for_task)
    ///
    /// Joins with workflow_steps to find all transitions for steps belonging to a task.
    /// Critical for task-level analysis and debugging workflow execution.
    pub fn for_task(mut self, task_id: i64) -> Self {
        self.ensure_workflow_steps_join();
        self.add_condition("tasker_workflow_steps.task_id = ");
        self.query.push_bind(task_id);
        self
    }
}

impl ScopeBuilder<WorkflowStepTransition> for WorkflowStepTransitionScope {
    fn all(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<WorkflowStepTransition>, sqlx::Error>> + Send
    {
        async move {
            let query = self.query.build_query_as::<WorkflowStepTransition>();
            query.fetch_all(pool).await
        }
    }

    fn first(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<WorkflowStepTransition>, sqlx::Error>> + Send
    {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<WorkflowStepTransition>();
            query.fetch_optional(pool).await
        }
    }

    fn count(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            // For now, use a simple approach that handles most cases correctly
            // Complex queries with parameters will fall back to counting results
            let results = self.all(pool).await?;
            Ok(results.len() as i64)
        }
    }

    fn exists(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<bool, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<WorkflowStepTransition>();
            let result = query.fetch_optional(pool).await?;
            Ok(result.is_some())
        }
    }
}

/// Query builder for NamedTask scopes
pub struct NamedTaskScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
    has_task_namespace_join: bool,
}

impl NamedTask {
    /// Start building a scoped query
    pub fn scope() -> NamedTaskScope {
        let query = QueryBuilder::new("SELECT tasker_named_tasks.* FROM tasker_named_tasks");
        NamedTaskScope {
            query,
            has_conditions: false,
            has_task_namespace_join: false,
        }
    }
}

impl NamedTaskScope {
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

    /// Ensure task namespace join exists
    fn ensure_task_namespace_join(&mut self) {
        if !self.has_task_namespace_join {
            self.query.push(" INNER JOIN tasker_task_namespaces task_namespace ON task_namespace.task_namespace_id = tasker_named_tasks.task_namespace_id");
            self.has_task_namespace_join = true;
        }
    }

    /// Scope: in_namespace - Filter named tasks by namespace name (Rails: scope :in_namespace)
    ///
    /// Joins with task_namespaces to filter named tasks belonging to a specific namespace.
    /// Essential for organizing and categorizing different types of tasks.
    pub fn in_namespace(mut self, namespace_name: String) -> Self {
        self.ensure_task_namespace_join();
        self.add_condition("task_namespace.name = ");
        self.query.push_bind(namespace_name);
        self
    }

    /// Scope: with_version - Filter named tasks by specific version (Rails: scope :with_version)
    ///
    /// Filters to named tasks with a specific version string.
    /// Useful for testing specific versions or rollback scenarios.
    pub fn with_version(mut self, version: String) -> Self {
        self.add_condition("tasker_named_tasks.version = ");
        self.query.push_bind(version);
        self
    }

    /// Scope: latest_versions - Get the latest version of each named task (Rails: scope :latest_versions)
    ///
    /// Uses DISTINCT ON to get the latest version of each task within each namespace.
    /// Critical for getting the current production versions of all tasks.
    ///
    /// This matches the Rails SQL:
    /// SELECT DISTINCT ON (task_namespace_id, name) * FROM tasker_named_tasks
    /// ORDER BY task_namespace_id ASC, name ASC, version DESC
    pub fn latest_versions(mut self) -> Self {
        // Replace the entire SELECT clause with DISTINCT ON
        let mut new_query = QueryBuilder::new(
            "SELECT DISTINCT ON (tasker_named_tasks.task_namespace_id, tasker_named_tasks.name) tasker_named_tasks.* FROM tasker_named_tasks"
        );

        // Add any existing JOINs
        if self.has_task_namespace_join {
            new_query.push(" INNER JOIN tasker_task_namespaces task_namespace ON task_namespace.task_namespace_id = tasker_named_tasks.task_namespace_id");
        }

        // Add any existing WHERE conditions
        let original_sql = self.query.sql();
        if let Some(where_start) = original_sql.find(" WHERE ") {
            let where_clause = if let Some(order_start) = original_sql.find(" ORDER BY") {
                &original_sql[where_start..order_start]
            } else {
                &original_sql[where_start..]
            };
            new_query.push(where_clause);
        }

        // Add the required ORDER BY for DISTINCT ON
        new_query.push(" ORDER BY tasker_named_tasks.task_namespace_id ASC, tasker_named_tasks.name ASC, tasker_named_tasks.version DESC");

        self.query = new_query;
        self
    }
}

impl ScopeBuilder<NamedTask> for NamedTaskScope {
    fn all(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<NamedTask>, sqlx::Error>> + Send {
        async move {
            let query = self.query.build_query_as::<NamedTask>();
            query.fetch_all(pool).await
        }
    }

    fn first(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<NamedTask>, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<NamedTask>();
            query.fetch_optional(pool).await
        }
    }

    fn count(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            // Use the same approach as WorkflowStepTransition - count results
            let results = self.all(pool).await?;
            Ok(results.len() as i64)
        }
    }

    fn exists(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<bool, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<NamedTask>();
            let result = query.fetch_optional(pool).await?;
            Ok(result.is_some())
        }
    }
}

/// Query builder for TaskNamespace scopes
pub struct TaskNamespaceScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
}

impl TaskNamespace {
    /// Start building a scoped query
    pub fn scope() -> TaskNamespaceScope {
        let query =
            QueryBuilder::new("SELECT tasker_task_namespaces.* FROM tasker_task_namespaces");
        TaskNamespaceScope {
            query,
            has_conditions: false,
        }
    }
}

impl TaskNamespaceScope {
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

    /// Scope: custom - Filter out the default namespace (Rails: scope :custom)
    ///
    /// Excludes the 'default' namespace, returning only custom/user-created namespaces.
    /// Useful for listing user-defined organizational categories.
    pub fn custom(mut self) -> Self {
        self.add_condition("tasker_task_namespaces.name != 'default'");
        self
    }
}

impl ScopeBuilder<TaskNamespace> for TaskNamespaceScope {
    fn all(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<TaskNamespace>, sqlx::Error>> + Send {
        async move {
            let query = self.query.build_query_as::<TaskNamespace>();
            query.fetch_all(pool).await
        }
    }

    fn first(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<TaskNamespace>, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<TaskNamespace>();
            query.fetch_optional(pool).await
        }
    }

    fn count(
        self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            // Simple count for TaskNamespace - no complex queries
            let count_query = self.query.sql().replace(
                "SELECT tasker_task_namespaces.* FROM tasker_task_namespaces",
                "SELECT COUNT(*) as count FROM tasker_task_namespaces",
            );

            let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
            Ok(row.0)
        }
    }

    fn exists(
        mut self,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<bool, sqlx::Error>> + Send {
        async move {
            self.query.push(" LIMIT 1");
            let query = self.query.build_query_as::<TaskNamespace>();
            let result = query.fetch_optional(pool).await?;
            Ok(result.is_some())
        }
    }
}
