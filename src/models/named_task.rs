use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// NamedTask represents task templates/definitions with versioning
/// Maps to `tasker_named_tasks` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedTask {
    pub named_task_id: i32,
    pub name: String,
    pub version: String, // Version is a string in the Rails schema
    pub description: Option<String>,
    pub task_namespace_id: i64, // This is bigint in Rails schema
    pub configuration: Option<serde_json::Value>, // Added configuration field
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New NamedTask for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNamedTask {
    pub name: String,
    pub version: Option<String>, // Defaults to "0.1.0" if not provided
    pub description: Option<String>,
    pub task_namespace_id: i64,
    pub configuration: Option<serde_json::Value>,
}

/// NamedTask with associated steps for delegation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedTaskWithSteps {
    pub named_task: NamedTask,
    pub step_associations: Vec<NamedTaskStepAssociation>,
}

/// Association between named tasks and named steps
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NamedTaskStepAssociation {
    pub id: i32,
    pub named_task_id: i32,
    pub named_step_id: i32,
    pub skippable: bool,
    pub default_retryable: bool,
    pub default_retry_limit: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl NamedTask {
    /// Create a new named task
    pub async fn create(pool: &PgPool, new_task: NewNamedTask) -> Result<NamedTask, sqlx::Error> {
        let version = new_task.version.unwrap_or_else(|| "0.1.0".to_string());
        let configuration = new_task.configuration.unwrap_or_else(|| serde_json::json!({}));
        
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            INSERT INTO tasker_named_tasks (name, version, description, task_namespace_id, configuration)
            VALUES ($1, $2, $3, $4::bigint, $5)
            RETURNING named_task_id, name, version, description, task_namespace_id, configuration, created_at, updated_at
            "#,
            new_task.name,
            version,
            new_task.description,
            new_task.task_namespace_id,
            configuration
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
            SELECT named_task_id, name, version, description, task_namespace_id, configuration, created_at, updated_at
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
        version: &str,
        namespace_id: i64,
    ) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_id, name, version, description, task_namespace_id, configuration, created_at, updated_at
            FROM tasker_named_tasks
            WHERE name = $1 AND version = $2 AND task_namespace_id = $3::bigint
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
        namespace_id: i64,
    ) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_id, name, version, description, task_namespace_id, configuration, created_at, updated_at
            FROM tasker_named_tasks
            WHERE name = $1 AND task_namespace_id = $2::bigint
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            name,
            namespace_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// List all versions of a named task by name and namespace
    pub async fn list_versions_by_name_namespace(
        pool: &PgPool,
        name: &str,
        namespace_id: i64,
    ) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_id, name, version, description, task_namespace_id, configuration, created_at, updated_at
            FROM tasker_named_tasks
            WHERE name = $1 AND task_namespace_id = $2::bigint
            ORDER BY created_at DESC
            "#,
            name,
            namespace_id
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// List all tasks in a namespace
    pub async fn list_by_namespace(
        pool: &PgPool,
        namespace_id: i64,
    ) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_id, name, version, description, task_namespace_id, configuration, created_at, updated_at
            FROM tasker_named_tasks
            WHERE task_namespace_id = $1::bigint
            ORDER BY name, created_at DESC
            "#,
            namespace_id
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// List all tasks with their latest versions in a namespace
    pub async fn list_latest_by_namespace(
        pool: &PgPool,
        namespace_id: i64,
    ) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT DISTINCT ON (name) 
                named_task_id, name, version, description, task_namespace_id, configuration, created_at, updated_at
            FROM tasker_named_tasks
            WHERE task_namespace_id = $1::bigint
            ORDER BY name, created_at DESC
            "#,
            namespace_id
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Update a named task
    pub async fn update(
        pool: &PgPool,
        id: i32,
        description: Option<String>,
        configuration: Option<serde_json::Value>,
    ) -> Result<NamedTask, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            UPDATE tasker_named_tasks
            SET 
                description = COALESCE($2, description),
                configuration = COALESCE($3, configuration),
                updated_at = NOW()
            WHERE named_task_id = $1
            RETURNING named_task_id, name, version, description, task_namespace_id, configuration, created_at, updated_at
            "#,
            id,
            description,
            configuration
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

    /// Check if name/version combination is unique within namespace
    pub async fn is_version_unique(
        pool: &PgPool,
        name: &str,
        version: &str,
        namespace_id: i64,
        exclude_id: Option<i32>,
    ) -> Result<bool, sqlx::Error> {
        let count = if let Some(id) = exclude_id {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_named_tasks
                WHERE name = $1 AND version = $2 AND task_namespace_id = $3::bigint AND named_task_id != $4
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
                WHERE name = $1 AND version = $2 AND task_namespace_id = $3::bigint
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

    /// Get task identifier for delegation
    pub fn get_task_identifier(&self) -> String {
        format!("{}:{}", self.name, self.version)
    }

    /// Get associated step definitions
    pub async fn get_step_associations(
        &self,
        pool: &PgPool,
    ) -> Result<Vec<NamedTaskStepAssociation>, sqlx::Error> {
        let associations = sqlx::query_as!(
            NamedTaskStepAssociation,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable, default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1
            ORDER BY id
            "#,
            self.named_task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Add step association
    pub async fn add_step_association(
        &self,
        pool: &PgPool,
        named_step_id: i32,
        skippable: Option<bool>,
        default_retryable: Option<bool>,
        default_retry_limit: Option<i32>,
    ) -> Result<NamedTaskStepAssociation, sqlx::Error> {
        let association = sqlx::query_as!(
            NamedTaskStepAssociation,
            r#"
            INSERT INTO tasker_named_tasks_named_steps 
            (named_task_id, named_step_id, skippable, default_retryable, default_retry_limit)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, named_task_id, named_step_id, skippable, default_retryable, default_retry_limit, created_at, updated_at
            "#,
            self.named_task_id,
            named_step_id,
            skippable.unwrap_or(false),
            default_retryable.unwrap_or(true),
            default_retry_limit.unwrap_or(3)
        )
        .fetch_one(pool)
        .await?;

        Ok(association)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_named_task_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Create a namespace first
        let namespace = crate::models::task_namespace::TaskNamespace::create(pool, crate::models::task_namespace::NewTaskNamespace {
            name: format!("test_namespace_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            description: None,
        }).await.expect("Failed to create namespace");

        // Test creation
        let new_task = NewNamedTask {
            name: format!("test_task_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            version: Some("1.0.0".to_string()),
            description: Some("Test task description".to_string()),
            task_namespace_id: namespace.task_namespace_id as i64,
            configuration: Some(serde_json::json!({"timeout": 300})),
        };

        let created = NamedTask::create(pool, new_task).await.expect("Failed to create task");
        assert!(created.name.starts_with("test_task_"));
        assert_eq!(created.version, "1.0.0");

        // Test find by ID
        let found = NamedTask::find_by_id(pool, created.named_task_id)
            .await
            .expect("Failed to find task")
            .expect("Task not found");
        assert_eq!(found.named_task_id, created.named_task_id);

        // Test find by name/version/namespace
        let found_by_nvn = NamedTask::find_by_name_version_namespace(pool, &created.name, "1.0.0", namespace.task_namespace_id as i64)
            .await
            .expect("Failed to find task by nvn")
            .expect("Task not found by nvn");
        assert_eq!(found_by_nvn.named_task_id, created.named_task_id);

        // Test version uniqueness
        let is_unique = NamedTask::is_version_unique(pool, &created.name, "2.0.0", namespace.task_namespace_id as i64, None)
            .await
            .expect("Failed to check uniqueness");
        assert!(is_unique);

        let is_not_unique = NamedTask::is_version_unique(pool, &created.name, "1.0.0", namespace.task_namespace_id as i64, None)
            .await
            .expect("Failed to check uniqueness");
        assert!(!is_not_unique);

        // Test delegation methods
        assert_eq!(created.get_task_identifier(), format!("{}:1.0.0", created.name));

        // Test deletion
        let deleted = NamedTask::delete(pool, created.named_task_id)
            .await
            .expect("Failed to delete task");
        assert!(deleted);
        
        // Delete namespace
        crate::models::task_namespace::TaskNamespace::delete(pool, namespace.task_namespace_id)
            .await
            .expect("Failed to delete namespace");

        db.close().await;
    }
}