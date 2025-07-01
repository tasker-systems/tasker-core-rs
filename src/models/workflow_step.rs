use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// WorkflowStep represents individual step instances with result handling
/// Maps to `tasker_workflow_steps` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkflowStep {
    pub workflow_step_id: i64,
    pub state: String,
    pub context: serde_json::Value,
    pub output: serde_json::Value,
    pub retry_count: i32,
    pub max_retries: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub most_recent_error_message: Option<String>,
    pub most_recent_error_backtrace: Option<String>,
    pub task_id: i64,
    pub named_step_id: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New WorkflowStep for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkflowStep {
    pub context: serde_json::Value,
    pub max_retries: Option<i32>, // Defaults to 3
    pub task_id: i64,
    pub named_step_id: i32,
}

impl WorkflowStep {
    /// Create a new workflow step
    pub async fn create(pool: &PgPool, new_step: NewWorkflowStep) -> Result<WorkflowStep, sqlx::Error> {
        let max_retries = new_step.max_retries.unwrap_or(3);
        
        let step = sqlx::query_as!(
            WorkflowStep,
            r#"
            INSERT INTO tasker_workflow_steps (context, max_retries, task_id, named_step_id)
            VALUES ($1, $2, $3, $4)
            RETURNING workflow_step_id, state, context, output, retry_count, max_retries, next_retry_at, 
                      most_recent_error_message, most_recent_error_backtrace, task_id, named_step_id, created_at, updated_at
            "#,
            new_step.context,
            max_retries,
            new_step.task_id,
            new_step.named_step_id
        )
        .fetch_one(pool)
        .await?;

        Ok(step)
    }

    /// Find a workflow step by ID
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<WorkflowStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT workflow_step_id, state, context, output, retry_count, max_retries, next_retry_at,
                   most_recent_error_message, most_recent_error_backtrace, task_id, named_step_id, created_at, updated_at
            FROM tasker_workflow_steps
            WHERE workflow_step_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Find workflow steps by task
    pub async fn find_by_task(pool: &PgPool, task_id: i64) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT workflow_step_id, state, context, output, retry_count, max_retries, next_retry_at,
                   most_recent_error_message, most_recent_error_backtrace, task_id, named_step_id, created_at, updated_at
            FROM tasker_workflow_steps
            WHERE task_id = $1
            ORDER BY created_at ASC
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Update step state
    pub async fn update_state(
        pool: &PgPool,
        step_id: i64,
        new_state: &str,
    ) -> Result<WorkflowStep, sqlx::Error> {
        let step = sqlx::query_as!(
            WorkflowStep,
            r#"
            UPDATE tasker_workflow_steps
            SET 
                state = $2,
                updated_at = NOW()
            WHERE workflow_step_id = $1
            RETURNING workflow_step_id, state, context, output, retry_count, max_retries, next_retry_at,
                      most_recent_error_message, most_recent_error_backtrace, task_id, named_step_id, created_at, updated_at
            "#,
            step_id,
            new_state
        )
        .fetch_one(pool)
        .await?;

        Ok(step)
    }

    /// Update step output
    pub async fn update_output(
        pool: &PgPool,
        step_id: i64,
        output: serde_json::Value,
    ) -> Result<WorkflowStep, sqlx::Error> {
        let step = sqlx::query_as!(
            WorkflowStep,
            r#"
            UPDATE tasker_workflow_steps
            SET 
                output = $2,
                updated_at = NOW()
            WHERE workflow_step_id = $1
            RETURNING workflow_step_id, state, context, output, retry_count, max_retries, next_retry_at,
                      most_recent_error_message, most_recent_error_backtrace, task_id, named_step_id, created_at, updated_at
            "#,
            step_id,
            output
        )
        .fetch_one(pool)
        .await?;

        Ok(step)
    }

    /// Check if step can be retried
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Check if step is ready for retry
    pub fn is_ready_for_retry(&self) -> bool {
        if let Some(next_retry_at) = self.next_retry_at {
            Utc::now() >= next_retry_at
        } else {
            true
        }
    }

    /// Get step identifier for delegation
    pub fn get_step_identifier(&self) -> String {
        format!("step:{}", self.workflow_step_id)
    }
}