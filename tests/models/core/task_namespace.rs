//! Task Namespace Model Tests
//!
//! Tests for the TaskNamespace model using SQLx native testing

use sqlx::PgPool;
use tasker_core::models::task_namespace::{NewTaskNamespace, TaskNamespace};

#[sqlx::test]
async fn test_task_namespace_crud(pool: PgPool) -> sqlx::Result<()> {
    // Test creation
    let unique_name = "test_namespace".to_string();
    let new_namespace = NewTaskNamespace {
        name: unique_name.clone(),
        description: Some("Test namespace description".to_string()),
    };

    let created = TaskNamespace::create(&pool, new_namespace).await?;
    assert_eq!(created.name, unique_name);
    assert_eq!(
        created.description,
        Some("Test namespace description".to_string())
    );

    // Test find by ID
    let found = TaskNamespace::find_by_id(&pool, created.task_namespace_id)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(found.task_namespace_id, created.task_namespace_id);

    // Test find by name
    let found_by_name = TaskNamespace::find_by_name(&pool, &unique_name)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(found_by_name.task_namespace_id, created.task_namespace_id);

    // Test update
    let updated_name = "updated_test_namespace".to_string();
    let updated = TaskNamespace::update(
        &pool,
        created.task_namespace_id,
        Some(updated_name.clone()),
        Some("Updated description".to_string()),
    )
    .await?;
    assert_eq!(updated.name, updated_name);
    assert_eq!(updated.description, Some("Updated description".to_string()));

    // Test uniqueness check
    let is_unique = TaskNamespace::is_name_unique(&pool, "unique_name", None).await?;
    assert!(is_unique);

    let is_not_unique = TaskNamespace::is_name_unique(&pool, &updated_name, None).await?;
    assert!(!is_not_unique);

    // Test deletion
    let deleted = TaskNamespace::delete(&pool, created.task_namespace_id).await?;
    assert!(deleted);

    // Verify deletion
    let not_found = TaskNamespace::find_by_id(&pool, created.task_namespace_id).await?;
    assert!(not_found.is_none());

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}
