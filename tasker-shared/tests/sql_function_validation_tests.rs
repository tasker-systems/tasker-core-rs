//! # Comprehensive SQL Function Validation Tests
//!
//! Tests for validating the refactored SQL functions with WaitingForRetry support
//! and optimized readiness calculations.

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

use serde_json::json;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::models::core::workflow_step_transition::NewWorkflowStepTransition;
use tasker_shared::models::factories::base::{SqlxFactory, StateFactory};
use tasker_shared::models::factories::core::{TaskFactory, WorkflowStepFactory};
use tasker_shared::models::factories::relationships::WorkflowStepEdgeFactory;
use tasker_shared::models::{Task, WorkflowStep, WorkflowStepTransition};

/// Helper to create a task with initial state
async fn create_test_task(pool: &PgPool, name: &str) -> Result<Task> {
    TaskFactory::new()
        .with_named_task(name)
        .with_initial_state("pending")
        .create(pool)
        .await
        .map_err(Into::into)
}

/// Helper to create a step with initial state
async fn create_test_step(pool: &PgPool, task_uuid: Uuid, name: &str) -> Result<WorkflowStep> {
    WorkflowStepFactory::new()
        .for_task(task_uuid)
        .with_named_step(name)
        .with_initial_state("pending")
        .create(pool)
        .await
        .map_err(Into::into)
}

