use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// AnnotationType represents categories for task annotations
/// Maps to `tasker.annotation_types` table - simple categorization system (669B Rails model)
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
            INSERT INTO tasker.annotation_types (name, description, created_at, updated_at)
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
            FROM tasker.annotation_types
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
            FROM tasker.annotation_types
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
            FROM tasker.annotation_types
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
            FROM tasker.annotation_types at
            LEFT JOIN tasker.task_annotations ta ON ta.annotation_type_id = at.annotation_type_id
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
            FROM tasker.annotation_types
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
            FROM tasker.annotation_types
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
            UPDATE tasker.annotation_types
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
            DELETE FROM tasker.annotation_types
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
            FROM tasker.task_annotations
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
            FROM tasker.task_annotations
            WHERE annotation_type_id = $1
            "#,
            self.annotation_type_id
        )
        .fetch_one(pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }
}
