use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// TaskAnnotation represents annotations applied to tasks
/// Maps to `tasker_task_annotations` table - task metadata and notes (1.1KB Rails model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskAnnotation {
    pub task_annotation_id: i64,
    pub task_id: i64,
    pub annotation_type_id: i32,
    pub content: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New TaskAnnotation for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskAnnotation {
    pub task_id: i64,
    pub annotation_type_id: i32,
    pub content: Option<String>,
}

/// TaskAnnotation with annotation type details
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskAnnotationWithType {
    pub task_annotation_id: i64,
    pub task_id: i64,
    pub annotation_type_id: i32,
    pub content: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub type_name: String,
    pub type_description: Option<String>,
}

/// Annotation type count summary
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AnnotationTypeCount {
    pub annotation_type_id: i32,
    pub type_name: String,
    pub annotation_count: Option<i64>,
}

impl TaskAnnotation {
    /// Create a new task annotation
    pub async fn create(pool: &PgPool, new_annotation: NewTaskAnnotation) -> Result<TaskAnnotation, sqlx::Error> {
        let annotation = sqlx::query_as!(
            TaskAnnotation,
            r#"
            INSERT INTO tasker_task_annotations (task_id, annotation_type_id, content)
            VALUES ($1, $2, $3)
            RETURNING task_annotation_id, task_id, annotation_type_id, content, created_at, updated_at
            "#,
            new_annotation.task_id,
            new_annotation.annotation_type_id,
            new_annotation.content
        )
        .fetch_one(pool)
        .await?;

        Ok(annotation)
    }

    /// Find a task annotation by ID
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<TaskAnnotation>, sqlx::Error> {
        let annotation = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_id, annotation_type_id, content, created_at, updated_at
            FROM tasker_task_annotations
            WHERE task_annotation_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(annotation)
    }

    /// Get annotations for a specific task (Rails scope: for_task)
    pub async fn for_task(pool: &PgPool, task_id: i64) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_id, annotation_type_id, content, created_at, updated_at
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

    /// Get annotations by type (Rails scope: by_type)
    pub async fn by_type(pool: &PgPool, annotation_type_id: i32) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_id, annotation_type_id, content, created_at, updated_at
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

