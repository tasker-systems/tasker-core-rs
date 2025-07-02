use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use crate::query_builder::TaskScopes;

/// Task represents actual task instances with delegation metadata
/// Maps to `tasker_tasks` table matching Rails schema exactly
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct Task {
    pub task_id: i64,
    pub named_task_id: i32,
    pub complete: bool,
    pub requested_at: NaiveDateTime,
    pub initiator: Option<String>,
    pub source_system: Option<String>,
    pub reason: Option<String>,
    pub bypass_steps: Option<serde_json::Value>,
    pub tags: Option<serde_json::Value>,
    pub context: Option<serde_json::Value>,
    pub identity_hash: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New Task for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTask {
    pub named_task_id: i32,
    pub requested_at: Option<NaiveDateTime>, // Defaults to NOW() if not provided
    pub initiator: Option<String>,
    pub source_system: Option<String>,
    pub reason: Option<String>,
    pub bypass_steps: Option<serde_json::Value>,
    pub tags: Option<serde_json::Value>,
    pub context: Option<serde_json::Value>,
    pub identity_hash: String,
}

/// Task with delegation metadata for orchestration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskForOrchestration {
    pub task: Task,
    pub task_name: String,
    pub task_version: String,
    pub namespace_name: String,
}

impl Task {
    /// Create a new task
    pub async fn create(pool: &PgPool, new_task: NewTask) -> Result<Task, sqlx::Error> {
        let requested_at = new_task.requested_at.unwrap_or_else(|| chrono::Utc::now().naive_utc());
        
        let task = sqlx::query_as!(
            Task,
            r#"
            INSERT INTO tasker_tasks (
                named_task_id, complete, requested_at, initiator, source_system, 
                reason, bypass_steps, tags, context, identity_hash
            )
            VALUES ($1, false, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING task_id, named_task_id, complete, requested_at, initiator, source_system,
                      reason, bypass_steps, tags, context, identity_hash, created_at, updated_at
            "#,
            new_task.named_task_id,
            requested_at,
            new_task.initiator,
            new_task.source_system,
            new_task.reason,
            new_task.bypass_steps,
            new_task.tags,
            new_task.context,
            new_task.identity_hash
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
            SELECT task_id, named_task_id, complete, requested_at, initiator, source_system,
                   reason, bypass_steps, tags, context, identity_hash, created_at, updated_at
            FROM tasker_tasks
            WHERE task_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Find a task by identity hash
    pub async fn find_by_identity_hash(pool: &PgPool, hash: &str) -> Result<Option<Task>, sqlx::Error> {
        let task = sqlx::query_as!(
            Task,
            r#"
            SELECT task_id, named_task_id, complete, requested_at, initiator, source_system,
                   reason, bypass_steps, tags, context, identity_hash, created_at, updated_at
            FROM tasker_tasks
            WHERE identity_hash = $1
            "#,
            hash
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// List tasks by named task ID
    pub async fn list_by_named_task(pool: &PgPool, named_task_id: i32) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT task_id, named_task_id, complete, requested_at, initiator, source_system,
                   reason, bypass_steps, tags, context, identity_hash, created_at, updated_at
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

    /// List incomplete tasks
    pub async fn list_incomplete(pool: &PgPool) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT task_id, named_task_id, complete, requested_at, initiator, source_system,
                   reason, bypass_steps, tags, context, identity_hash, created_at, updated_at
            FROM tasker_tasks
            WHERE complete = false
            ORDER BY requested_at ASC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Mark task as complete
    pub async fn mark_complete(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_tasks 
            SET complete = true, updated_at = NOW()
            WHERE task_id = $1
            "#,
            self.task_id
        )
        .execute(pool)
        .await?;

        self.complete = true;
        Ok(())
    }

    /// Update task context
    pub async fn update_context(
        &mut self, 
        pool: &PgPool, 
        context: serde_json::Value
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_tasks 
            SET context = $2, updated_at = NOW()
            WHERE task_id = $1
            "#,
            self.task_id,
            context
        )
        .execute(pool)
        .await?;

        self.context = Some(context);
        Ok(())
    }

    /// Update task tags
    pub async fn update_tags(
        &mut self, 
        pool: &PgPool, 
        tags: serde_json::Value
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_tasks 
            SET tags = $2, updated_at = NOW()
            WHERE task_id = $1
            "#,
            self.task_id,
            tags
        )
        .execute(pool)
        .await?;

        self.tags = Some(tags);
        Ok(())
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

    /// Get current state from transitions (since state is managed separately)
    pub async fn get_current_state(&self, pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT to_state
            FROM tasker_task_transitions
            WHERE task_id = $1 AND most_recent = true
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            self.task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| r.to_state))
    }

    /// Check if task has any workflow steps
    pub async fn has_workflow_steps(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_workflow_steps
            WHERE task_id = $1
            "#,
            self.task_id
        )
        .fetch_one(pool)
        .await?
        .count;

        Ok(count.unwrap_or(0) > 0)
    }

