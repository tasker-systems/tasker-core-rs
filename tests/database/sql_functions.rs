//! SQL Function Integration Tests
//!
//! Tests for the SQL function execution layer, covering all function categories
//! and ensuring proper integration with the PostgreSQL database.

use tasker_core::database::sql_functions::*;

#[sqlx::test]
async fn test_step_readiness_blocking_reason(_pool: sqlx::PgPool) -> sqlx::Result<()> {
    let step = StepReadinessStatus {
        workflow_step_id: 1,
        task_id: 1,
        named_step_id: 1,
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
        retry_limit: 3,
        backoff_request_seconds: None,
        last_attempted_at: None,
    };

    assert_eq!(step.blocking_reason(), Some("dependencies_not_satisfied"));
    assert!(!step.can_execute_now());
    Ok(())
}

#[sqlx::test]
async fn test_system_health_score_calculation(_pool: sqlx::PgPool) -> sqlx::Result<()> {
    let health = SystemHealthCounts {
        total_tasks: 100,
        complete_tasks: 80,
        error_tasks: 5,
        active_connections: 20,
        max_connections: 100,
        ..Default::default()
    };

    let score = health.health_score();
    assert!(score > 0.0 && score <= 1.0);
    assert!(!health.is_under_heavy_load());
    Ok(())
}

#[sqlx::test]
async fn test_task_execution_context_status(_pool: sqlx::PgPool) -> sqlx::Result<()> {
    let context = TaskExecutionContext {
        task_id: 1,
        total_steps: 10,
        completed_steps: 8,
        pending_steps: 2,
        error_steps: 0,
        ready_steps: 2,
        blocked_steps: 0,
        completion_percentage: 80.0,
        estimated_duration_seconds: Some(300),
        recommended_action: "execute_ready_steps".to_string(),
        next_steps_to_execute: vec![9, 10],
        critical_path_steps: vec![9],
        bottleneck_steps: vec![],
    };

    assert!(context.can_proceed());
    assert!(!context.is_complete());
    assert!(!context.is_blocked());
    assert_eq!(context.get_priority_steps(), &[9]);
    Ok(())
}

#[sqlx::test]
async fn test_step_backoff_calculation(_pool: sqlx::PgPool) -> sqlx::Result<()> {
    let step = StepReadinessStatus {
        workflow_step_id: 1,
        task_id: 1,
        named_step_id: 1,
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
        retry_limit: 5,
        backoff_request_seconds: None,
        last_attempted_at: None,
    };

    assert_eq!(step.effective_backoff_seconds(), 8); // 2^3 = 8
    Ok(())
}
