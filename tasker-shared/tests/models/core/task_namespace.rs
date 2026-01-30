//! Task Namespace Model Tests
//!
//! Tests for the TaskNamespace model using SQLx native testing

use sqlx::PgPool;
use tasker_shared::models::task_namespace::{NewTaskNamespace, TaskNamespace};

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
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
    let found = TaskNamespace::find_by_uuid(&pool, created.task_namespace_uuid)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(found.task_namespace_uuid, created.task_namespace_uuid);

    // Test find by name
    let found_by_name = TaskNamespace::find_by_name(&pool, &unique_name)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(
        found_by_name.task_namespace_uuid,
        created.task_namespace_uuid
    );

    // Test update
    let updated_name = "updated_test_namespace".to_string();
    let updated = TaskNamespace::update(
        &pool,
        created.task_namespace_uuid,
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
    let deleted = TaskNamespace::delete(&pool, created.task_namespace_uuid).await?;
    assert!(deleted);

    // Verify deletion
    let not_found = TaskNamespace::find_by_uuid(&pool, created.task_namespace_uuid).await?;
    assert!(not_found.is_none());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_namespace_list_all(pool: PgPool) -> sqlx::Result<()> {
    // Create several namespaces
    for i in 1..=4 {
        TaskNamespace::create(
            &pool,
            NewTaskNamespace {
                name: format!("list_ns_{i:02}"),
                description: Some(format!("Namespace {i}")),
            },
        )
        .await?;
    }

    // List all
    let all_namespaces = TaskNamespace::list_all(&pool).await?;
    assert!(all_namespaces.len() >= 4);

    // Verify ordering is by name
    for window in all_namespaces.windows(2) {
        assert!(
            window[0].name <= window[1].name,
            "Namespaces should be ordered by name"
        );
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_namespace_find_or_create(pool: PgPool) -> sqlx::Result<()> {
    let ns_name = "idempotent_namespace";

    // First call should create
    let created = TaskNamespace::find_or_create(&pool, ns_name).await?;
    assert_eq!(created.name, ns_name);
    assert!(created
        .description
        .as_ref()
        .unwrap()
        .contains("Auto-created"));

    // Second call should find existing
    let found = TaskNamespace::find_or_create(&pool, ns_name).await?;
    assert_eq!(found.task_namespace_uuid, created.task_namespace_uuid);

    // Different name should create new
    let other = TaskNamespace::find_or_create(&pool, "other_namespace").await?;
    assert_ne!(other.task_namespace_uuid, created.task_namespace_uuid);
    assert_eq!(other.name, "other_namespace");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_namespace_with_tasks(pool: PgPool) -> sqlx::Result<()> {
    // Create namespace and add tasks
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_with_tasks".to_string(),
            description: Some("Has tasks".to_string()),
        },
    )
    .await?;

    // Create tasks in the namespace
    for i in 1..=3 {
        tasker_shared::models::named_task::NamedTask::create(
            &pool,
            tasker_shared::models::named_task::NewNamedTask {
                name: format!("ns_task_{i}"),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_uuid: namespace.task_namespace_uuid,
                configuration: None,
                identity_strategy: tasker_shared::models::core::IdentityStrategy::Strict,
            },
        )
        .await?;
    }

    // Verify via get_namespace_info_with_handler_count
    let info = TaskNamespace::get_namespace_info_with_handler_count(&pool).await?;
    let our_ns = info
        .iter()
        .find(|(ns, _)| ns.task_namespace_uuid == namespace.task_namespace_uuid);
    assert!(our_ns.is_some());
    let (_, count) = our_ns.unwrap();
    assert_eq!(*count, 3);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_namespace_handler_count_empty(pool: PgPool) -> sqlx::Result<()> {
    // Create a namespace with no tasks
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_empty_count".to_string(),
            description: None,
        },
    )
    .await?;

    let info = TaskNamespace::get_namespace_info_with_handler_count(&pool).await?;
    let our_ns = info
        .iter()
        .find(|(ns, _)| ns.task_namespace_uuid == namespace.task_namespace_uuid);
    assert!(our_ns.is_some());
    let (_, count) = our_ns.unwrap();
    assert_eq!(*count, 0);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_namespace_update_partial(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "partial_update_ns".to_string(),
            description: Some("Original description".to_string()),
        },
    )
    .await?;

    // Update only the name (description = None means keep existing via COALESCE)
    let updated_name = TaskNamespace::update(
        &pool,
        namespace.task_namespace_uuid,
        Some("renamed_ns".to_string()),
        None,
    )
    .await?;
    assert_eq!(updated_name.name, "renamed_ns");
    assert_eq!(
        updated_name.description,
        Some("Original description".to_string())
    );

    // Update only the description (name = None means keep existing via COALESCE)
    let updated_desc = TaskNamespace::update(
        &pool,
        namespace.task_namespace_uuid,
        None,
        Some("New description".to_string()),
    )
    .await?;
    assert_eq!(updated_desc.name, "renamed_ns");
    assert_eq!(
        updated_desc.description,
        Some("New description".to_string())
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_namespace_unique_constraint(pool: PgPool) -> sqlx::Result<()> {
    let ns_name = "unique_constraint_ns";

    // Create first namespace
    TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: ns_name.to_string(),
            description: None,
        },
    )
    .await?;

    // Try to create duplicate - should fail due to unique constraint
    let duplicate_result = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: ns_name.to_string(),
            description: Some("Different description".to_string()),
        },
    )
    .await;
    assert!(
        duplicate_result.is_err(),
        "Should not allow duplicate namespace names"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_namespace_is_name_unique_with_exclude(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "exclude_check_ns".to_string(),
            description: None,
        },
    )
    .await?;

    // Without exclude: should not be unique (it exists)
    let not_unique = TaskNamespace::is_name_unique(&pool, "exclude_check_ns", None).await?;
    assert!(!not_unique);

    // With exclude for self: should be unique (excluding itself)
    let unique_excluding_self = TaskNamespace::is_name_unique(
        &pool,
        "exclude_check_ns",
        Some(namespace.task_namespace_uuid),
    )
    .await?;
    assert!(unique_excluding_self);

    // With exclude for different UUID: should not be unique
    let not_unique_other =
        TaskNamespace::is_name_unique(&pool, "exclude_check_ns", Some(uuid::Uuid::now_v7()))
            .await?;
    assert!(!not_unique_other);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_namespace_delete_nonexistent(pool: PgPool) -> sqlx::Result<()> {
    // Deleting a non-existent namespace should return false
    let deleted = TaskNamespace::delete(&pool, uuid::Uuid::now_v7()).await?;
    assert!(!deleted);

    Ok(())
}
