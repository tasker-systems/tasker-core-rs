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

    /// Find steps by multiple IDs (efficient batch query)
    pub async fn find_by_ids(pool: &PgPool, ids: &[i32]) -> Result<Vec<NamedStep>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_id, dependent_system_id, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE named_step_id = ANY($1)
            ORDER BY named_step_id
            "#,
            ids
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
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
        let search_pattern = format!("%{pattern}%");

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

    /// Find existing named step by name or create a new one with dependent system
    /// This method also ensures the dependent system exists
    pub async fn find_or_create_by_name(
        pool: &PgPool,
        name: &str,
        dependent_system_name: &str,
    ) -> Result<NamedStep, sqlx::Error> {
        // Import the DependentSystem model to use its find_or_create method
        use crate::models::DependentSystem;

        // First ensure the dependent system exists
        let dependent_system = DependentSystem::find_or_create_by_name_with_description(
            pool,
            dependent_system_name,
            Some(format!(
                "Auto-created system for steps: {dependent_system_name}"
            )),
        )
        .await?;

        // Try to find existing named step by system and name
        if let Some(existing) =
            Self::find_by_system_and_name(pool, dependent_system.dependent_system_id, name).await?
        {
            return Ok(existing);
        }

        // Create new named step if not found
        let new_step = NewNamedStep {
            dependent_system_id: dependent_system.dependent_system_id,
            name: name.to_string(),
            description: Some(format!("Auto-created step: {name}")),
        };

        Self::create(pool, new_step).await
    }

    /// Find existing named step by name only (first match) or create with default dependent system
    pub async fn find_or_create_by_name_simple(
        pool: &PgPool,
        name: &str,
    ) -> Result<NamedStep, sqlx::Error> {
        // Try to find any existing step with this name first
        if let Ok(existing_steps) = Self::find_by_name(pool, name).await {
            if let Some(existing) = existing_steps.first() {
                return Ok(existing.clone());
            }
        }

        // Create with default dependent system
        Self::find_or_create_by_name(pool, name, "shared_testing_factory").await
    }
}