/// Helper to transition a step to a specific state with proper metadata
async fn transition_step_to_state(
    pool: &PgPool,
    step: &WorkflowStep,
    to_state: &str,
    attempts: Option<i32>,
    backoff_seconds: Option<i32>,
) -> Result<()> {
    // Update step metadata if needed
    if let Some(attempts) = attempts {
        sqlx::query!(
            "UPDATE tasker_workflow_steps SET attempts = $1 WHERE workflow_step_uuid = $2",
            attempts,
            step.workflow_step_uuid
        )
        .execute(pool)
        .await?;
    }

    if let Some(backoff) = backoff_seconds {
        sqlx::query!(
            "UPDATE tasker_workflow_steps SET backoff_request_seconds = $1 WHERE workflow_step_uuid = $2",
            backoff,
            step.workflow_step_uuid
        )
        .execute(pool)
        .await?;
    }

    // For WaitingForRetry, also set last_attempted_at
    if to_state == "waiting_for_retry" {
        sqlx::query!(
            "UPDATE tasker_workflow_steps SET last_attempted_at = NOW() WHERE workflow_step_uuid = $1",
            step.workflow_step_uuid
        )
        .execute(pool)
        .await?;
    }

    // Create the transition
    let metadata = json!({
        "test_transition": true,
        "to_state": to_state
    });

    let new_transition = NewWorkflowStepTransition {
        workflow_step_uuid: step.workflow_step_uuid,
        from_state: Some("pending".to_string()),
        to_state: to_state.to_string(),
        metadata: Some(metadata),
    };

    WorkflowStepTransition::create(pool, new_transition).await?;
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_readiness_excludes_error_state(pool: PgPool) -> Result<()> {
    // Create task with steps
    let task = create_test_task(&pool, "test_error_exclusion").await?;

    let step1 = create_test_step(&pool, task.task_uuid, "step_in_error").await?;
    let step2 = create_test_step(&pool, task.task_uuid, "step_in_pending").await?;
    let step3 = create_test_step(&pool, task.task_uuid, "step_in_waiting_for_retry").await?;

    // Transition steps to different states
    transition_step_to_state(&pool, &step1, "error", Some(3), None).await?;
    transition_step_to_state(&pool, &step3, "waiting_for_retry", Some(1), Some(5)).await?;

    // Test SQL readiness function
    let executor = SqlFunctionExecutor::new(pool.clone());
    let readiness = executor
        .get_step_readiness_status(task.task_uuid, None)
        .await?;

    // Verify error state is NOT ready
    let error_step = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == step1.workflow_step_uuid)
        .expect("Should find error step");

    assert_eq!(error_step.current_state, "error");
    assert!(
        !error_step.ready_for_execution,
        "Error state should never be ready"
    );

    // Verify pending state IS ready
    let pending_step = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == step2.workflow_step_uuid)
        .expect("Should find pending step");

    assert_eq!(pending_step.current_state, "pending");
    assert!(
        pending_step.ready_for_execution,
        "Pending state should be ready"
    );

    // Verify waiting_for_retry may be ready (depends on backoff timing)
    let retry_step = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == step3.workflow_step_uuid)
        .expect("Should find waiting_for_retry step");

    assert_eq!(retry_step.current_state, "waiting_for_retry");
    // Don't assert readiness as it depends on backoff timing

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_waiting_for_retry_backoff_calculation(pool: PgPool) -> Result<()> {
    let task = create_test_task(&pool, "test_backoff").await?;

    // Create step with custom backoff
    let step_custom = create_test_step(&pool, task.task_uuid, "custom_backoff").await?;
    transition_step_to_state(&pool, &step_custom, "waiting_for_retry", Some(2), Some(10)).await?;

    // Create step with exponential backoff - first transition to error to create failure_time
    let step_exp = create_test_step(&pool, task.task_uuid, "exponential_backoff").await?;
    transition_step_to_state(&pool, &step_exp, "error", Some(3), None).await?;
    transition_step_to_state(&pool, &step_exp, "waiting_for_retry", Some(3), None).await?;

    let executor = SqlFunctionExecutor::new(pool.clone());
    let readiness = executor
        .get_step_readiness_status(task.task_uuid, None)
        .await?;

    // Check custom backoff
    let custom_step = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == step_custom.workflow_step_uuid)
        .expect("Should find custom backoff step");

    assert_eq!(custom_step.backoff_request_seconds, Some(10));
    assert!(
        custom_step.next_retry_at.is_some(),
        "Should have next retry time"
    );

    // Check exponential backoff (2^3 = 8 seconds, capped at 30)
    let exp_step = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == step_exp.workflow_step_uuid)
        .expect("Should find exponential backoff step");

    assert!(
        exp_step.next_retry_at.is_some(),
        "Should have calculated retry time"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_dependency_satisfaction_logic(pool: PgPool) -> Result<()> {
    let task = create_test_task(&pool, "test_dependencies").await?;

    // Create parent-child step relationship
    let parent_step = create_test_step(&pool, task.task_uuid, "parent_step").await?;
    let child_step = create_test_step(&pool, task.task_uuid, "child_step").await?;

    // Create dependency edge
    WorkflowStepEdgeFactory::new()
        .with_from_step(parent_step.workflow_step_uuid)
        .with_to_step(child_step.workflow_step_uuid)
        .create(&pool)
        .await?;

    // Test readiness with parent in pending state
    let executor = SqlFunctionExecutor::new(pool.clone());
    let readiness = executor
        .get_step_readiness_status(task.task_uuid, None)
        .await?;

    let child = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == child_step.workflow_step_uuid)
        .expect("Should find child step");

    assert!(
        !child.dependencies_satisfied,
        "Dependencies not satisfied when parent pending"
    );
    assert!(
        !child.ready_for_execution,
        "Child not ready when parent incomplete"
    );

    // Complete parent step
    transition_step_to_state(&pool, &parent_step, "complete", None, None).await?;

    // Re-check readiness
    let readiness = executor
        .get_step_readiness_status(task.task_uuid, None)
        .await?;

    let child = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == child_step.workflow_step_uuid)
        .expect("Should find child step");

    assert!(
        child.dependencies_satisfied,
        "Dependencies satisfied when parent complete"
    );
    assert!(
        child.ready_for_execution,
        "Child ready when dependencies met"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_batch_readiness_function_delegation(pool: PgPool) -> Result<()> {
    // Create multiple tasks
    let task1 = create_test_task(&pool, "batch_task_1").await?;
    let task2 = create_test_task(&pool, "batch_task_2").await?;

    // Create steps for each task
    let _step1_1 = create_test_step(&pool, task1.task_uuid, "step1_1").await?;
    let step1_2 = create_test_step(&pool, task1.task_uuid, "step1_2").await?;
    let step2_1 = create_test_step(&pool, task2.task_uuid, "step2_1").await?;

    // Set different states
    transition_step_to_state(&pool, &step1_2, "error", Some(3), None).await?;
    transition_step_to_state(&pool, &step2_1, "waiting_for_retry", Some(1), Some(5)).await?;

    // Test batch function
    let executor = SqlFunctionExecutor::new(pool.clone());
    let task_uuids = vec![task1.task_uuid, task2.task_uuid];
    let readiness = executor.get_step_readiness_status_batch(task_uuids).await?;

    // Should get results for all steps across both tasks
    assert_eq!(readiness.len(), 3);

    // Verify task1 steps
    let task1_steps: Vec<_> = readiness
        .iter()
        .filter(|s| s.task_uuid == task1.task_uuid)
        .collect();
    assert_eq!(task1_steps.len(), 2);

    // Verify task2 steps
    let task2_steps: Vec<_> = readiness
        .iter()
        .filter(|s| s.task_uuid == task2.task_uuid)
        .collect();
    assert_eq!(task2_steps.len(), 1);

    // Single function should delegate to batch function with same results
    let single_readiness = executor
        .get_step_readiness_status(task1.task_uuid, None)
        .await?;
    assert_eq!(single_readiness.len(), 2);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_helper_functions_via_readiness_api(pool: PgPool) -> Result<()> {
    let task = create_test_task(&pool, "test_helpers").await?;
    let step = create_test_step(&pool, task.task_uuid, "helper_test_step").await?;

    // Set up step for retry with backoff that has expired
    sqlx::query!(
        r#"
        UPDATE tasker_workflow_steps
        SET
            attempts = 2,
            retry_limit = 5,
            backoff_request_seconds = 2,
            last_attempted_at = NOW() - INTERVAL '10 seconds'
        WHERE workflow_step_uuid = $1
        "#,
        step.workflow_step_uuid
    )
    .execute(&pool)
    .await?;

    transition_step_to_state(&pool, &step, "waiting_for_retry", Some(2), Some(2)).await?;

    // Override last_attempted_at to be 10 seconds ago so backoff has expired
    sqlx::query!(
        r#"
        UPDATE tasker_workflow_steps
        SET last_attempted_at = NOW() - INTERVAL '10 seconds'
        WHERE workflow_step_uuid = $1
        "#,
        step.workflow_step_uuid
    )
    .execute(&pool)
    .await?;

    // Test via the readiness API - helper functions are called internally
    let executor = SqlFunctionExecutor::new(pool.clone());
    let readiness = executor
        .get_step_readiness_status(task.task_uuid, None)
        .await?;

    let step_status = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == step.workflow_step_uuid)
        .expect("Should find step");

    // Verify helper function results are reflected in the API response
    assert_eq!(step_status.backoff_request_seconds, Some(2));
    assert!(
        step_status.next_retry_at.is_some(),
        "Should have calculated retry time"
    );

    // Since backoff was 2 seconds and we set last_attempted_at to 10 seconds ago,
    // the step should be ready
    assert!(step_status.retry_eligible, "Should be retry eligible");
    assert!(
        step_status.ready_for_execution,
        "Should be ready when backoff expired"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_retry_limit_enforcement(pool: PgPool) -> Result<()> {
    let task = create_test_task(&pool, "test_retry_limits").await?;

    // Create step at retry limit
    let step_exhausted = create_test_step(&pool, task.task_uuid, "exhausted_retries").await?;
    sqlx::query!(
        "UPDATE tasker_workflow_steps SET attempts = 3, retry_limit = 3 WHERE workflow_step_uuid = $1",
        step_exhausted.workflow_step_uuid
    )
    .execute(&pool)
    .await?;
    transition_step_to_state(&pool, &step_exhausted, "waiting_for_retry", Some(3), None).await?;

    // Create step under retry limit
    let step_retryable = create_test_step(&pool, task.task_uuid, "can_retry").await?;
    sqlx::query!(
        "UPDATE tasker_workflow_steps SET attempts = 2, retry_limit = 5 WHERE workflow_step_uuid = $1",
        step_retryable.workflow_step_uuid
    )
    .execute(&pool)
    .await?;
    transition_step_to_state(&pool, &step_retryable, "waiting_for_retry", Some(2), None).await?;

    let executor = SqlFunctionExecutor::new(pool.clone());
    let readiness = executor
        .get_step_readiness_status(task.task_uuid, None)
        .await?;

    // Check exhausted step
    let exhausted = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == step_exhausted.workflow_step_uuid)
        .expect("Should find exhausted step");

    assert!(
        !exhausted.retry_eligible,
        "Should not be retry eligible at limit"
    );
    assert!(
        !exhausted.ready_for_execution,
        "Should not be ready when retry exhausted"
    );

    // Check retryable step
    let retryable = readiness
        .iter()
        .find(|s| s.workflow_step_uuid == step_retryable.workflow_step_uuid)
        .expect("Should find retryable step");

    assert!(
        retryable.retry_eligible,
        "Should be retry eligible under limit"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_execution_context_with_waiting_for_retry(pool: PgPool) -> Result<()> {
    let task = create_test_task(&pool, "test_execution_context").await?;

    // Create steps in various states
    let _step_pending = create_test_step(&pool, task.task_uuid, "pending").await?;
    let step_waiting = create_test_step(&pool, task.task_uuid, "waiting").await?;
    let step_error = create_test_step(&pool, task.task_uuid, "error").await?;
    let step_complete = create_test_step(&pool, task.task_uuid, "complete").await?;

    transition_step_to_state(&pool, &step_waiting, "waiting_for_retry", Some(1), Some(5)).await?;
    transition_step_to_state(&pool, &step_error, "error", Some(3), None).await?;
    transition_step_to_state(&pool, &step_complete, "complete", None, None).await?;

    // Test get_task_execution_context
    let executor = SqlFunctionExecutor::new(pool.clone());
    let context = executor.get_task_execution_context(task.task_uuid).await?;

    assert!(context.is_some(), "Should have execution context");
    let ctx = context.unwrap();

    // Verify step counts
    assert_eq!(ctx.total_steps, 4);
    assert_eq!(ctx.completed_steps, 1);
    // Note: TaskExecutionContext doesn't have direct error_steps or waiting_for_retry_steps fields
    // These are tracked in the execution_status field

    // Pending + waiting_for_retry should both be considered for readiness
    assert!(ctx.ready_steps > 0, "Should have ready steps");

    Ok(())
}
