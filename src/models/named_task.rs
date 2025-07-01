use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// NamedTask represents task templates/definitions with versioning
/// Maps to `tasker_named_tasks` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedTask {
    pub named_task_id: i32,
    pub name: String,
    pub version: i32,
    pub description: Option<String>,
    pub task_namespace_id: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New NamedTask for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNamedTask {
    pub name: String,
    pub version: Option<i32>, // Defaults to 1 if not provided
    pub description: Option<String>,
    pub task_namespace_id: i32,
}

/// NamedTask with associated steps for delegation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedTaskWithSteps {
    pub named_task: NamedTask,
    pub step_names: Vec<String>,
}

impl NamedTask {
    /// Create a new named task
    pub async fn create(pool: &PgPool, new_task: NewNamedTask) -> Result<NamedTask, sqlx::Error> {
        let version = new_task.version.unwrap_or(1);
        
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            INSERT INTO tasker_named_tasks (name, version, description, task_namespace_id)
            VALUES ($1, $2, $3, $4)
            RETURNING named_task_id, name, version, description, task_namespace_id, created_at, updated_at
            "#,
            new_task.name,
            version,
            new_task.description,
            new_task.task_namespace_id
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Find a named task by ID
    pub async fn find_by_id(pool: &PgPool, id: i32) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_id, name, version, description, task_namespace_id, created_at, updated_at
            FROM tasker_named_tasks
            WHERE named_task_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Find a named task by name, version, and namespace
    pub async fn find_by_name_version_namespace(
        pool: &PgPool,
        name: &str,
        version: i32,
        namespace_id: i32,
    ) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_id, name, version, description, task_namespace_id, created_at, updated_at
            FROM tasker_named_tasks
            WHERE name = $1 AND version = $2 AND task_namespace_id = $3
            "#,
            name,
            version,
            namespace_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Find the latest version of a named task by name and namespace
    pub async fn find_latest_by_name_namespace(
        pool: &PgPool,
        name: &str,
        namespace_id: i32,
    ) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_id, name, version, description, task_namespace_id, created_at, updated_at
            FROM tasker_named_tasks
            WHERE name = $1 AND task_namespace_id = $2
            ORDER BY version DESC
            LIMIT 1
            "#,
            name,
            namespace_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// List all versions of a named task in a namespace
    pub async fn list_versions(
        pool: &PgPool,
        name: &str,
        namespace_id: i32,
    ) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_id, name, version, description, task_namespace_id, created_at, updated_at
            FROM tasker_named_tasks
            WHERE name = $1 AND task_namespace_id = $2
            ORDER BY version DESC
            "#,
            name,
            namespace_id
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// List all named tasks in a namespace (latest versions)
    pub async fn list_by_namespace(pool: &PgPool, namespace_id: i32) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT DISTINCT ON (name) 
                named_task_id, name, version, description, task_namespace_id, created_at, updated_at
            FROM tasker_named_tasks
            WHERE task_namespace_id = $1
            ORDER BY name, version DESC
            "#,
            namespace_id
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Get associated step names for delegation
    pub async fn get_step_names(pool: &PgPool, named_task_id: i32) -> Result<Vec<String>, sqlx::Error> {
        let step_names = sqlx::query!(
            r#"
            SELECT ns.name
            FROM tasker_named_steps ns
            INNER JOIN tasker_named_tasks_named_steps nts ON ns.named_step_id = nts.named_step_id
            WHERE nts.named_task_id = $1
            ORDER BY ns.name
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect();

        Ok(step_names)
    }

    /// Get named task with its associated steps
    pub async fn find_with_steps(
        pool: &PgPool,
        named_task_id: i32,
    ) -> Result<Option<NamedTaskWithSteps>, sqlx::Error> {
        if let Some(named_task) = Self::find_by_id(pool, named_task_id).await? {
            let step_names = Self::get_step_names(pool, named_task_id).await?;
            Ok(Some(NamedTaskWithSteps {
                named_task,
                step_names,
            }))
        } else {
            Ok(None)
        }
    }

    /// Associate a step with this named task
    pub async fn add_step(
        pool: &PgPool,
        named_task_id: i32,
        named_step_id: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO tasker_named_tasks_named_steps (named_task_id, named_step_id)
            VALUES ($1, $2)
            ON CONFLICT (named_task_id, named_step_id) DO NOTHING
            "#,
            named_task_id,
            named_step_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Remove a step from this named task
    pub async fn remove_step(
        pool: &PgPool,
        named_task_id: i32,
        named_step_id: i32,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1 AND named_step_id = $2
            "#,
            named_task_id,
            named_step_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Update a named task
    pub async fn update(
        pool: &PgPool,
        id: i32,
        description: Option<String>,
    ) -> Result<NamedTask, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            UPDATE tasker_named_tasks
            SET 
                description = COALESCE($2, description),
                updated_at = NOW()
            WHERE named_task_id = $1
            RETURNING named_task_id, name, version, description, task_namespace_id, created_at, updated_at
            "#,
            id,
            description
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Delete a named task
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_named_tasks
            WHERE named_task_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if name/version/namespace combination is unique
    pub async fn is_version_unique(
        pool: &PgPool,
        name: &str,
        version: i32,
        namespace_id: i32,
        exclude_id: Option<i32>,
    ) -> Result<bool, sqlx::Error> {
        let count = if let Some(id) = exclude_id {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_named_tasks
                WHERE name = $1 AND version = $2 AND task_namespace_id = $3 AND named_task_id != $4
                "#,
                name,
                version,
                namespace_id,
                id
            )
            .fetch_one(pool)
            .await?
            .count
        } else {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_named_tasks
                WHERE name = $1 AND version = $2 AND task_namespace_id = $3
                "#,
                name,
                version,
                namespace_id
            )
            .fetch_one(pool)
            .await?
            .count
        };

        Ok(count.unwrap_or(0) == 0)
    }

    /// Get the next available version number for a task name in namespace
    pub async fn next_version(
        pool: &PgPool,
        name: &str,
        namespace_id: i32,
    ) -> Result<i32, sqlx::Error> {
        let max_version = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(version), 0) as max_version
            FROM tasker_named_tasks
            WHERE name = $1 AND task_namespace_id = $2
            "#,
            name,
            namespace_id
        )
        .fetch_one(pool)
        .await?
        .max_version;

        Ok(max_version.unwrap_or(0) + 1)
    }

