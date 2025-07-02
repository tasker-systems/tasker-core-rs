use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// WorkflowStep represents individual step instances with result handling
/// Maps to `tasker_workflow_steps` table matching Rails schema exactly
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkflowStep {
    pub workflow_step_id: i64,
    pub task_id: i64,
    pub named_step_id: i32,
    pub retryable: bool,
    pub retry_limit: Option<i32>,
    pub in_process: bool,
    pub processed: bool,
    pub processed_at: Option<NaiveDateTime>,
    pub attempts: Option<i32>,
    pub last_attempted_at: Option<NaiveDateTime>,
    pub backoff_request_seconds: Option<i32>,
    pub inputs: Option<serde_json::Value>,
    pub results: Option<serde_json::Value>,
    pub skippable: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New WorkflowStep for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkflowStep {
    pub task_id: i64,
    pub named_step_id: i32,
    pub retryable: Option<bool>, // Defaults to true
    pub retry_limit: Option<i32>, // Defaults to 3
    pub inputs: Option<serde_json::Value>,
    pub skippable: Option<bool>, // Defaults to false
}

impl WorkflowStep {
    /// Create a new workflow step
    pub async fn create(pool: &PgPool, new_step: NewWorkflowStep) -> Result<WorkflowStep, sqlx::Error> {
        let step = sqlx::query_as!(
            WorkflowStep,
            r#"
            INSERT INTO tasker_workflow_steps (
                task_id, named_step_id, retryable, retry_limit, inputs, skippable
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING workflow_step_id, task_id, named_step_id, retryable, retry_limit, 
                      in_process, processed, processed_at, attempts, last_attempted_at,
                      backoff_request_seconds, inputs, results, skippable, created_at, updated_at
            "#,
            new_step.task_id,
            new_step.named_step_id,
            new_step.retryable.unwrap_or(true),
            new_step.retry_limit.unwrap_or(3),
            new_step.inputs,
            new_step.skippable.unwrap_or(false)
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
            SELECT workflow_step_id, task_id, named_step_id, retryable, retry_limit, 
                   in_process, processed, processed_at, attempts, last_attempted_at,
                   backoff_request_seconds, inputs, results, skippable, created_at, updated_at
            FROM tasker_workflow_steps
            WHERE workflow_step_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// List all workflow steps for a task
    pub async fn list_by_task(pool: &PgPool, task_id: i64) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, retryable, retry_limit, 
                   in_process, processed, processed_at, attempts, last_attempted_at,
                   backoff_request_seconds, inputs, results, skippable, created_at, updated_at
            FROM tasker_workflow_steps
            WHERE task_id = $1
            ORDER BY workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Alias for list_by_task - get workflow steps for a task (Rails scope: for_task)
    pub async fn for_task(pool: &PgPool, task_id: i64) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        Self::list_by_task(pool, task_id).await
    }

    /// List workflow steps by named step ID
    pub async fn list_by_named_step(pool: &PgPool, named_step_id: i32) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, retryable, retry_limit, 
                   in_process, processed, processed_at, attempts, last_attempted_at,
                   backoff_request_seconds, inputs, results, skippable, created_at, updated_at
            FROM tasker_workflow_steps
            WHERE named_step_id = $1
            ORDER BY created_at DESC
            "#,
            named_step_id
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// List unprocessed workflow steps
    pub async fn list_unprocessed(pool: &PgPool) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, retryable, retry_limit, 
                   in_process, processed, processed_at, attempts, last_attempted_at,
                   backoff_request_seconds, inputs, results, skippable, created_at, updated_at
            FROM tasker_workflow_steps
            WHERE processed = false AND in_process = false
            ORDER BY created_at ASC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// List steps currently in process
    pub async fn list_in_process(pool: &PgPool) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, retryable, retry_limit, 
                   in_process, processed, processed_at, attempts, last_attempted_at,
                   backoff_request_seconds, inputs, results, skippable, created_at, updated_at
            FROM tasker_workflow_steps
            WHERE in_process = true
            ORDER BY last_attempted_at ASC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Mark step as in process
    pub async fn mark_in_process(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().naive_utc();
        
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_steps 
            SET in_process = true, 
                last_attempted_at = $2,
                attempts = COALESCE(attempts, 0) + 1,
                updated_at = NOW()
            WHERE workflow_step_id = $1
            "#,
            self.workflow_step_id,
            now
        )
        .execute(pool)
        .await?;

        self.in_process = true;
        self.last_attempted_at = Some(now);
        self.attempts = Some(self.attempts.unwrap_or(0) + 1);
        
        Ok(())
    }

    /// Mark step as processed with results
    pub async fn mark_processed(
        &mut self, 
        pool: &PgPool, 
        results: Option<serde_json::Value>
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().naive_utc();
        
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_steps 
            SET processed = true, 
                in_process = false,
                processed_at = $2,
                results = $3,
                updated_at = NOW()
            WHERE workflow_step_id = $1
            "#,
            self.workflow_step_id,
            now,
            results
        )
        .execute(pool)
        .await?;

        self.processed = true;
        self.in_process = false;
        self.processed_at = Some(now);
        self.results = results;
        
        Ok(())
    }

    /// Set backoff for retry
    pub async fn set_backoff(&mut self, pool: &PgPool, seconds: i32) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_steps 
            SET backoff_request_seconds = $2,
                in_process = false,
                updated_at = NOW()
            WHERE workflow_step_id = $1
            "#,
            self.workflow_step_id,
            seconds
        )
        .execute(pool)
        .await?;

        self.backoff_request_seconds = Some(seconds);
        self.in_process = false;
        
        Ok(())
    }

    /// Reset step for retry
    pub async fn reset_for_retry(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_steps 
            SET in_process = false,
                processed = false,
                processed_at = NULL,
                results = NULL,
                backoff_request_seconds = NULL,
                updated_at = NOW()
            WHERE workflow_step_id = $1
            "#,
            self.workflow_step_id
        )
        .execute(pool)
        .await?;

        self.in_process = false;
        self.processed = false;
        self.processed_at = None;
        self.results = None;
        self.backoff_request_seconds = None;
        
        Ok(())
    }

    /// Update inputs
    pub async fn update_inputs(
        &mut self, 
        pool: &PgPool, 
        inputs: serde_json::Value
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_steps 
            SET inputs = $2, updated_at = NOW()
            WHERE workflow_step_id = $1
            "#,
            self.workflow_step_id,
            inputs
        )
        .execute(pool)
        .await?;

        self.inputs = Some(inputs);
        Ok(())
    }

    /// Delete a workflow step
    pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_workflow_steps
            WHERE workflow_step_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if step has exceeded retry limit
    pub fn has_exceeded_retry_limit(&self) -> bool {
        if let (Some(attempts), Some(limit)) = (self.attempts, self.retry_limit) {
            attempts >= limit
        } else {
            false
        }
    }

    /// Check if step is ready for processing
    pub fn is_ready_for_processing(&self) -> bool {
        !self.processed && !self.in_process && self.backoff_request_seconds.is_none()
    }

    /// Get current state from transitions (since state is managed separately)
    pub async fn get_current_state(&self, pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT to_state
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1 AND most_recent = true
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            self.workflow_step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| r.to_state))
    }

    /// Get dependencies (parent steps) for this step
    pub async fn get_dependencies(&self, pool: &PgPool) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT ws.workflow_step_id, ws.task_id, ws.named_step_id, ws.retryable, ws.retry_limit, 
                   ws.in_process, ws.processed, ws.processed_at, ws.attempts, ws.last_attempted_at,
                   ws.backoff_request_seconds, ws.inputs, ws.results, ws.skippable, ws.created_at, ws.updated_at
            FROM tasker_workflow_steps ws
            INNER JOIN tasker_workflow_step_edges wse ON wse.from_step_id = ws.workflow_step_id
            WHERE wse.to_step_id = $1
            ORDER BY ws.workflow_step_id
            "#,
            self.workflow_step_id
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Get dependents (child steps) for this step
    pub async fn get_dependents(&self, pool: &PgPool) -> Result<Vec<WorkflowStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT ws.workflow_step_id, ws.task_id, ws.named_step_id, ws.retryable, ws.retry_limit, 
                   ws.in_process, ws.processed, ws.processed_at, ws.attempts, ws.last_attempted_at,
                   ws.backoff_request_seconds, ws.inputs, ws.results, ws.skippable, ws.created_at, ws.updated_at
            FROM tasker_workflow_steps ws
            INNER JOIN tasker_workflow_step_edges wse ON wse.to_step_id = ws.workflow_step_id
            WHERE wse.from_step_id = $1
            ORDER BY ws.workflow_step_id
            "#,
            self.workflow_step_id
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use serde_json::json;

    #[tokio::test]
    async fn test_workflow_step_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Test creation
        let new_step = NewWorkflowStep {
            task_id: 1,
            named_step_id: 1,
            retryable: Some(true),
            retry_limit: Some(5),
            inputs: Some(json!({"param1": "value1", "param2": 42})),
            skippable: Some(false),
        };

        let created = WorkflowStep::create(pool, new_step).await.expect("Failed to create step");
        assert_eq!(created.task_id, 1);
        assert_eq!(created.named_step_id, 1);
        assert!(created.retryable);
        assert_eq!(created.retry_limit, Some(5));
        assert!(!created.processed);
        assert!(!created.in_process);

        // Test find by ID
        let found = WorkflowStep::find_by_id(pool, created.workflow_step_id)
            .await
            .expect("Failed to find step")
            .expect("Step not found");
        assert_eq!(found.workflow_step_id, created.workflow_step_id);

        // Test mark in process
        let mut step_to_process = found.clone();
        step_to_process.mark_in_process(pool).await.expect("Failed to mark in process");
        assert!(step_to_process.in_process);
        assert!(step_to_process.last_attempted_at.is_some());
        assert_eq!(step_to_process.attempts, Some(1));

        // Test mark processed with results
        let results = json!({"output": "success", "count": 10});
        step_to_process.mark_processed(pool, Some(results.clone())).await.expect("Failed to mark processed");
        assert!(step_to_process.processed);
        assert!(!step_to_process.in_process);
        assert!(step_to_process.processed_at.is_some());
        assert_eq!(step_to_process.results, Some(results));

        // Test retry logic
        assert!(!step_to_process.has_exceeded_retry_limit());
        assert!(!step_to_process.is_ready_for_processing()); // Already processed

        // Test inputs update
        let new_inputs = json!({"updated_param": "new_value"});
        step_to_process.update_inputs(pool, new_inputs.clone()).await.expect("Failed to update inputs");
        assert_eq!(step_to_process.inputs, Some(new_inputs));

        // Test deletion
        let deleted = WorkflowStep::delete(pool, created.workflow_step_id)
            .await
            .expect("Failed to delete step");
        assert!(deleted);

        db.close().await;
    }

    #[test]
    fn test_retry_limit_logic() {
        let mut step = WorkflowStep {
            workflow_step_id: 1,
            task_id: 1,
            named_step_id: 1,
            retryable: true,
            retry_limit: Some(3),
            in_process: false,
            processed: false,
            processed_at: None,
            attempts: Some(2),
            last_attempted_at: None,
            backoff_request_seconds: None,
            inputs: None,
            results: None,
            skippable: false,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        // Not exceeded yet
        assert!(!step.has_exceeded_retry_limit());
        assert!(step.is_ready_for_processing());

        // Exceed limit
        step.attempts = Some(3);
        assert!(step.has_exceeded_retry_limit());

        // In backoff
        step.attempts = Some(1);
        step.backoff_request_seconds = Some(60);
        assert!(!step.is_ready_for_processing());
    }
}