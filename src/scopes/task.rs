//! # Task Scopes
//!
//! Query scopes for the Task model with support for complex filtering,
//! JOIN operations, and state-based queries.

use super::common::{state_helpers, ScopeBuilder};
use crate::models::Task;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};

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
    ///
    /// # Current Limitation
    ///
    /// Due to the current QueryBuilder architecture, JOINs cannot be added after WHERE conditions.
    /// This is because SQLx's QueryBuilder builds SQL sequentially and doesn't support reordering.
    ///
    /// **Workaround**: Always call scopes that require JOINs before scopes that add WHERE conditions.
    ///
    /// Example:
    /// ```rust,no_run
    /// use tasker_core::models::Task;
    /// use tasker_core::scopes::ScopeBuilder;
    /// use chrono::{Utc, Duration};
    /// # async fn example(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    /// # let timestamp = Utc::now() - Duration::hours(1);
    /// // ✅ Correct order - JOINs first, then WHERE conditions
    /// Task::scope()
    ///     .with_task_name("process_order".to_string())  // Adds JOIN
    ///     .created_since(timestamp)               // Adds WHERE
    ///     .all(pool).await?;
    ///
    /// // ❌ Incorrect order - WHERE first, then JOIN (will be ignored)
    /// Task::scope()
    ///     .created_since(timestamp)               // Adds WHERE
    ///     .with_task_name("process_order".to_string())  // JOIN will be ignored!
    ///     .all(pool).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// TODO: Refactor to use a proper SQL AST builder that can reorder clauses correctly.
    fn ensure_named_tasks_join(&mut self) {
        if !self.has_named_tasks_join {
            if !self.has_conditions {
                self.query.push(" INNER JOIN tasker_named_tasks ON tasker_named_tasks.named_task_id = tasker_tasks.named_task_id");
                self.has_named_tasks_join = true;
            } else {
                eprintln!("Warning: Cannot add named_tasks JOIN after WHERE conditions. Call .with_task_name() before other scopes.");
            }
        }
    }

    /// Ensure namespace join exists (requires named tasks join)
    ///
    /// Subject to the same JOIN ordering limitation as ensure_named_tasks_join().
    fn ensure_namespaces_join(&mut self) {
        self.ensure_named_tasks_join();
        if !self.has_namespaces_join {
            if !self.has_conditions {
                self.query.push(" INNER JOIN tasker_task_namespaces ON tasker_task_namespaces.task_namespace_id = tasker_named_tasks.task_namespace_id");
                self.has_namespaces_join = true;
            } else {
                eprintln!("Warning: Cannot add namespaces JOIN after WHERE conditions. Call namespace scopes before other scopes.");
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

    /// Ensure workflow step transitions join exists
    fn ensure_workflow_step_transitions_join(&mut self) {
        self.ensure_workflow_steps_join();
        if !self.has_workflow_step_transitions_join {
            self.query.push(" INNER JOIN ( \
                SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state, created_at, most_recent \
                FROM tasker_workflow_step_transitions \
                ORDER BY workflow_step_id, sort_key DESC \
            ) wst ON wst.workflow_step_id = tasker_workflow_steps.workflow_step_id");
            self.has_workflow_step_transitions_join = true;
        }
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

    /// Scope: by_current_state - Tasks filtered by their current state
    pub fn by_current_state(mut self, state: crate::state_machine::TaskState) -> Self {
        self.ensure_current_transitions_join();
        self.add_condition("current_transitions.to_state = ");
        self.query.push_bind(state.to_string());
        self
    }

    /// Scope: active - Tasks that have active (non-complete) workflow steps
    pub fn active(mut self) -> Self {
        self.ensure_workflow_step_transitions_join();
        self.add_condition(
            "wst.most_recent = true \
             AND wst.to_state NOT IN ('complete', 'error', 'skipped', 'resolved_manually')",
        );
        self
    }
}

impl ScopeBuilder<Task> for TaskScope {
    async fn all(mut self, pool: &PgPool) -> Result<Vec<Task>, sqlx::Error> {
        let query = self.query.build_query_as::<Task>();
        query.fetch_all(pool).await
    }

    async fn first(mut self, pool: &PgPool) -> Result<Option<Task>, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<Task>();
        query.fetch_optional(pool).await
    }

    async fn count(self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let count_query = self.query.sql().replace(
            "SELECT tasker_tasks.* FROM tasker_tasks",
            "SELECT COUNT(*) as count FROM tasker_tasks",
        );

        let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
        Ok(row.0)
    }

    async fn exists(mut self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<Task>();
        let result = query.fetch_optional(pool).await?;
        Ok(result.is_some())
    }
}
