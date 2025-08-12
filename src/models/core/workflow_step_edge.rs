//! # Workflow Step Edge
//!
//! DAG (Directed Acyclic Graph) edge management for workflow orchestration.
//!
//! ## Overview
//!
//! The `WorkflowStepEdge` model manages dependency relationships between workflow steps,
//! ensuring the workflow forms a valid DAG without cycles. This is critical for:
//! - Determining step execution order
//! - Parallelization opportunities
//! - Dependency validation
//!
//! ## Cycle Detection
//!
//! The module includes sophisticated cycle detection using recursive CTEs:
//!
//! ```sql
//! WITH RECURSIVE cycle_check AS (
//!     -- Base: Start from destination step
//!     SELECT to_step_uuid FROM workflow_step_edges WHERE from_step_uuid = $1
//!     UNION
//!     -- Recursive: Follow all paths
//!     SELECT e.to_step_uuid
//!     FROM workflow_step_edges e
//!     JOIN cycle_check c ON e.from_step_uuid = c.to_step_uuid
//! )
//! -- If we can reach the source from destination, it's a cycle
//! SELECT EXISTS (SELECT 1 FROM cycle_check WHERE to_step_uuid = $2)
//! ```
//!
//! ## Rails Heritage
//!
//! Migrated from `app/models/tasker/workflow_step_edge.rb` (3.3KB, 95 lines)

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Represents a directed edge in the workflow DAG connecting two steps.
/// Uses UUID v7 for primary key and foreign keys to ensure time-ordered UUIDs.
///
/// Each edge indicates that `to_step_uuid` depends on `from_step_uuid` completing
/// before it can execute. The collection of edges must form a DAG (no cycles).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkflowStepEdge {
    pub workflow_step_edge_uuid: Uuid,
    pub from_step_uuid: Uuid,
    pub to_step_uuid: Uuid,
    pub name: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New WorkflowStepEdge for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkflowStepEdge {
    pub from_step_uuid: Uuid,
    pub to_step_uuid: Uuid,
    pub name: String,
}

impl WorkflowStepEdge {
    /// Create a new workflow step edge
    pub async fn create(
        pool: &PgPool,
        new_edge: NewWorkflowStepEdge,
    ) -> Result<WorkflowStepEdge, sqlx::Error> {
        let edge = sqlx::query_as!(
            WorkflowStepEdge,
            r#"
            INSERT INTO tasker_workflow_step_edges (from_step_uuid, to_step_uuid, name, created_at, updated_at)
            VALUES ($1::uuid, $2::uuid, $3, NOW(), NOW())
            RETURNING workflow_step_edge_uuid, from_step_uuid, to_step_uuid, name, created_at, updated_at
            "#,
            new_edge.from_step_uuid,
            new_edge.to_step_uuid,
            new_edge.name
        )
        .fetch_one(pool)
        .await?;

        Ok(edge)
    }