    /// Get annotations with type information (Rails includes: annotation_type)
    pub async fn with_types(pool: &PgPool) -> Result<Vec<TaskAnnotationWithType>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotationWithType,
            r#"
            SELECT 
                ta.task_annotation_id,
                ta.task_id,
                ta.annotation_type_id,
                ta.content,
                ta.created_at,
                ta.updated_at,
                at.name as type_name,
                at.description as type_description
            FROM tasker_task_annotations ta
            JOIN tasker_annotation_types at ON at.annotation_type_id = ta.annotation_type_id
            ORDER BY ta.created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Get annotations for task with type information (Rails scope: for_task_with_types)
    pub async fn for_task_with_types(pool: &PgPool, task_id: i64) -> Result<Vec<TaskAnnotationWithType>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotationWithType,
            r#"
            SELECT 
                ta.task_annotation_id,
                ta.task_id,
                ta.annotation_type_id,
                ta.content,
                ta.created_at,
                ta.updated_at,
                at.name as type_name,
                at.description as type_description
            FROM tasker_task_annotations ta
            JOIN tasker_annotation_types at ON at.annotation_type_id = ta.annotation_type_id
            WHERE ta.task_id = $1
            ORDER BY ta.created_at DESC
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Search annotations by content (Rails scope: search_content)
    pub async fn search_content(pool: &PgPool, search_term: &str) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_id, annotation_type_id, content, created_at, updated_at
            FROM tasker_task_annotations
            WHERE content ILIKE $1
            ORDER BY created_at DESC
            "#,
            format!("%{}%", search_term)
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Get recent annotations (Rails scope: recent)
    pub async fn recent(pool: &PgPool, limit: Option<i64>) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let limit = limit.unwrap_or(50);
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_id, annotation_type_id, content, created_at, updated_at
            FROM tasker_task_annotations
            ORDER BY created_at DESC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Get annotations created in date range (Rails scope: created_between)
    pub async fn created_between(
        pool: &PgPool,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<TaskAnnotation>, sqlx::Error> {
        let annotations = sqlx::query_as!(
            TaskAnnotation,
            r#"
            SELECT task_annotation_id, task_id, annotation_type_id, content, created_at, updated_at
            FROM tasker_task_annotations
            WHERE created_at BETWEEN $1 AND $2
            ORDER BY created_at DESC
            "#,
            start_date,
            end_date
        )
        .fetch_all(pool)
        .await?;

        Ok(annotations)
    }

    /// Get annotation type usage statistics (Rails scope: type_counts)
    pub async fn get_type_counts(pool: &PgPool) -> Result<Vec<AnnotationTypeCount>, sqlx::Error> {
        let counts = sqlx::query_as!(
            AnnotationTypeCount,
            r#"
            SELECT 
                at.annotation_type_id,
                at.name as type_name,
                COUNT(ta.task_annotation_id) as annotation_count
            FROM tasker_annotation_types at
            LEFT JOIN tasker_task_annotations ta ON ta.annotation_type_id = at.annotation_type_id
            GROUP BY at.annotation_type_id, at.name
            ORDER BY annotation_count DESC, at.name
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(counts)
    }

    /// Update a task annotation
    pub async fn update(
        &mut self,
        pool: &PgPool,
        annotation_type_id: Option<i32>,
        content: Option<String>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_task_annotations
            SET annotation_type_id = COALESCE($2, annotation_type_id),
                content = COALESCE($3, content),
                updated_at = NOW()
            WHERE task_annotation_id = $1
            "#,
            self.task_annotation_id,
            annotation_type_id,
            content
        )
        .execute(pool)
        .await?;

        // Update local instance
        if let Some(type_id) = annotation_type_id {
            self.annotation_type_id = type_id;
        }
        if let Some(new_content) = content {
            self.content = Some(new_content);
        }

        Ok(())
    }

    /// Delete a task annotation
    pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_annotations
            WHERE task_annotation_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all annotations for a task
    pub async fn delete_for_task(pool: &PgPool, task_id: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_annotations
            WHERE task_id = $1
            "#,
            task_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete all annotations of a specific type
    pub async fn delete_by_type(pool: &PgPool, annotation_type_id: i32) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_annotations
            WHERE annotation_type_id = $1
            "#,
            annotation_type_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get count of annotations for a task
    pub async fn count_for_task(pool: &PgPool, task_id: i64) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_task_annotations
            WHERE task_id = $1
            "#,
            task_id
        )
        .fetch_one(pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }

    /// Check if task has annotations of a specific type
    pub async fn task_has_type(pool: &PgPool, task_id: i64, annotation_type_id: i32) -> Result<bool, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_task_annotations
            WHERE task_id = $1 AND annotation_type_id = $2
            "#,
            task_id,
            annotation_type_id
        )
        .fetch_one(pool)
        .await?;

