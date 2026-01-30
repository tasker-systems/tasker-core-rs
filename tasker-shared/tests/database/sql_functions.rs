//! SQL Function Integration Tests
//!
//! Tests for the SQL function execution layer, covering all function categories
//! and ensuring proper integration with the PostgreSQL database.

use tasker_shared::database::sql_functions::*;
use tasker_shared::models::orchestration::execution_status::{ExecutionStatus, RecommendedAction};
use uuid::Uuid;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_readiness_blocking_reason(_pool: sqlx::PgPool) -> sqlx::Result<()> {
    let named_step_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();
    let workflow_step_uuid = Uuid::now_v7();
    let step = StepReadinessStatus {
        workflow_step_uuid,
        task_uuid,
        named_step_uuid,
        name: "test".to_string(),
        current_state: "pending".to_string(),
        dependencies_satisfied: false,
        retry_eligible: true,
        ready_for_execution: false,
        last_failure_at: None,
        next_retry_at: None,
        total_parents: 2,
        completed_parents: 1,
        attempts: 0,
        max_attempts: 3,
        backoff_request_seconds: None,
        last_attempted_at: None,
    };

    assert_eq!(step.blocking_reason(), Some("dependencies_not_satisfied"));
    assert!(!step.can_execute_now());
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_system_health_score_calculation(_pool: sqlx::PgPool) -> sqlx::Result<()> {
    let health = SystemHealthCounts {
        total_tasks: 100,
        complete_tasks: 80,
        error_tasks: 5,
        ..Default::default()
    };

    let success = health.success_rate();
    assert!(success > 0.0 && success <= 1.0);
    assert!(health.error_rate() < 0.5);
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_execution_context_status(_pool: sqlx::PgPool) -> sqlx::Result<()> {
    let task_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let context = TaskExecutionContext {
        task_uuid,
        named_task_uuid,
        status: "in_progress".to_string(),
        total_steps: 10,
        pending_steps: 2,
        in_progress_steps: 0,
        completed_steps: 8,
        failed_steps: 0,
        ready_steps: 2,
        execution_status: ExecutionStatus::HasReadySteps,
        recommended_action: Some(RecommendedAction::ExecuteReadySteps),
        completion_percentage: sqlx::types::BigDecimal::from(80),
        health_status: "healthy".to_string(),
        enqueued_steps: 0,
    };

    assert!(context.has_ready_steps());
    assert!(!context.is_complete());
    assert!(!context.execution_status.is_blocked());
    assert_eq!(context.execution_status.as_str(), "has_ready_steps");
    assert_eq!(context.health_status, "healthy");
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_backoff_calculation(_pool: sqlx::PgPool) -> sqlx::Result<()> {
    let task_uuid = Uuid::now_v7();
    let named_step_uuid = Uuid::now_v7();
    let workflow_step_uuid = Uuid::now_v7();
    let step = StepReadinessStatus {
        workflow_step_uuid,
        task_uuid,
        named_step_uuid,
        name: "test".to_string(),
        current_state: "error".to_string(),
        dependencies_satisfied: true,
        retry_eligible: true,
        ready_for_execution: false,
        last_failure_at: None,
        next_retry_at: None,
        total_parents: 0,
        completed_parents: 0,
        attempts: 3,
        max_attempts: 5,
        backoff_request_seconds: None,
        last_attempted_at: None,
    };

    assert_eq!(step.effective_backoff_seconds(), 8); // 2^3 = 8
    Ok(())
}
