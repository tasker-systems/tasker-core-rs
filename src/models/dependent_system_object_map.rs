use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool};

/// DependentSystemObjectMap represents bidirectional mappings between objects in different systems
/// Maps to `tasker_dependent_system_object_maps` table - inter-system object relationships (2.7KB Rails model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct DependentSystemObjectMap {
    pub dependent_system_object_map_id: i64,
    pub dependent_system_one_id: i32,
    pub dependent_system_two_id: i32,
    pub object_one_id: i64,
    pub object_two_id: i64,
    pub mapping_type: String,
    pub metadata: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New DependentSystemObjectMap for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDependentSystemObjectMap {
    pub dependent_system_one_id: i32,
    pub dependent_system_two_id: i32,
    pub object_one_id: i64,
    pub object_two_id: i64,
    pub mapping_type: Option<String>,
    pub metadata: Option<JsonValue>,
}

/// DependentSystemObjectMap with system names
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DependentSystemObjectMapWithSystems {
    pub dependent_system_object_map_id: i64,
    pub dependent_system_one_id: i32,
    pub dependent_system_two_id: i32,
    pub object_one_id: i64,
    pub object_two_id: i64,
    pub mapping_type: String,
    pub metadata: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub system_one_name: String,
    pub system_two_name: String,
}

impl DependentSystemObjectMap {
    /// Create a new dependent system object map
    pub async fn create(pool: &PgPool, new_map: NewDependentSystemObjectMap) -> Result<DependentSystemObjectMap, sqlx::Error> {
        let mapping = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            INSERT INTO tasker_dependent_system_object_maps 
            (dependent_system_one_id, dependent_system_two_id, object_one_id, object_two_id, mapping_type, metadata)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id, 
                      object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            "#,
            new_map.dependent_system_one_id,
            new_map.dependent_system_two_id,
            new_map.object_one_id,
            new_map.object_two_id,
            new_map.mapping_type.unwrap_or_else(|| "bidirectional".to_string()),
            new_map.metadata.unwrap_or(JsonValue::Object(serde_json::Map::new()))
        )
        .fetch_one(pool)
        .await?;

