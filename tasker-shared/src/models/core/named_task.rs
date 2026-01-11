use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// NamedTask represents task templates/definitions with versioning
/// Uses UUID v7 for primary key to ensure time-ordered UUIDs
/// Maps to `tasker.named_tasks` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedTask {
    pub named_task_uuid: Uuid,
    pub name: String,
    pub version: String, // Version is a string in the Rails schema
    pub description: Option<String>,
    pub task_namespace_uuid: Uuid,
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
    pub task_namespace_uuid: Uuid,
    pub configuration: Option<serde_json::Value>,
}

/// NamedTask with associated steps for delegation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedTaskWithSteps {
    pub named_task: NamedTask,
    pub step_associations: Vec<NamedTaskStepAssociation>,
}

/// Re-export the junction table type for convenience
pub use super::named_tasks_named_step::NamedTasksNamedStep as NamedTaskStepAssociation;

impl NamedTask {
    /// Create a new named task
    pub async fn create(pool: &PgPool, new_task: NewNamedTask) -> Result<NamedTask, sqlx::Error> {
        let version = new_task.version.unwrap_or_else(|| "0.1.0".to_string());
        let configuration = new_task
            .configuration
            .unwrap_or_else(|| serde_json::json!({}));

        let task = sqlx::query_as!(
            NamedTask,
            r#"
            INSERT INTO tasker.named_tasks (name, version, description, task_namespace_uuid, configuration, created_at, updated_at)
            VALUES ($1, $2, $3, $4::uuid, $5, NOW(), NOW())
            RETURNING named_task_uuid, name, version, description, task_namespace_uuid, configuration, created_at, updated_at
            "#,
            new_task.name,
            version,
            new_task.description,
            new_task.task_namespace_uuid,
            configuration
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Find a named task by UUID
    pub async fn find_by_uuid(pool: &PgPool, uuid: Uuid) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_uuid, name, version, description, task_namespace_uuid, configuration, created_at, updated_at
            FROM tasker.named_tasks
            WHERE named_task_uuid = $1::uuid
            "#,
            uuid
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
        namespace_uuid: Uuid,
    ) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_uuid, name, version, description, task_namespace_uuid, configuration, created_at, updated_at
            FROM tasker.named_tasks
            WHERE name = $1 AND version = $2 AND task_namespace_uuid = $3::uuid
            "#,
            name,
            version,
            namespace_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Find the latest version of a named task by name and namespace
    pub async fn find_latest_by_name_namespace(
        pool: &PgPool,
        name: &str,
        namespace_uuid: Uuid,
    ) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_uuid, name, version, description, task_namespace_uuid, configuration, created_at, updated_at
            FROM tasker.named_tasks
            WHERE name = $1 AND task_namespace_uuid = $2::uuid
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            name,
            namespace_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// List all versions of a named task by name and namespace
    pub async fn list_versions_by_name_namespace(
        pool: &PgPool,
        name: &str,
        namespace_uuid: Uuid,
    ) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_uuid, name, version, description, task_namespace_uuid, configuration, created_at, updated_at
            FROM tasker.named_tasks
            WHERE name = $1 AND task_namespace_uuid = $2::uuid
            ORDER BY created_at DESC
            "#,
            name,
            namespace_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// List all tasks in a namespace
    pub async fn list_by_namespace(
        pool: &PgPool,
        namespace_uuid: Uuid,
    ) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_uuid, name, version, description, task_namespace_uuid, configuration, created_at, updated_at
            FROM tasker.named_tasks
            WHERE task_namespace_uuid = $1::uuid
            ORDER BY name, created_at DESC
            "#,
            namespace_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// List all tasks with their latest versions in a namespace
    pub async fn list_latest_by_namespace(
        pool: &PgPool,
        namespace_uuid: Uuid,
    ) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT DISTINCT ON (name)
                named_task_uuid, name, version, description, task_namespace_uuid, configuration, created_at, updated_at
            FROM tasker.named_tasks
            WHERE task_namespace_uuid = $1::uuid
            ORDER BY name, created_at DESC
            "#,
            namespace_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Update a named task
    pub async fn update(
        pool: &PgPool,
        uuid: Uuid,
        description: Option<String>,
        configuration: Option<serde_json::Value>,
    ) -> Result<NamedTask, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            UPDATE tasker.named_tasks
            SET
                description = COALESCE($2, description),
                configuration = COALESCE($3, configuration),
                updated_at = NOW()
            WHERE named_task_uuid = $1::uuid
            RETURNING named_task_uuid, name, version, description, task_namespace_uuid, configuration, created_at, updated_at
            "#,
            uuid,
            description,
            configuration
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Delete a named task
    pub async fn delete(pool: &PgPool, uuid: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker.named_tasks
            WHERE named_task_uuid = $1::uuid
            "#,
            uuid
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
        namespace_uuid: Uuid,
        exclude_uuid: Option<Uuid>,
    ) -> Result<bool, sqlx::Error> {
        let count = if let Some(uuid) = exclude_uuid {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker.named_tasks
                WHERE name = $1 AND version = $2 AND task_namespace_uuid = $3::uuid AND named_task_uuid != $4::uuid
                "#,
                name,
                version,
                namespace_uuid,
                uuid
            )
            .fetch_one(pool)
            .await?
            .count
        } else {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker.named_tasks
                WHERE name = $1 AND version = $2 AND task_namespace_uuid = $3::uuid
                "#,
                name,
                version,
                namespace_uuid
            )
            .fetch_one(pool)
            .await?
            .count
        };

