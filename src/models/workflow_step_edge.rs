use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// WorkflowStepEdge represents dependency relationships (DAG) between workflow steps
/// Maps to `tasker_workflow_step_edges` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkflowStepEdge {
    pub from_step_id: i64,
    pub to_step_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New WorkflowStepEdge for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkflowStepEdge {
    pub from_step_id: i64,
    pub to_step_id: i64,
}

impl WorkflowStepEdge {
    /// Create a new workflow step edge
    pub async fn create(pool: &PgPool, new_edge: NewWorkflowStepEdge) -> Result<WorkflowStepEdge, sqlx::Error> {
        let edge = sqlx::query_as!(
            WorkflowStepEdge,
            r#"
            INSERT INTO tasker_workflow_step_edges (from_step_id, to_step_id)
            VALUES ($1, $2)
            RETURNING from_step_id, to_step_id, created_at, updated_at
            "#,
            new_edge.from_step_id,
            new_edge.to_step_id
        )
        .fetch_one(pool)
        .await?;

        Ok(edge)
    }

    /// Find dependencies for a step (steps that must complete before this step)
    pub async fn find_dependencies(pool: &PgPool, step_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let dependencies = sqlx::query!(
            r#"
            SELECT from_step_id
            FROM tasker_workflow_step_edges
            WHERE to_step_id = $1
            "#,
            step_id
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.from_step_id)
        .collect();

        Ok(dependencies)
    }

    /// Find dependents for a step (steps that depend on this step)
    pub async fn find_dependents(pool: &PgPool, step_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let dependents = sqlx::query!(
            r#"
            SELECT to_step_id
            FROM tasker_workflow_step_edges
            WHERE from_step_id = $1
            "#,
            step_id
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.to_step_id)
        .collect();

        Ok(dependents)
    }

    /// Get all edges for a task (for DAG analysis)
    pub async fn find_by_task(pool: &PgPool, task_id: i64) -> Result<Vec<WorkflowStepEdge>, sqlx::Error> {
        let edges = sqlx::query_as!(
            WorkflowStepEdge,
            r#"
            SELECT wse.from_step_id, wse.to_step_id, wse.created_at, wse.updated_at
            FROM tasker_workflow_step_edges wse
            INNER JOIN tasker_workflow_steps ws_from ON wse.from_step_id = ws_from.workflow_step_id
            INNER JOIN tasker_workflow_steps ws_to ON wse.to_step_id = ws_to.workflow_step_id
            WHERE ws_from.task_id = $1 AND ws_to.task_id = $1
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(edges)
    }

    /// Check if adding this edge would create a cycle
    pub async fn would_create_cycle(
        pool: &PgPool,
        from_step_id: i64,
        to_step_id: i64,
    ) -> Result<bool, sqlx::Error> {
        // Check if there's already a path from to_step_id to from_step_id
        let has_path = sqlx::query!(
            r#"
            WITH RECURSIVE step_path AS (
                -- Base case: direct dependencies
                SELECT from_step_id, to_step_id, 1 as depth
                FROM tasker_workflow_step_edges
                WHERE from_step_id = $1

                UNION ALL

                -- Recursive case: follow the path
                SELECT sp.from_step_id, wse.to_step_id, sp.depth + 1
                FROM step_path sp
                JOIN tasker_workflow_step_edges wse ON sp.to_step_id = wse.from_step_id
                WHERE sp.depth < 100  -- Prevent infinite recursion
            )
            SELECT COUNT(*) as count
            FROM step_path
            WHERE to_step_id = $2
            "#,
            to_step_id,
            from_step_id
        )
        .fetch_one(pool)
        .await?
        .count;

        Ok(has_path.unwrap_or(0) > 0)
    }

    /// Delete a workflow step edge
    pub async fn delete(
        pool: &PgPool,
        from_step_id: i64,
        to_step_id: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_workflow_step_edges
            WHERE from_step_id = $1 AND to_step_id = $2
            "#,
            from_step_id,
            to_step_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get root steps for a task (steps with no dependencies)
    pub async fn find_root_steps(pool: &PgPool, task_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let root_steps = sqlx::query!(
            r#"
            SELECT ws.workflow_step_id
            FROM tasker_workflow_steps ws
            WHERE ws.task_id = $1
              AND NOT EXISTS (
                SELECT 1
                FROM tasker_workflow_step_edges wse
                WHERE wse.to_step_id = ws.workflow_step_id
              )
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.workflow_step_id)
        .collect();

        Ok(root_steps)
    }

    /// Get leaf steps for a task (steps with no dependents)
    pub async fn find_leaf_steps(pool: &PgPool, task_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let leaf_steps = sqlx::query!(
            r#"
            SELECT ws.workflow_step_id
            FROM tasker_workflow_steps ws
            WHERE ws.task_id = $1
              AND NOT EXISTS (
                SELECT 1
                FROM tasker_workflow_step_edges wse
                WHERE wse.from_step_id = ws.workflow_step_id
              )
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.workflow_step_id)
        .collect();

        Ok(leaf_steps)
    }
}