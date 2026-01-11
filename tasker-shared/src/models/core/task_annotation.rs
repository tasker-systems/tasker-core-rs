use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// TaskAnnotation represents metadata annotations attached to tasks
/// Maps to `tasker.task_annotations` table - task metadata storage
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[sqlx(rename_all = "snake_case")]
pub struct TaskAnnotation {
    pub task_annotation_id: i64, // bigint in actual schema
    pub task_uuid: Uuid,
    pub annotation_type_id: i32,
    pub annotation: JsonValue, // jsonb field in actual schema
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New TaskAnnotation for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskAnnotation {
    pub task_uuid: Uuid,
    pub annotation_type_id: i32,
    pub annotation: JsonValue,
}

/// TaskAnnotation with annotation type details
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[sqlx(rename_all = "snake_case")]
pub struct TaskAnnotationWithType {
    pub task_annotation_id: i64,
    pub task_uuid: Uuid,
    pub annotation_type_id: i32,
    pub annotation: JsonValue,
    pub annotation_type_name: String,
    pub annotation_type_description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Annotation type usage count
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AnnotationTypeCount {
    pub annotation_type_id: i32,
    pub annotation_type_name: String,
    pub usage_count: i64,
}

impl TaskAnnotation {
    /// Create a new task annotation
    pub async fn create(
        pool: &PgPool,
        new_annotation: NewTaskAnnotation,
    ) -> Result<TaskAnnotation, sqlx::Error> {
        let annotation = sqlx::query_as!(
            TaskAnnotation,
            r#"
            INSERT INTO tasker.task_annotations (task_uuid, annotation_type_id, annotation, created_at, updated_at)
            VALUES ($1, $2, $3, NOW(), NOW())
            RETURNING task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            "#,
            new_annotation.task_uuid,
            new_annotation.annotation_type_id,
            new_annotation.annotation
        )
        .fetch_one(pool)
        .await?;

        Ok(annotation)
    }

    /// Find annotation by ID
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<TaskAnnotation>, sqlx::Error> {
        let annotation = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            FROM tasker.task_annotations
            WHERE task_annotation_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(annotation)
    }