        Ok(count.unwrap_or(0) == 0)
    }

    /// Get task identifier for delegation
    pub fn get_task_uuidentifier(&self) -> String {
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
            SELECT ntns_uuid, named_task_uuid, named_step_uuid, skippable, default_retryable,
                   default_max_attempts, created_at, updated_at
            FROM tasker.named_tasks_named_steps
            WHERE named_task_uuid = $1::uuid
            ORDER BY created_at
            "#,
            self.named_task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Add step association (simplified for actual schema)
    pub async fn add_step_association(
        &self,
        pool: &PgPool,
        named_step_uuid: Uuid,
    ) -> Result<NamedTaskStepAssociation, sqlx::Error> {
        let association = sqlx::query_as!(
            NamedTaskStepAssociation,
            r#"
            INSERT INTO tasker.named_tasks_named_steps
            (named_task_uuid, named_step_uuid, skippable, default_retryable, default_max_attempts, created_at, updated_at)
            VALUES ($1::uuid, $2::uuid, false, true, 3, NOW(), NOW())
            RETURNING ntns_uuid, named_task_uuid, named_step_uuid, skippable, default_retryable,
                      default_max_attempts, created_at, updated_at
            "#,
            self.named_task_uuid,
            named_step_uuid
        )
        .fetch_one(pool)
        .await?;

        Ok(association)
    }

    /// Find existing named task by name/version/namespace or create a new one if it doesn't exist
    pub async fn find_or_create_by_name_version_namespace(
        pool: &PgPool,
        name: &str,
        version: &str,
        namespace_uuid: Uuid,
    ) -> Result<NamedTask, sqlx::Error> {
        // Try to find existing named task first
        if let Some(existing) =
            Self::find_by_name_version_namespace(pool, name, version, namespace_uuid).await?
        {
            return Ok(existing);
        }

        // Create new named task if not found
        let new_named_task = NewNamedTask {
            name: name.to_string(),
            task_namespace_uuid: namespace_uuid,
            description: Some(format!("Auto-created task: {name} v{version}")),
            version: Some(version.to_string()),
            configuration: Some(serde_json::json!({"auto_created": true})),
        };

        Self::create(pool, new_named_task).await
    }

    /// List tasks in a namespace by namespace name (for registry API)
    pub async fn list_by_namespace_name(
        pool: &PgPool,
        namespace_name: &str,
    ) -> Result<Vec<NamedTask>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT nt.named_task_uuid, nt.name, nt.version, nt.description, 
                   nt.task_namespace_uuid, nt.configuration, nt.created_at, nt.updated_at
            FROM tasker.named_tasks nt
            JOIN tasker.task_namespaces tn ON nt.task_namespace_uuid = tn.task_namespace_uuid
            WHERE tn.name = $1
            ORDER BY nt.name, nt.version
            "#,
            namespace_name
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Find latest version of a task by name and namespace name (for registry API)
    pub async fn find_latest_by_name_and_namespace_name(
        pool: &PgPool,
        name: &str,
        namespace_name: &str,
    ) -> Result<Option<NamedTask>, sqlx::Error> {
        let task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT nt.named_task_uuid, nt.name, nt.version, nt.description,
                   nt.task_namespace_uuid, nt.configuration, nt.created_at, nt.updated_at
            FROM tasker.named_tasks nt
            JOIN tasker.task_namespaces tn ON nt.task_namespace_uuid = tn.task_namespace_uuid
            WHERE tn.name = $1 AND nt.name = $2
            ORDER BY nt.created_at DESC
            LIMIT 1
            "#,
            namespace_name,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }
}
