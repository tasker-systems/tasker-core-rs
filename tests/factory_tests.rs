//! Integration tests for the factory system

mod factories;

// Import factories directly to prevent clippy from removing them
use factories::base::SqlxFactory; // Trait providing create() method
use factories::core::{TaskFactory, WorkflowStepFactory};
use factories::foundation::{DependentSystemFactory, TaskNamespaceFactory};
use sqlx::PgPool;

#[sqlx::test]
async fn test_namespace_factory_basic(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = TaskNamespaceFactory::new()
        .with_name("test_namespace")
        .with_description("Test namespace")
        .create(&pool)
        .await?;

    assert_eq!(namespace.name, "test_namespace");
    assert_eq!(namespace.description, Some("Test namespace".to_string()));

    Ok(())
}

#[sqlx::test]
async fn test_system_factory_basic(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let system = DependentSystemFactory::new()
        .with_name("test_api")
        .with_description("Test API system")
        .create(&pool)
        .await?;

    assert_eq!(system.name, "test_api");
    assert_eq!(system.description, Some("Test API system".to_string()));

    Ok(())
}

#[sqlx::test]
async fn test_common_foundations_creation(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let namespaces = TaskNamespaceFactory::create_common_namespaces(&pool).await?;
    let systems = DependentSystemFactory::create_common_systems(&pool).await?;

    assert_eq!(namespaces.len(), 5);
    assert_eq!(systems.len(), 4);

    // Test find_or_create pattern
    let default_ns = TaskNamespaceFactory::new()
        .with_name("default")
        .find_or_create(&pool)
        .await?;

    // Should be the same as the one from common creation
    assert_eq!(
        default_ns.task_namespace_uuid,
        namespaces[0].task_namespace_uuid
    );

    Ok(())
}

#[sqlx::test]
async fn test_task_factory_basic(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new()
        .with_initiator("test_user")
        .create(&pool)
        .await?;

    assert_eq!(task.initiator, Some("test_user".to_string()));
    assert!(!task.complete);
    assert!(task.context.is_some());

    Ok(())
}

#[sqlx::test]
async fn test_workflow_step_factory(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    let step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .api_call_step()
        .create(&pool)
        .await?;

    assert_eq!(step.task_uuid, task.task_uuid);
    assert!(step.inputs.is_some());

    Ok(())
}

#[sqlx::test]
async fn test_complex_workflow_creation(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().complex_workflow().create(&pool).await?;

    let context = task.context.unwrap();
    assert_eq!(context["order_id"], 12345);
    assert_eq!(context["user_id"], 67890);
    assert!(context["items"].is_array());

    Ok(())
}
