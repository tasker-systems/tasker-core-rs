use chrono::{DateTime, Utc, NaiveDateTime};
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

    // ============================================================================
    // RAILS ACTIVERECORE SCOPE EQUIVALENTS (18 scopes)
    // ============================================================================

    /// Find tasks by annotation (Rails: scope :by_annotation)
    /// TODO: Fix SQLx database validation issues
    pub async fn by_annotation(
        pool: &PgPool, 
        _annotation_name: &str, 
        _key: &str, 
        _value: &str
    ) -> Result<Vec<Task>, sqlx::Error> {
        // For now, return incomplete tasks as placeholder until database schema issues are resolved
        Self::list_incomplete(pool).await
    }

    /// Find tasks by current state using transitions (Rails: scope :by_current_state)
    /// TODO: Implement complex state transition queries
    pub async fn by_current_state(pool: &PgPool, _state: Option<&str>) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find tasks with all associated records preloaded (Rails: scope :with_all_associated)
    /// TODO: Implement association preloading strategy
    pub async fn with_all_associated(pool: &PgPool) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find tasks created since a specific time (Rails: scope :created_since)
    /// TODO: Implement time-based filtering
    pub async fn created_since(pool: &PgPool, _since_time: DateTime<Utc>) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find tasks completed since a specific time (Rails: scope :completed_since)
    /// TODO: Implement completion time tracking via workflow step transitions
    pub async fn completed_since(pool: &PgPool, _since_time: DateTime<Utc>) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find tasks that failed since a specific time (Rails: scope :failed_since)
    /// TODO: Implement failure detection via current state transitions
    pub async fn failed_since(pool: &PgPool, _since_time: DateTime<Utc>) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find active tasks (not in terminal states) (Rails: scope :active)
    /// TODO: Implement active task detection via workflow step transitions
    pub async fn active(pool: &PgPool) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find tasks in a specific namespace (Rails: scope :in_namespace)
    /// TODO: Implement namespace-based filtering
    pub async fn in_namespace(pool: &PgPool, _namespace_name: &str) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find tasks with a specific task name (Rails: scope :with_task_name)
    /// TODO: Implement task name filtering
    pub async fn with_task_name(pool: &PgPool, _task_name: &str) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find tasks with a specific version (Rails: scope :with_version)
    /// TODO: Implement version filtering
    pub async fn with_version(pool: &PgPool, _version: &str) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    // ============================================================================
    // RAILS CLASS METHODS (6+ methods)
    // ============================================================================

    /// Create and save task with defaults from TaskRequest (Rails: create_with_defaults!)
    /// TODO: Implement TaskRequest integration and default value system
    pub async fn create_with_defaults(pool: &PgPool, _task_request: serde_json::Value) -> Result<Task, sqlx::Error> {
        // Placeholder implementation - would integrate with TaskRequest system
        let new_task = NewTask {
            named_task_id: 1, // Would be extracted from task_request
            requested_at: None,
            initiator: Some("system".to_string()),
            source_system: Some("default".to_string()),
            reason: Some("Created with defaults".to_string()),
            bypass_steps: None,
            tags: None,
            context: Some(serde_json::json!({"default": true})),
            identity_hash: Self::generate_identity_hash(1, &Some(serde_json::json!({"default": true}))),
        };
        Self::create(pool, new_task).await
    }

    /// Create unsaved task instance from TaskRequest (Rails: from_task_request)
    /// TODO: Implement TaskRequest to Task conversion
    pub fn from_task_request(_task_request: serde_json::Value) -> NewTask {
        // Placeholder implementation - would extract fields from task_request
        NewTask {
            named_task_id: 1, // Would be extracted from task_request
            requested_at: None,
            initiator: Some("system".to_string()),
            source_system: Some("default".to_string()),
            reason: Some("From task request".to_string()),
            bypass_steps: None,
            tags: None,
            context: Some(serde_json::json!({"from_request": true})),
            identity_hash: Self::generate_identity_hash(1, &Some(serde_json::json!({"from_request": true}))),
        }
    }

    /// Count unique task types (Rails: unique_task_types_count)
    pub async fn unique_task_types_count(pool: &PgPool) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(DISTINCT nt.name) as count
            FROM tasker_tasks t
            INNER JOIN tasker_named_tasks nt ON nt.named_task_id = t.named_task_id
            "#
        )
        .fetch_one(pool)
        .await?
        .count;

        Ok(count.unwrap_or(0))
    }

    /// Extract options from TaskRequest (Rails: get_request_options)
    /// TODO: Implement TaskRequest option extraction
    pub fn get_request_options(_task_request: serde_json::Value) -> serde_json::Value {
        // Placeholder implementation - would extract options from task_request structure
        serde_json::json!({
            "default_options": true,
            "extracted_from": "task_request"
        })
    }

    /// Get default task request options for a named task (Rails: get_default_task_request_options)
    /// TODO: Implement default option generation based on named task configuration
    pub async fn get_default_task_request_options(pool: &PgPool, named_task_id: i32) -> Result<serde_json::Value, sqlx::Error> {
        // Placeholder - would load from named_task configuration and apply defaults
        let _named_task = sqlx::query!(
            "SELECT configuration FROM tasker_named_tasks WHERE named_task_id = $1",
            named_task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(serde_json::json!({
            "default_timeout": 3600,
            "retry_limit": 3,
            "retryable": true,
            "skippable": false
        }))
    }

    // ============================================================================
    // RAILS INSTANCE METHODS (15+ methods)
    // ============================================================================

    /// Get state machine for this task (Rails: state_machine) - memoized
    /// TODO: Implement TaskStateMachine integration
    pub fn state_machine(&self) -> String {
        // Placeholder - would return TaskStateMachine instance
        format!("TaskStateMachine(task_id: {})", self.task_id)
    }

    /// Get current task status via state machine (Rails: status)
    pub async fn status(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        // Delegate to state machine - for now use get_current_state
        match self.get_current_state(pool).await? {
            Some(state) => Ok(state),
            None => Ok("pending".to_string()),
        }
    }

    /// Find workflow step by name (Rails: get_step_by_name)
    /// TODO: Implement step lookup by name
    pub async fn get_step_by_name(&self, pool: &PgPool, name: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        // Placeholder - would join with named_steps to find by name
        let _step = sqlx::query!(
            r#"
            SELECT ws.workflow_step_id 
            FROM tasker_workflow_steps ws
            INNER JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
            WHERE ws.task_id = $1 AND ns.name = $2
            LIMIT 1
            "#,
            self.task_id,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(Some(serde_json::json!({"step_name": name, "found": true})))
    }

    /// Get/create TaskDiagram for this task (Rails: diagram) - memoized
    /// TODO: Implement TaskDiagram integration
    pub async fn diagram(&self, pool: &PgPool, _base_url: Option<&str>) -> Result<String, sqlx::Error> {
        // Placeholder - would integrate with TaskDiagram model
        let _diagram = sqlx::query!(
            "SELECT diagram_data FROM tasker_task_diagrams WHERE task_id = $1 LIMIT 1",
            self.task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(format!("TaskDiagram(task_id: {})", self.task_id))
    }

    /// Get RuntimeGraphAnalyzer for this task (Rails: runtime_analyzer) - memoized
    /// TODO: Implement RuntimeGraphAnalyzer
    pub fn runtime_analyzer(&self) -> String {
        format!("RuntimeGraphAnalyzer(task_id: {})", self.task_id)
    }

    /// Get runtime dependency graph analysis (Rails: dependency_graph)
    /// TODO: Implement dependency graph analysis
    pub fn dependency_graph(&self) -> serde_json::Value {
        serde_json::json!({
            "task_id": self.task_id,
            "nodes": [],
            "edges": [],
            "analysis": "placeholder"
        })
    }

    /// Check if all workflow steps are complete (Rails: all_steps_complete?)
    pub async fn all_steps_complete(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_steps,
                COUNT(*) FILTER (WHERE processed = true) as completed_steps
            FROM tasker_workflow_steps
            WHERE task_id = $1
            "#,
            self.task_id
        )
        .fetch_one(pool)
        .await?;

        let total = result.total_steps.unwrap_or(0);
        let completed = result.completed_steps.unwrap_or(0);
        
        Ok(total > 0 && total == completed)
    }

    /// Get TaskExecutionContext for this task (Rails: task_execution_context) - memoized
    /// TODO: Implement TaskExecutionContext integration
    pub async fn task_execution_context(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        // Placeholder - would integrate with TaskExecutionContext model
        let _context = sqlx::query!(
            "SELECT execution_status FROM tasker_task_execution_contexts WHERE task_id = $1 LIMIT 1",
            self.task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(format!("TaskExecutionContext(task_id: {})", self.task_id))
    }

    /// Reload task and clear memoized instances (Rails: reload override)
    /// TODO: Implement memoization clearing
    pub async fn reload(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        // Reload the task from database
        if let Some(reloaded) = Self::find_by_id(pool, self.task_id).await? {
            *self = reloaded;
        }
        // TODO: Clear memoized instances (state_machine, diagram, etc.)
        Ok(())
    }

    // ============================================================================
    // RAILS DELEGATIONS (7 delegations)
    // ============================================================================

    /// Get task name (Rails: delegate :name, to: :named_task)
    pub async fn name(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        let name = sqlx::query!(
            "SELECT name FROM tasker_named_tasks WHERE named_task_id = $1",
            self.named_task_id
        )
        .fetch_one(pool)
        .await?
        .name;

        Ok(name)
    }

    /// Generate Mermaid diagram (Rails: delegate :to_mermaid, to: :diagram)
    /// TODO: Integrate with TaskDiagram.to_mermaid
    pub async fn to_mermaid(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        // Placeholder - would delegate to TaskDiagram
        let _diagram = self.diagram(pool, None).await?;
        Ok(format!("graph TD\n  A[Task {}] --> B[Placeholder]", self.task_id))
    }

    /// Get workflow summary (Rails: delegate :workflow_summary, to: :task_execution_context)
    /// TODO: Integrate with TaskExecutionContext.workflow_summary
    pub async fn workflow_summary(&self, pool: &PgPool) -> Result<serde_json::Value, sqlx::Error> {
        // Placeholder - would delegate to TaskExecutionContext
        let _context = self.task_execution_context(pool).await?;
        Ok(serde_json::json!({
            "task_id": self.task_id,
            "total_steps": 0,
            "completed_steps": 0,
            "status": "placeholder"
        }))
    }

    /// Get namespace name (Rails: delegate :namespace_name, to: :named_task)
    pub async fn namespace_name(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        let namespace_name = sqlx::query!(
            r#"
            SELECT tn.name
            FROM tasker_named_tasks nt
            INNER JOIN tasker_task_namespaces tn ON tn.task_namespace_id = nt.task_namespace_id
            WHERE nt.named_task_id = $1
            "#,
            self.named_task_id
        )
        .fetch_one(pool)
        .await?
        .name;

        Ok(namespace_name)
    }

    /// Get version (Rails: delegate :version, to: :named_task)
    pub async fn version(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        let version = sqlx::query!(
            "SELECT version FROM tasker_named_tasks WHERE named_task_id = $1",
            self.named_task_id
        )
        .fetch_one(pool)
        .await?
        .version;

        Ok(version)
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