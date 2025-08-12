use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// NamedStep represents step definitions/templates
/// Uses UUID v7 for primary key and foreign keys to ensure time-ordered UUIDs
/// Maps to `tasker_named_steps` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedStep {
    pub named_step_uuid: Uuid,
    pub dependent_system_uuid: Uuid, // NOT NULL in actual schema
    pub name: String,                // max 128 chars
    pub description: Option<String>, // max 255 chars
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New NamedStep for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNamedStep {
    pub dependent_system_uuid: Uuid, // Required field
    pub name: String,
    pub description: Option<String>,
}

impl NamedStep {
    /// Create a new named step
    pub async fn create(pool: &PgPool, new_step: NewNamedStep) -> Result<NamedStep, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            INSERT INTO tasker_named_steps (dependent_system_uuid, name, description, created_at, updated_at)
            VALUES ($1::uuid, $2, $3, NOW(), NOW())
            RETURNING named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
            "#,
            new_step.dependent_system_uuid,
            new_step.name,
            new_step.description
        )
        .fetch_one(pool)
        .await?;

        Ok(step)
    }

    /// Find step by UUID
    pub async fn find_by_uuid(pool: &PgPool, uuid: Uuid) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE named_step_uuid = $1::uuid
            "#,
            uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Find steps by multiple UUIDs (efficient batch query)
    pub async fn find_by_uuids(pool: &PgPool, uuids: &[Uuid]) -> Result<Vec<NamedStep>, sqlx::Error> {
        if uuids.is_empty() {
            return Ok(vec![]);
        }

        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE named_step_uuid = ANY($1)
            ORDER BY named_step_uuid
            "#,
            uuids
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Find steps by dependent system
    pub async fn find_by_system(
        pool: &PgPool,
        system_uuid: Uuid,
    ) -> Result<Vec<NamedStep>, sqlx::Error> {
        let steps = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE dependent_system_uuid = $1::uuid
            ORDER BY name
            "#,
            system_uuid
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
            SELECT named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
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
        system_uuid: Uuid,
        name: &str,
    ) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            SELECT named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
            FROM tasker_named_steps
            WHERE dependent_system_uuid = $1::uuid AND name = $2
            "#,
            system_uuid,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Update a named step
    pub async fn update(
        pool: &PgPool,
        uuid: Uuid,
        new_step: NewNamedStep,
    ) -> Result<Option<NamedStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            UPDATE tasker_named_steps 
            SET dependent_system_uuid = $2::uuid,
                name = $3,
                description = $4,
                updated_at = NOW()
            WHERE named_step_uuid = $1::uuid
            RETURNING named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
            "#,
            uuid,
            new_step.dependent_system_uuid,
            new_step.name,
            new_step.description
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Delete a named step
    pub async fn delete(pool: &PgPool, uuid: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM tasker_named_steps WHERE named_step_uuid = $1::uuid",
            uuid
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
            Self::find_by_system_and_name(pool, new_step.dependent_system_uuid, &new_step.name)
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
            SELECT named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
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
    pub async fn count_by_system(pool: &PgPool, system_uuid: Uuid) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_named_steps WHERE dependent_system_uuid = $1::uuid",
            system_uuid
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
            SELECT named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
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
            Self::find_by_system_and_name(pool, dependent_system.dependent_system_uuid, name).await?
        {
            return Ok(existing);
        }

        // Create new named step if not found
        let new_step = NewNamedStep {
            dependent_system_uuid: dependent_system.dependent_system_uuid,
            name: name.to_string(),
            description: Some(format!("Auto-created step: {name}")),
        };

        Self::create(pool, new_step).await
    }

    /// Find existing named step by name or create a new one with dependent system (transaction version)
    /// This method also ensures the dependent system exists
    pub async fn find_or_create_by_name_with_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        pool: &PgPool, // Still needed for non-transactional reads
        name: &str,
        dependent_system_name: &str,
    ) -> Result<NamedStep, sqlx::Error> {
        // Import the DependentSystem model to use its find_or_create method
        use crate::models::DependentSystem;

        // First ensure the dependent system exists (can use pool for find_or_create since it's idempotent)
        let dependent_system = DependentSystem::find_or_create_by_name_with_description(
            pool,
            dependent_system_name,
            Some(format!(
                "Auto-created system for steps: {dependent_system_name}"
            )),
        )
        .await?;

        // Try to find existing named step by system and name (using pool for read)
        if let Some(existing) =
            Self::find_by_system_and_name(pool, dependent_system.dependent_system_uuid, name).await?
        {
            return Ok(existing);
        }

        // Create new named step if not found (using transaction for write)
        let new_step = NewNamedStep {
            dependent_system_uuid: dependent_system.dependent_system_uuid,
            name: name.to_string(),
            description: Some(format!("Auto-created step: {name}")),
        };

        Self::create_with_transaction(tx, new_step).await
    }

    /// Create a new named step within a transaction
    pub async fn create_with_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        new_step: NewNamedStep,
    ) -> Result<NamedStep, sqlx::Error> {
        let step = sqlx::query_as!(
            NamedStep,
            r#"
            INSERT INTO tasker_named_steps (dependent_system_uuid, name, description, created_at, updated_at)
            VALUES ($1::uuid, $2, $3, NOW(), NOW())
            RETURNING named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at
            "#,
            new_step.dependent_system_uuid,
            new_step.name,
            new_step.description
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(step)
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
