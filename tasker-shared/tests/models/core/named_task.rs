use sqlx::PgPool;
use tasker_shared::models::core::IdentityStrategy;
use tasker_shared::models::named_task::{NamedTask, NewNamedTask};
use tasker_shared::models::task_namespace::{NewTaskNamespace, TaskNamespace};

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create a namespace first
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "test_namespace_named_task_crud".to_string(),
            description: None,
        },
    )
    .await?;

    // Test creation
    let new_task = NewNamedTask {
        name: "test_task_named_task_crud".to_string(),
        version: Some("1.0.0".to_string()),
        description: Some("Test task description".to_string()),
        task_namespace_uuid: namespace.task_namespace_uuid,
        configuration: Some(serde_json::json!({"timeout": 300})),
        identity_strategy: IdentityStrategy::Strict,
    };

    let created = NamedTask::create(&pool, new_task).await?;
    assert_eq!(created.name, "test_task_named_task_crud");
    assert_eq!(created.version, "1.0.0");

    // Test find by ID
    let found = NamedTask::find_by_uuid(&pool, created.named_task_uuid)
        .await?
        .expect("Task not found");
    assert_eq!(found.named_task_uuid, created.named_task_uuid);

    // Test find by name/version/namespace
    let found_by_nvn = NamedTask::find_by_name_version_namespace(
        &pool,
        &created.name,
        "1.0.0",
        namespace.task_namespace_uuid,
    )
    .await?
    .expect("Task not found by nvn");
    assert_eq!(found_by_nvn.named_task_uuid, created.named_task_uuid);

    // Test version uniqueness
    let is_unique = NamedTask::is_version_unique(
        &pool,
        &created.name,
        "2.0.0",
        namespace.task_namespace_uuid,
        None,
    )
    .await?;
    assert!(is_unique);

    let is_not_unique = NamedTask::is_version_unique(
        &pool,
        &created.name,
        "1.0.0",
        namespace.task_namespace_uuid,
        None,
    )
    .await?;
    assert!(!is_not_unique);

    // Test delegation methods
    assert_eq!(
        created.get_task_uuidentifier(),
        format!("{}:1.0.0", created.name)
    );

    // Test deletion
    let deleted = NamedTask::delete(&pool, created.named_task_uuid).await?;
    assert!(deleted);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_list_by_namespace(pool: PgPool) -> sqlx::Result<()> {
    // Create two namespaces
    let ns1 = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_list_test_1".to_string(),
            description: None,
        },
    )
    .await?;

    let ns2 = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_list_test_2".to_string(),
            description: None,
        },
    )
    .await?;

    // Create tasks in ns1
    for i in 1..=3 {
        NamedTask::create(
            &pool,
            NewNamedTask {
                name: format!("ns1_task_{i}"),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_uuid: ns1.task_namespace_uuid,
                configuration: None,
                identity_strategy: IdentityStrategy::Strict,
            },
        )
        .await?;
    }

    // Create tasks in ns2
    for i in 1..=2 {
        NamedTask::create(
            &pool,
            NewNamedTask {
                name: format!("ns2_task_{i}"),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_uuid: ns2.task_namespace_uuid,
                configuration: None,
                identity_strategy: IdentityStrategy::Strict,
            },
        )
        .await?;
    }

    // List tasks in ns1 - should have 3
    let ns1_tasks = NamedTask::list_by_namespace(&pool, ns1.task_namespace_uuid).await?;
    assert_eq!(ns1_tasks.len(), 3);

    // List tasks in ns2 - should have 2
    let ns2_tasks = NamedTask::list_by_namespace(&pool, ns2.task_namespace_uuid).await?;
    assert_eq!(ns2_tasks.len(), 2);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_find_by_name_version_namespace(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_find_nvn".to_string(),
            description: None,
        },
    )
    .await?;

    let task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "lookup_task".to_string(),
            version: Some("2.0.0".to_string()),
            description: Some("Task for lookup test".to_string()),
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: Some(serde_json::json!({"key": "value"})),
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    // Find with correct name, version, namespace
    let found = NamedTask::find_by_name_version_namespace(
        &pool,
        "lookup_task",
        "2.0.0",
        namespace.task_namespace_uuid,
    )
    .await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().named_task_uuid, task.named_task_uuid);

    // Wrong version
    let not_found = NamedTask::find_by_name_version_namespace(
        &pool,
        "lookup_task",
        "9.9.9",
        namespace.task_namespace_uuid,
    )
    .await?;
    assert!(not_found.is_none());

    // Wrong name
    let not_found = NamedTask::find_by_name_version_namespace(
        &pool,
        "wrong_name",
        "2.0.0",
        namespace.task_namespace_uuid,
    )
    .await?;
    assert!(not_found.is_none());

    // Wrong namespace
    let not_found = NamedTask::find_by_name_version_namespace(
        &pool,
        "lookup_task",
        "2.0.0",
        uuid::Uuid::now_v7(),
    )
    .await?;
    assert!(not_found.is_none());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_find_or_create(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_find_or_create".to_string(),
            description: None,
        },
    )
    .await?;

    // First call should create
    let task = NamedTask::find_or_create_by_name_version_namespace(
        &pool,
        "idempotent_task",
        "1.0.0",
        namespace.task_namespace_uuid,
    )
    .await?;
    assert_eq!(task.name, "idempotent_task");
    assert_eq!(task.version, "1.0.0");

    // Second call should find existing
    let found = NamedTask::find_or_create_by_name_version_namespace(
        &pool,
        "idempotent_task",
        "1.0.0",
        namespace.task_namespace_uuid,
    )
    .await?;
    assert_eq!(found.named_task_uuid, task.named_task_uuid);

    // Different version should create a new task
    let v2 = NamedTask::find_or_create_by_name_version_namespace(
        &pool,
        "idempotent_task",
        "2.0.0",
        namespace.task_namespace_uuid,
    )
    .await?;
    assert_ne!(v2.named_task_uuid, task.named_task_uuid);
    assert_eq!(v2.version, "2.0.0");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_update(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_update_test".to_string(),
            description: None,
        },
    )
    .await?;

    let task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "task_to_update".to_string(),
            version: Some("1.0.0".to_string()),
            description: Some("Original".to_string()),
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: Some(serde_json::json!({"old": true})),
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    // Update description only
    let updated = NamedTask::update(
        &pool,
        task.named_task_uuid,
        Some("Updated description".to_string()),
        None,
    )
    .await?;
    assert_eq!(updated.description, Some("Updated description".to_string()));
    // Configuration should remain unchanged
    assert_eq!(
        updated.configuration,
        Some(serde_json::json!({"old": true}))
    );

    // Update configuration only
    let updated = NamedTask::update(
        &pool,
        task.named_task_uuid,
        None,
        Some(serde_json::json!({"new": true, "version": 2})),
    )
    .await?;
    assert_eq!(
        updated.configuration,
        Some(serde_json::json!({"new": true, "version": 2}))
    );
    // Description should remain unchanged
    assert_eq!(updated.description, Some("Updated description".to_string()));

    // Update both
    let updated = NamedTask::update(
        &pool,
        task.named_task_uuid,
        Some("Final description".to_string()),
        Some(serde_json::json!({"final": true})),
    )
    .await?;
    assert_eq!(updated.description, Some("Final description".to_string()));
    assert_eq!(
        updated.configuration,
        Some(serde_json::json!({"final": true}))
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_versioning(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_version_test".to_string(),
            description: None,
        },
    )
    .await?;

    // Create multiple versions of the same task
    let _v1 = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "versioned_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: Some("Version 1".to_string()),
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    let _v2 = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "versioned_task".to_string(),
            version: Some("2.0.0".to_string()),
            description: Some("Version 2".to_string()),
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    let v3 = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "versioned_task".to_string(),
            version: Some("3.0.0".to_string()),
            description: Some("Version 3".to_string()),
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    // List all versions
    let versions = NamedTask::list_versions_by_name_namespace(
        &pool,
        "versioned_task",
        namespace.task_namespace_uuid,
    )
    .await?;
    assert_eq!(versions.len(), 3);

    // Find latest version (should be v3 by created_at DESC)
    let latest = NamedTask::find_latest_by_name_namespace(
        &pool,
        "versioned_task",
        namespace.task_namespace_uuid,
    )
    .await?;
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().named_task_uuid, v3.named_task_uuid);

    // List latest by namespace
    let latest_tasks =
        NamedTask::list_latest_by_namespace(&pool, namespace.task_namespace_uuid).await?;
    assert_eq!(latest_tasks.len(), 1);
    assert_eq!(latest_tasks[0].version, "3.0.0");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_with_steps(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_with_steps".to_string(),
            description: None,
        },
    )
    .await?;

    let task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "task_with_steps".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    // Create steps
    let step1 = tasker_shared::models::named_step::NamedStep::create(
        &pool,
        tasker_shared::models::named_step::NewNamedStep {
            name: "with_steps_step_1".to_string(),
            description: Some("First step".to_string()),
        },
    )
    .await?;

    let step2 = tasker_shared::models::named_step::NamedStep::create(
        &pool,
        tasker_shared::models::named_step::NewNamedStep {
            name: "with_steps_step_2".to_string(),
            description: Some("Second step".to_string()),
        },
    )
    .await?;

    // Associate steps with task
    let assoc1 = task
        .add_step_association(&pool, step1.named_step_uuid)
        .await?;
    assert_eq!(assoc1.named_task_uuid, task.named_task_uuid);
    assert_eq!(assoc1.named_step_uuid, step1.named_step_uuid);

    let _assoc2 = task
        .add_step_association(&pool, step2.named_step_uuid)
        .await?;

    // Get step associations
    let associations = task.get_step_associations(&pool).await?;
    assert_eq!(associations.len(), 2);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_identity_strategy_update(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_identity_strategy".to_string(),
            description: None,
        },
    )
    .await?;

    let task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "identity_strategy_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;
    assert_eq!(task.identity_strategy, IdentityStrategy::Strict);

    // Update identity strategy
    let updated = NamedTask::update_identity_strategy(
        &pool,
        task.named_task_uuid,
        IdentityStrategy::AlwaysUnique,
    )
    .await?;
    assert_eq!(updated.identity_strategy, IdentityStrategy::AlwaysUnique);

    // Verify persisted
    let found = NamedTask::find_by_uuid(&pool, task.named_task_uuid)
        .await?
        .unwrap();
    assert_eq!(found.identity_strategy, IdentityStrategy::AlwaysUnique);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_list_by_namespace_name(pool: PgPool) -> sqlx::Result<()> {
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "ns_by_name_lookup".to_string(),
            description: None,
        },
    )
    .await?;

    // Create tasks in the namespace
    for i in 1..=3 {
        NamedTask::create(
            &pool,
            NewNamedTask {
                name: format!("ns_name_task_{i}"),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_uuid: namespace.task_namespace_uuid,
                configuration: None,
                identity_strategy: IdentityStrategy::Strict,
            },
        )
        .await?;
    }

    // List tasks by namespace name
    let tasks = NamedTask::list_by_namespace_name(&pool, "ns_by_name_lookup").await?;
    assert_eq!(tasks.len(), 3);

    // Non-existent namespace name should return empty
    let empty_tasks = NamedTask::list_by_namespace_name(&pool, "nonexistent_ns").await?;
    assert!(empty_tasks.is_empty());

    Ok(())
}