    /// Get task identifier for delegation
    pub fn get_task_identifier(&self) -> String {
        format!("{}:v{}", self.name, self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use crate::models::task_namespace::{TaskNamespace, NewTaskNamespace};
    use crate::models::named_step::{NamedStep, NewNamedStep};

    #[tokio::test]
    async fn test_named_task_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Create a test namespace first
        let timestamp = chrono::Utc::now().timestamp_millis();
        let new_namespace = NewTaskNamespace {
            name: format!("test_namespace_for_task_{}", timestamp),
            description: Some("Test namespace".to_string()),
        };
        let namespace = TaskNamespace::create(pool, new_namespace).await.expect("Failed to create namespace");

        // Create a test step
        let new_step = NewNamedStep {
            name: format!("test_step_for_task_{}", timestamp),
            version: Some(1),
            description: Some("Test step".to_string()),
            handler_class: "TestHandler".to_string(),
        };
        let step = NamedStep::create(pool, new_step).await.expect("Failed to create step");

        // Test named task creation
        let new_task = NewNamedTask {
            name: format!("test_task_{}", timestamp),
            version: Some(1),
            description: Some("Test task description".to_string()),
            task_namespace_id: namespace.task_namespace_id,
        };

        let created = NamedTask::create(pool, new_task).await.expect("Failed to create task");
        assert!(created.name.starts_with("test_task_"));
        assert_eq!(created.version, 1);
        assert_eq!(created.task_namespace_id, namespace.task_namespace_id);

        // Test adding step to task
        NamedTask::add_step(pool, created.named_task_id, step.named_step_id)
            .await
            .expect("Failed to add step to task");

        // Test finding with steps
        let task_with_steps = NamedTask::find_with_steps(pool, created.named_task_id)
            .await
            .expect("Failed to find task with steps")
            .expect("Task with steps not found");
        
        assert_eq!(task_with_steps.step_names.len(), 1);
        assert!(task_with_steps.step_names[0].starts_with("test_step_for_task_"));

        // Test delegation methods
        assert!(created.get_task_identifier().starts_with("test_task_") && created.get_task_identifier().ends_with(":v1"));

        // Cleanup - remove step association first
        NamedTask::remove_step(pool, created.named_task_id, step.named_step_id)
            .await
            .expect("Failed to remove step from task");
        NamedTask::delete(pool, created.named_task_id).await.expect("Failed to delete task");
        NamedStep::delete(pool, step.named_step_id).await.expect("Failed to delete step");
        TaskNamespace::delete(pool, namespace.task_namespace_id).await.expect("Failed to delete namespace");

        db.close().await;
    }
}