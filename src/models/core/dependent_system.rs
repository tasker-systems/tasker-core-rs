use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// DependentSystem represents external systems that steps depend on
/// Maps to `tasker_dependent_systems` table - simple lookup table (738B Rails model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct DependentSystem {
    pub dependent_system_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New DependentSystem for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDependentSystem {
    pub name: String,
    pub description: Option<String>,
}

impl DependentSystem {
    /// Create a new dependent system
    pub async fn create(
        pool: &PgPool,
        new_system: NewDependentSystem,
    ) -> Result<DependentSystem, sqlx::Error> {
        let system = sqlx::query_as!(
            DependentSystem,
            r#"
            INSERT INTO tasker_dependent_systems (name, description, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            RETURNING dependent_system_id, name, description, created_at, updated_at
            "#,
            new_system.name,
            new_system.description
        )
        .fetch_one(pool)
        .await?;

        Ok(system)
    }

    /// Find a dependent system by ID
    pub async fn find_by_id(
        pool: &PgPool,
        id: i32,
    ) -> Result<Option<DependentSystem>, sqlx::Error> {
        let system = sqlx::query_as!(
            DependentSystem,
            r#"
            SELECT dependent_system_id, name, description, created_at, updated_at
            FROM tasker_dependent_systems
            WHERE dependent_system_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(system)
    }

    /// Find a dependent system by name
    pub async fn find_by_name(
        pool: &PgPool,
        name: &str,
    ) -> Result<Option<DependentSystem>, sqlx::Error> {
        let system = sqlx::query_as!(
            DependentSystem,
            r#"
            SELECT dependent_system_id, name, description, created_at, updated_at
            FROM tasker_dependent_systems
            WHERE name = $1
            "#,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(system)
    }

    /// Find or create by name (Rails find_or_create_by! equivalent)
    pub async fn find_or_create_by_name(
        pool: &PgPool,
        name: &str,
    ) -> Result<DependentSystem, sqlx::Error> {
        // First try to find existing system
        if let Some(existing) = Self::find_by_name(pool, name).await? {
            return Ok(existing);
        }

        // Create new system if not found - handle race condition with error catch
        let new_system = NewDependentSystem {
            name: name.to_string(),
            description: None,
        };

        match Self::create(pool, new_system).await {
            Ok(system) => Ok(system),
            Err(sqlx::Error::Database(ref db_err)) if db_err.code().as_deref() == Some("23505") => {
                // Unique constraint violation - try to find it again
                Self::find_by_name(pool, name)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound)
            }
            Err(e) => Err(e),
        }
    }

    /// Find or create by name with description
    pub async fn find_or_create_by_name_with_description(
        pool: &PgPool,
        name: &str,
        description: Option<String>,
    ) -> Result<DependentSystem, sqlx::Error> {
        // First try to find existing system
        if let Some(existing) = Self::find_by_name(pool, name).await? {
            return Ok(existing);
        }

        // Create new system if not found - handle race condition with error catch
        let new_system = NewDependentSystem {
            name: name.to_string(),
            description,
        };

        match Self::create(pool, new_system).await {
            Ok(system) => Ok(system),
            Err(sqlx::Error::Database(ref db_err)) if db_err.code().as_deref() == Some("23505") => {
                // Unique constraint violation - try to find it again
                Self::find_by_name(pool, name)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound)
            }
            Err(e) => Err(e),
        }
    }

    /// List all dependent systems
    pub async fn list_all(pool: &PgPool) -> Result<Vec<DependentSystem>, sqlx::Error> {
        let systems = sqlx::query_as!(
            DependentSystem,
            r#"
            SELECT dependent_system_id, name, description, created_at, updated_at
            FROM tasker_dependent_systems
            ORDER BY name
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(systems)
    }

    /// Update a dependent system
    pub async fn update(
        &mut self,
        pool: &PgPool,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_dependent_systems
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                updated_at = NOW()
            WHERE dependent_system_id = $1
            "#,
            self.dependent_system_id,
            name,
            description
        )
        .execute(pool)
        .await?;

        if let Some(new_name) = name {
            self.name = new_name.to_string();
        }
        if let Some(new_description) = description {
            self.description = Some(new_description.to_string());
        }

        Ok(())
    }

    /// Delete a dependent system
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_dependent_systems
            WHERE dependent_system_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if name is unique
    pub async fn is_name_unique(
        pool: &PgPool,
        name: &str,
        exclude_id: Option<i32>,
    ) -> Result<bool, sqlx::Error> {
        let count = if let Some(id) = exclude_id {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_dependent_systems
                WHERE name = $1 AND dependent_system_id != $2
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
                FROM tasker_dependent_systems
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

    /// Get dependent system object maps for this system (bidirectional search)
    pub async fn get_object_maps(&self, pool: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
        let map_ids = sqlx::query!(
            r#"
            SELECT dependent_system_object_map_id
            FROM tasker_dependent_system_object_maps
            WHERE dependent_system_one_id = $1 OR dependent_system_two_id = $1
            "#,
            self.dependent_system_id
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.dependent_system_object_map_id)
        .collect();

        Ok(map_ids)
    }

    /// Get named steps for this dependent system
    pub async fn get_named_steps(&self, pool: &PgPool) -> Result<Vec<i32>, sqlx::Error> {
        let step_ids = sqlx::query!(
            r#"
            SELECT named_step_id
            FROM tasker_named_steps
            WHERE dependent_system_id = $1
            ORDER BY name
            "#,
            self.dependent_system_id
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.named_step_id)
        .collect();

        Ok(step_ids)
    }

    /// Count named steps for this dependent system
    pub async fn count_named_steps(&self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_named_steps
            WHERE dependent_system_id = $1
            "#,
            self.dependent_system_id
        )
        .fetch_one(pool)
        .await?
        .count
        .unwrap_or(0);

        Ok(count)
    }

    /// Search dependent systems by name pattern
    pub async fn search_by_name(
        pool: &PgPool,
        pattern: &str,
    ) -> Result<Vec<DependentSystem>, sqlx::Error> {
        let systems = sqlx::query_as!(
            DependentSystem,
            r#"
            SELECT dependent_system_id, name, description, created_at, updated_at
            FROM tasker_dependent_systems
            WHERE name ILIKE $1
            ORDER BY name
            "#,
            format!("%{}%", pattern)
        )
        .fetch_all(pool)
        .await?;

        Ok(systems)
    }

    /// Get usage statistics for this dependent system
    pub async fn get_usage_stats(
        &self,
        pool: &PgPool,
    ) -> Result<DependentSystemStats, sqlx::Error> {
        let named_steps_count = self.count_named_steps(pool).await?;

        let object_maps_count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_dependent_system_object_maps
            WHERE dependent_system_one_id = $1 OR dependent_system_two_id = $1
            "#,
            self.dependent_system_id
        )
        .fetch_one(pool)
        .await?
        .count
        .unwrap_or(0);

        Ok(DependentSystemStats {
            named_steps_count,
            object_maps_count,
        })
    }
}

/// Statistics for a dependent system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependentSystemStats {
    pub named_steps_count: i64,
    pub object_maps_count: i64,
}
