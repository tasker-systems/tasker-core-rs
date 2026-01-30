//! Task Execution Context Tests
//!
//! Tests for the TaskExecutionContext model using SQLx native testing

use bigdecimal::BigDecimal;
use sqlx::PgPool;
use tasker_shared::models::core::IdentityStrategy;
use tasker_shared::models::named_task::{NamedTask, NewNamedTask};
use tasker_shared::models::orchestration::task_execution_context::TaskExecutionContext;
use tasker_shared::models::task::{NewTask, Task};
use tasker_shared::models::task_namespace::{NewTaskNamespace, TaskNamespace};

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_task_execution_context(pool: PgPool) -> sqlx::Result<()> {
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
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await
    .expect("Failed to create named task");

    let task = Task::create(
        &pool,
        NewTask {
            named_task_uuid: named_task.named_task_uuid,
            requested_at: None,
            initiator: Some("test".to_string()),
            source_system: Some("test".to_string()),
            reason: Some("test".to_string()),

            tags: None,
            context: None,
            identity_hash: format!(
                "test_hash_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            priority: None,
            correlation_id: uuid::Uuid::now_v7(),
            parent_correlation_id: None,
        },
    )
    .await
    .expect("Failed to create task");

    // Test getting execution context
    let context = TaskExecutionContext::get_for_task(&pool, task.task_uuid)
        .await
        .expect("Failed to get execution context");

    if let Some(ctx) = context {
        assert_eq!(ctx.task_uuid, task.task_uuid);
        assert_eq!(ctx.named_task_uuid, task.named_task_uuid);
        // Task with no steps should have zero counts
        assert_eq!(ctx.total_steps, 0);
        assert_eq!(ctx.completed_steps, 0);
        assert_eq!(ctx.ready_steps, 0);
    }

    // Test batch operation
    let contexts = TaskExecutionContext::get_for_tasks(&pool, &[task.task_uuid])
        .await
        .expect("Failed to get batch execution contexts");

    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].task_uuid, task.task_uuid);

    Ok(())
}

#[test]
fn test_helper_methods() {
    use tasker_shared::models::orchestration::execution_status::ExecutionStatus;

    let context = TaskExecutionContext {
        task_uuid: uuid::Uuid::now_v7(),
        named_task_uuid: uuid::Uuid::now_v7(),
        status: "processing".to_string(),
        total_steps: 10,
        pending_steps: 2,
        in_progress_steps: 3,
        completed_steps: 4,
        failed_steps: 1,
        ready_steps: 2,
        execution_status: ExecutionStatus::HasReadySteps,
        recommended_action: None,
        completion_percentage: BigDecimal::from(40),
        health_status: "healthy".to_string(),
        enqueued_steps: 0,
    };

    assert!(context.has_ready_steps());
    assert!(context.is_processing());
    assert!(!context.is_complete());
    assert!(context.has_failures());
    assert_eq!(context.completion_ratio(), 0.4);
    assert_eq!(context.status_summary(), "Processing");
}
