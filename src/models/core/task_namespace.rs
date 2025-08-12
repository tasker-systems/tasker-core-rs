use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// TaskNamespace represents organizational hierarchy for tasks
/// Uses UUID v7 for primary key to ensure time-ordered UUIDs
/// Maps to `tasker_task_namespaces` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskNamespace {
    pub task_namespace_uuid: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New TaskNamespace for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskNamespace {
    pub name: String,
    pub description: Option<String>,
}

impl TaskNamespace {
    /// Create a new task namespace
    pub async fn create(
        pool: &PgPool,
        new_namespace: NewTaskNamespace,
    ) -> Result<TaskNamespace, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            INSERT INTO tasker_task_namespaces (name, description, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            RETURNING task_namespace_uuid, name, description, created_at, updated_at
            "#,
            new_namespace.name,
            new_namespace.description
        )
        .fetch_one(pool)
        .await?;

        Ok(namespace)
    }

    /// Find a task namespace by UUID
    pub async fn find_by_uuid(pool: &PgPool, uuid: Uuid) -> Result<Option<TaskNamespace>, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            SELECT task_namespace_uuid, name, description, created_at, updated_at
            FROM tasker_task_namespaces
            WHERE task_namespace_uuid = $1::uuid
            "#,
            uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(namespace)
    }

    /// Find a task namespace by name
    pub async fn find_by_name(
        pool: &PgPool,
        name: &str,
    ) -> Result<Option<TaskNamespace>, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            SELECT task_namespace_uuid, name, description, created_at, updated_at
            FROM tasker_task_namespaces
            WHERE name = $1
            "#,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(namespace)
    }

    /// List all task namespaces
    pub async fn list_all(pool: &PgPool) -> Result<Vec<TaskNamespace>, sqlx::Error> {
        let namespaces = sqlx::query_as!(
            TaskNamespace,
            r#"
            SELECT task_namespace_uuid, name, description, created_at, updated_at
            FROM tasker_task_namespaces
            ORDER BY name
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(namespaces)
    }

    /// Update a task namespace
    pub async fn update(
        pool: &PgPool,
        uuid: Uuid,
        name: Option<String>,
        description: Option<String>,
    ) -> Result<TaskNamespace, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            UPDATE tasker_task_namespaces
            SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                updated_at = NOW()
            WHERE task_namespace_uuid = $1::uuid
            RETURNING task_namespace_uuid, name, description, created_at, updated_at
            "#,
            uuid,
            name,
            description
        )
        .fetch_one(pool)
        .await?;

        Ok(namespace)
    }

    /// Delete a task namespace
    pub async fn delete(pool: &PgPool, uuid: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_namespaces
            WHERE task_namespace_uuid = $1::uuid
            "#,
            uuid
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if namespace name is unique (for validation)
    pub async fn is_name_unique(
        pool: &PgPool,
        name: &str,
        exclude_uuid: Option<Uuid>,
    ) -> Result<bool, sqlx::Error> {
        let count = if let Some(uuid) = exclude_uuid {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_task_namespaces
                WHERE name = $1 AND task_namespace_uuid != $2::uuid
                "#,
                name,
                uuid
            )
            .fetch_one(pool)
            .await?
            .count
        } else {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_task_namespaces
                WHERE name = $1
                "#,
                name
            )
            .fetch_one(pool)
            .await?
            .count
        };

        Ok(count.unwrap_or(0) == 0)
    }

    /// Find existing namespace by name or create a new one if it doesn't exist
    pub async fn find_or_create(pool: &PgPool, name: &str) -> Result<TaskNamespace, sqlx::Error> {
        // Try to find existing namespace first
        if let Some(existing) = Self::find_by_name(pool, name).await? {
            return Ok(existing);
        }

        // Create new namespace if not found
        let new_namespace = NewTaskNamespace {
            name: name.to_string(),
            description: Some(format!("Auto-created namespace: {name}")),
        };

        Self::create(pool, new_namespace).await
    }

    pub async fn find_or_create_with_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        name: &str,
    ) -> Result<TaskNamespace, sqlx::Error> {
        // Try to find existing namespace first
        if let Some(existing) = Self::find_by_name_with_transaction(tx, name).await? {
            return Ok(existing);
        }

        // Create new namespace if not found
        let new_namespace = NewTaskNamespace {
            name: name.to_string(),
            description: Some(format!("Auto-created namespace: {name}")),
        };

        Self::create_with_transaction(tx, new_namespace).await
    }

    pub async fn find_by_name_with_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        name: &str,
    ) -> Result<Option<TaskNamespace>, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            SELECT task_namespace_uuid, name, description, created_at, updated_at
            FROM tasker_task_namespaces
            WHERE name = $1
            "#,
            name
        )
        .fetch_optional(&mut **tx)
        .await?;

        Ok(namespace)
    }

    pub async fn create_with_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        new_namespace: NewTaskNamespace,
    ) -> Result<TaskNamespace, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            INSERT INTO tasker_task_namespaces (name, description, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            RETURNING task_namespace_uuid, name, description, created_at, updated_at
            "#,
            new_namespace.name,
            new_namespace.description
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(namespace)
    }
}
