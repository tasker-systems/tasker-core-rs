//! Tests for NamedTasksNamedStep model
//!
//! Tests the association between NamedTask and NamedStep with configuration options.

use sqlx::PgPool;
use tasker_shared::models::named_tasks_named_step::{NamedTasksNamedStep, NewNamedTasksNamedStep};

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_named_tasks_named_step_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies
    let namespace = tasker_shared::models::task_namespace::TaskNamespace::create(
        &pool,
        tasker_shared::models::task_namespace::NewTaskNamespace {
            name: "test_namespace_crud".to_string(),
            description: None,
        },
    )
    .await?;

    let named_task = tasker_shared::models::named_task::NamedTask::create(
        &pool,
        tasker_shared::models::named_task::NewNamedTask {
            name: "test_task_crud".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
        },
    )
    .await?;

    let dependent_system = tasker_shared::models::dependent_system::DependentSystem::create(
        &pool,
        tasker_shared::models::dependent_system::NewDependentSystem {
            name: "test_system_crud".to_string(),
            description: None,
        },
    )
    .await?;

    let named_step = tasker_shared::models::named_step::NamedStep::create(
        &pool,
        tasker_shared::models::named_step::NewNamedStep {
            name: "test_step_crud".to_string(),
            description: None,
            dependent_system_uuid: dependent_system.dependent_system_uuid,
        },
    )
    .await?;

    // Test creation with custom configuration
    let new_association = NewNamedTasksNamedStep {
        named_task_uuid: named_task.named_task_uuid,
        named_step_uuid: named_step.named_step_uuid,
        skippable: Some(true),
        default_retryable: Some(false),
        default_retry_limit: Some(5),
    };

    let association = NamedTasksNamedStep::create(&pool, new_association.clone()).await?;
    assert_eq!(association.named_task_uuid, named_task.named_task_uuid);
    assert!(association.skippable);
    assert!(!association.default_retryable);
    assert_eq!(association.default_retry_limit, 5);

    // Test find by ID
    let found = NamedTasksNamedStep::find_by_id(&pool, association.ntns_uuid).await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().named_step_uuid, named_step.named_step_uuid);

    // Test find by task and step
    let found_specific = NamedTasksNamedStep::find_by_task_and_step(
        &pool,
        named_task.named_task_uuid,
        named_step.named_step_uuid,
    )
    .await?;
    assert!(found_specific.is_some());

    // Test find_or_create (should find existing)
    let found_or_created = NamedTasksNamedStep::find_or_create(&pool, new_association).await?;
    assert_eq!(found_or_created.ntns_uuid, association.ntns_uuid);

    // Test finding skippable associations
    let skippable =
        NamedTasksNamedStep::find_skippable_by_task(&pool, named_task.named_task_uuid).await?;
    assert!(!skippable.is_empty());

    // Test finding non-retryable associations
    let non_retryable =
        NamedTasksNamedStep::find_non_retryable_by_task(&pool, named_task.named_task_uuid).await?;
    assert!(!non_retryable.is_empty());

    // Cleanup
    let deleted = NamedTasksNamedStep::delete(&pool, association.ntns_uuid).await?;
    assert!(deleted);

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_default_values(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies with minimal data
    let namespace = tasker_shared::models::task_namespace::TaskNamespace::create(
        &pool,
        tasker_shared::models::task_namespace::NewTaskNamespace {
            name: "test_namespace_defaults".to_string(),
            description: None,
        },
    )
    .await?;

    let named_task = tasker_shared::models::named_task::NamedTask::create(
        &pool,
        tasker_shared::models::named_task::NewNamedTask {
            name: "test_task_defaults".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
        },
    )
    .await?;

    let dependent_system = tasker_shared::models::dependent_system::DependentSystem::create(
        &pool,
        tasker_shared::models::dependent_system::NewDependentSystem {
            name: "test_system_defaults".to_string(),
            description: None,
        },
    )
    .await?;

    let named_step = tasker_shared::models::named_step::NamedStep::create(
        &pool,
        tasker_shared::models::named_step::NewNamedStep {
            name: "test_step_defaults".to_string(),
            description: None,
            dependent_system_uuid: dependent_system.dependent_system_uuid,
        },
    )
    .await?;

    // Test creation with default values
    let new_association = NewNamedTasksNamedStep {
        named_task_uuid: named_task.named_task_uuid,
        named_step_uuid: named_step.named_step_uuid,
        skippable: None,           // Should default to false
        default_retryable: None,   // Should default to true
        default_retry_limit: None, // Should default to 3
    };

    let association = NamedTasksNamedStep::create(&pool, new_association).await?;
    assert!(!association.skippable);
    assert!(association.default_retryable);
    assert_eq!(association.default_retry_limit, 3);

    // Cleanup
    NamedTasksNamedStep::delete(&pool, association.ntns_uuid).await?;

    Ok(())
}
