use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool};

/// TaskAnnotation represents metadata annotations attached to tasks
/// Maps to `tasker_task_annotations` table - task metadata storage
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskAnnotation {
    pub task_annotation_id: i64, // bigint in actual schema
    pub task_id: i64,
    pub annotation_type_id: i32,
    pub annotation: JsonValue, // jsonb field in actual schema
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New TaskAnnotation for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskAnnotation {
    pub task_id: i64,
    pub annotation_type_id: i32,
    pub annotation: JsonValue,
}

/// TaskAnnotation with annotation type details
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskAnnotationWithType {
    pub task_annotation_id: i64,
    pub task_id: i64,
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
            INSERT INTO tasker_task_annotations (task_id, annotation_type_id, annotation, created_at, updated_at)
            VALUES ($1, $2, $3, NOW(), NOW())
            RETURNING task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            "#,
            new_annotation.task_id,
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
            SELECT task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            FROM tasker_task_annotations
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
        task_id: i64,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            FROM tasker_task_annotations
            WHERE task_id = $1
            ORDER BY created_at DESC
            "#,
            task_id
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
            SELECT task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            FROM tasker_task_annotations
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
        task_id: i64,
        annotation_type_id: i32,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            FROM tasker_task_annotations
            WHERE task_id = $1 AND annotation_type_id = $2
            ORDER BY created_at DESC
            "#,
            task_id,
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
            SELECT task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            FROM tasker_task_annotations
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
            SELECT task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            FROM tasker_task_annotations
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
        task_id: Option<i64>,
        limit: Option<i32>,
    ) -> Result<Vec<TaskAnnotationWithType>, sqlx::Error> {
        let limit_val = limit.unwrap_or(100);

        let annotations = match task_id {
            Some(task_id) => {
                sqlx::query_as!(
                    TaskAnnotationWithType,
                    r#"
                    SELECT 
                        ta.task_annotation_id,
                        ta.task_id,
                        ta.annotation_type_id,
                        ta.annotation,
                        at.name as annotation_type_name,
                        at.description as annotation_type_description,
                        ta.created_at,
                        ta.updated_at
                    FROM tasker_task_annotations ta
                    INNER JOIN tasker_annotation_types at ON at.annotation_type_id = ta.annotation_type_id
                    WHERE ta.task_id = $1
                    ORDER BY ta.created_at DESC
                    LIMIT $2
                    "#,
                    task_id,
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
                        ta.task_id,
                        ta.annotation_type_id,
                        ta.annotation,
                        at.name as annotation_type_name,
                        at.description as annotation_type_description,
                        ta.created_at,
                        ta.updated_at
                    FROM tasker_task_annotations ta
                    INNER JOIN tasker_annotation_types at ON at.annotation_type_id = ta.annotation_type_id
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
                COUNT(ta.task_annotation_id)::bigint as "usage_count!: i64"
            FROM tasker_annotation_types at
            LEFT JOIN tasker_task_annotations ta ON ta.annotation_type_id = at.annotation_type_id
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
            UPDATE tasker_task_annotations 
            SET task_id = $2,
                annotation_type_id = $3,
                annotation = $4,
                updated_at = NOW()
            WHERE task_annotation_id = $1
            RETURNING task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            "#,
            id,
            new_annotation.task_id,
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
            "DELETE FROM tasker_task_annotations WHERE task_annotation_id = $1",
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all annotations for a task
    pub async fn delete_by_task(pool: &PgPool, task_id: i64) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM tasker_task_annotations WHERE task_id = $1",
            task_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    /// Count annotations by task
    pub async fn count_by_task(pool: &PgPool, task_id: i64) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_task_annotations WHERE task_id = $1",
            task_id
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
            SELECT task_annotation_id, task_id, annotation_type_id, annotation, created_at, updated_at
            FROM tasker_task_annotations
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_task_annotation_crud() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Create test dependencies
        let namespace = crate::models::task_namespace::TaskNamespace::create(
            pool,
            crate::models::task_namespace::NewTaskNamespace {
                name: format!(
                    "test_namespace_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
            },
        )
        .await
        .expect("Failed to create namespace");

        let named_task = crate::models::named_task::NamedTask::create(
            pool,
            crate::models::named_task::NewNamedTask {
                name: format!(
                    "test_task_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_id: namespace.task_namespace_id as i64,
                configuration: None,
            },
        )
        .await
        .expect("Failed to create named task");

        let task = crate::models::task::Task::create(
            pool,
            crate::models::task::NewTask {
                named_task_id: named_task.named_task_id,
                identity_hash: format!(
                    "test_hash_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                requested_at: None,
                initiator: None,
                source_system: None,
                reason: None,
                bypass_steps: None,
                tags: None,
                context: None,
            },
        )
        .await
        .expect("Failed to create task");

        let annotation_type = crate::models::annotation_type::AnnotationType::create(
            pool,
            crate::models::annotation_type::NewAnnotationType {
                name: format!(
                    "test_type_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: Some("Test annotation type".to_string()),
            },
        )
        .await
        .expect("Failed to create annotation type");

        // Test creation
        let new_annotation = NewTaskAnnotation {
            task_id: task.task_id,
            annotation_type_id: annotation_type.annotation_type_id,
            annotation: serde_json::json!({
                "key": "test_value",
                "priority": "high",
                "metadata": {
                    "source": "test"
                }
            }),
        };

        let annotation = TaskAnnotation::create(pool, new_annotation.clone())
            .await
            .expect("Failed to create annotation");
        assert_eq!(annotation.task_id, task.task_id);
        assert_eq!(
            annotation.annotation_type_id,
            annotation_type.annotation_type_id
        );

        // Test find by ID
        let found = TaskAnnotation::find_by_id(pool, annotation.task_annotation_id)
            .await
            .expect("Failed to find annotation");
        assert!(found.is_some());
        assert_eq!(found.unwrap().annotation["key"], "test_value");

        // Test find by task
        let task_annotations = TaskAnnotation::find_by_task(pool, task.task_id)
            .await
            .expect("Failed to find annotations by task");
        assert!(!task_annotations.is_empty());

        // Test JSON search
        let json_search = serde_json::json!({"key": "test_value"});
        let search_results = TaskAnnotation::search_containing_json(pool, &json_search, Some(10))
            .await
            .expect("Failed to search annotations");
        assert!(!search_results.is_empty());

        // Test annotations with types
        let with_types = TaskAnnotation::find_with_types(pool, Some(task.task_id), Some(10))
            .await
            .expect("Failed to get annotations with types");
        assert!(!with_types.is_empty());
        assert_eq!(with_types[0].annotation_type_name, annotation_type.name);

        // Cleanup
        let deleted = TaskAnnotation::delete(pool, annotation.task_annotation_id)
            .await
            .expect("Failed to delete annotation");
        assert!(deleted);
    }

    #[tokio::test]
    async fn test_json_operations() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Create minimal test data
        let namespace = crate::models::task_namespace::TaskNamespace::create(
            pool,
            crate::models::task_namespace::NewTaskNamespace {
                name: format!(
                    "test_namespace_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
            },
        )
        .await
        .expect("Failed to create namespace");

        let named_task = crate::models::named_task::NamedTask::create(
            pool,
            crate::models::named_task::NewNamedTask {
                name: format!(
                    "test_task_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_id: namespace.task_namespace_id as i64,
                configuration: None,
            },
        )
        .await
        .expect("Failed to create named task");

        let task = crate::models::task::Task::create(
            pool,
            crate::models::task::NewTask {
                named_task_id: named_task.named_task_id,
                identity_hash: format!(
                    "test_hash_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                requested_at: None,
                initiator: None,
                source_system: None,
                reason: None,
                bypass_steps: None,
                tags: None,
                context: None,
            },
        )
        .await
        .expect("Failed to create task");

        let annotation_type = crate::models::annotation_type::AnnotationType::create(
            pool,
            crate::models::annotation_type::NewAnnotationType {
                name: format!(
                    "test_type_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
            },
        )
        .await
        .expect("Failed to create annotation type");

        // Create annotation with complex JSON structure
        let complex_json = serde_json::json!({
            "config": {
                "retry_count": 3,
                "timeout": 30
            },
            "tags": ["urgent", "customer-facing"],
            "owner": "test-team"
        });

        let new_annotation = NewTaskAnnotation {
            task_id: task.task_id,
            annotation_type_id: annotation_type.annotation_type_id,
            annotation: complex_json.clone(),
        };

        let annotation = TaskAnnotation::create(pool, new_annotation)
            .await
            .expect("Failed to create complex annotation");

        // Test JSON containment search
        let search_subset = serde_json::json!({
            "config": {
                "retry_count": 3
            }
        });

        let containment_results =
            TaskAnnotation::search_containing_json(pool, &search_subset, Some(5))
                .await
                .expect("Failed to search by containment");
        assert!(
            !containment_results.is_empty(),
            "Should find annotation containing search subset"
        );

        // Cleanup
        TaskAnnotation::delete(pool, annotation.task_annotation_id)
            .await
            .expect("Failed to delete annotation");
    }
}