        Ok(count.count.unwrap_or(0) > 0)
    }

    /// Clean up orphaned annotations (for deleted tasks)
    pub async fn cleanup_orphaned_annotations(pool: &PgPool) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_annotations
            WHERE task_id NOT IN (
                SELECT task_id FROM tasker_tasks
            )
            "#
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use crate::models::{Task, NamedTask, TaskNamespace, AnnotationType};
    use crate::models::task::NewTask;
    use crate::models::named_task::NewNamedTask;
    use crate::models::task_namespace::NewTaskNamespace;
    use crate::models::annotation_type::NewAnnotationType;

    #[tokio::test]
    async fn test_task_annotation_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Create test dependencies
        let namespace = TaskNamespace::create(pool, NewTaskNamespace {
            name: "test_namespace".to_string(),
            description: None,
        }).await.expect("Failed to create namespace");

        let named_task = NamedTask::create(pool, NewNamedTask {
            name: "test_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_id: namespace.task_namespace_id as i64,
            configuration: None,
        }).await.expect("Failed to create named task");

        let task = Task::create(pool, NewTask {
            named_task_id: named_task.named_task_id as i32,
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
            identity_hash: "test_hash".to_string(),
        }).await.expect("Failed to create task");

        let annotation_type = AnnotationType::create(pool, NewAnnotationType {
            name: "test_annotation".to_string(),
            description: Some("Test annotation type".to_string()),
        }).await.expect("Failed to create annotation type");

        // Test annotation creation
        let new_annotation = NewTaskAnnotation {
            task_id: task.task_id,
            annotation_type_id: annotation_type.annotation_type_id,
            content: Some("This is a test annotation".to_string()),
        };

        let created = TaskAnnotation::create(pool, new_annotation)
            .await
            .expect("Failed to create annotation");
        assert_eq!(created.task_id, task.task_id);
        assert_eq!(created.annotation_type_id, annotation_type.annotation_type_id);

        // Test find by ID
        let found = TaskAnnotation::find_by_id(pool, created.task_annotation_id)
            .await
            .expect("Failed to find annotation")
            .expect("Annotation not found");
        assert_eq!(found.task_annotation_id, created.task_annotation_id);

        // Test for_task
        let task_annotations = TaskAnnotation::for_task(pool, task.task_id)
            .await
            .expect("Failed to get annotations for task");
        assert_eq!(task_annotations.len(), 1);

        // Test by_type
        let type_annotations = TaskAnnotation::by_type(pool, annotation_type.annotation_type_id)
            .await
            .expect("Failed to get annotations by type");
        assert_eq!(type_annotations.len(), 1);

        // Test with_types
        let with_types = TaskAnnotation::with_types(pool)
            .await
            .expect("Failed to get annotations with types");
        assert!(!with_types.is_empty());

        // Test for_task_with_types
        let task_with_types = TaskAnnotation::for_task_with_types(pool, task.task_id)
            .await
            .expect("Failed to get task annotations with types");
        assert_eq!(task_with_types.len(), 1);
        assert_eq!(task_with_types[0].type_name, "test_annotation");

        // Test search_content
        let search_results = TaskAnnotation::search_content(pool, "test annotation")
            .await
            .expect("Failed to search annotation content");
        assert!(!search_results.is_empty());

        // Test recent
        let recent = TaskAnnotation::recent(pool, Some(10))
            .await
            .expect("Failed to get recent annotations");
        assert!(!recent.is_empty());

        // Test created_between
        let start_date = chrono::Utc::now() - chrono::Duration::hours(1);
        let end_date = chrono::Utc::now() + chrono::Duration::hours(1);
        let between = TaskAnnotation::created_between(pool, start_date, end_date)
            .await
            .expect("Failed to get annotations created between dates");
        assert!(!between.is_empty());

        // Test get_type_counts
        let type_counts = TaskAnnotation::get_type_counts(pool)
            .await
            .expect("Failed to get type counts");
        assert!(!type_counts.is_empty());

        // Test update
        let mut annotation = created.clone();
        annotation.update(
            pool,
            None,
            Some("Updated test annotation".to_string()),
        )
        .await
        .expect("Failed to update annotation");

        // Test count_for_task
        let count = TaskAnnotation::count_for_task(pool, task.task_id)
            .await
            .expect("Failed to count annotations for task");
        assert_eq!(count, 1);

        // Test task_has_type
        let has_type = TaskAnnotation::task_has_type(pool, task.task_id, annotation_type.annotation_type_id)
            .await
            .expect("Failed to check if task has annotation type");
        assert!(has_type);

        // Cleanup
        TaskAnnotation::delete(pool, created.task_annotation_id)
            .await
            .expect("Failed to delete annotation");
        AnnotationType::delete(pool, annotation_type.annotation_type_id)
            .await
            .expect("Failed to delete annotation type");
        Task::delete(pool, task.task_id)
            .await
            .expect("Failed to delete task");
        NamedTask::delete(pool, named_task.named_task_id)
            .await
            .expect("Failed to delete named task");
        TaskNamespace::delete(pool, namespace.task_namespace_id)
            .await
            .expect("Failed to delete namespace");

        db.close().await;
    }
}