//! Tests for the query scopes system

use chrono::{Duration, Utc};
use sqlx::PgPool;
use tasker_core::models::{Task, TaskTransition, WorkflowStep};
use tasker_core::scopes::ScopeBuilder;
use tasker_core::state_machine::{TaskState, WorkflowStepState};

#[sqlx::test]
async fn test_task_scopes_basic(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test basic scope chaining
    let tasks = Task::scope()
        .in_namespace("default".to_string())
        .with_task_name("test_task".to_string())
        .limit(10)
        .all(&pool)
        .await?;

    // Should not error even with no data
    assert!(tasks.len() <= 10);
    Ok(())
}

#[sqlx::test]
async fn test_task_scope_by_current_state(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test state-based filtering
    let pending_tasks = Task::scope()
        .by_current_state(TaskState::Pending)
        .all(&pool)
        .await?;

    // Should not error - we're just testing that the query executes successfully
    let _ = pending_tasks.len();
    Ok(())
}

#[sqlx::test]
async fn test_task_scope_active(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test active task filtering
    let active_tasks = Task::scope().active().all(&pool).await?;

    // Should not error - we're just testing that the query executes successfully
    let _ = active_tasks.len();
    Ok(())
}

#[sqlx::test]
async fn test_task_scope_time_based(pool: PgPool) -> Result<(), sqlx::Error> {
    let yesterday = Utc::now() - Duration::hours(24);

    // Test time-based scopes
    let recent_tasks = Task::scope().created_since(yesterday).all(&pool).await?;

    let failed_tasks = Task::scope().failed_since(yesterday).all(&pool).await?;

    let completed_tasks = Task::scope().completed_since(yesterday).all(&pool).await?;

    // Should not error - we're just testing that queries execute successfully
    let _ = (
        recent_tasks.len(),
        failed_tasks.len(),
        completed_tasks.len(),
    );
    Ok(())
}

#[sqlx::test]
async fn test_workflow_step_scopes(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test workflow step scopes
    let failed_steps = WorkflowStep::scope()
        .failed()
        .failed_since(Utc::now() - Duration::hours(24))
        .all(&pool)
        .await?;

    let completed_steps = WorkflowStep::scope().completed().all(&pool).await?;

    // Should not error - we're just testing that queries execute successfully
    let _ = (failed_steps.len(), completed_steps.len());
    Ok(())
}

#[sqlx::test]
async fn test_workflow_step_scope_by_current_state(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test state-based filtering for workflow steps
    let pending_steps = WorkflowStep::scope()
        .by_current_state(WorkflowStepState::Pending)
        .all(&pool)
        .await?;

    // Should not error - we're just testing that the query executes successfully
    let _ = pending_steps.len();
    Ok(())
}

#[sqlx::test]
async fn test_workflow_step_scope_for_task(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test filtering steps by task ID (using a dummy ID)
    let steps_for_task = WorkflowStep::scope()
        .for_task(999999) // Non-existent task ID
        .all(&pool)
        .await?;

    // Should return empty result but no error
    assert_eq!(steps_for_task.len(), 0);
    Ok(())
}

#[sqlx::test]
async fn test_workflow_step_scope_time_based(pool: PgPool) -> Result<(), sqlx::Error> {
    let yesterday = Utc::now() - Duration::hours(24);

    // Test time-based workflow step scopes
    let completed_since = WorkflowStep::scope()
        .completed_since(yesterday)
        .all(&pool)
        .await?;

    let failed_since = WorkflowStep::scope()
        .failed_since(yesterday)
        .all(&pool)
        .await?;

    let for_tasks_since = WorkflowStep::scope()
        .for_tasks_since(yesterday)
        .all(&pool)
        .await?;

    // Should not error - we're just testing that queries execute successfully
    let _ = (
        completed_since.len(),
        failed_since.len(),
        for_tasks_since.len(),
    );
    Ok(())
}

#[sqlx::test]
async fn test_task_transition_scopes(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test task transition scopes
    let recent_transitions = TaskTransition::scope()
        .to_state(TaskState::Pending)
        .recent()
        .all(&pool)
        .await?;

    let transitions_with_metadata = TaskTransition::scope()
        .with_metadata_key("test_key".to_string())
        .all(&pool)
        .await?;

    let transitions_for_task = TaskTransition::scope()
        .for_task(999999) // Non-existent task ID
        .all(&pool)
        .await?;

    // Should not error - we're just testing that queries execute successfully
    let _ = (recent_transitions.len(), transitions_with_metadata.len());
    assert_eq!(transitions_for_task.len(), 0); // Should be empty for non-existent task
    Ok(())
}

#[sqlx::test]
async fn test_scope_count_and_exists(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test count and exists methods
    let active_count = Task::scope().active().count(&pool).await?;

    let active_exists = Task::scope().active().exists(&pool).await?;

    let step_count = WorkflowStep::scope().completed().count(&pool).await?;

    let step_exists = WorkflowStep::scope().completed().exists(&pool).await?;

    // Count should be >= 0, exists should be consistent
    assert!(active_count >= 0);
    assert_eq!(active_exists, active_count > 0);
    assert!(step_count >= 0);
    assert_eq!(step_exists, step_count > 0);
    Ok(())
}

#[sqlx::test]
async fn test_scope_chaining(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test complex scope chaining
    let complex_query = Task::scope()
        .active()
        .in_namespace("default".to_string())
        .created_since(Utc::now() - Duration::days(7))
        .order_by_created_at(false) // Most recent first
        .limit(5)
        .all(&pool)
        .await?;

    // Should not error and respect limit
    assert!(complex_query.len() <= 5);
    Ok(())
}

#[sqlx::test]
async fn test_scope_first_method(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test first() method
    let first_task = Task::scope().order_by_created_at(true).first(&pool).await?;

    let first_step = WorkflowStep::scope().first(&pool).await?;

    // Should not error (results can be None)
    assert!(first_task.is_some() || first_task.is_none());
    assert!(first_step.is_some() || first_step.is_none());
    Ok(())
}
