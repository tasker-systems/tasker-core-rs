use chrono::{NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool};

/// StepReadinessStatus represents the readiness state of workflow steps for execution
/// Maps to `tasker_step_readiness_statuses` table - step execution readiness tracking (2.1KB Rails model)
/// This model serves as both a cache and authoritative source for step readiness calculations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct StepReadinessStatus {
    pub step_readiness_status_id: i64,
    pub workflow_step_id: i64,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub last_readiness_check_at: Option<NaiveDateTime>,
    pub next_readiness_check_at: Option<NaiveDateTime>,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub backoff_request_seconds: Option<i32>,
    pub last_attempted_at: Option<NaiveDateTime>,
    pub metadata: JsonValue,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New StepReadinessStatus for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStepReadinessStatus {
    pub workflow_step_id: i64,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub last_readiness_check_at: Option<NaiveDateTime>,
    pub next_readiness_check_at: Option<NaiveDateTime>,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub backoff_request_seconds: Option<i32>,
    pub last_attempted_at: Option<NaiveDateTime>,
    pub metadata: Option<JsonValue>,
}

/// Step readiness with workflow step details
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StepReadinessWithStep {
    pub step_readiness_status_id: i64,
    pub workflow_step_id: i64,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub last_readiness_check_at: Option<NaiveDateTime>,
    pub next_readiness_check_at: Option<NaiveDateTime>,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub backoff_request_seconds: Option<i32>,
    pub last_attempted_at: Option<NaiveDateTime>,
    pub metadata: JsonValue,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub task_id: i64,
    pub named_step_id: i32,
    pub step_retryable: bool,
    pub step_processed: bool,
    pub step_in_process: bool,
}

