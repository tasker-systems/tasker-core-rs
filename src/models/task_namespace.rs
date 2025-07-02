use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// TaskNamespace represents organizational hierarchy for tasks
/// Maps to `tasker_task_namespaces` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskNamespace {
    pub task_namespace_id: i32,
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
    pub async fn create(pool: &PgPool, new_namespace: NewTaskNamespace) -> Result<TaskNamespace, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            INSERT INTO tasker_task_namespaces (name, description)
            VALUES ($1, $2)
            RETURNING task_namespace_id, name, description, created_at, updated_at
            "#,
            new_namespace.name,
            new_namespace.description
        )
        .fetch_one(pool)
        .await?;

        Ok(namespace)
    }

    /// Find a task namespace by ID
    pub async fn find_by_id(pool: &PgPool, id: i32) -> Result<Option<TaskNamespace>, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            SELECT task_namespace_id, name, description, created_at, updated_at
            FROM tasker_task_namespaces
            WHERE task_namespace_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(namespace)
    }

    /// Find a task namespace by name
    pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<TaskNamespace>, sqlx::Error> {
        let namespace = sqlx::query_as!(
            TaskNamespace,
            r#"
            SELECT task_namespace_id, name, description, created_at, updated_at
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
            SELECT task_namespace_id, name, description, created_at, updated_at
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
        id: i32,
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
            WHERE task_namespace_id = $1
            RETURNING task_namespace_id, name, description, created_at, updated_at
            "#,
            id,
            name,
            description
        )
        .fetch_one(pool)
        .await?;

        Ok(namespace)
    }

    /// Delete a task namespace
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_namespaces
            WHERE task_namespace_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if namespace name is unique (for validation)
    pub async fn is_name_unique(pool: &PgPool, name: &str, exclude_id: Option<i32>) -> Result<bool, sqlx::Error> {
        let count = if let Some(id) = exclude_id {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_task_namespaces
                WHERE name = $1 AND task_namespace_id != $2
                "#,
                name,
                id
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_task_namespace_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Test creation
        let new_namespace = NewTaskNamespace {
            name: "test_namespace".to_string(),
            description: Some("Test namespace description".to_string()),
        };

        let created = TaskNamespace::create(pool, new_namespace).await.expect("Failed to create namespace");
        assert_eq!(created.name, "test_namespace");
        assert_eq!(created.description, Some("Test namespace description".to_string()));

        // Test find by ID
        let found = TaskNamespace::find_by_id(pool, created.task_namespace_id)
            .await
            .expect("Failed to find namespace")
            .expect("Namespace not found");
        assert_eq!(found.task_namespace_id, created.task_namespace_id);

        // Test find by name
        let found_by_name = TaskNamespace::find_by_name(pool, "test_namespace")
            .await
            .expect("Failed to find namespace by name")
            .expect("Namespace not found by name");
        assert_eq!(found_by_name.task_namespace_id, created.task_namespace_id);

        // Test update
        let updated = TaskNamespace::update(
            pool,
            created.task_namespace_id,
            Some("updated_test_namespace".to_string()),
            Some("Updated description".to_string()),
        )
        .await
        .expect("Failed to update namespace");
        assert_eq!(updated.name, "updated_test_namespace");
        assert_eq!(updated.description, Some("Updated description".to_string()));

        // Test uniqueness check
        let is_unique = TaskNamespace::is_name_unique(pool, "unique_name", None)
            .await
            .expect("Failed to check uniqueness");
        assert!(is_unique);

        let is_not_unique = TaskNamespace::is_name_unique(pool, "updated_test_namespace", None)
            .await
            .expect("Failed to check uniqueness");
        assert!(!is_not_unique);

        // Test deletion
        let deleted = TaskNamespace::delete(pool, created.task_namespace_id)
            .await
            .expect("Failed to delete namespace");
        assert!(deleted);

        // Verify deletion
        let not_found = TaskNamespace::find_by_id(pool, created.task_namespace_id)
            .await
            .expect("Failed to query after deletion");
        assert!(not_found.is_none());

        db.close().await;
    }
}