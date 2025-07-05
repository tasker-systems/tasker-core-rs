//! Task Model Tests
//!
//! Tests for the Task model using SQLx native testing

use serde_json::json;
use sqlx::PgPool;
use tasker_core::models::named_task::{NamedTask, NewNamedTask};
use tasker_core::models::task::{NewTask, Task};
use tasker_core::models::task_namespace::{NewTaskNamespace, TaskNamespace};

#[sqlx::test]
async fn test_task_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "test_namespace".to_string(),
            description: None,
        },
    )
    .await?;

    let named_task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "test_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_id: namespace.task_namespace_id as i64,
            configuration: None,
        },
    )
    .await?;

    // Test creation
    let new_task = NewTask {
        named_task_id: named_task.named_task_id,
        requested_at: None, // Will default to now
        initiator: Some("test_user".to_string()),
        source_system: Some("test_system".to_string()),
        reason: Some("Testing task creation".to_string()),
        bypass_steps: None,
        tags: Some(json!({"priority": "high", "team": "engineering"})),
        context: Some(json!({"input_data": "test_value"})),
        identity_hash: Task::generate_identity_hash(
            named_task.named_task_id,
            &Some(json!({"input_data": "test_value"})),
        ),
    };

    let created = Task::create(&pool, new_task).await?;
    assert_eq!(created.named_task_id, named_task.named_task_id);
    assert!(!created.complete);
    assert_eq!(created.initiator, Some("test_user".to_string()));

    // Test find by ID
    let found = Task::find_by_id(&pool, created.task_id)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(found.task_id, created.task_id);

    // Test find by identity hash
    let found_by_hash = Task::find_by_identity_hash(&pool, &created.identity_hash)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(found_by_hash.task_id, created.task_id);

    // Test mark complete
    let mut task_to_complete = found.clone();
    task_to_complete.mark_complete(&pool).await?;
    assert!(task_to_complete.complete);

    // Test context update
    let new_context = json!({"updated": true, "processed": "2024-01-01"});
    task_to_complete
        .update_context(&pool, new_context.clone())
        .await?;
    assert_eq!(task_to_complete.context, Some(new_context));

    // Test deletion
    let deleted = Task::delete(&pool, created.task_id).await?;
    assert!(deleted);

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[test]
fn test_identity_hash_generation() {
    let context = Some(json!({"key": "value"}));
    let hash1 = Task::generate_identity_hash(1, &context);
    let hash2 = Task::generate_identity_hash(1, &context);
    let hash3 = Task::generate_identity_hash(2, &context);

    // Same inputs should produce same hash
    assert_eq!(hash1, hash2);

    // Different inputs should produce different hash
    assert_ne!(hash1, hash3);
}
