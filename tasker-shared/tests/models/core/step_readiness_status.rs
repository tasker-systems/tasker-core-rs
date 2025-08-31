//! Step Readiness Status Tests
//!
//! Tests for the StepReadinessStatus model using SQLx native testing

use sqlx::PgPool;
use tasker_shared::models::named_task::{NamedTask, NewNamedTask};
use tasker_shared::models::orchestration::step_readiness_status::StepReadinessStatus;
use tasker_shared::models::task::{NewTask, Task};
use tasker_shared::models::task_namespace::{NewTaskNamespace, TaskNamespace};

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_get_step_readiness_status(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies first
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: format!(
                "test_namespace_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            description: None,
        },
    )
    .await
    .expect("Failed to create namespace");

    let named_task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: format!(
                "test_task_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
        },
    )
    .await
    .expect("Failed to create named task");

    let task = Task::create(
        &pool,
        NewTask {
            named_task_uuid: named_task.named_task_uuid,
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
            identity_hash: format!(
                "test_hash_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
        },
    )
    .await
    .expect("Failed to create task");

    // Test getting step readiness status (might be empty for a new task)
    let _statuses = StepReadinessStatus::get_for_task(&pool, task.task_uuid)
        .await
        .expect("Failed to get step readiness status");

    // Basic test - should return without error
    // Function should return without error (could be empty if no steps exist)

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_readiness_summary(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: format!(
                "test_namespace_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            description: None,
        },
    )
    .await
    .expect("Failed to create namespace");

    let named_task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: format!(
                "test_task_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
        },
    )
    .await
    .expect("Failed to create named task");

    let task = Task::create(
        &pool,
        NewTask {
            named_task_uuid: named_task.named_task_uuid,
            identity_hash: format!(
                "test_hash_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
        },
    )
    .await
    .expect("Failed to create task");

    let summary = StepReadinessStatus::get_readiness_summary(&pool, task.task_uuid)
        .await
        .expect("Failed to get readiness summary");

    // Basic validation
    // All step counts should be non-negative by definition
    assert!(summary.ready_steps <= summary.total_steps);
    assert!(summary.blocked_steps <= summary.total_steps);

    Ok(())
}
