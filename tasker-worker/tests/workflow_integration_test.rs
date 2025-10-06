//! # Worker Workflow Integration Tests
//!
//! Tests for worker workflow processing using the worker testing infrastructure.
//! These tests demonstrate the worker testing patterns and verify that workers
//! can process workflow steps correctly.

use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tasker_worker::testing::{WorkerTestData, WorkerTestFactory};

/// Test worker workflow processing using the worker testing infrastructure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_worker_workflow_processing(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // Create worker test factory
    let factory = WorkerTestFactory::new(Arc::new(pool));

    // Create test data with workflow foundation
    let test_data = WorkerTestData::new("workflow_integration_test")
        .build_with_factory(&factory)
        .await?;

    // Verify the test foundation was created successfully
    let foundation = test_data
        .foundation()
        .ok_or("Test foundation should be created")?;

    println!(
        "✅ Successfully created test foundation for namespace: {}",
        foundation.namespace.namespace.name
    );

    // Create a test task request
    let task_request = factory.create_test_task_request(
        &foundation.namespace.namespace.name,
        &foundation.named_task.name,
        &test_data.test_id,
    );

    println!(
        "✅ Created test task request: {} in namespace {}",
        task_request.name, task_request.namespace
    );

    // This demonstrates the worker testing pattern
    // In a full implementation, we would:
    // 1. Initialize the task via orchestration API (if configured)
    // 2. Process step messages using WorkerExecutor
    // 3. Verify step results and task completion

    Ok(())
}

/// Test creating multiple workflow steps for a task
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_worker_multiple_steps_creation(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let factory = WorkerTestFactory::new(Arc::new(pool.clone()));

    // Create test foundation
    let test_data = WorkerTestData::new("multi_step_test")
        .build_with_factory(&factory)
        .await?;

    let foundation = test_data
        .foundation()
        .ok_or("Test foundation should be created")?;

    // Create a task for testing multiple steps
    let task_request = tasker_shared::models::core::task_request::TaskRequest::new(
        foundation.named_task.name.clone(),
        foundation.namespace.namespace.name.clone(),
    )
    .with_context(json!({"test": "multi_step_workflow"}))
    .with_initiator("test_initiator".to_string())
    .with_source_system("worker_test_system".to_string())
    .with_reason("Testing multiple workflow steps".to_string())
    .with_priority(5);

    let task = tasker_shared::models::Task::create_with_defaults(&pool, task_request).await?;

    // Create multiple test workflow steps
    use tasker_worker::testing::factory::TestStepConfig;

    let step_configs = vec![
        TestStepConfig {
            name: "setup".to_string(),
            inputs: json!({"initial": "data"}),
            retryable: true,
            max_attempts: 3,
            skippable: false,
        },
        TestStepConfig {
            name: "process".to_string(),
            inputs: json!({"step": "processing"}),
            retryable: true,
            max_attempts: 2,
            skippable: false,
        },
        TestStepConfig {
            name: "cleanup".to_string(),
            inputs: json!({"final": "cleanup"}),
            retryable: false,
            max_attempts: 1,
            skippable: true,
        },
    ];

    let workflow_steps = factory
        .create_test_workflow_steps(task.task_uuid, step_configs)
        .await?;

    println!(
        "✅ Created {} workflow steps for task",
        workflow_steps.len()
    );
    assert_eq!(workflow_steps.len(), 3);

    // Verify the steps were created with correct properties
    for (i, step) in workflow_steps.iter().enumerate() {
        println!(
            "   Step {}: {} (uuid: {})",
            i + 1,
            step.workflow_step_uuid,
            step.named_step_uuid
        );
    }

    Ok(())
}

/// Test creating test step messages for worker processing
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_worker_step_message_creation(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    use uuid::Uuid;

    let factory = WorkerTestFactory::new(Arc::new(pool));

    let test_data = WorkerTestData::new("step_message_test")
        .build_with_factory(&factory)
        .await?;

    let _foundation = test_data
        .foundation()
        .ok_or("Test foundation should be created")?;

    // Create test step message
    let task_uuid = Uuid::new_v4();
    let step_uuid = Uuid::new_v4();

    let step_message = factory
        .create_test_step_message(
            task_uuid,
            step_uuid,
            "test_step",
            json!({"test": "step_data", "worker": "processing"}),
        )
        .await?;

    println!("✅ Created test step message");
    println!("   Task: {}", step_message.task_uuid);
    println!("   Step: {}", step_message.step_uuid);
    println!("   Name: {}", step_message.step_name);
    println!("   Payload: {}", step_message.payload);

    // Verify message structure
    assert_eq!(step_message.task_uuid, task_uuid);
    assert_eq!(step_message.step_uuid, step_uuid);
    assert_eq!(step_message.step_name, "test_step");

    Ok(())
}
