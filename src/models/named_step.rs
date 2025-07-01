use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// NamedStep represents step definitions/templates
/// Maps to `tasker_named_steps` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedStep {
    pub named_step_id: i32,
    pub name: String,
    pub version: i32,
    pub description: Option<String>,
    pub handler_class: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New NamedStep for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNamedStep {
    pub name: String,
    pub version: Option<i32>, // Defaults to 1 if not provided
    pub description: Option<String>,
    pub handler_class: String,
}

impl NamedStep {
    /// Create a new named step
    pub async fn create(pool: &PgPool, new_step: NewNamedStep) -> Result<NamedStep, sqlx::Error> {
        let version = new_step.version.unwrap_or(1);
        
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            INSERT INTO tasker_named_steps (name, version, description, handler_class)
            VALUES ($1, $2, $3, $4)
            RETURNING named_step_id, name, version, description, handler_class, created_at, updated_at
            "#,
            new_step.name,
            version,
            new_step.description,
            new_step.handler_class
        )
        .fetch_one(pool)
        .await?;

        Ok(step)
    }

    /// Find a named step by ID
    pub async fn find_by_id(pool: &PgPool, id: i32) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, name, version, description, handler_class, created_at, updated_at
            FROM tasker_named_steps
            WHERE named_step_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Find a named step by name and version
    pub async fn find_by_name_and_version(
        pool: &PgPool,
        name: &str,
        version: i32,
    ) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, name, version, description, handler_class, created_at, updated_at
            FROM tasker_named_steps
            WHERE name = $1 AND version = $2
            "#,
            name,
            version
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Find the latest version of a named step by name
    pub async fn find_latest_by_name(pool: &PgPool, name: &str) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, name, version, description, handler_class, created_at, updated_at
            FROM tasker_named_steps
            WHERE name = $1
            ORDER BY version DESC
            LIMIT 1
            "#,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// List all versions of a named step
    pub async fn list_versions(pool: &PgPool, name: &str) -> Result<Vec<NamedStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, name, version, description, handler_class, created_at, updated_at
            FROM tasker_named_steps
            WHERE name = $1
            ORDER BY version DESC
            "#,
            name
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// List all named steps with their latest versions
    pub async fn list_all_latest(pool: &PgPool) -> Result<Vec<NamedStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT DISTINCT ON (name) 
                named_step_id, name, version, description, handler_class, created_at, updated_at
            FROM tasker_named_steps
            ORDER BY name, version DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Update a named step
    pub async fn update(
        pool: &PgPool,
        id: i32,
        description: Option<String>,
        handler_class: Option<String>,
    ) -> Result<NamedStep, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            UPDATE tasker_named_steps
            SET 
                description = COALESCE($2, description),
                handler_class = COALESCE($3, handler_class),
                updated_at = NOW()
            WHERE named_step_id = $1
            RETURNING named_step_id, name, version, description, handler_class, created_at, updated_at
            "#,
            id,
            description,
            handler_class
        )
        .fetch_one(pool)
        .await?;

        Ok(step)
    }

    /// Delete a named step
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_named_steps
            WHERE named_step_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if name/version combination is unique
    pub async fn is_version_unique(
        pool: &PgPool,
        name: &str,
        version: i32,
        exclude_id: Option<i32>,
    ) -> Result<bool, sqlx::Error> {
        let count = if let Some(id) = exclude_id {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_named_steps
                WHERE name = $1 AND version = $2 AND named_step_id != $3
                "#,
                name,
                version,
                id
            )
            .fetch_one(pool)
            .await?
            .count
        } else {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_named_steps
                WHERE name = $1 AND version = $2
                "#,
                name,
                version
            )
            .fetch_one(pool)
            .await?
            .count
        };

        Ok(count.unwrap_or(0) == 0)
    }

    /// Get the next available version number for a step name
    pub async fn next_version(pool: &PgPool, name: &str) -> Result<i32, sqlx::Error> {
        let max_version = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(version), 0) as max_version
            FROM tasker_named_steps
            WHERE name = $1
            "#,
            name
        )
        .fetch_one(pool)
        .await?
        .max_version;

        Ok(max_version.unwrap_or(0) + 1)
    }

    /// Get handler class for step execution delegation
    pub fn get_handler_class(&self) -> &str {
        &self.handler_class
    }

    /// Get step identifier for delegation
    pub fn get_step_identifier(&self) -> String {
        format!("{}:v{}", self.name, self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_named_step_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Test creation
        let new_step = NewNamedStep {
            name: "test_step".to_string(),
            version: Some(1),
            description: Some("Test step description".to_string()),
            handler_class: "TestStepHandler".to_string(),
        };

        let created = NamedStep::create(pool, new_step).await.expect("Failed to create step");
        assert_eq!(created.name, "test_step");
        assert_eq!(created.version, 1);
        assert_eq!(created.handler_class, "TestStepHandler");

        // Test find by ID
        let found = NamedStep::find_by_id(pool, created.named_step_id)
            .await
            .expect("Failed to find step")
            .expect("Step not found");
        assert_eq!(found.named_step_id, created.named_step_id);

        // Test find by name and version
        let found_by_name = NamedStep::find_by_name_and_version(pool, "test_step", 1)
            .await
            .expect("Failed to find step by name")
            .expect("Step not found by name");
        assert_eq!(found_by_name.named_step_id, created.named_step_id);

        // Test version uniqueness
        let is_unique = NamedStep::is_version_unique(pool, "test_step", 2, None)
            .await
            .expect("Failed to check uniqueness");
        assert!(is_unique);

        let is_not_unique = NamedStep::is_version_unique(pool, "test_step", 1, None)
            .await
            .expect("Failed to check uniqueness");
        assert!(!is_not_unique);

        // Test next version
        let next_version = NamedStep::next_version(pool, "test_step")
            .await
            .expect("Failed to get next version");
        assert_eq!(next_version, 2);

        // Test update
        let updated = NamedStep::update(
            pool,
            created.named_step_id,
            Some("Updated description".to_string()),
            Some("UpdatedStepHandler".to_string()),
        )
        .await
        .expect("Failed to update step");
        assert_eq!(updated.description, Some("Updated description".to_string()));
        assert_eq!(updated.handler_class, "UpdatedStepHandler");

        // Test delegation methods
        assert_eq!(updated.get_handler_class(), "UpdatedStepHandler");
        assert_eq!(updated.get_step_identifier(), "test_step:v1");

        // Test deletion
        let deleted = NamedStep::delete(pool, created.named_step_id)
            .await
            .expect("Failed to delete step");
        assert!(deleted);

        db.close().await;
    }
}