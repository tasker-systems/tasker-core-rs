//! Tests for NamedTasksNamedStep model
//!
//! Tests the association between NamedTask and NamedStep with configuration options.

use sqlx::PgPool;
use tasker_shared::models::core::IdentityStrategy;
use tasker_shared::models::named_step::{NamedStep, NewNamedStep};
use tasker_shared::models::named_task::{NamedTask, NewNamedTask};
use tasker_shared::models::named_tasks_named_step::{NamedTasksNamedStep, NewNamedTasksNamedStep};
use tasker_shared::models::task_namespace::{NewTaskNamespace, TaskNamespace};

/// Helper to create prerequisite namespace + task + step for association tests
async fn create_test_dependencies(
    pool: &PgPool,
    suffix: &str,
) -> sqlx::Result<(TaskNamespace, NamedTask, NamedStep)> {
    let namespace = TaskNamespace::create(
        pool,
        NewTaskNamespace {
            name: format!("test_ns_{suffix}"),
            description: None,
        },
    )
    .await?;

    let named_task = NamedTask::create(
        pool,
        NewNamedTask {
            name: format!("test_task_{suffix}"),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    let named_step = NamedStep::create(
        pool,
        NewNamedStep {
            name: format!("test_step_{suffix}"),
            description: None,
        },
    )
    .await?;

    Ok((namespace, named_task, named_step))
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_tasks_named_step_crud(pool: PgPool) -> sqlx::Result<()> {
    let (_namespace, named_task, named_step) = create_test_dependencies(&pool, "crud").await?;

    // Test creation with custom configuration
    let new_association = NewNamedTasksNamedStep {
        named_task_uuid: named_task.named_task_uuid,
        named_step_uuid: named_step.named_step_uuid,
        default_retryable: Some(false),
        default_max_attempts: Some(5),
    };

    let association = NamedTasksNamedStep::create(&pool, new_association.clone()).await?;
    assert_eq!(association.named_task_uuid, named_task.named_task_uuid);
    assert!(!association.default_retryable);
    assert_eq!(association.default_max_attempts, 5);

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

    // Test finding non-retryable associations
    let non_retryable =
        NamedTasksNamedStep::find_non_retryable_by_task(&pool, named_task.named_task_uuid).await?;
    assert!(!non_retryable.is_empty());

    // Cleanup
    let deleted = NamedTasksNamedStep::delete(&pool, association.ntns_uuid).await?;
    assert!(deleted);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_default_values(pool: PgPool) -> sqlx::Result<()> {
    let (_namespace, named_task, named_step) = create_test_dependencies(&pool, "defaults").await?;

    // Test creation with default values (None fields)
    let new_association = NewNamedTasksNamedStep {
        named_task_uuid: named_task.named_task_uuid,
        named_step_uuid: named_step.named_step_uuid,
        default_retryable: None,    // Should default to true
        default_max_attempts: None, // Should default to 3
    };

    let association = NamedTasksNamedStep::create(&pool, new_association).await?;
    assert!(association.default_retryable);
    assert_eq!(association.default_max_attempts, 3);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ntns_list_by_task(pool: PgPool) -> sqlx::Result<()> {
    let (_namespace, named_task, _named_step) =
        create_test_dependencies(&pool, "list_by_task").await?;

    // Create multiple steps and associate them with the task
    let mut step_uuids = Vec::new();
    for i in 1..=3 {
        let step = NamedStep::create(
            &pool,
            NewNamedStep {
                name: format!("list_task_step_{i}"),
                description: None,
            },
        )
        .await?;
        step_uuids.push(step.named_step_uuid);

        NamedTasksNamedStep::create(
            &pool,
            NewNamedTasksNamedStep {
                named_task_uuid: named_task.named_task_uuid,
                named_step_uuid: step.named_step_uuid,
                default_retryable: Some(true),
                default_max_attempts: Some(3),
            },
        )
        .await?;
    }

    // Find all associations for this task
    let task_associations =
        NamedTasksNamedStep::find_by_task(&pool, named_task.named_task_uuid).await?;
    assert_eq!(task_associations.len(), 3);

    // All should reference our task
    for assoc in &task_associations {
        assert_eq!(assoc.named_task_uuid, named_task.named_task_uuid);
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ntns_list_by_step(pool: PgPool) -> sqlx::Result<()> {
    // Create a shared step used by multiple tasks
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_list_by_step".to_string(),
            description: None,
        },
    )
    .await?;

    let shared_step = NamedStep::create(
        &pool,
        NewNamedStep {
            name: "shared_step_for_tasks".to_string(),
            description: Some("Shared step".to_string()),
        },
    )
    .await?;

    // Create multiple tasks referencing the same step
    for i in 1..=3 {
        let task = NamedTask::create(
            &pool,
            NewNamedTask {
                name: format!("step_ref_task_{i}"),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_uuid: namespace.task_namespace_uuid,
                configuration: None,
                identity_strategy: IdentityStrategy::Strict,
            },
        )
        .await?;

        NamedTasksNamedStep::create(
            &pool,
            NewNamedTasksNamedStep {
                named_task_uuid: task.named_task_uuid,
                named_step_uuid: shared_step.named_step_uuid,
                default_retryable: Some(true),
                default_max_attempts: Some(3),
            },
        )
        .await?;
    }

    // Find all tasks using this step
    let step_associations =
        NamedTasksNamedStep::find_by_step(&pool, shared_step.named_step_uuid).await?;
    assert_eq!(step_associations.len(), 3);

    // All should reference our step
    for assoc in &step_associations {
        assert_eq!(assoc.named_step_uuid, shared_step.named_step_uuid);
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ntns_find_with_details(pool: PgPool) -> sqlx::Result<()> {
    let (_namespace, named_task, named_step) =
        create_test_dependencies(&pool, "with_details").await?;

    // Create an association
    NamedTasksNamedStep::create(
        &pool,
        NewNamedTasksNamedStep {
            named_task_uuid: named_task.named_task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            default_retryable: Some(true),
            default_max_attempts: Some(5),
        },
    )
    .await?;

    // Find with details (joins task and step names)
    let details = NamedTasksNamedStep::find_with_details(&pool, Some(100)).await?;
    assert!(!details.is_empty());

    // Find our specific association in the results
    let our_detail = details.iter().find(|d| {
        d.named_task_uuid == named_task.named_task_uuid
            && d.named_step_uuid == named_step.named_step_uuid
    });
    assert!(our_detail.is_some());
    let detail = our_detail.unwrap();
    assert_eq!(detail.task_name, format!("test_task_with_details"));
    assert_eq!(detail.step_name, format!("test_step_with_details"));
    assert!(detail.default_retryable);
    assert_eq!(detail.default_max_attempts, 5);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ntns_update(pool: PgPool) -> sqlx::Result<()> {
    let (_namespace, named_task, named_step) = create_test_dependencies(&pool, "update").await?;

    // Create initial association
    let association = NamedTasksNamedStep::create(
        &pool,
        NewNamedTasksNamedStep {
            named_task_uuid: named_task.named_task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            default_retryable: Some(true),
            default_max_attempts: Some(3),
        },
    )
    .await?;
    assert!(association.default_retryable);
    assert_eq!(association.default_max_attempts, 3);

    // Update the association
    let updated = NamedTasksNamedStep::update(
        &pool,
        association.ntns_uuid,
        NewNamedTasksNamedStep {
            named_task_uuid: named_task.named_task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            default_retryable: Some(false),
            default_max_attempts: Some(10),
        },
    )
    .await?;
    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert!(!updated.default_retryable);
    assert_eq!(updated.default_max_attempts, 10);

    // Verify persisted
    let found = NamedTasksNamedStep::find_by_id(&pool, association.ntns_uuid)
        .await?
        .unwrap();
    assert!(!found.default_retryable);
    assert_eq!(found.default_max_attempts, 10);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ntns_batch_create(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_batch_create".to_string(),
            description: None,
        },
    )
    .await?;

    let task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "batch_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    // Create multiple steps and associate them all with the task
    for i in 1..=5 {
        let step = NamedStep::create(
            &pool,
            NewNamedStep {
                name: format!("batch_step_{i}"),
                description: None,
            },
        )
        .await?;

        NamedTasksNamedStep::create(
            &pool,
            NewNamedTasksNamedStep {
                named_task_uuid: task.named_task_uuid,
                named_step_uuid: step.named_step_uuid,
                default_retryable: Some(i % 2 == 0), // Alternate retryable
                default_max_attempts: Some(i),
            },
        )
        .await?;
    }

    // Verify all 5 were created
    let associations = NamedTasksNamedStep::find_by_task(&pool, task.named_task_uuid).await?;
    assert_eq!(associations.len(), 5);

    // Verify the non-retryable ones
    let non_retryable =
        NamedTasksNamedStep::find_non_retryable_by_task(&pool, task.named_task_uuid).await?;
    // Steps 1, 3, 5 have default_retryable = false (odd i)
    assert_eq!(non_retryable.len(), 3);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ntns_unique_constraint(pool: PgPool) -> sqlx::Result<()> {
    let (_namespace, named_task, named_step) =
        create_test_dependencies(&pool, "unique_assoc").await?;

    // Create first association
    let new_association = NewNamedTasksNamedStep {
        named_task_uuid: named_task.named_task_uuid,
        named_step_uuid: named_step.named_step_uuid,
        default_retryable: Some(true),
        default_max_attempts: Some(3),
    };
    let _assoc = NamedTasksNamedStep::create(&pool, new_association.clone()).await?;

    // Try to create duplicate association with same task+step - should fail
    let duplicate = NamedTasksNamedStep::create(&pool, new_association).await;
    assert!(
        duplicate.is_err(),
        "Should not allow duplicate task+step combination"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ntns_list_all(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_list_all_assoc".to_string(),
            description: None,
        },
    )
    .await?;

    let task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "list_all_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    // Create several associations
    for i in 1..=4 {
        let step = NamedStep::create(
            &pool,
            NewNamedStep {
                name: format!("list_all_step_{i}"),
                description: None,
            },
        )
        .await?;

        NamedTasksNamedStep::create(
            &pool,
            NewNamedTasksNamedStep {
                named_task_uuid: task.named_task_uuid,
                named_step_uuid: step.named_step_uuid,
                default_retryable: Some(true),
                default_max_attempts: Some(3),
            },
        )
        .await?;
    }

    // List all with default pagination
    let all = NamedTasksNamedStep::list_all(&pool, None, None).await?;
    assert!(all.len() >= 4);

    // List with limit
    let limited = NamedTasksNamedStep::list_all(&pool, None, Some(2)).await?;
    assert_eq!(limited.len(), 2);

    // List with offset
    let offset = NamedTasksNamedStep::list_all(&pool, Some(1), Some(2)).await?;
    assert_eq!(offset.len(), 2);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ntns_delete_nonexistent(pool: PgPool) -> sqlx::Result<()> {
    // Deleting a non-existent association should return false
    let deleted = NamedTasksNamedStep::delete(&pool, uuid::Uuid::now_v7()).await?;
    assert!(!deleted);

    Ok(())
}