    /// Get delegation metadata for orchestration
    pub async fn for_orchestration(&self, pool: &PgPool) -> Result<TaskForOrchestration, sqlx::Error> {
        let task_metadata = sqlx::query!(
            r#"
            SELECT nt.name as task_name, nt.version as task_version, tn.name as namespace_name
            FROM tasker_tasks t
            INNER JOIN tasker_named_tasks nt ON nt.named_task_id = t.named_task_id
            INNER JOIN tasker_task_namespaces tn ON tn.task_namespace_id = nt.task_namespace_id
            WHERE t.task_id = $1
            "#,
            self.task_id
        )
        .fetch_one(pool)
        .await?;

        Ok(TaskForOrchestration {
            task: self.clone(),
            task_name: task_metadata.task_name,
            task_version: task_metadata.task_version,
            namespace_name: task_metadata.namespace_name,
        })
    }

    /// Generate a unique identity hash for deduplication
    pub fn generate_identity_hash(named_task_id: i32, context: &Option<serde_json::Value>) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        named_task_id.hash(&mut hasher);
        if let Some(ctx) = context {
            ctx.to_string().hash(&mut hasher);
        }
        format!("{:x}", hasher.finish())
    }

    /// Apply query builder scopes
    pub fn scopes() -> TaskScopes {
        TaskScopes::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use serde_json::json;

    #[tokio::test]
    async fn test_task_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Test creation
        let new_task = NewTask {
            named_task_id: 1,
            requested_at: None, // Will default to now
            initiator: Some("test_user".to_string()),
            source_system: Some("test_system".to_string()),
            reason: Some("Testing task creation".to_string()),
            bypass_steps: None,
            tags: Some(json!({"priority": "high", "team": "engineering"})),
            context: Some(json!({"input_data": "test_value"})),
            identity_hash: Task::generate_identity_hash(1, &Some(json!({"input_data": "test_value"}))),
        };

        let created = Task::create(pool, new_task).await.expect("Failed to create task");
        assert_eq!(created.named_task_id, 1);
        assert!(!created.complete);
        assert_eq!(created.initiator, Some("test_user".to_string()));

        // Test find by ID
        let found = Task::find_by_id(pool, created.task_id)
            .await
            .expect("Failed to find task")
            .expect("Task not found");
        assert_eq!(found.task_id, created.task_id);

        // Test find by identity hash
        let found_by_hash = Task::find_by_identity_hash(pool, &created.identity_hash)
            .await
            .expect("Failed to find task by hash")
            .expect("Task not found by hash");
        assert_eq!(found_by_hash.task_id, created.task_id);

        // Test mark complete
        let mut task_to_complete = found.clone();
        task_to_complete.mark_complete(pool).await.expect("Failed to mark complete");
        assert!(task_to_complete.complete);

        // Test context update
        let new_context = json!({"updated": true, "processed": "2024-01-01"});
        task_to_complete.update_context(pool, new_context.clone()).await.expect("Failed to update context");
        assert_eq!(task_to_complete.context, Some(new_context));

        // Test deletion
        let deleted = Task::delete(pool, created.task_id)
            .await
            .expect("Failed to delete task");
        assert!(deleted);

        db.close().await;
    }

    #[test]
    fn test_identity_hash_generation() {
        let context = Some(json!({"key": "value"}));
        let hash1 = Task::generate_identity_hash(1, &context);
        let hash2 = Task::generate_identity_hash(1, &context);
        let hash3 = Task::generate_identity_hash(2, &context);

        // Same inputs should produce same hash
        assert_eq!(hash1, hash2);
        
        // Different inputs should produce different hash
        assert_ne!(hash1, hash3);
    }
}