impl StepReadinessStatus {
    /// Create or update step readiness status
    pub async fn upsert(pool: &PgPool, new_status: NewStepReadinessStatus) -> Result<StepReadinessStatus, sqlx::Error> {
        let status = sqlx::query_as!(
            StepReadinessStatus,
            r#"
            INSERT INTO tasker_step_readiness_statuses 
            (workflow_step_id, dependencies_satisfied, retry_eligible, ready_for_execution,
             last_readiness_check_at, next_readiness_check_at, total_parents, completed_parents,
             backoff_request_seconds, last_attempted_at, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (workflow_step_id) DO UPDATE SET
                dependencies_satisfied = EXCLUDED.dependencies_satisfied,
                retry_eligible = EXCLUDED.retry_eligible,
                ready_for_execution = EXCLUDED.ready_for_execution,
                last_readiness_check_at = EXCLUDED.last_readiness_check_at,
                next_readiness_check_at = EXCLUDED.next_readiness_check_at,
                total_parents = EXCLUDED.total_parents,
                completed_parents = EXCLUDED.completed_parents,
                backoff_request_seconds = EXCLUDED.backoff_request_seconds,
                last_attempted_at = EXCLUDED.last_attempted_at,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            RETURNING step_readiness_status_id, workflow_step_id, dependencies_satisfied, 
                      retry_eligible, ready_for_execution, last_readiness_check_at, 
                      next_readiness_check_at, total_parents, completed_parents,
                      backoff_request_seconds, last_attempted_at, metadata, created_at, updated_at
            "#,
            new_status.workflow_step_id,
            new_status.dependencies_satisfied,
            new_status.retry_eligible,
            new_status.ready_for_execution,
            new_status.last_readiness_check_at,
            new_status.next_readiness_check_at,
            new_status.total_parents,
            new_status.completed_parents,
            new_status.backoff_request_seconds,
            new_status.last_attempted_at,
            new_status.metadata.unwrap_or(JsonValue::Object(serde_json::Map::new()))
        )
        .fetch_one(pool)
        .await?;

        Ok(status)
    }

    /// Find step readiness status by workflow step ID
    pub async fn find_by_step_id(pool: &PgPool, workflow_step_id: i64) -> Result<Option<StepReadinessStatus>, sqlx::Error> {
        let status = sqlx::query_as!(
            StepReadinessStatus,
            r#"
            SELECT step_readiness_status_id, workflow_step_id, dependencies_satisfied,
                   retry_eligible, ready_for_execution, last_readiness_check_at,
                   next_readiness_check_at, total_parents, completed_parents,
                   backoff_request_seconds, last_attempted_at, metadata, created_at, updated_at
            FROM tasker_step_readiness_statuses
            WHERE workflow_step_id = $1
            "#,
            workflow_step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(status)
    }

    /// Get all readiness statuses for a task (Rails scope: for_task)
    pub async fn for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessWithStep>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessWithStep,
            r#"
            SELECT 
                srs.step_readiness_status_id,
                srs.workflow_step_id,
                srs.dependencies_satisfied,
                srs.retry_eligible,
                srs.ready_for_execution,
                srs.last_readiness_check_at,
                srs.next_readiness_check_at,
                srs.total_parents,
                srs.completed_parents,
                srs.backoff_request_seconds,
                srs.last_attempted_at,
                srs.metadata,
                srs.created_at,
                srs.updated_at,
                ws.task_id,
                ws.named_step_id,
                ws.retryable as step_retryable,
                ws.processed as step_processed,
                ws.in_process as step_in_process
            FROM tasker_step_readiness_statuses srs
            JOIN tasker_workflow_steps ws ON ws.workflow_step_id = srs.workflow_step_id
            WHERE ws.task_id = $1
            ORDER BY srs.workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get ready steps for a task (Rails scope: ready_for_task)
    pub async fn ready_for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessWithStep>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessWithStep,
            r#"
            SELECT 
                srs.step_readiness_status_id,
                srs.workflow_step_id,
                srs.dependencies_satisfied,
                srs.retry_eligible,
                srs.ready_for_execution,
                srs.last_readiness_check_at,
                srs.next_readiness_check_at,
                srs.total_parents,
                srs.completed_parents,
                srs.backoff_request_seconds,
                srs.last_attempted_at,
                srs.metadata,
                srs.created_at,
                srs.updated_at,
                ws.task_id,
                ws.named_step_id,
                ws.retryable as step_retryable,
                ws.processed as step_processed,
                ws.in_process as step_in_process
            FROM tasker_step_readiness_statuses srs
            JOIN tasker_workflow_steps ws ON ws.workflow_step_id = srs.workflow_step_id
            WHERE ws.task_id = $1 AND srs.ready_for_execution = true
            ORDER BY srs.workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get steps blocked by dependencies (Rails scope: blocked_by_dependencies_for_task)
    pub async fn blocked_by_dependencies_for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessWithStep>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessWithStep,
            r#"
            SELECT 
                srs.step_readiness_status_id,
                srs.workflow_step_id,
                srs.dependencies_satisfied,
                srs.retry_eligible,
                srs.ready_for_execution,
                srs.last_readiness_check_at,
                srs.next_readiness_check_at,
                srs.total_parents,
                srs.completed_parents,
                srs.backoff_request_seconds,
                srs.last_attempted_at,
                srs.metadata,
                srs.created_at,
                srs.updated_at,
                ws.task_id,
                ws.named_step_id,
                ws.retryable as step_retryable,
                ws.processed as step_processed,
                ws.in_process as step_in_process
            FROM tasker_step_readiness_statuses srs
            JOIN tasker_workflow_steps ws ON ws.workflow_step_id = srs.workflow_step_id
            WHERE ws.task_id = $1 AND srs.dependencies_satisfied = false
            ORDER BY srs.workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get steps blocked by retry timing (Rails scope: blocked_by_retry_for_task)
    pub async fn blocked_by_retry_for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessWithStep>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessWithStep,
            r#"
            SELECT 
                srs.step_readiness_status_id,
                srs.workflow_step_id,
                srs.dependencies_satisfied,
                srs.retry_eligible,
                srs.ready_for_execution,
                srs.last_readiness_check_at,
                srs.next_readiness_check_at,
                srs.total_parents,
                srs.completed_parents,
                srs.backoff_request_seconds,
                srs.last_attempted_at,
                srs.metadata,
                srs.created_at,
                srs.updated_at,
                ws.task_id,
                ws.named_step_id,
                ws.retryable as step_retryable,
                ws.processed as step_processed,
                ws.in_process as step_in_process
            FROM tasker_step_readiness_statuses srs
            JOIN tasker_workflow_steps ws ON ws.workflow_step_id = srs.workflow_step_id
            WHERE ws.task_id = $1 
              AND srs.retry_eligible = true 
              AND srs.next_readiness_check_at > NOW()
            ORDER BY srs.next_readiness_check_at
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get pending steps (not ready, no dependencies) (Rails scope: pending_for_task)
    pub async fn pending_for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessWithStep>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessWithStep,
            r#"
            SELECT 
                srs.step_readiness_status_id,
                srs.workflow_step_id,
                srs.dependencies_satisfied,
                srs.retry_eligible,
                srs.ready_for_execution,
                srs.last_readiness_check_at,
                srs.next_readiness_check_at,
                srs.total_parents,
                srs.completed_parents,
                srs.backoff_request_seconds,
                srs.last_attempted_at,
                srs.metadata,
                srs.created_at,
                srs.updated_at,
                ws.task_id,
                ws.named_step_id,
                ws.retryable as step_retryable,
                ws.processed as step_processed,
                ws.in_process as step_in_process
            FROM tasker_step_readiness_statuses srs
            JOIN tasker_workflow_steps ws ON ws.workflow_step_id = srs.workflow_step_id
            WHERE ws.task_id = $1 
              AND ws.processed = false 
              AND ws.in_process = false
              AND srs.ready_for_execution = false
            ORDER BY srs.workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get steps in progress (Rails scope: in_progress_for_task)
    pub async fn in_progress_for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessWithStep>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessWithStep,
            r#"
            SELECT 
                srs.step_readiness_status_id,
                srs.workflow_step_id,
                srs.dependencies_satisfied,
                srs.retry_eligible,
                srs.ready_for_execution,
                srs.last_readiness_check_at,
                srs.next_readiness_check_at,
                srs.total_parents,
                srs.completed_parents,
                srs.backoff_request_seconds,
                srs.last_attempted_at,
                srs.metadata,
                srs.created_at,
                srs.updated_at,
                ws.task_id,
                ws.named_step_id,
                ws.retryable as step_retryable,
                ws.processed as step_processed,
                ws.in_process as step_in_process
            FROM tasker_step_readiness_statuses srs
            JOIN tasker_workflow_steps ws ON ws.workflow_step_id = srs.workflow_step_id
            WHERE ws.task_id = $1 AND ws.in_process = true
            ORDER BY srs.workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get completed steps (Rails scope: complete_for_task)
    pub async fn complete_for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessWithStep>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessWithStep,
            r#"
            SELECT 
                srs.step_readiness_status_id,
                srs.workflow_step_id,
                srs.dependencies_satisfied,
                srs.retry_eligible,
                srs.ready_for_execution,
                srs.last_readiness_check_at,
                srs.next_readiness_check_at,
                srs.total_parents,
                srs.completed_parents,
                srs.backoff_request_seconds,
                srs.last_attempted_at,
                srs.metadata,
                srs.created_at,
                srs.updated_at,
                ws.task_id,
                ws.named_step_id,
                ws.retryable as step_retryable,
                ws.processed as step_processed,
                ws.in_process as step_in_process
            FROM tasker_step_readiness_statuses srs
            JOIN tasker_workflow_steps ws ON ws.workflow_step_id = srs.workflow_step_id
            WHERE ws.task_id = $1 AND ws.processed = true
            ORDER BY srs.workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Check if all steps are complete for a task (Rails method: all_steps_complete_for_task?)
    pub async fn all_steps_complete_for_task(pool: &PgPool, task_id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_steps,
                COUNT(*) FILTER (WHERE ws.processed = true) as completed_steps
            FROM tasker_workflow_steps ws
            WHERE ws.task_id = $1
            "#,
            task_id
        )
        .fetch_one(pool)
        .await?;

        let total = result.total_steps.unwrap_or(0);
        let completed = result.completed_steps.unwrap_or(0);
        
        Ok(total > 0 && total == completed)
    }

    /// Bulk update readiness statuses using SQL function
    pub async fn bulk_update_for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessResult>, sqlx::Error> {
        // This calls the get_step_readiness_status SQL function to calculate and update statuses
        let results = sqlx::query_as!(
            StepReadinessResult,
            r#"
            SELECT 
                workflow_step_id,
                task_id,
                named_step_id,
                name,
                current_state,
                dependencies_satisfied,
                retry_eligible,
                ready_for_execution,
                last_failure_at,
                next_retry_at,
                total_parents,
                completed_parents,
                attempts,
                retry_limit,
                backoff_request_seconds,
                last_attempted_at
            FROM get_step_readiness_status($1)
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        // Update our cache table with the results
        for result in &results {
            let new_status = NewStepReadinessStatus {
                workflow_step_id: result.workflow_step_id.unwrap_or(0),
                dependencies_satisfied: result.dependencies_satisfied.unwrap_or(false),
                retry_eligible: result.retry_eligible.unwrap_or(false),
                ready_for_execution: result.ready_for_execution.unwrap_or(false),
                last_readiness_check_at: Some(chrono::Utc::now().naive_utc()),
                next_readiness_check_at: result.next_retry_at,
                total_parents: result.total_parents.unwrap_or(0),
                completed_parents: result.completed_parents.unwrap_or(0),
                backoff_request_seconds: result.backoff_request_seconds,
                last_attempted_at: result.last_attempted_at,
                metadata: None,
            };
            
            Self::upsert(pool, new_status).await?;
        }

        Ok(results)
    }

    /// Update readiness status for a specific step
    pub async fn update_readiness(
        &mut self,
        pool: &PgPool,
        dependencies_satisfied: Option<bool>,
        retry_eligible: Option<bool>,
        ready_for_execution: Option<bool>,
        next_readiness_check_at: Option<NaiveDateTime>,
        backoff_request_seconds: Option<i32>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_step_readiness_statuses
            SET dependencies_satisfied = COALESCE($2, dependencies_satisfied),
                retry_eligible = COALESCE($3, retry_eligible),
                ready_for_execution = COALESCE($4, ready_for_execution),
                next_readiness_check_at = COALESCE($5, next_readiness_check_at),
                backoff_request_seconds = COALESCE($6, backoff_request_seconds),
                last_readiness_check_at = NOW(),
                updated_at = NOW()
            WHERE step_readiness_status_id = $1
            "#,
            self.step_readiness_status_id,
            dependencies_satisfied,
            retry_eligible,
            ready_for_execution,
            next_readiness_check_at,
            backoff_request_seconds
        )
        .execute(pool)
        .await?;

        // Update local instance
        if let Some(deps) = dependencies_satisfied {
            self.dependencies_satisfied = deps;
        }
        if let Some(retry) = retry_eligible {
            self.retry_eligible = retry;
        }
        if let Some(ready) = ready_for_execution {
            self.ready_for_execution = ready;
        }
        self.next_readiness_check_at = next_readiness_check_at.or(self.next_readiness_check_at);
        self.backoff_request_seconds = backoff_request_seconds.or(self.backoff_request_seconds);
        self.last_readiness_check_at = Some(chrono::Utc::now().naive_utc());

        Ok(())
    }

    /// Delete step readiness status
    pub async fn delete(pool: &PgPool, workflow_step_id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_step_readiness_statuses
            WHERE workflow_step_id = $1
            "#,
            workflow_step_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Clean up old readiness statuses for completed/deleted steps
    pub async fn cleanup_stale_statuses(pool: &PgPool) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_step_readiness_statuses
            WHERE workflow_step_id NOT IN (
                SELECT workflow_step_id FROM tasker_workflow_steps
            )
            "#
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }
}

/// Result from the get_step_readiness_status SQL function
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StepReadinessResult {
    pub workflow_step_id: Option<i64>,
    pub task_id: Option<i64>,
    pub named_step_id: Option<i32>,
    pub name: Option<String>,
    pub current_state: Option<String>,
    pub dependencies_satisfied: Option<bool>,
    pub retry_eligible: Option<bool>,
    pub ready_for_execution: Option<bool>,
    pub last_failure_at: Option<NaiveDateTime>,
    pub next_retry_at: Option<NaiveDateTime>,
    pub total_parents: Option<i32>,
    pub completed_parents: Option<i32>,
    pub attempts: Option<i32>,
    pub retry_limit: Option<i32>,
    pub backoff_request_seconds: Option<i32>,
    pub last_attempted_at: Option<NaiveDateTime>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use crate::models::{Task, NamedTask, TaskNamespace, WorkflowStep, NamedStep, DependentSystem};
    use crate::models::task::NewTask;
    use crate::models::named_task::NewNamedTask;
    use crate::models::task_namespace::NewTaskNamespace;
    use crate::models::workflow_step::NewWorkflowStep;
    use crate::models::named_step::NewNamedStep;

    #[tokio::test]
    async fn test_step_readiness_status_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Create test dependencies
        let namespace = TaskNamespace::create(pool, NewTaskNamespace {
            name: "test_namespace".to_string(),
            description: None,
        }).await.expect("Failed to create namespace");

        let named_task = NamedTask::create(pool, NewNamedTask {
            name: "test_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_id: namespace.task_namespace_id as i64,
            configuration: None,
        }).await.expect("Failed to create named task");

        let task = Task::create(pool, NewTask {
            named_task_id: named_task.named_task_id as i32,
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
            identity_hash: "test_hash".to_string(),
        }).await.expect("Failed to create task");

        let system = DependentSystem::find_or_create_by_name(pool, "default").await.expect("Failed to get system");

        let named_step = NamedStep::create(pool, NewNamedStep {
            dependent_system_id: system.dependent_system_id,
            name: "test_step".to_string(),
            description: Some("Test step".to_string()),
        }).await.expect("Failed to create named step");

        let workflow_step = WorkflowStep::create(pool, NewWorkflowStep {
            task_id: task.task_id,
            named_step_id: named_step.named_step_id,
            retryable: Some(true),
            retry_limit: Some(3),
            inputs: None,
            skippable: Some(false),
        }).await.expect("Failed to create workflow step");

        // Test step readiness status creation
        let new_status = NewStepReadinessStatus {
            workflow_step_id: workflow_step.workflow_step_id,
            dependencies_satisfied: false,
            retry_eligible: false,
            ready_for_execution: false,
            last_readiness_check_at: None,
            next_readiness_check_at: None,
            total_parents: 2,
            completed_parents: 1,
            backoff_request_seconds: None,
            last_attempted_at: None,
            metadata: Some(serde_json::json!({"priority": "high"})),
        };

        let created = StepReadinessStatus::upsert(pool, new_status)
            .await
            .expect("Failed to create step readiness status");
        assert_eq!(created.workflow_step_id, workflow_step.workflow_step_id);
        assert_eq!(created.total_parents, 2);
        assert_eq!(created.completed_parents, 1);

        // Test find by step ID
        let found = StepReadinessStatus::find_by_step_id(pool, workflow_step.workflow_step_id)
            .await
            .expect("Failed to find status")
            .expect("Status not found");
        assert_eq!(found.step_readiness_status_id, created.step_readiness_status_id);

        // Test for_task
        let task_statuses = StepReadinessStatus::for_task(pool, task.task_id)
            .await
            .expect("Failed to get statuses for task");
        assert_eq!(task_statuses.len(), 1);

        // Test blocked_by_dependencies_for_task
        let blocked = StepReadinessStatus::blocked_by_dependencies_for_task(pool, task.task_id)
            .await
            .expect("Failed to get blocked statuses");
        assert_eq!(blocked.len(), 1); // Should be blocked since dependencies_satisfied = false

        // Test update readiness
        let mut status = created.clone();
        status.update_readiness(
            pool,
            Some(true),  // dependencies satisfied
            Some(false), // not retry eligible
            Some(true),  // ready for execution
            None,
            None,
        )
        .await
        .expect("Failed to update readiness");
        assert!(status.dependencies_satisfied);
        assert!(status.ready_for_execution);

        // Test ready_for_task (should now have 1 ready step)
        let ready = StepReadinessStatus::ready_for_task(pool, task.task_id)
            .await
            .expect("Failed to get ready statuses");
        assert_eq!(ready.len(), 1);

        // Test all_steps_complete_for_task
        let all_complete = StepReadinessStatus::all_steps_complete_for_task(pool, task.task_id)
            .await
            .expect("Failed to check if all complete");
        assert!(!all_complete); // Step is not processed yet

        // Test bulk_update_for_task using SQL function
        let bulk_results = StepReadinessStatus::bulk_update_for_task(pool, task.task_id)
            .await
            .expect("Failed to bulk update");
        assert!(!bulk_results.is_empty());

        // Cleanup
        StepReadinessStatus::delete(pool, workflow_step.workflow_step_id)
            .await
            .expect("Failed to delete status");
        WorkflowStep::delete(pool, workflow_step.workflow_step_id)
            .await
            .expect("Failed to delete workflow step");
        NamedStep::delete(pool, named_step.named_step_id)
            .await
            .expect("Failed to delete named step");
        Task::delete(pool, task.task_id)
            .await
            .expect("Failed to delete task");
        NamedTask::delete(pool, named_task.named_task_id)
            .await
            .expect("Failed to delete named task");
        TaskNamespace::delete(pool, namespace.task_namespace_id)
            .await
            .expect("Failed to delete namespace");

        db.close().await;
    }
}