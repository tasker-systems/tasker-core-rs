use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Task represents actual task instances with delegation metadata
/// Maps to `tasker_tasks` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct Task {
    pub task_id: i64,
    pub state: String,
    pub context: serde_json::Value,
    pub most_recent_error_message: Option<String>,
    pub most_recent_error_backtrace: Option<String>,
    pub named_task_id: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New Task for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTask {
    pub context: serde_json::Value,
    pub named_task_id: i32,
}

/// Task with delegation metadata for orchestration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskForOrchestration {
    pub task: Task,
    pub task_name: String,
    pub task_version: i32,
    pub namespace_name: String,
}

impl Task {
    /// Create a new task
    pub async fn create(pool: &PgPool, new_task: NewTask) -> Result<Task, sqlx::Error> {
        let task = sqlx::query_as!(
            Task,
            r#"
            INSERT INTO tasker_tasks (context, named_task_id)
            VALUES ($1, $2)
            RETURNING task_id, state, context, most_recent_error_message, most_recent_error_backtrace, named_task_id, created_at, updated_at
            "#,
            new_task.context,
            new_task.named_task_id
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Find a task by ID
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Task>, sqlx::Error> {
        let task = sqlx::query_as!(
            Task,
            r#"
            SELECT task_id, state, context, most_recent_error_message, most_recent_error_backtrace, named_task_id, created_at, updated_at
            FROM tasker_tasks
            WHERE task_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Find task with orchestration metadata for delegation
    pub async fn find_for_orchestration(pool: &PgPool, task_id: i64) -> Result<Option<TaskForOrchestration>, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 
                t.task_id, t.state, t.context, t.most_recent_error_message, t.most_recent_error_backtrace, t.named_task_id, t.created_at, t.updated_at,
                nt.name as task_name, nt.version as task_version,
                tn.name as namespace_name
            FROM tasker_tasks t
            INNER JOIN tasker_named_tasks nt ON t.named_task_id = nt.named_task_id
            INNER JOIN tasker_task_namespaces tn ON nt.task_namespace_id = tn.task_namespace_id
            WHERE t.task_id = $1
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        if let Some(row) = result {
            let task = Task {
                task_id: row.task_id,
                state: row.state,
                context: row.context,
                most_recent_error_message: row.most_recent_error_message,
                most_recent_error_backtrace: row.most_recent_error_backtrace,
                named_task_id: row.named_task_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
            };

            Ok(Some(TaskForOrchestration {
                task,
                task_name: row.task_name,
                task_version: row.task_version,
                namespace_name: row.namespace_name,
            }))
        } else {
            Ok(None)
        }
    }

    /// List tasks by state
    pub async fn list_by_state(pool: &PgPool, state: &str) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT task_id, state, context, most_recent_error_message, most_recent_error_backtrace, named_task_id, created_at, updated_at
            FROM tasker_tasks
            WHERE state = $1
            ORDER BY created_at ASC
            "#,
            state
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// List tasks by named task
    pub async fn list_by_named_task(pool: &PgPool, named_task_id: i32) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT task_id, state, context, most_recent_error_message, most_recent_error_backtrace, named_task_id, created_at, updated_at
            FROM tasker_tasks
            WHERE named_task_id = $1
            ORDER BY created_at DESC
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Update task state
    pub async fn update_state(
        pool: &PgPool,
        task_id: i64,
        new_state: &str,
    ) -> Result<Task, sqlx::Error> {
        let task = sqlx::query_as!(
            Task,
            r#"
            UPDATE tasker_tasks
            SET 
                state = $2,
                updated_at = NOW()
            WHERE task_id = $1
            RETURNING task_id, state, context, most_recent_error_message, most_recent_error_backtrace, named_task_id, created_at, updated_at
            "#,
            task_id,
            new_state
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Update task context
    pub async fn update_context(
        pool: &PgPool,
        task_id: i64,
        context: serde_json::Value,
    ) -> Result<Task, sqlx::Error> {
        let task = sqlx::query_as!(
            Task,
            r#"
            UPDATE tasker_tasks
            SET 
                context = $2,
                updated_at = NOW()
            WHERE task_id = $1
            RETURNING task_id, state, context, most_recent_error_message, most_recent_error_backtrace, named_task_id, created_at, updated_at
            "#,
            task_id,
            context
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Update task error information
    pub async fn update_error(
        pool: &PgPool,
        task_id: i64,
        error_message: Option<String>,
        error_backtrace: Option<String>,
    ) -> Result<Task, sqlx::Error> {
        let task = sqlx::query_as!(
            Task,
            r#"
            UPDATE tasker_tasks
            SET 
                most_recent_error_message = $2,
                most_recent_error_backtrace = $3,
                updated_at = NOW()
            WHERE task_id = $1
            RETURNING task_id, state, context, most_recent_error_message, most_recent_error_backtrace, named_task_id, created_at, updated_at
            "#,
            task_id,
            error_message,
            error_backtrace
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Delete a task
    pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_tasks
            WHERE task_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get task identifier for delegation
    pub fn get_task_identifier(&self) -> String {
        format!("task:{}", self.task_id)
    }

    /// Check if task is in a final state
    pub fn is_final_state(&self) -> bool {
        matches!(self.state.as_str(), "complete" | "error" | "cancelled")
    }

    /// Check if task is in an active state
    pub fn is_active_state(&self) -> bool {
        matches!(self.state.as_str(), "created" | "running" | "pending")
    }

    /// Get task context value by key
    pub fn get_context_value(&self, key: &str) -> Option<&serde_json::Value> {
        self.context.get(key)
    }

    /// Check if task has error information
    pub fn has_error(&self) -> bool {
        self.most_recent_error_message.is_some()
    }
}