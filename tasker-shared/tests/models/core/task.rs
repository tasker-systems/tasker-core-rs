//! Task Model Tests
//!
//! Tests for the Task model using SQLx native testing

use serde_json::json;
use sqlx::PgPool;
use tasker_shared::models::core::IdentityStrategy;
use tasker_shared::models::named_task::{NamedTask, NewNamedTask};
use tasker_shared::models::task::{NewTask, Task};
use tasker_shared::models::task_namespace::{NewTaskNamespace, TaskNamespace};
use uuid::Uuid;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
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
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    // Test creation
    let new_task = NewTask {
        named_task_uuid: named_task.named_task_uuid,
        requested_at: None, // Will default to now
        initiator: Some("test_user".to_string()),
        source_system: Some("test_system".to_string()),
        reason: Some("Testing task creation".to_string()),

        tags: Some(json!({"priority": "high", "team": "engineering"})),
        context: Some(json!({"input_data": "test_value"})),
        identity_hash: Task::generate_identity_hash(
            named_task.named_task_uuid,
            &Some(json!({"input_data": "test_value"})),
        ),
        priority: Some(5),
        correlation_id: Uuid::now_v7(),
        parent_correlation_id: None,
    };

    let created = Task::create(&pool, new_task).await?;
    assert_eq!(created.named_task_uuid, named_task.named_task_uuid);
    assert!(!created.complete);
    assert_eq!(created.initiator, Some("test_user".to_string()));

    // Test find by ID
    let found = Task::find_by_id(&pool, created.task_uuid)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(found.task_uuid, created.task_uuid);

    // Test find by identity hash
    let found_by_hash = Task::find_by_identity_hash(&pool, &created.identity_hash)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(found_by_hash.task_uuid, created.task_uuid);

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
    let deleted = Task::delete(&pool, created.task_uuid).await?;
    assert!(deleted);

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[test]
fn test_identity_hash_generation() {
    let context = Some(json!({"key": "value"}));
    let shared_uuid = Uuid::now_v7();
    let hash1 = Task::generate_identity_hash(shared_uuid, &context);
    let hash2 = Task::generate_identity_hash(shared_uuid, &context);
    let hash3 = Task::generate_identity_hash(Uuid::now_v7(), &context);

    // Same inputs should produce same hash
    assert_eq!(hash1, hash2);

    // Different inputs should produce different hash
    assert_ne!(hash1, hash3);
}

// =============================================================================
// TAS-154: Identity Strategy Tests
// =============================================================================

#[test]
fn test_compute_identity_hash_strict_strategy() {
    let named_task_uuid = Uuid::now_v7();
    let context = Some(json!({"order_id": 12345}));

    // STRICT strategy: hash(named_task_uuid, context)
    let hash1 = Task::compute_identity_hash(
        IdentityStrategy::Strict,
        named_task_uuid,
        &context,
        None, // No idempotency key
    )
    .unwrap();

    let hash2 =
        Task::compute_identity_hash(IdentityStrategy::Strict, named_task_uuid, &context, None)
            .unwrap();

    // Same inputs should produce same hash
    assert_eq!(hash1, hash2);

    // Different context should produce different hash
    let different_context = Some(json!({"order_id": 99999}));
    let hash3 = Task::compute_identity_hash(
        IdentityStrategy::Strict,
        named_task_uuid,
        &different_context,
        None,
    )
    .unwrap();
    assert_ne!(hash1, hash3);
}

#[test]
fn test_compute_identity_hash_caller_provided_strategy() {
    let named_task_uuid = Uuid::now_v7();
    let context = Some(json!({"order_id": 12345}));

    // CALLER_PROVIDED strategy requires idempotency_key
    let result = Task::compute_identity_hash(
        IdentityStrategy::CallerProvided,
        named_task_uuid,
        &context,
        None, // Missing required key
    );

    // Should fail without idempotency_key
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("idempotency_key is required"));

    // With idempotency_key, should succeed
    let hash = Task::compute_identity_hash(
        IdentityStrategy::CallerProvided,
        named_task_uuid,
        &context,
        Some("my-unique-key"),
    )
    .unwrap();
    assert!(!hash.is_empty());

    // Same key should produce same hash
    let hash2 = Task::compute_identity_hash(
        IdentityStrategy::CallerProvided,
        named_task_uuid,
        &context,
        Some("my-unique-key"),
    )
    .unwrap();
    assert_eq!(hash, hash2);
}

#[test]
fn test_compute_identity_hash_always_unique_strategy() {
    let named_task_uuid = Uuid::now_v7();
    let context = Some(json!({"order_id": 12345}));

    // ALWAYS_UNIQUE strategy: generates unique hash each time
    let hash1 = Task::compute_identity_hash(
        IdentityStrategy::AlwaysUnique,
        named_task_uuid,
        &context,
        None,
    )
    .unwrap();

    let hash2 = Task::compute_identity_hash(
        IdentityStrategy::AlwaysUnique,
        named_task_uuid,
        &context,
        None,
    )
    .unwrap();

    // Each call should produce a different hash (UUIDv7)
    assert_ne!(hash1, hash2);

    // Hashes should be valid UUIDs
    assert!(Uuid::parse_str(&hash1).is_ok());
    assert!(Uuid::parse_str(&hash2).is_ok());
}

#[test]
fn test_compute_identity_hash_idempotency_key_override() {
    let named_task_uuid = Uuid::now_v7();
    let context = Some(json!({"order_id": 12345}));
    let idempotency_key = "custom-override-key";

    // With idempotency_key, all strategies should use the key
    let strict_hash = Task::compute_identity_hash(
        IdentityStrategy::Strict,
        named_task_uuid,
        &context,
        Some(idempotency_key),
    )
    .unwrap();

    let caller_hash = Task::compute_identity_hash(
        IdentityStrategy::CallerProvided,
        named_task_uuid,
        &context,
        Some(idempotency_key),
    )
    .unwrap();

    let unique_hash = Task::compute_identity_hash(
        IdentityStrategy::AlwaysUnique,
        named_task_uuid,
        &context,
        Some(idempotency_key),
    )
    .unwrap();

    // All should produce the same hash when idempotency_key is provided
    assert_eq!(strict_hash, caller_hash);
    assert_eq!(caller_hash, unique_hash);
}

#[test]
fn test_compute_identity_hash_includes_named_task_uuid() {
    let context = Some(json!({"order_id": 12345}));
    let idempotency_key = "same-key";

    // Different named tasks with same key should produce different hashes
    // (prevents cross-task collisions)
    let hash1 = Task::compute_identity_hash(
        IdentityStrategy::CallerProvided,
        Uuid::now_v7(),
        &context,
        Some(idempotency_key),
    )
    .unwrap();

    let hash2 = Task::compute_identity_hash(
        IdentityStrategy::CallerProvided,
        Uuid::now_v7(),
        &context,
        Some(idempotency_key),
    )
    .unwrap();

    // Different named_task_uuids should produce different hashes
    // even with the same idempotency key
    assert_ne!(hash1, hash2);
}
