//! # Workflow Step Edge Scopes
//!
//! Query scopes for WorkflowStepEdge model providing DAG navigation and dependency analysis.
//! These scopes enable sophisticated workflow orchestration queries.

use super::common::{ScopeBuilder, SpecialQuery};
use crate::models::WorkflowStepEdge;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

/// Query builder for WorkflowStepEdge scopes
pub struct WorkflowStepEdgeScope {
    query: QueryBuilder<'static, Postgres>,
    has_conditions: bool,
    is_custom_query: bool,
    /// Special handling for complex CTE queries that need direct execution
    special_query: Option<SpecialQuery>,
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
            special_query: None,
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

    /// Execute siblings_of query with proper parameter binding
    async fn execute_siblings_query(
        step_uuid: Uuid,
        pool: &PgPool,
    ) -> Result<Vec<WorkflowStepEdge>, sqlx::Error> {
        sqlx::query_as::<_, WorkflowStepEdge>(
            r"
            WITH step_parents AS (
                SELECT from_step_uuid
                FROM tasker_workflow_step_edges
                WHERE to_step_uuid = $1
            ),
            potential_siblings AS (
                SELECT to_step_uuid
                FROM tasker_workflow_step_edges
                WHERE from_step_uuid IN (SELECT from_step_uuid FROM step_parents)
                AND to_step_uuid != $1
            ),
            siblings AS (
                SELECT to_step_uuid
                FROM tasker_workflow_step_edges
                WHERE to_step_uuid IN (SELECT to_step_uuid FROM potential_siblings)
                GROUP BY to_step_uuid
                HAVING ARRAY_AGG(from_step_uuid ORDER BY from_step_uuid) =
                      (SELECT ARRAY_AGG(from_step_uuid ORDER BY from_step_uuid) FROM step_parents)
            )
            SELECT e.*
            FROM tasker_workflow_step_edges e
            JOIN siblings ON e.to_step_uuid = siblings.to_step_uuid
            ",
        )
        .bind(step_uuid)
        .fetch_all(pool)
        .await
    }

    /// Execute provides_to_children query with proper parameter binding
    async fn execute_provides_to_children_query(
        step_uuid: Uuid,
        pool: &PgPool,
    ) -> Result<Vec<WorkflowStepEdge>, sqlx::Error> {
        sqlx::query_as::<_, WorkflowStepEdge>(
            r"
            SELECT tasker_workflow_step_edges.*
            FROM tasker_workflow_step_edges
            WHERE tasker_workflow_step_edges.name = 'provides'
            AND tasker_workflow_step_edges.to_step_uuid IN (
                SELECT to_step_uuid
                FROM tasker_workflow_step_edges
                WHERE from_step_uuid = $1
            )
            ",
        )
        .bind(step_uuid)
        .fetch_all(pool)
        .await
    }

    /// Scope: children_of - Get edges where step is the parent (from_step)
    ///
    /// This finds all workflow steps that are direct children of the given step.
    /// Essential for DAG forward navigation.
    pub fn children_of(mut self, step_uuid: Uuid) -> Self {
        self.add_condition("tasker_workflow_step_edges.from_step_uuid = ");
        self.query.push_bind(step_uuid);
        self
    }

    /// Scope: parents_of - Get edges where step is the child (to_step)
    ///
    /// This finds all workflow steps that are direct parents of the given step.
    /// Essential for DAG backward navigation and dependency checking.
    pub fn parents_of(mut self, step_uuid: Uuid) -> Self {
        self.add_condition("tasker_workflow_step_edges.to_step_uuid = ");
        self.query.push_bind(step_uuid);
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
    pub fn provides_to_children(mut self, step_uuid: Uuid) -> Self {
        self.special_query = Some(SpecialQuery::ProvidesToChildren(step_uuid));
        self
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
    pub fn siblings_of(mut self, step_uuid: Uuid) -> Self {
        self.special_query = Some(SpecialQuery::SiblingsOf(step_uuid));
        self
    }
}

impl ScopeBuilder<WorkflowStepEdge> for WorkflowStepEdgeScope {
    async fn all(mut self, pool: &PgPool) -> Result<Vec<WorkflowStepEdge>, sqlx::Error> {
        // Handle special queries that need direct execution
        if let Some(special) = self.special_query {
            match special {
                SpecialQuery::SiblingsOf(step_uuid) => {
                    return Self::execute_siblings_query(step_uuid, pool).await;
                }
                SpecialQuery::ProvidesToChildren(step_uuid) => {
                    return Self::execute_provides_to_children_query(step_uuid, pool).await;
                }
            }
        }

        let query = self.query.build_query_as::<WorkflowStepEdge>();
        query.fetch_all(pool).await
    }

    async fn first(mut self, pool: &PgPool) -> Result<Option<WorkflowStepEdge>, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<WorkflowStepEdge>();
        query.fetch_optional(pool).await
    }

    async fn count(self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        // Handle special queries that need direct execution
        if let Some(special) = self.special_query {
            let results = match special {
                SpecialQuery::SiblingsOf(step_uuid) => {
                    Self::execute_siblings_query(step_uuid, pool).await?
                }
                SpecialQuery::ProvidesToChildren(step_uuid) => {
                    Self::execute_provides_to_children_query(step_uuid, pool).await?
                }
            };
            return Ok(results.len() as i64);
        }

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

    async fn exists(mut self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        self.query.push(" LIMIT 1");
        let query = self.query.build_query_as::<WorkflowStepEdge>();
        let result = query.fetch_optional(pool).await?;
        Ok(result.is_some())
    }
}
