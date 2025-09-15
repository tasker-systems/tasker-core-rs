use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// DependentSystemObjectMap represents bidirectional mappings between objects in different dependent systems
/// Uses integer primary key but UUID foreign keys to dependent systems
/// Maps to `tasker_dependent_system_object_maps` table - bidirectional system object relationships
///
/// This table enables mapping objects between two different systems, allowing for complex
/// system integrations where objects in one system correspond to objects in another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct DependentSystemObjectMap {
    pub dependent_system_object_map_id: i64,
    pub dependent_system_one_uuid: Uuid,
    pub dependent_system_two_uuid: Uuid,
    pub remote_id_one: String, // max 128 chars
    pub remote_id_two: String, // max 128 chars
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New DependentSystemObjectMap for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDependentSystemObjectMap {
    pub dependent_system_one_uuid: Uuid,
    pub dependent_system_two_uuid: Uuid,
    pub remote_id_one: String,
    pub remote_id_two: String,
}

/// DependentSystemObjectMap with system details for queries
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DependentSystemObjectMapWithSystems {
    pub dependent_system_object_map_id: i64,
    pub dependent_system_one_uuid: Uuid,
    pub dependent_system_two_uuid: Uuid,
    pub remote_id_one: String,
    pub remote_id_two: String,
    pub system_one_name: String,
    pub system_two_name: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Statistics about mappings between systems
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MappingStats {
    pub system_one_uuid: Uuid,
    pub system_two_uuid: Uuid,
    pub system_one_name: String,
    pub system_two_name: String,
    pub total_mappings: i64,
}

impl DependentSystemObjectMap {
    /// Create a new dependent system object mapping
    pub async fn create(
        pool: &PgPool,
        new_mapping: NewDependentSystemObjectMap,
    ) -> Result<DependentSystemObjectMap, sqlx::Error> {
        let mapping = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            INSERT INTO tasker_dependent_system_object_maps
            (dependent_system_one_uuid, dependent_system_two_uuid, remote_id_one, remote_id_two, created_at, updated_at)
            VALUES ($1::uuid, $2::uuid, $3, $4, NOW(), NOW())
            RETURNING dependent_system_object_map_id, dependent_system_one_uuid, dependent_system_two_uuid,
                      remote_id_one, remote_id_two, created_at, updated_at
            "#,
            new_mapping.dependent_system_one_uuid,
            new_mapping.dependent_system_two_uuid,
            new_mapping.remote_id_one,
            new_mapping.remote_id_two
        )
        .fetch_one(pool)
        .await?;

        Ok(mapping)
    }

    /// Find mapping by ID
    pub async fn find_by_id(
        pool: &PgPool,
        id: i64,
    ) -> Result<Option<DependentSystemObjectMap>, sqlx::Error> {
        let mapping = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_uuid, dependent_system_two_uuid,
                   remote_id_one, remote_id_two, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE dependent_system_object_map_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(mapping)
    }

    /// Find mapping by system UUIDs and remote IDs (uses unique constraint)
    pub async fn find_by_systems_and_remote_ids(
        pool: &PgPool,
        system_one_uuid: Uuid,
        system_two_uuid: Uuid,
        remote_id_one: &str,
        remote_id_two: &str,
    ) -> Result<Option<DependentSystemObjectMap>, sqlx::Error> {
        let mapping = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_uuid, dependent_system_two_uuid,
                   remote_id_one, remote_id_two, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE dependent_system_one_uuid = $1::uuid
              AND dependent_system_two_uuid = $2::uuid
              AND remote_id_one = $3
              AND remote_id_two = $4
            "#,
            system_one_uuid,
            system_two_uuid,
            remote_id_one,
            remote_id_two
        )
        .fetch_optional(pool)
        .await?;

        Ok(mapping)
    }

    /// Find all mappings for a specific system pair
    pub async fn find_by_systems(
        pool: &PgPool,
        system_one_uuid: Uuid,
        system_two_uuid: Uuid,
    ) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_uuid, dependent_system_two_uuid,
                   remote_id_one, remote_id_two, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE dependent_system_one_uuid = $1::uuid AND dependent_system_two_uuid = $2::uuid
            ORDER BY created_at DESC
            "#,
            system_one_uuid,
            system_two_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Find mappings by remote ID in either direction
    pub async fn find_by_remote_id(
        pool: &PgPool,
        remote_id: &str,
    ) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_uuid, dependent_system_two_uuid,
                   remote_id_one, remote_id_two, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE remote_id_one = $1 OR remote_id_two = $1
            ORDER BY created_at DESC
            "#,
            remote_id
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Get mappings with system details
    pub async fn find_with_systems(
        pool: &PgPool,
        limit: Option<i32>,
    ) -> Result<Vec<DependentSystemObjectMapWithSystems>, sqlx::Error> {
        let limit_clause = limit.unwrap_or(100);

        let mappings = sqlx::query_as!(
            DependentSystemObjectMapWithSystems,
            r#"
            SELECT
                dsom.dependent_system_object_map_id,
                dsom.dependent_system_one_uuid,
                dsom.dependent_system_two_uuid,
                dsom.remote_id_one,
                dsom.remote_id_two,
                ds1.name as system_one_name,
                ds2.name as system_two_name,
                dsom.created_at,
                dsom.updated_at
            FROM tasker_dependent_system_object_maps dsom
            INNER JOIN tasker_dependent_systems ds1 ON ds1.dependent_system_uuid = dsom.dependent_system_one_uuid
            INNER JOIN tasker_dependent_systems ds2 ON ds2.dependent_system_uuid = dsom.dependent_system_two_uuid
            ORDER BY dsom.created_at DESC
            LIMIT $1
            "#,
            limit_clause as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Get mapping statistics by system pairs
    pub async fn get_mapping_stats(pool: &PgPool) -> Result<Vec<MappingStats>, sqlx::Error> {
        let stats = sqlx::query_as!(
            MappingStats,
            r#"
            SELECT
                dsom.dependent_system_one_uuid as system_one_uuid,
                dsom.dependent_system_two_uuid as system_two_uuid,
                ds1.name as system_one_name,
                ds2.name as system_two_name,
                COUNT(*)::BIGINT as "total_mappings!: i64"
            FROM tasker_dependent_system_object_maps dsom
            INNER JOIN tasker_dependent_systems ds1 ON ds1.dependent_system_uuid = dsom.dependent_system_one_uuid
            INNER JOIN tasker_dependent_systems ds2 ON ds2.dependent_system_uuid = dsom.dependent_system_two_uuid
            GROUP BY dsom.dependent_system_one_uuid, dsom.dependent_system_two_uuid, ds1.name, ds2.name
            ORDER BY COUNT(*) DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(stats)
    }

    /// Delete a mapping
    pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM tasker_dependent_system_object_maps WHERE dependent_system_object_map_id = $1",
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Update a mapping
    pub async fn update(
        pool: &PgPool,
        id: i64,
        new_mapping: NewDependentSystemObjectMap,
    ) -> Result<Option<DependentSystemObjectMap>, sqlx::Error> {
        let mapping = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            UPDATE tasker_dependent_system_object_maps
            SET dependent_system_one_uuid = $2::uuid,
                dependent_system_two_uuid = $3::uuid,
                remote_id_one = $4,
                remote_id_two = $5,
                updated_at = NOW()
            WHERE dependent_system_object_map_id = $1
            RETURNING dependent_system_object_map_id, dependent_system_one_uuid, dependent_system_two_uuid,
                      remote_id_one, remote_id_two, created_at, updated_at
            "#,
            id,
            new_mapping.dependent_system_one_uuid,
            new_mapping.dependent_system_two_uuid,
            new_mapping.remote_id_one,
            new_mapping.remote_id_two
        )
        .fetch_optional(pool)
        .await?;

        Ok(mapping)
    }

    /// Create or find existing mapping (idempotent)
    pub async fn find_or_create(
        pool: &PgPool,
        new_mapping: NewDependentSystemObjectMap,
    ) -> Result<DependentSystemObjectMap, sqlx::Error> {
        // Try to find existing mapping first
        if let Some(existing) = Self::find_by_systems_and_remote_ids(
            pool,
            new_mapping.dependent_system_one_uuid,
            new_mapping.dependent_system_two_uuid,
            &new_mapping.remote_id_one,
            &new_mapping.remote_id_two,
        )
        .await?
        {
            return Ok(existing);
        }

        // Create new mapping if not found
        Self::create(pool, new_mapping).await
    }

    /// List all mappings with pagination
    pub async fn list_all(
        pool: &PgPool,
        offset: Option<i32>,
        limit: Option<i32>,
    ) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let offset_val = offset.unwrap_or(0);
        let limit_val = limit.unwrap_or(50);

        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_uuid, dependent_system_two_uuid,
                   remote_id_one, remote_id_two, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
            limit_val as i64,
            offset_val as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }
}
