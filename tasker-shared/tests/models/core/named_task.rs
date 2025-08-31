use sqlx::PgPool;
use tasker_shared::models::named_task::{NamedTask, NewNamedTask};
use tasker_shared::models::task_namespace::{NewTaskNamespace, TaskNamespace};

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
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

    // Delete namespace
    TaskNamespace::delete(&pool, namespace.task_namespace_uuid).await?;

    Ok(())
}
