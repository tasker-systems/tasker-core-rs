use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// AnnotationType represents categories for task annotations
/// Maps to `tasker_annotation_types` table - simple categorization system (669B Rails model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct AnnotationType {
    pub annotation_type_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New AnnotationType for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAnnotationType {
    pub name: String,
    pub description: Option<String>,
}

/// AnnotationType with usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AnnotationTypeWithStats {
    pub annotation_type_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub annotation_count: Option<i64>,
}

impl AnnotationType {
    /// Create a new annotation type
    pub async fn create(
        pool: &PgPool,
        new_type: NewAnnotationType,
    ) -> Result<AnnotationType, sqlx::Error> {
        let annotation_type = sqlx::query_as!(
            AnnotationType,
            r#"
            INSERT INTO tasker_annotation_types (name, description, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            RETURNING annotation_type_id, name, description, created_at, updated_at
            "#,
            new_type.name,
            new_type.description
        )
        .fetch_one(pool)
        .await?;

        Ok(annotation_type)
    }

    /// Find an annotation type by ID
    pub async fn find_by_id(pool: &PgPool, id: i32) -> Result<Option<AnnotationType>, sqlx::Error> {
        let annotation_type = sqlx::query_as!(
            AnnotationType,
            r#"
            SELECT annotation_type_id, name, description, created_at, updated_at
            FROM tasker_annotation_types
            WHERE annotation_type_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(annotation_type)
    }

    /// Find an annotation type by name
    pub async fn find_by_name(
        pool: &PgPool,
        name: &str,
    ) -> Result<Option<AnnotationType>, sqlx::Error> {
        let annotation_type = sqlx::query_as!(
            AnnotationType,
            r#"
            SELECT annotation_type_id, name, description, created_at, updated_at
            FROM tasker_annotation_types
            WHERE name = $1
            "#,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(annotation_type)
    }

    /// Find or create annotation type by name (Rails method: find_or_create_by!)
    pub async fn find_or_create_by_name(
        pool: &PgPool,
        name: &str,
    ) -> Result<AnnotationType, sqlx::Error> {
        if let Some(existing) = Self::find_by_name(pool, name).await? {
            return Ok(existing);
        }

        Self::create(
            pool,
            NewAnnotationType {
                name: name.to_string(),
                description: None,
            },
        )
        .await
    }

    /// Get all annotation types (Rails scope: all)
    pub async fn all(pool: &PgPool) -> Result<Vec<AnnotationType>, sqlx::Error> {
        let annotation_types = sqlx::query_as!(
            AnnotationType,
            r#"
            SELECT annotation_type_id, name, description, created_at, updated_at
            FROM tasker_annotation_types
            ORDER BY name
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(annotation_types)
    }

    /// Get annotation types with usage statistics (Rails scope: with_counts)
    pub async fn with_usage_stats(
        pool: &PgPool,
    ) -> Result<Vec<AnnotationTypeWithStats>, sqlx::Error> {
        let types_with_stats = sqlx::query_as!(
            AnnotationTypeWithStats,
            r#"
            SELECT 
                at.annotation_type_id,
                at.name,
                at.description,
                at.created_at,
                at.updated_at,
                COUNT(ta.task_annotation_id) as annotation_count
            FROM tasker_annotation_types at
            LEFT JOIN tasker_task_annotations ta ON ta.annotation_type_id = at.annotation_type_id
            GROUP BY at.annotation_type_id, at.name, at.description, at.created_at, at.updated_at
            ORDER BY annotation_count DESC, at.name
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(types_with_stats)
    }

    /// Search annotation types by name pattern (Rails scope: search)
    pub async fn search(pool: &PgPool, pattern: &str) -> Result<Vec<AnnotationType>, sqlx::Error> {
        let annotation_types = sqlx::query_as!(
            AnnotationType,
            r#"
            SELECT annotation_type_id, name, description, created_at, updated_at
            FROM tasker_annotation_types
            WHERE name ILIKE $1 OR description ILIKE $1
            ORDER BY name
            "#,
            format!("%{}%", pattern)
        )
        .fetch_all(pool)
        .await?;

        Ok(annotation_types)
    }

    /// Get recently created annotation types (Rails scope: recent)
    pub async fn recent(
        pool: &PgPool,
        limit: Option<i64>,
    ) -> Result<Vec<AnnotationType>, sqlx::Error> {
        let limit = limit.unwrap_or(10);
        let annotation_types = sqlx::query_as!(
            AnnotationType,
            r#"
            SELECT annotation_type_id, name, description, created_at, updated_at
            FROM tasker_annotation_types
            ORDER BY created_at DESC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(annotation_types)
    }

    /// Update an annotation type
    pub async fn update(
        &mut self,
        pool: &PgPool,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_annotation_types
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                updated_at = NOW()
            WHERE annotation_type_id = $1
            "#,
            self.annotation_type_id,
            name,
            description
        )
        .execute(pool)
        .await?;

        // Update local instance
        if let Some(new_name) = name {
            self.name = new_name.to_string();
        }
        if let Some(new_description) = description {
            self.description = Some(new_description.to_string());
        }

        Ok(())
    }

    /// Delete an annotation type
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_annotation_types
            WHERE annotation_type_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if annotation type is in use
    pub async fn is_in_use(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_task_annotations
            WHERE annotation_type_id = $1
            "#,
            self.annotation_type_id
        )
        .fetch_one(pool)
        .await?;

        Ok(count.count.unwrap_or(0) > 0)
    }

    /// Get count of annotations using this type
    pub async fn annotation_count(&self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_task_annotations
            WHERE annotation_type_id = $1
            "#,
            self.annotation_type_id
        )
        .fetch_one(pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_annotation_type_crud() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Test annotation type creation with unique name
        let type_name = format!(
            "bug_report_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let new_type = NewAnnotationType {
            name: type_name.clone(),
            description: Some("Bug report annotations".to_string()),
        };

        let created = AnnotationType::create(pool, new_type)
            .await
            .expect("Failed to create annotation type");
        assert_eq!(created.name, type_name);
        assert_eq!(
            created.description,
            Some("Bug report annotations".to_string())
        );

        // Test find by ID
        let found = AnnotationType::find_by_id(pool, created.annotation_type_id)
            .await
            .expect("Failed to find annotation type")
            .expect("Annotation type not found");
        assert_eq!(found.annotation_type_id, created.annotation_type_id);

        // Test find by name
        let found_by_name = AnnotationType::find_by_name(pool, &type_name)
            .await
            .expect("Failed to find annotation type by name")
            .expect("Annotation type not found");
        assert_eq!(found_by_name.annotation_type_id, created.annotation_type_id);

        // Test find_or_create_by_name (should find existing)
        let found_or_created = AnnotationType::find_or_create_by_name(pool, &type_name)
            .await
            .expect("Failed to find or create annotation type");
        assert_eq!(
            found_or_created.annotation_type_id,
            created.annotation_type_id
        );

        // Test find_or_create_by_name (should create new)
        let new_type_name = format!(
            "feature_request_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let new_type = AnnotationType::find_or_create_by_name(pool, &new_type_name)
            .await
            .expect("Failed to find or create annotation type");
        assert_eq!(new_type.name, new_type_name);

        // Test all
        let all_types = AnnotationType::all(pool)
            .await
            .expect("Failed to get all annotation types");
        assert!(all_types.len() >= 2);

        // Test with_usage_stats
        let with_stats = AnnotationType::with_usage_stats(pool)
            .await
            .expect("Failed to get annotation types with stats");
        assert!(with_stats.len() >= 2);

        // Test search
        let search_results = AnnotationType::search(pool, "bug_report")
            .await
            .expect("Failed to search annotation types");
        assert!(!search_results.is_empty());

        // Test recent
        let recent = AnnotationType::recent(pool, Some(5))
            .await
            .expect("Failed to get recent annotation types");
        assert!(!recent.is_empty());

        // Test update
        let updated_name = format!(
            "updated_bug_report_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let mut annotation_type = created.clone();
        annotation_type
            .update(pool, Some(&updated_name), Some("Updated description"))
            .await
            .expect("Failed to update annotation type");
        assert_eq!(annotation_type.name, updated_name);

        // Test is_in_use
        let in_use = annotation_type
            .is_in_use(pool)
            .await
            .expect("Failed to check if annotation type is in use");
        assert!(!in_use); // Should not be in use yet

        // Test annotation_count
        let count = annotation_type
            .annotation_count(pool)
            .await
            .expect("Failed to get annotation count");
        assert_eq!(count, 0);

        // Cleanup
        AnnotationType::delete(pool, created.annotation_type_id)
            .await
            .expect("Failed to delete annotation type");
        AnnotationType::delete(pool, new_type.annotation_type_id)
            .await
            .expect("Failed to delete new annotation type");

        db.close().await;
    }
}
