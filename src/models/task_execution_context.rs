use chrono::{NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool};
use sqlx::types::BigDecimal;

/// TaskExecutionContext represents the execution state and context of a task
/// Maps to `tasker_task_execution_contexts` table - task execution metadata (967B Rails model)
/// This model delegates to SQL functions for high-performance queries
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskExecutionContext {
    pub task_execution_context_id: i64,
    pub task_id: i64,
    pub execution_status: String,
    pub recommended_action: Option<String>,
    pub completion_percentage: BigDecimal,
    pub health_status: String,
    pub metadata: JsonValue,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New TaskExecutionContext for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskExecutionContext {
    pub task_id: i64,
    pub execution_status: Option<String>,
    pub recommended_action: Option<String>,
    pub completion_percentage: Option<BigDecimal>,
    pub health_status: Option<String>,
    pub metadata: Option<JsonValue>,
}

/// Task execution context with task details
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskExecutionContextWithTask {
    pub task_execution_context_id: i64,
    pub task_id: i64,
    pub execution_status: String,
    pub recommended_action: Option<String>,
    pub completion_percentage: BigDecimal,
    pub health_status: String,
    pub metadata: JsonValue,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub task_name: Option<String>,
    pub task_complete: bool,
    pub task_status: Option<String>,
}

/// Workflow summary from SQL function
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkflowSummary {
    pub task_id: Option<i64>,
    pub total_steps: Option<i64>,
    pub completed_steps: Option<i64>,
    pub pending_steps: Option<i64>,
    pub in_progress_steps: Option<i64>,
    pub failed_steps: Option<i64>,
    pub completion_percentage: Option<BigDecimal>,
    pub execution_status: Option<String>,
    pub health_status: Option<String>,
    pub recommended_action: Option<String>,
}

impl TaskExecutionContext {
    /// Create or update task execution context (Rails delegation to SQL function)
    pub async fn upsert(pool: &PgPool, new_context: NewTaskExecutionContext) -> Result<TaskExecutionContext, sqlx::Error> {
        let context = sqlx::query_as!(
            TaskExecutionContext,
            r#"
            INSERT INTO tasker_task_execution_contexts 
            (task_id, execution_status, recommended_action, completion_percentage, health_status, metadata)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (task_id) DO UPDATE SET
                execution_status = EXCLUDED.execution_status,
                recommended_action = EXCLUDED.recommended_action,
                completion_percentage = EXCLUDED.completion_percentage,
                health_status = EXCLUDED.health_status,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            RETURNING task_execution_context_id, task_id, execution_status, recommended_action, 
                      completion_percentage, health_status, metadata, created_at, updated_at
            "#,
            new_context.task_id,
            new_context.execution_status.unwrap_or_else(|| "pending".to_string()),
            new_context.recommended_action,
            new_context.completion_percentage.unwrap_or_else(|| BigDecimal::from(0)),
            new_context.health_status.unwrap_or_else(|| "unknown".to_string()),
            new_context.metadata.unwrap_or(JsonValue::Object(serde_json::Map::new()))
        )
        .fetch_one(pool)
        .await?;

        Ok(context)
    }

    /// Find task execution context by task ID (Rails delegation)
    pub async fn find_by_task_id(pool: &PgPool, task_id: i64) -> Result<Option<TaskExecutionContext>, sqlx::Error> {
        let context = sqlx::query_as!(
            TaskExecutionContext,
            r#"
            SELECT task_execution_context_id, task_id, execution_status, recommended_action,
                   completion_percentage, health_status, metadata, created_at, updated_at
            FROM tasker_task_execution_contexts
            WHERE task_id = $1
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(context)
    }

    /// Get task execution contexts for multiple tasks (Rails method: for_tasks)
    pub async fn for_tasks(pool: &PgPool, task_ids: &[i64]) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let contexts = sqlx::query_as!(
            TaskExecutionContext,
            r#"
            SELECT task_execution_context_id, task_id, execution_status, recommended_action,
                   completion_percentage, health_status, metadata, created_at, updated_at
            FROM tasker_task_execution_contexts
            WHERE task_id = ANY($1)
            ORDER BY task_id
            "#,
            task_ids
        )
        .fetch_all(pool)
        .await?;

        Ok(contexts)
    }

    /// Get contexts with task information (Rails includes: task)
    pub async fn with_tasks(pool: &PgPool) -> Result<Vec<TaskExecutionContextWithTask>, sqlx::Error> {
        let contexts = sqlx::query_as!(
            TaskExecutionContextWithTask,
            r#"
            SELECT 
                tec.task_execution_context_id,
                tec.task_id,
                tec.execution_status,
                tec.recommended_action,
                tec.completion_percentage,
                tec.health_status,
                tec.metadata,
                tec.created_at,
                tec.updated_at,
                nt.name as task_name,
                t.complete as task_complete,
                CASE 
                    WHEN t.complete THEN 'complete'
                    WHEN EXISTS(
                        SELECT 1 FROM tasker_workflow_steps ws 
                        WHERE ws.task_id = t.task_id AND ws.in_process = true
                    ) THEN 'in_progress'
                    ELSE 'pending'
                END as task_status
            FROM tasker_task_execution_contexts tec
            JOIN tasker_tasks t ON t.task_id = tec.task_id
            LEFT JOIN tasker_named_tasks nt ON nt.named_task_id = t.named_task_id
            ORDER BY tec.created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(contexts)
    }

    /// Get workflow summary using SQL function (Rails method: workflow_summary)
    pub async fn get_workflow_summary(pool: &PgPool, task_id: i64) -> Result<Option<WorkflowSummary>, sqlx::Error> {
        // This calls the get_task_execution_context SQL function
        let summary = sqlx::query_as!(
            WorkflowSummary,
            r#"
            SELECT 
                task_id,
                total_steps,
                completed_steps,
                pending_steps,
                in_progress_steps,
                failed_steps,
                completion_percentage,
                execution_status,
                health_status,
                recommended_action
            FROM get_task_execution_context($1)
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(summary)
    }

    /// Get contexts by execution status (Rails scope: by_status)
    pub async fn by_status(pool: &PgPool, status: &str) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let contexts = sqlx::query_as!(
            TaskExecutionContext,
            r#"
            SELECT task_execution_context_id, task_id, execution_status, recommended_action,
                   completion_percentage, health_status, metadata, created_at, updated_at
            FROM tasker_task_execution_contexts
            WHERE execution_status = $1
            ORDER BY created_at DESC
            "#,
            status
        )
        .fetch_all(pool)
        .await?;

        Ok(contexts)
    }

    /// Get contexts by health status (Rails scope: by_health_status)
    pub async fn by_health_status(pool: &PgPool, health_status: &str) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let contexts = sqlx::query_as!(
            TaskExecutionContext,
            r#"
            SELECT task_execution_context_id, task_id, execution_status, recommended_action,
                   completion_percentage, health_status, metadata, created_at, updated_at
            FROM tasker_task_execution_contexts
            WHERE health_status = $1
            ORDER BY created_at DESC
            "#,
            health_status
        )
        .fetch_all(pool)
        .await?;

        Ok(contexts)
    }

    /// Get contexts with completion percentage above threshold (Rails scope: above_completion)
    pub async fn above_completion(pool: &PgPool, threshold: BigDecimal) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let contexts = sqlx::query_as!(
            TaskExecutionContext,
            r#"
            SELECT task_execution_context_id, task_id, execution_status, recommended_action,
                   completion_percentage, health_status, metadata, created_at, updated_at
            FROM tasker_task_execution_contexts
            WHERE completion_percentage >= $1
            ORDER BY completion_percentage DESC
            "#,
            threshold
        )
        .fetch_all(pool)
        .await?;

        Ok(contexts)
    }

    /// Get contexts with recommended actions (Rails scope: with_recommendations)
    pub async fn with_recommendations(pool: &PgPool) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let contexts = sqlx::query_as!(
            TaskExecutionContext,
            r#"
            SELECT task_execution_context_id, task_id, execution_status, recommended_action,
                   completion_percentage, health_status, metadata, created_at, updated_at
            FROM tasker_task_execution_contexts
            WHERE recommended_action IS NOT NULL AND recommended_action != ''
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(contexts)
    }

    /// Search contexts by metadata (Rails scope: search_metadata)
    pub async fn search_metadata(pool: &PgPool, key: &str, value: &str) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let contexts = sqlx::query_as!(
            TaskExecutionContext,
            r#"
            SELECT task_execution_context_id, task_id, execution_status, recommended_action,
                   completion_percentage, health_status, metadata, created_at, updated_at
            FROM tasker_task_execution_contexts
            WHERE metadata ->> $1 ILIKE $2
            ORDER BY created_at DESC
            "#,
            key,
            format!("%{}%", value)
        )
        .fetch_all(pool)
        .await?;

        Ok(contexts)
    }

    /// Update task execution context
    pub async fn update(
        &mut self,
        pool: &PgPool,
        execution_status: Option<&str>,
        recommended_action: Option<&str>,
        completion_percentage: Option<BigDecimal>,
        health_status: Option<&str>,
        metadata: Option<JsonValue>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_task_execution_contexts
            SET execution_status = COALESCE($2, execution_status),
                recommended_action = COALESCE($3, recommended_action),
                completion_percentage = COALESCE($4, completion_percentage),
                health_status = COALESCE($5, health_status),
                metadata = COALESCE($6, metadata),
                updated_at = NOW()
            WHERE task_execution_context_id = $1
            "#,
            self.task_execution_context_id,
            execution_status,
            recommended_action,
            completion_percentage,
            health_status,
            metadata
        )
        .execute(pool)
        .await?;

        // Update local instance
        if let Some(status) = execution_status {
            self.execution_status = status.to_string();
        }
        if let Some(action) = recommended_action {
            self.recommended_action = Some(action.to_string());
        }
        if let Some(percentage) = completion_percentage {
            self.completion_percentage = percentage;
        }
        if let Some(health) = health_status {
            self.health_status = health.to_string();
        }
        if let Some(meta) = metadata {
            self.metadata = meta;
        }

        Ok(())
    }

    /// Delete task execution context
    pub async fn delete(pool: &PgPool, task_id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_execution_contexts
            WHERE task_id = $1
            "#,
            task_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Clean up contexts for deleted tasks
    pub async fn cleanup_orphaned_contexts(pool: &PgPool) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_execution_contexts
            WHERE task_id NOT IN (
                SELECT task_id FROM tasker_tasks
            )
            "#
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Refresh execution context using SQL function (Rails method: refresh!)
    pub async fn refresh_from_task_state(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        if let Some(summary) = Self::get_workflow_summary(pool, self.task_id).await? {
            self.update(
                pool,
                summary.execution_status.as_deref(),
                summary.recommended_action.as_deref(),
                summary.completion_percentage,
                summary.health_status.as_deref(),
                None,
            ).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use crate::models::{Task, NamedTask, TaskNamespace};
    use crate::models::task::NewTask;
    use crate::models::named_task::NewNamedTask;
    use crate::models::task_namespace::NewTaskNamespace;

    #[tokio::test]
    async fn test_task_execution_context_crud() {
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

        // Test context creation
        let new_context = NewTaskExecutionContext {
            task_id: task.task_id,
            execution_status: Some("in_progress".to_string()),
            recommended_action: Some("continue_processing".to_string()),
            completion_percentage: Some(BigDecimal::from(50)), // 50.00
            health_status: Some("healthy".to_string()),
            metadata: Some(serde_json::json!({"priority": "high", "timeout": 3600})),
        };

        let created = TaskExecutionContext::upsert(pool, new_context)
            .await
            .expect("Failed to create context");
        assert_eq!(created.task_id, task.task_id);
        assert_eq!(created.execution_status, "in_progress");
        assert_eq!(created.completion_percentage, BigDecimal::from(50));

        // Test find by task ID
        let found = TaskExecutionContext::find_by_task_id(pool, task.task_id)
            .await
            .expect("Failed to find context")
            .expect("Context not found");
        assert_eq!(found.task_execution_context_id, created.task_execution_context_id);

        // Test for_tasks
        let contexts = TaskExecutionContext::for_tasks(pool, &[task.task_id])
            .await
            .expect("Failed to get contexts for tasks");
        assert_eq!(contexts.len(), 1);

        // Test with_tasks
        let with_tasks = TaskExecutionContext::with_tasks(pool)
            .await
            .expect("Failed to get contexts with tasks");
        assert!(!with_tasks.is_empty());

        // Test get_workflow_summary using SQL function
        let summary = TaskExecutionContext::get_workflow_summary(pool, task.task_id)
            .await
            .expect("Failed to get workflow summary");
        // Summary might be None if SQL function returns no results for this minimal task
        println!("Workflow summary: {:?}", summary);

        // Test by_status
        let by_status = TaskExecutionContext::by_status(pool, "in_progress")
            .await
            .expect("Failed to get contexts by status");
        assert!(!by_status.is_empty());

        // Test by_health_status
        let by_health = TaskExecutionContext::by_health_status(pool, "healthy")
            .await
            .expect("Failed to get contexts by health status");
        assert!(!by_health.is_empty());

        // Test above_completion
        let above_completion = TaskExecutionContext::above_completion(pool, BigDecimal::from(25)) // 25.00
            .await
            .expect("Failed to get contexts above completion");
        assert!(!above_completion.is_empty());

        // Test with_recommendations
        let with_recommendations = TaskExecutionContext::with_recommendations(pool)
            .await
            .expect("Failed to get contexts with recommendations");
        assert!(!with_recommendations.is_empty());

        // Test search_metadata
        let metadata_search = TaskExecutionContext::search_metadata(pool, "priority", "high")
            .await
            .expect("Failed to search metadata");
        assert!(!metadata_search.is_empty());

        // Test update
        let mut context = created.clone();
        context.update(
            pool,
            Some("completed"),
            Some("task_complete"),
            Some(BigDecimal::from(100)), // 100.00
            Some("excellent"),
            Some(serde_json::json!({"priority": "high", "completed_at": "2023-07-02T12:00:00Z"})),
        )
        .await
        .expect("Failed to update context");
        assert_eq!(context.execution_status, "completed");
        assert_eq!(context.completion_percentage, BigDecimal::from(100));

        // Test refresh_from_task_state
        let mut context_refresh = context.clone();
        context_refresh.refresh_from_task_state(pool)
            .await
            .expect("Failed to refresh context");

        // Cleanup
        TaskExecutionContext::delete(pool, task.task_id)
            .await
            .expect("Failed to delete context");
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