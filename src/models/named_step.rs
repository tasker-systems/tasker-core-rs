use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// NamedStep represents step definitions/templates
/// Maps to `tasker_named_steps` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedStep {
    pub named_step_id: i32,
    pub dependent_system_id: i32,    // NOT NULL in actual schema
    pub name: String,                // max 128 chars
    pub description: Option<String>, // max 255 chars
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New NamedStep for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNamedStep {
    pub dependent_system_id: i32, // Required field
    pub name: String,
    pub description: Option<String>,
}

impl NamedStep {
    /// Create a new named step
    pub async fn create(pool: &PgPool, new_step: NewNamedStep) -> Result<NamedStep, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            INSERT INTO tasker_named_steps (dependent_system_id, name, description, created_at, updated_at)
            VALUES ($1, $2, $3, NOW(), NOW())
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

    /// Find step by ID
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

    /// Find steps by dependent system
    pub async fn find_by_system(
        pool: &PgPool,
        system_id: i32,
    ) -> Result<Vec<NamedStep>, sqlx::Error> {
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

    /// Find steps by name
    pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Vec<NamedStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE name = $1
            ORDER BY created_at
            "#,
            name
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Find step by system and name (uses unique constraint)
    pub async fn find_by_system_and_name(
        pool: &PgPool,
        system_id: i32,
        name: &str,
    ) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE dependent_system_id = $1 AND name = $2
            "#,
            system_id,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Update a named step
    pub async fn update(
        pool: &PgPool,
        id: i32,
        new_step: NewNamedStep,
    ) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            UPDATE tasker_named_steps 
            SET dependent_system_id = $2,
                name = $3,
                description = $4,
                updated_at = NOW()
            WHERE named_step_id = $1
            RETURNING named_step_id, dependent_system_id, name, description, created_at, updated_at
            "#,
            id,
            new_step.dependent_system_id,
            new_step.name,
            new_step.description
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Delete a named step
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM tasker_named_steps WHERE named_step_id = $1",
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Create or find existing step (idempotent)
    pub async fn find_or_create(
        pool: &PgPool,
        new_step: NewNamedStep,
    ) -> Result<NamedStep, sqlx::Error> {
        // Try to find existing step first
        if let Some(existing) =
            Self::find_by_system_and_name(pool, new_step.dependent_system_id, &new_step.name)
                .await?
        {
            return Ok(existing);
        }

        // Create new step if not found
        Self::create(pool, new_step).await
    }

    /// List all steps with pagination
    pub async fn list_all(
        pool: &PgPool,
        offset: Option<i32>,
        limit: Option<i32>,
    ) -> Result<Vec<NamedStep>, sqlx::Error> {
        let offset_val = offset.unwrap_or(0);
        let limit_val = limit.unwrap_or(50);

        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            ORDER BY name
            LIMIT $1 OFFSET $2
            "#,
            limit_val as i64,
            offset_val as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Count steps by system
    pub async fn count_by_system(pool: &PgPool, system_id: i32) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_named_steps WHERE dependent_system_id = $1",
            system_id
        )
        .fetch_one(pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }

    /// Search steps by name pattern
    pub async fn search_by_name(
        pool: &PgPool,
        pattern: &str,
        limit: Option<i32>,
    ) -> Result<Vec<NamedStep>, sqlx::Error> {
        let limit_val = limit.unwrap_or(20);
        let search_pattern = format!("%{}%", pattern);

        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE name ILIKE $1
            ORDER BY name
            LIMIT $2
            "#,
            search_pattern,
            limit_val as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_named_step_crud() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Create test system first
        let system = crate::models::dependent_system::DependentSystem::find_or_create_by_name(
            pool,
            &format!(
                "test_system_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
        )
        .await
        .expect("Failed to create system");

        // Test creation
        let new_step = NewNamedStep {
            dependent_system_id: system.dependent_system_id,
            name: format!(
                "test_step_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            description: Some("Test step description".to_string()),
        };

        let step = NamedStep::create(pool, new_step.clone())
            .await
            .expect("Failed to create step");
        assert_eq!(step.dependent_system_id, system.dependent_system_id);
        assert_eq!(step.description, Some("Test step description".to_string()));

        // Test find by ID
        let found = NamedStep::find_by_id(pool, step.named_step_id)
            .await
            .expect("Failed to find step");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, step.name);

        // Test find by system and name
        let found_specific =
            NamedStep::find_by_system_and_name(pool, system.dependent_system_id, &step.name)
                .await
                .expect("Failed to find specific step");
        assert!(found_specific.is_some());

        // Test find_or_create (should find existing)
        let found_or_created = NamedStep::find_or_create(pool, new_step)
            .await
            .expect("Failed to find or create");
        assert_eq!(found_or_created.named_step_id, step.named_step_id);

        // Test find by system
        let system_steps = NamedStep::find_by_system(pool, system.dependent_system_id)
            .await
            .expect("Failed to find steps by system");
        assert!(!system_steps.is_empty());

        // Test search by name
        let search_results = NamedStep::search_by_name(pool, "test", Some(10))
            .await
            .expect("Failed to search steps");
        assert!(!search_results.is_empty());

        // Test count by system
        let count = NamedStep::count_by_system(pool, system.dependent_system_id)
            .await
            .expect("Failed to count steps");
        assert!(count > 0);

        // Cleanup
        let deleted = NamedStep::delete(pool, step.named_step_id)
            .await
            .expect("Failed to delete step");
        assert!(deleted);
    }

    #[tokio::test]
    async fn test_unique_constraint() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        let system = crate::models::dependent_system::DependentSystem::find_or_create_by_name(
            pool,
            &format!(
                "test_system_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
        )
        .await
        .expect("Failed to create system");

        let step_name = format!(
            "unique_test_step_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );

        // Create first step
        let new_step = NewNamedStep {
            dependent_system_id: system.dependent_system_id,
            name: step_name.clone(),
            description: None,
        };

        let step1 = NamedStep::create(pool, new_step.clone())
            .await
            .expect("Failed to create first step");

        // Try to create duplicate - should fail
        let duplicate_result = NamedStep::create(pool, new_step).await;
        assert!(
            duplicate_result.is_err(),
            "Should not allow duplicate system + name"
        );

        // Cleanup
        NamedStep::delete(pool, step1.named_step_id)
            .await
            .expect("Failed to delete step");
    }
}
