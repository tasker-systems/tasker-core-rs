use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// TaskTransition represents task state change audit trail
/// Maps to `tasker_task_transitions` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskTransition {
    pub id: i64,
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: serde_json::Value,
    pub sort_key: i32,
    pub most_recent: bool,
    pub task_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// WorkflowStepTransition represents step state change audit trail
/// Maps to `tasker_workflow_step_transitions` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkflowStepTransition {
    pub id: i64,
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: serde_json::Value,
    pub sort_key: i32,
    pub most_recent: bool,
    pub workflow_step_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New TaskTransition for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskTransition {
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: serde_json::Value,
    pub task_id: i64,
}

/// New WorkflowStepTransition for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkflowStepTransition {
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: serde_json::Value,
    pub workflow_step_id: i64,
}

impl TaskTransition {
    /// Create a new task transition
    pub async fn create(pool: &PgPool, new_transition: NewTaskTransition) -> Result<TaskTransition, sqlx::Error> {
        // Get the next sort_key
        let sort_key = Self::get_next_sort_key(pool, new_transition.task_id).await?;

        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            INSERT INTO tasker_task_transitions (to_state, from_state, metadata, sort_key, most_recent, task_id)
            VALUES ($1, $2, $3, $4, true, $5)
            RETURNING id, to_state, from_state, metadata, sort_key, most_recent, task_id, created_at, updated_at
            "#,
            new_transition.to_state,
            new_transition.from_state,
            new_transition.metadata,
            sort_key,
            new_transition.task_id
        )
        .fetch_one(pool)
        .await?;

        // Mark previous transitions as not most recent
        sqlx::query!(
            r#"
            UPDATE tasker_task_transitions
            SET most_recent = false
            WHERE task_id = $1 AND id != $2
            "#,
            new_transition.task_id,
            transition.id
        )
        .execute(pool)
        .await?;

        Ok(transition)
    }

    /// Get transition history for a task
    pub async fn get_history(pool: &PgPool, task_id: i64) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, task_id, created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_id = $1
            ORDER BY sort_key DESC
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get the most recent transition for a task
    pub async fn get_most_recent(pool: &PgPool, task_id: i64) -> Result<Option<TaskTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, task_id, created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_id = $1 AND most_recent = true
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    async fn get_next_sort_key(pool: &PgPool, task_id: i64) -> Result<i32, sqlx::Error> {
        let max_sort_key = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(sort_key), 0) as max_sort_key
            FROM tasker_task_transitions
            WHERE task_id = $1
            "#,
            task_id
        )
        .fetch_one(pool)
        .await?
        .max_sort_key;

        Ok(max_sort_key.unwrap_or(0) + 1)
    }
}

impl WorkflowStepTransition {
    /// Create a new workflow step transition
    pub async fn create(pool: &PgPool, new_transition: NewWorkflowStepTransition) -> Result<WorkflowStepTransition, sqlx::Error> {
        // Get the next sort_key
        let sort_key = Self::get_next_sort_key(pool, new_transition.workflow_step_id).await?;

        let transition = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            INSERT INTO tasker_workflow_step_transitions (to_state, from_state, metadata, sort_key, most_recent, workflow_step_id)
            VALUES ($1, $2, $3, $4, true, $5)
            RETURNING id, to_state, from_state, metadata, sort_key, most_recent, workflow_step_id, created_at, updated_at
            "#,
            new_transition.to_state,
            new_transition.from_state,
            new_transition.metadata,
            sort_key,
            new_transition.workflow_step_id
        )
        .fetch_one(pool)
        .await?;

        // Mark previous transitions as not most recent
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_step_transitions
            SET most_recent = false
            WHERE workflow_step_id = $1 AND id != $2
            "#,
            new_transition.workflow_step_id,
            transition.id
        )
        .execute(pool)
        .await?;

        Ok(transition)
    }

    /// Get transition history for a workflow step
    pub async fn get_history(pool: &PgPool, workflow_step_id: i64) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1
            ORDER BY sort_key DESC
            "#,
            workflow_step_id
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get the most recent transition for a workflow step
    pub async fn get_most_recent(pool: &PgPool, workflow_step_id: i64) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1 AND most_recent = true
            "#,
            workflow_step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    async fn get_next_sort_key(pool: &PgPool, workflow_step_id: i64) -> Result<i32, sqlx::Error> {
        let max_sort_key = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(sort_key), 0) as max_sort_key
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1
            "#,
            workflow_step_id
        )
        .fetch_one(pool)
        .await?
        .max_sort_key;

        Ok(max_sort_key.unwrap_or(0) + 1)
    }
}