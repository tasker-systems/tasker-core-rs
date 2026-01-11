//! # Named Task and Task Namespace Scopes
//!
//! Query scopes for NamedTask and TaskNamespace models.
//! These scopes support template management, versioning, and namespace organization.

use super::common::ScopeBuilder;
use crate::models::{NamedTask, TaskNamespace};
use sqlx::{PgPool, Postgres, QueryBuilder};

/// Query builder for NamedTask scopes
pub struct NamedTaskScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
    has_task_namespace_join: bool,
}

crate::debug_with_query_builder!(NamedTaskScope {
    query: QueryBuilder,
    has_conditions,
    has_task_namespace_join
});

impl NamedTask {
    /// Start building a scoped query
    pub fn scope() -> NamedTaskScope {
        let query = QueryBuilder::new("SELECT tasker.named_tasks.* FROM tasker.named_tasks");
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
            self.query.push(" INNER JOIN tasker.task_namespaces task_namespace ON task_namespace.task_namespace_uuid = tasker.named_tasks.task_namespace_uuid");
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
        self.add_condition("tasker.named_tasks.version = ");
        self.query.push_bind(version);
        self
    }

    /// Scope: latest_versions - Get the latest version of each named task (Rails: scope :latest_versions)
    ///
    /// Uses DISTINCT ON to get the latest version of each task within each namespace.
    /// Critical for getting the current production versions of all tasks.
    ///
    /// This matches the Rails SQL:
    /// SELECT DISTINCT ON (task_namespace_id, name) * FROM tasker.named_tasks
    /// ORDER BY task_namespace_id ASC, name ASC, version DESC
    pub fn latest_versions(mut self) -> Self {
        // Replace the entire SELECT clause with DISTINCT ON
        let mut new_query = QueryBuilder::new(
            "SELECT DISTINCT ON (tasker.named_tasks.task_namespace_uuid, tasker.named_tasks.name) tasker.named_tasks.* FROM tasker.named_tasks"
        );

        // Add any existing JOINs
        if self.has_task_namespace_join {
            new_query.push(" INNER JOIN tasker.task_namespaces task_namespace ON task_namespace.task_namespace_uuid = tasker.named_tasks.task_namespace_uuid");
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
        new_query.push(" ORDER BY tasker.named_tasks.task_namespace_uuid ASC, tasker.named_tasks.name ASC, tasker.named_tasks.version DESC");

        self.query = new_query;
        self
    }
}

impl ScopeBuilder<NamedTask> for NamedTaskScope {
    async fn all(mut self, pool: &PgPool) -> Result<Vec<NamedTask>, sqlx::Error> {
        let query = self.query.build_query_as::<NamedTask>();
        query.fetch_all(pool).await
    }

    async fn first(mut self, pool: &PgPool) -> Result<Option<NamedTask>, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<NamedTask>();
        query.fetch_optional(pool).await
    }

    async fn count(self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        // Use the same approach as WorkflowStepTransition - count results
        let results = self.all(pool).await?;
        Ok(results.len() as i64)
    }

    async fn exists(mut self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<NamedTask>();
        let result = query.fetch_optional(pool).await?;
        Ok(result.is_some())
    }
}

/// Query builder for TaskNamespace scopes
pub struct TaskNamespaceScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
}

crate::debug_with_query_builder!(TaskNamespaceScope {
    query: QueryBuilder,
    has_conditions
});

impl TaskNamespace {
    /// Start building a scoped query
    pub fn scope() -> TaskNamespaceScope {
        let query =
            QueryBuilder::new("SELECT tasker.task_namespaces.* FROM tasker.task_namespaces");
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
        self.add_condition("tasker.task_namespaces.name != 'default'");
        self
    }
}

impl ScopeBuilder<TaskNamespace> for TaskNamespaceScope {
    async fn all(mut self, pool: &PgPool) -> Result<Vec<TaskNamespace>, sqlx::Error> {
        let query = self.query.build_query_as::<TaskNamespace>();
        query.fetch_all(pool).await
    }

    async fn first(mut self, pool: &PgPool) -> Result<Option<TaskNamespace>, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<TaskNamespace>();
        query.fetch_optional(pool).await
    }

    async fn count(self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        // Simple count for TaskNamespace - no complex queries
        let count_query = self.query.sql().replace(
            "SELECT tasker.task_namespaces.* FROM tasker.task_namespaces",
            "SELECT COUNT(*) as count FROM tasker.task_namespaces",
        );

        let row: (i64,) = sqlx::query_as(&count_query).fetch_one(pool).await?;
        Ok(row.0)
    }

    async fn exists(mut self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<TaskNamespace>();
        let result = query.fetch_optional(pool).await?;
        Ok(result.is_some())
    }
}