    /// Create a new workflow step edge within a transaction
    pub async fn create_with_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        new_edge: NewWorkflowStepEdge,
    ) -> Result<WorkflowStepEdge, sqlx::Error> {
        let edge = sqlx::query_as!(
            WorkflowStepEdge,
            r#"
            INSERT INTO tasker_workflow_step_edges (from_step_uuid, to_step_uuid, name, created_at, updated_at)
            VALUES ($1::uuid, $2::uuid, $3, NOW(), NOW())
            RETURNING workflow_step_edge_uuid, from_step_uuid, to_step_uuid, name, created_at, updated_at
            "#,
            new_edge.from_step_uuid,
            new_edge.to_step_uuid,
            new_edge.name
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(edge)
    }

    /// Find dependencies for a step (steps that must complete before this step)
    pub async fn find_dependencies(
        pool: &PgPool,
        step_uuid: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let dependencies = sqlx::query!(
            r#"
            SELECT from_step_uuid
            FROM tasker_workflow_step_edges
            WHERE to_step_uuid = $1::uuid
            "#,
            step_uuid
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.from_step_uuid)
        .collect();

        Ok(dependencies)
    }

    /// Find dependents for a step (steps that depend on this step)
    pub async fn find_dependents(pool: &PgPool, step_uuid: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        let dependents = sqlx::query!(
            r#"
            SELECT to_step_uuid
            FROM tasker_workflow_step_edges
            WHERE from_step_uuid = $1::uuid
            "#,
            step_uuid
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.to_step_uuid)
        .collect();

        Ok(dependents)
    }

    /// Get all edges for a task (for DAG analysis)
    pub async fn find_by_task(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<WorkflowStepEdge>, sqlx::Error> {
        let edges = sqlx::query_as!(
            WorkflowStepEdge,
            r#"
            SELECT wse.workflow_step_edge_uuid, wse.from_step_uuid, wse.to_step_uuid, wse.name, wse.created_at, wse.updated_at
            FROM tasker_workflow_step_edges wse
            INNER JOIN tasker_workflow_steps ws_from ON wse.from_step_uuid = ws_from.workflow_step_uuid
            INNER JOIN tasker_workflow_steps ws_to ON wse.to_step_uuid = ws_to.workflow_step_uuid
            WHERE ws_from.task_uuid = $1::uuid AND ws_to.task_uuid = $1::uuid
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(edges)
    }

    /// Detects if adding an edge would create a cycle in the workflow DAG.
    ///
    /// This is a critical validation that must be performed before adding any edge
    /// to ensure the workflow remains a valid Directed Acyclic Graph.
    ///
    /// # Algorithm
    ///
    /// Uses a recursive CTE to check if there's already a path from the proposed
    /// destination back to the source. If such a path exists, adding the new edge
    /// would complete a cycle.
    ///
    /// # SQL Implementation
    ///
    /// ```sql
    /// WITH RECURSIVE step_path AS (
    ///     -- Base case: Start from proposed destination (to_step_uuid)
    ///     SELECT from_step_uuid, to_step_uuid, 1 as depth
    ///     FROM workflow_step_edges
    ///     WHERE from_step_uuid = $1  -- Start from destination
    ///
    ///     UNION ALL
    ///
    ///     -- Recursive case: Follow all outgoing edges
    ///     SELECT sp.from_step_uuid, wse.to_step_uuid, sp.depth + 1
    ///     FROM step_path sp
    ///     JOIN workflow_step_edges wse ON sp.to_step_uuid = wse.from_step_uuid
    ///     WHERE sp.depth < 100  -- Prevent infinite recursion
    /// )
    /// -- Check if we can reach the source (from_step_uuid) from destination
    /// SELECT COUNT(*) FROM step_path WHERE to_step_uuid = $2
    /// ```
    ///
    /// # Depth Limit
    ///
    /// The query includes a depth limit of 100 to prevent infinite recursion in case
    /// of data corruption (existing cycles). In practice, workflow depths rarely exceed 10-20.
    ///
    /// # Performance
    ///
    /// - **Complexity**: O(V + E) where V = vertices (steps), E = edges
    /// - **Typical Performance**: <5ms for workflows with 100 steps
    /// - **Worst Case**: Linear scan of all reachable steps from destination
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use sqlx::PgPool;
    /// use tasker_core::models::WorkflowStepEdge;
    /// use uuid::Uuid;
    ///
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// # let step_a_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// # let step_b_uuid = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap();
    /// // Before adding edge A -> B, check if B can already reach A
    /// if WorkflowStepEdge::would_create_cycle(&pool, step_a_uuid, step_b_uuid).await? {
    ///     return Err(sqlx::Error::RowNotFound); // Would create circular reference
    /// }
    ///
    /// // Safe to add the dependency - no cycle will be created
    /// println!("Edge {} -> {} is safe to add", step_a_uuid, step_b_uuid);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For complete examples with test data setup, see `tests/models/workflow_step_edge.rs`.
    pub async fn would_create_cycle(
        pool: &PgPool,
        from_step_uuid: Uuid,
        to_step_uuid: Uuid,
    ) -> Result<bool, sqlx::Error> {
        // Check if there's already a path from to_step_uuid to from_step_uuid
        let has_path = sqlx::query!(
            r#"
            WITH RECURSIVE step_path AS (
                -- Base case: direct dependencies
                SELECT from_step_uuid, to_step_uuid, 1 as depth
                FROM tasker_workflow_step_edges
                WHERE from_step_uuid = $1::uuid

                UNION ALL

                -- Recursive case: follow the path
                SELECT sp.from_step_uuid, wse.to_step_uuid, sp.depth + 1
                FROM step_path sp
                JOIN tasker_workflow_step_edges wse ON sp.to_step_uuid = wse.from_step_uuid
                WHERE sp.depth < 100  -- Prevent infinite recursion
            )
            SELECT COUNT(*) as count
            FROM step_path
            WHERE to_step_uuid = $2::uuid
            "#,
            to_step_uuid,
            from_step_uuid
        )
        .fetch_one(pool)
        .await?
        .count;

        Ok(has_path.unwrap_or(0) > 0)
    }

    /// Find an existing edge by step IDs and name
    pub async fn find_by_steps_and_name(
        pool: &PgPool,
        from_step_uuid: Uuid,
        to_step_uuid: Uuid,
        name: &str,
    ) -> Result<Option<WorkflowStepEdge>, sqlx::Error> {
        let edge = sqlx::query_as!(
            WorkflowStepEdge,
            r#"
            SELECT workflow_step_edge_uuid, from_step_uuid, to_step_uuid, name, created_at, updated_at
            FROM tasker_workflow_step_edges
            WHERE from_step_uuid = $1::uuid AND to_step_uuid = $2::uuid AND name = $3
            "#,
            from_step_uuid,
            to_step_uuid,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(edge)
    }

    /// Delete a workflow step edge
    pub async fn delete(
        pool: &PgPool,
        from_step_uuid: Uuid,
        to_step_uuid: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_workflow_step_edges
            WHERE from_step_uuid = $1::uuid AND to_step_uuid = $2::uuid
            "#,
            from_step_uuid,
            to_step_uuid
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get root steps for a task (steps with no dependencies)
    pub async fn find_root_steps(pool: &PgPool, task_uuid: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        let root_steps = sqlx::query!(
            r#"
            SELECT ws.workflow_step_uuid
            FROM tasker_workflow_steps ws
            WHERE ws.task_uuid = $1::uuid
              AND NOT EXISTS (
                SELECT 1
                FROM tasker_workflow_step_edges wse
                WHERE wse.to_step_uuid = ws.workflow_step_uuid
              )
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.workflow_step_uuid)
        .collect();

        Ok(root_steps)
    }

    /// Get leaf steps for a task (steps with no dependents)
    pub async fn find_leaf_steps(pool: &PgPool, task_uuid: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        let leaf_steps = sqlx::query!(
            r#"
            SELECT ws.workflow_step_uuid
            FROM tasker_workflow_steps ws
            WHERE ws.task_uuid = $1::uuid
              AND NOT EXISTS (
                SELECT 1
                FROM tasker_workflow_step_edges wse
                WHERE wse.from_step_uuid = ws.workflow_step_uuid
              )
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.workflow_step_uuid)
        .collect();

        Ok(leaf_steps)
    }
}
