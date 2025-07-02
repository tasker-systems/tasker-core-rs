use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// NamedStep represents step definitions/templates
/// Maps to `tasker_named_steps` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedStep {
    pub named_step_id: i32,
    pub dependent_system_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New NamedStep for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNamedStep {
    pub dependent_system_id: i32,
    pub name: String,
    pub description: Option<String>,
}

impl NamedStep {
    /// Create a new named step
    pub async fn create(pool: &PgPool, new_step: NewNamedStep) -> Result<NamedStep, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            INSERT INTO tasker_named_steps (dependent_system_id, name, description)
            VALUES ($1, $2, $3)
            RETURNING named_step_id, dependent_system_id, name, description, created_at, updated_at
            "#,
            new_step.dependent_system_id,
            new_step.name,
            new_step.description
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
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE named_step_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Find a named step by name
    pub async fn find_by_name(
        pool: &PgPool,
        name: &str,
    ) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE name = $1
            "#,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Find all named steps by dependent system
    pub async fn find_by_dependent_system(pool: &PgPool, system_id: i32) -> Result<Vec<NamedStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE dependent_system_id = $1
            ORDER BY name
            "#,
            system_id
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// List all named steps
    pub async fn list_all(pool: &PgPool) -> Result<Vec<NamedStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            ORDER BY name
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
        name: Option<&str>,
        description: Option<&str>,
        dependent_system_id: Option<i32>,
    ) -> Result<NamedStep, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            UPDATE tasker_named_steps
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                dependent_system_id = COALESCE($4, dependent_system_id),
                updated_at = NOW()
            WHERE named_step_id = $1
            RETURNING named_step_id, dependent_system_id, name, description, created_at, updated_at
            "#,
            id,
            name,
            description,
            dependent_system_id
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

    /// Check if name is unique (since there's no version)
    pub async fn is_name_unique(
        pool: &PgPool,
        name: &str,
        exclude_id: Option<i32>,
    ) -> Result<bool, sqlx::Error> {
        let count = if let Some(id) = exclude_id {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_named_steps
                WHERE name = $1 AND named_step_id != $2
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
                FROM tasker_named_steps
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

    /// Get step identifier for delegation
    pub fn get_step_identifier(&self) -> String {
        format!("{}:{}", self.name, self.dependent_system_id)
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
            dependent_system_id: 1,
            name: "test_step".to_string(),
            description: Some("Test step description".to_string()),
        };

        let created = NamedStep::create(pool, new_step).await.expect("Failed to create step");
        assert_eq!(created.name, "test_step");
        assert_eq!(created.dependent_system_id, 1);

        // Test find by ID
        let found = NamedStep::find_by_id(pool, created.named_step_id)
            .await
            .expect("Failed to find step")
            .expect("Step not found");
        assert_eq!(found.named_step_id, created.named_step_id);

        // Test find by name
        let found_by_name = NamedStep::find_by_name(pool, "test_step")
            .await
            .expect("Failed to find step by name")
            .expect("Step not found by name");
        assert_eq!(found_by_name.named_step_id, created.named_step_id);

        // Test name uniqueness
        let is_unique = NamedStep::is_name_unique(pool, "test_step_unique", None)
            .await
            .expect("Failed to check uniqueness");
        assert!(is_unique);

        let is_not_unique = NamedStep::is_name_unique(pool, "test_step", None)
            .await
            .expect("Failed to check uniqueness");
        assert!(!is_not_unique);

        // Test update
        let updated = NamedStep::update(
            pool,
            created.named_step_id,
            Some("updated_test_step"),
            Some("Updated description"),
            None,
        )
        .await
        .expect("Failed to update step");
        assert_eq!(updated.name, "updated_test_step");
        assert_eq!(updated.description, Some("Updated description".to_string()));

        // Test delegation methods
        assert_eq!(updated.get_step_identifier(), "updated_test_step:1");

        // Test deletion
        let deleted = NamedStep::delete(pool, created.named_step_id)
            .await
            .expect("Failed to delete step");
        assert!(deleted);

        db.close().await;
    }
}