    /// Find all annotations for a specific task
    pub async fn find_by_task(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            FROM tasker.task_annotations
            WHERE task_uuid = $1
            ORDER BY created_at DESC
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Find annotations by type
    pub async fn find_by_type(
        pool: &PgPool,
        annotation_type_id: i32,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            FROM tasker.task_annotations
            WHERE annotation_type_id = $1
            ORDER BY created_at DESC
            "#,
            annotation_type_id
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Find annotations by task and type
    pub async fn find_by_task_and_type(
        pool: &PgPool,
        task_uuid: Uuid,
        annotation_type_id: i32,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            FROM tasker.task_annotations
            WHERE task_uuid = $1 AND annotation_type_id = $2
            ORDER BY created_at DESC
            "#,
            task_uuid,
            annotation_type_id
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Search annotations by JSON path expression
    pub async fn search_by_json_path(
        pool: &PgPool,
        json_path: &str,
        value: &JsonValue,
        limit: Option<i32>,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let limit_val = limit.unwrap_or(50);

        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            FROM tasker.task_annotations
            WHERE annotation #> $1::text[] = $2
            ORDER BY created_at DESC
            LIMIT $3
            "#,
            &[json_path.to_string()],
            value,
            limit_val as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Search annotations containing specific JSON data
    pub async fn search_containing_json(
        pool: &PgPool,
        search_json: &JsonValue,
        limit: Option<i32>,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let limit_val = limit.unwrap_or(50);

        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            FROM tasker.task_annotations
            WHERE annotation @> $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            search_json,
            limit_val as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Get annotations with annotation type details
    pub async fn find_with_types(
        pool: &PgPool,
        task_uuid: Option<Uuid>,
        limit: Option<i32>,
    ) -> Result<Vec<TaskAnnotationWithType>, sqlx::Error> {
        let limit_val = limit.unwrap_or(100);

        let annotations = match task_uuid {
            Some(task_uuid) => {
                sqlx::query_as!(
                    TaskAnnotationWithType,
                    r#"
                    SELECT
                        ta.task_annotation_id,
                        ta.task_uuid,
                        ta.annotation_type_id,
                        ta.annotation,
                        at.name as annotation_type_name,
                        at.description as annotation_type_description,
                        ta.created_at,
                        ta.updated_at
                    FROM tasker.task_annotations ta
                    JOIN tasker.annotation_types at ON ta.annotation_type_id = at.annotation_type_id
                    WHERE ta.task_uuid = $1
                    ORDER BY ta.created_at DESC
                    LIMIT $2
                    "#,
                    task_uuid,
                    limit_val as i64
                )
                .fetch_all(pool)
                .await?
            },
            None => {
                sqlx::query_as!(
                    TaskAnnotationWithType,
                    r#"
                    SELECT
                        ta.task_annotation_id,
                        ta.task_uuid,
                        ta.annotation_type_id,
                        ta.annotation,
                        at.name as annotation_type_name,
                        at.description as annotation_type_description,
                        ta.created_at,
                        ta.updated_at
                    FROM tasker.task_annotations ta
                    INNER JOIN tasker.annotation_types at ON at.annotation_type_id = ta.annotation_type_id
                    ORDER BY ta.created_at DESC
                    LIMIT $1
                    "#,
                    limit_val as i64
                )
                .fetch_all(pool)
                .await?
            }
        };

        Ok(annotations)
    }

    /// Get annotation type usage statistics
    pub async fn get_type_usage_stats(
        pool: &PgPool,
    ) -> Result<Vec<AnnotationTypeCount>, sqlx::Error> {
        let stats = sqlx::query_as!(
            AnnotationTypeCount,
            r#"
            SELECT
                at.annotation_type_id,
                at.name as annotation_type_name,
                COUNT(ta.task_annotation_id)::BIGINT as "usage_count!: i64"
            FROM tasker.annotation_types at
            LEFT JOIN tasker.task_annotations ta ON ta.annotation_type_id = at.annotation_type_id
            GROUP BY at.annotation_type_id, at.name
            ORDER BY COUNT(ta.task_annotation_id) DESC, at.name
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(stats)
    }

    /// Update annotation content
    pub async fn update(
        pool: &PgPool,
        id: i64,
        new_annotation: NewTaskAnnotation,
    ) -> Result<Option<TaskAnnotation>, sqlx::Error> {
        let annotation = sqlx::query_as!(
            TaskAnnotation,
            r#"
            UPDATE tasker.task_annotations
            SET task_uuid = $2,
                annotation_type_id = $3,
                annotation = $4,
                updated_at = NOW()
            WHERE task_annotation_id = $1
            RETURNING task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            "#,
            id,
            new_annotation.task_uuid,
            new_annotation.annotation_type_id,
            new_annotation.annotation
        )
        .fetch_optional(pool)
        .await?;

        Ok(annotation)
    }

    /// Delete an annotation
    pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM tasker.task_annotations WHERE task_annotation_id = $1",
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all annotations for a task
    pub async fn delete_by_task(pool: &PgPool, task_uuid: Uuid) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM tasker.task_annotations WHERE task_uuid = $1",
            task_uuid
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    /// Count annotations by task
    pub async fn count_by_task(pool: &PgPool, task_uuid: Uuid) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker.task_annotations WHERE task_uuid = $1",
            task_uuid
        )
        .fetch_one(pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }

    /// List all annotations with pagination
    pub async fn list_all(
        pool: &PgPool,
        offset: Option<i32>,
        limit: Option<i32>,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let offset_val = offset.unwrap_or(0);
        let limit_val = limit.unwrap_or(50);

        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at
            FROM tasker.task_annotations
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
            limit_val as i64,
            offset_val as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }
}