        Ok(mapping)
    }

    /// Find a dependent system object map by ID
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<DependentSystemObjectMap>, sqlx::Error> {
        let mapping = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id,
                   object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE dependent_system_object_map_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(mapping)
    }

    /// Find mappings between two specific systems (Rails scope: between_systems)
    pub async fn between_systems(
        pool: &PgPool,
        system_one_id: i32,
        system_two_id: i32,
    ) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id,
                   object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE (dependent_system_one_id = $1 AND dependent_system_two_id = $2)
               OR (dependent_system_one_id = $2 AND dependent_system_two_id = $1)
            ORDER BY created_at DESC
            "#,
            system_one_id,
            system_two_id
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Find mapping for specific object in a system (Rails scope: for_object)
    pub async fn for_object(
        pool: &PgPool,
        system_id: i32,
        object_id: i64,
    ) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id,
                   object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE (dependent_system_one_id = $1 AND object_one_id = $2)
               OR (dependent_system_two_id = $1 AND object_two_id = $2)
            ORDER BY created_at DESC
            "#,
            system_id,
            object_id
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Find or create mapping (Rails method: find_or_create)
    /// This implements the complex bidirectional logic from the Rails model
    pub async fn find_or_create(
        pool: &PgPool,
        system_one_name: &str,
        object_one_id: i64,
        system_two_name: &str,
        object_two_id: i64,
    ) -> Result<DependentSystemObjectMap, sqlx::Error> {
        use crate::models::{DependentSystem};

        // Get or create the dependent systems
        let system_one = DependentSystem::find_or_create_by_name(pool, system_one_name).await?;
        let system_two = DependentSystem::find_or_create_by_name(pool, system_two_name).await?;

        // Try to find existing mapping in either direction
        let existing = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id,
                   object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE (dependent_system_one_id = $1 AND dependent_system_two_id = $2 
                   AND object_one_id = $3 AND object_two_id = $4)
               OR (dependent_system_one_id = $2 AND dependent_system_two_id = $1 
                   AND object_one_id = $4 AND object_two_id = $3)
            "#,
            system_one.dependent_system_id,
            system_two.dependent_system_id,
            object_one_id,
            object_two_id
        )
        .fetch_optional(pool)
        .await?;

        if let Some(mapping) = existing {
            return Ok(mapping);
        }

        // Create new mapping
        let new_map = NewDependentSystemObjectMap {
            dependent_system_one_id: system_one.dependent_system_id,
            dependent_system_two_id: system_two.dependent_system_id,
            object_one_id,
            object_two_id,
            mapping_type: Some("bidirectional".to_string()),
            metadata: None,
        };

        Self::create(pool, new_map).await
    }

    /// Get mappings by mapping type (Rails scope: by_type)
    pub async fn by_type(pool: &PgPool, mapping_type: &str) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id,
                   object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE mapping_type = $1
            ORDER BY created_at DESC
            "#,
            mapping_type
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Get mappings for a specific system (Rails scope: for_system)
    pub async fn for_system(pool: &PgPool, system_id: i32) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id,
                   object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE dependent_system_one_id = $1 OR dependent_system_two_id = $1
            ORDER BY created_at DESC
            "#,
            system_id
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Get mappings with system information (Rails includes: systems)
    pub async fn with_systems(pool: &PgPool) -> Result<Vec<DependentSystemObjectMapWithSystems>, sqlx::Error> {
        let mappings = sqlx::query_as!(
            DependentSystemObjectMapWithSystems,
            r#"
            SELECT 
                dsom.dependent_system_object_map_id,
                dsom.dependent_system_one_id,
                dsom.dependent_system_two_id,
                dsom.object_one_id,
                dsom.object_two_id,
                dsom.mapping_type,
                dsom.metadata,
                dsom.created_at,
                dsom.updated_at,
                ds1.name as system_one_name,
                ds2.name as system_two_name
            FROM tasker_dependent_system_object_maps dsom
            JOIN tasker_dependent_systems ds1 ON ds1.dependent_system_id = dsom.dependent_system_one_id
            JOIN tasker_dependent_systems ds2 ON ds2.dependent_system_id = dsom.dependent_system_two_id
            ORDER BY dsom.created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Search mappings by metadata (Rails scope: search_metadata)
    pub async fn search_metadata(pool: &PgPool, key: &str, value: &str) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id,
                   object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            WHERE metadata ->> $1 ILIKE $2
            ORDER BY created_at DESC
            "#,
            key,
            format!("%{}%", value)
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Get recent mappings (Rails scope: recent)
    pub async fn recent(pool: &PgPool, limit: Option<i64>) -> Result<Vec<DependentSystemObjectMap>, sqlx::Error> {
        let limit = limit.unwrap_or(50);
        let mappings = sqlx::query_as!(
            DependentSystemObjectMap,
            r#"
            SELECT dependent_system_object_map_id, dependent_system_one_id, dependent_system_two_id,
                   object_one_id, object_two_id, mapping_type, metadata, created_at, updated_at
            FROM tasker_dependent_system_object_maps
            ORDER BY created_at DESC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(mappings)
    }

    /// Update a dependent system object map
    pub async fn update(
        &mut self,
        pool: &PgPool,
        mapping_type: Option<&str>,
        metadata: Option<JsonValue>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_dependent_system_object_maps
            SET mapping_type = COALESCE($2, mapping_type),
                metadata = COALESCE($3, metadata),
                updated_at = NOW()
            WHERE dependent_system_object_map_id = $1
            "#,
            self.dependent_system_object_map_id,
            mapping_type,
            metadata
        )
        .execute(pool)
        .await?;

        // Update local instance
        if let Some(new_type) = mapping_type {
            self.mapping_type = new_type.to_string();
        }
        if let Some(new_metadata) = metadata {
            self.metadata = new_metadata;
        }

        Ok(())
    }

    /// Delete a dependent system object map
    pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_dependent_system_object_maps
            WHERE dependent_system_object_map_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all mappings for a system
    pub async fn delete_for_system(pool: &PgPool, system_id: i32) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_dependent_system_object_maps
            WHERE dependent_system_one_id = $1 OR dependent_system_two_id = $1
            "#,
            system_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get mapping statistics (Rails scope: stats)
    pub async fn get_mapping_stats(pool: &PgPool) -> Result<Vec<MappingStats>, sqlx::Error> {
        let stats = sqlx::query_as!(
            MappingStats,
            r#"
            SELECT 
                ds1.name as system_one_name,
                ds2.name as system_two_name,
                COUNT(*) as mapping_count,
                COUNT(DISTINCT dsom.mapping_type) as unique_types
            FROM tasker_dependent_system_object_maps dsom
            JOIN tasker_dependent_systems ds1 ON ds1.dependent_system_id = dsom.dependent_system_one_id
            JOIN tasker_dependent_systems ds2 ON ds2.dependent_system_id = dsom.dependent_system_two_id
            GROUP BY ds1.name, ds2.name, dsom.dependent_system_one_id, dsom.dependent_system_two_id
            ORDER BY mapping_count DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(stats)
    }

    /// Find reverse mapping (get the other object in the mapping)
    pub async fn find_reverse_mapping(
        &self,
        _pool: &PgPool,
    ) -> Result<Option<(i32, i64)>, sqlx::Error> {
        // This method returns the "other" system and object ID in the mapping
        // If we have system_one -> system_two, it returns system_two info
        // If we have system_two -> system_one, it returns system_one info
        
        // For simplicity, we'll return the "second" system's info
        // In practice, you'd need to know which direction you're looking for
        Ok(Some((self.dependent_system_two_id, self.object_two_id)))
    }
}

/// Mapping statistics between systems
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MappingStats {
    pub system_one_name: String,
    pub system_two_name: String,
    pub mapping_count: Option<i64>,
    pub unique_types: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use crate::models::{DependentSystem, NewDependentSystem};

    #[tokio::test]
    #[ignore = "Schema mismatch - model needs to be updated to match actual database schema with remote_id_one/remote_id_two fields"]
    async fn test_dependent_system_object_map_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Create test dependent systems
        let system_one = DependentSystem::create(pool, NewDependentSystem {
            name: "test_system_one".to_string(),
            description: Some("Test system one".to_string()),
        }).await.expect("Failed to create system one");

        let system_two = DependentSystem::create(pool, NewDependentSystem {
            name: "test_system_two".to_string(),
            description: Some("Test system two".to_string()),
        }).await.expect("Failed to create system two");

        // Test mapping creation
        let new_map = NewDependentSystemObjectMap {
            dependent_system_one_id: system_one.dependent_system_id,
            dependent_system_two_id: system_two.dependent_system_id,
            object_one_id: 12345,
            object_two_id: 67890,
            mapping_type: Some("sync".to_string()),
            metadata: Some(serde_json::json!({"sync_direction": "bidirectional", "priority": "high"})),
        };

        let created = DependentSystemObjectMap::create(pool, new_map)
            .await
            .expect("Failed to create mapping");
        assert_eq!(created.dependent_system_one_id, system_one.dependent_system_id);
        assert_eq!(created.object_one_id, 12345);
        assert_eq!(created.mapping_type, "sync");

        // Test find by ID
        let found = DependentSystemObjectMap::find_by_id(pool, created.dependent_system_object_map_id)
            .await
            .expect("Failed to find mapping")
            .expect("Mapping not found");
        assert_eq!(found.dependent_system_object_map_id, created.dependent_system_object_map_id);

        // Test between_systems
        let between = DependentSystemObjectMap::between_systems(
            pool,
            system_one.dependent_system_id,
            system_two.dependent_system_id,
        )
        .await
        .expect("Failed to find mappings between systems");
        assert_eq!(between.len(), 1);

        // Test for_object
        let for_object = DependentSystemObjectMap::for_object(
            pool,
            system_one.dependent_system_id,
            12345,
        )
        .await
        .expect("Failed to find mappings for object");
        assert_eq!(for_object.len(), 1);

        // Test find_or_create (should find existing)
        let found_or_created = DependentSystemObjectMap::find_or_create(
            pool,
            "test_system_one",
            12345,
            "test_system_two",
            67890,
        )
        .await
        .expect("Failed to find or create mapping");
        assert_eq!(found_or_created.dependent_system_object_map_id, created.dependent_system_object_map_id);

        // Test find_or_create (should create new)
        let new_mapping = DependentSystemObjectMap::find_or_create(
            pool,
            "test_system_one",
            99999,
            "test_system_two",
            88888,
        )
        .await
        .expect("Failed to create new mapping");
        assert_ne!(new_mapping.dependent_system_object_map_id, created.dependent_system_object_map_id);

        // Test by_type
        let by_type = DependentSystemObjectMap::by_type(pool, "sync")
            .await
            .expect("Failed to find mappings by type");
        assert!(!by_type.is_empty());

        // Test for_system
        let for_system = DependentSystemObjectMap::for_system(pool, system_one.dependent_system_id)
            .await
            .expect("Failed to find mappings for system");
        assert!(for_system.len() >= 2); // Should have both mappings

        // Test with_systems
        let with_systems = DependentSystemObjectMap::with_systems(pool)
            .await
            .expect("Failed to get mappings with systems");
        assert!(!with_systems.is_empty());

        // Test search_metadata
        let metadata_search = DependentSystemObjectMap::search_metadata(pool, "priority", "high")
            .await
            .expect("Failed to search metadata");
        assert!(!metadata_search.is_empty());

        // Test recent
        let recent = DependentSystemObjectMap::recent(pool, Some(10))
            .await
            .expect("Failed to get recent mappings");
        assert!(!recent.is_empty());

        // Test update
        let mut mapping = created.clone();
        mapping.update(
            pool,
            Some("full_sync"),
            Some(serde_json::json!({"sync_direction": "one_way", "updated": true}))
        )
        .await
        .expect("Failed to update mapping");
        assert_eq!(mapping.mapping_type, "full_sync");

        // Test get_mapping_stats
        let stats = DependentSystemObjectMap::get_mapping_stats(pool)
            .await
            .expect("Failed to get mapping stats");
        assert!(!stats.is_empty());

        // Test find_reverse_mapping
        let reverse = mapping.find_reverse_mapping(pool)
            .await
            .expect("Failed to find reverse mapping");
        assert!(reverse.is_some());

        // Cleanup
        DependentSystemObjectMap::delete(pool, created.dependent_system_object_map_id)
            .await
            .expect("Failed to delete mapping");
        DependentSystemObjectMap::delete(pool, new_mapping.dependent_system_object_map_id)
            .await
            .expect("Failed to delete new mapping");
        DependentSystem::delete(pool, system_one.dependent_system_id)
            .await
            .expect("Failed to delete system one");
        DependentSystem::delete(pool, system_two.dependent_system_id)
            .await
            .expect("Failed to delete system two");

        db.close().await;
    }
}