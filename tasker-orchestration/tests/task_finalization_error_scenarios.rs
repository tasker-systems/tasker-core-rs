//! # Task Finalization Error Scenario Tests
//!
//! Comprehensive tests validating error handling in task finalization across various
//! failure scenarios, including:
//! - Tasks with steps in permanent error state (should fail the task)
//! - Tasks with steps in WaitingForRetry state (should respect backoff timing)
//! - Mixed scenarios with both errored and successful steps
//! - Error classification and state transition validation
//! - Integration with backoff calculator for retry timing

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;

use tasker_shared::models::factories::base::{SqlxFactory, StateFactory};
use tasker_shared::models::factories::core::{TaskFactory, WorkflowStepFactory};
use tasker_shared::models::orchestration::{ExecutionStatus, TaskExecutionContext};
use tasker_shared::models::{Task, WorkflowStep};
use tasker_shared::state_machine::{
    events::TaskEvent,
    states::{TaskState, WorkflowStepState},
    task_state_machine::TaskStateMachine,
};
use tasker_shared::system_context::SystemContext;

use tasker_orchestration::orchestration::{
    backoff_calculator::{BackoffCalculator, BackoffCalculatorConfig},
    error_handling_service::{ErrorHandlingAction, ErrorHandlingConfig, ErrorHandlingService},
    lifecycle::{
        step_enqueuer_service::StepEnqueuerService,
        task_finalizer::{FinalizationAction, TaskFinalizer},
    },
};

/// Helper to create test system context
async fn create_test_system_context(pool: PgPool) -> Result<Arc<SystemContext>> {
    let system_context = SystemContext::with_pool(pool).await?;
    Ok(Arc::new(system_context))
}

/// Helper to create test task with steps
async fn create_test_task_with_steps(
    pool: &PgPool,
    step_count: usize,
) -> Result<(Task, Vec<WorkflowStep>)> {
    // Create task using factory pattern with initial state transitions
    let task = TaskFactory::new()
        .with_named_task("error_test_task")
        .with_initial_state("pending") // Ensure initial transitions are created
        .create(pool)
        .await?;

    // Verify task was created and can be found
    let found_task = Task::find_by_id(pool, task.task_uuid).await?;
    if found_task.is_none() {
        return Err(anyhow::anyhow!(
            "Task was created but cannot be found immediately after creation: {}",
            task.task_uuid
        )
        .into());
    }

    // Create steps with proper initial state transitions
    let mut steps = Vec::new();
    for i in 0..step_count {
        let workflow_step = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step(&format!("test_step_{}", i))
            .with_initial_state("pending") // Ensure initial transitions are created
            .create(pool)
            .await?;
        steps.push(workflow_step);
    }

    Ok((task, steps))
}

/// Helper to transition step to specific state using direct database operations
async fn transition_step_to_state(
    pool: &PgPool,
    _system_context: Arc<SystemContext>,
    step: &WorkflowStep,
    target_state: WorkflowStepState,
) -> Result<()> {
    use serde_json::json;
    use tasker_shared::models::core::workflow_step_transition::NewWorkflowStepTransition;
    use tasker_shared::models::WorkflowStepTransition;

    // Create the target state transition directly in the database
    let metadata = json!({
        "test_transition": true,
        "target_state": target_state.to_string(),
        "created_by": "test_helper"
    });

    let new_transition = NewWorkflowStepTransition {
        workflow_step_uuid: step.workflow_step_uuid,
        from_state: Some("pending".to_string()),
        to_state: target_state.to_string(),
        metadata: Some(metadata),
    };

    // Create the transition directly
    WorkflowStepTransition::create(pool, new_transition).await?;

    // Update step fields if needed based on target state
    match target_state {
        WorkflowStepState::InProgress => {
            sqlx::query!(
                "UPDATE tasker_workflow_steps SET in_process = true WHERE workflow_step_uuid = $1",
                step.workflow_step_uuid
            )
            .execute(pool)
            .await?;
        }
        WorkflowStepState::Complete => {
            sqlx::query!(
                "UPDATE tasker_workflow_steps SET processed = true, results = $2 WHERE workflow_step_uuid = $1",
                step.workflow_step_uuid,
                json!({"test_result": "completed"})
            )
            .execute(pool)
            .await?;
        }
        WorkflowStepState::Error | WorkflowStepState::WaitingForRetry => {
            sqlx::query!(
                "UPDATE tasker_workflow_steps SET attempts = COALESCE(attempts, 0) + 1 WHERE workflow_step_uuid = $1",
                step.workflow_step_uuid
            )
            .execute(pool)
            .await?;
        }
        _ => {
            // No additional updates needed
        }
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_task_finalization_with_all_steps_in_error_state(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create task with 3 steps
    let (task, steps) = create_test_task_with_steps(&pool, 3).await?;

    // Put all steps in error state
    for workflow_step in &steps {
        transition_step_to_state(
            &pool,
            system_context.clone(),
            workflow_step,
            WorkflowStepState::Error,
        )
        .await?;
    }

    // Create task finalizer
    let step_enqueuer_service = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let finalizer = TaskFinalizer::new(system_context.clone(), step_enqueuer_service);

    // Test finalization - should mark task as failed
    let result = finalizer.finalize_task(task.task_uuid).await?;

    assert_eq!(result.task_uuid, task.task_uuid);
    assert!(matches!(result.action, FinalizationAction::Failed));
    assert_eq!(result.reason, Some("Steps in error state".to_string()));

    // Verify task state is now Error
    let task_state_machine = TaskStateMachine::for_task(
        task.task_uuid,
        pool.clone(),
        system_context.processor_uuid(),
    )
    .await?;
    let final_state = task_state_machine.current_state().await?;
    assert_eq!(final_state, TaskState::Error);

    // Verify task execution context shows blocked by failures
    let context = TaskExecutionContext::get_for_task(&pool, task.task_uuid).await?;
    assert!(context.is_some());
    let context = context.unwrap();
    assert_eq!(context.execution_status, ExecutionStatus::BlockedByFailures);

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_task_finalization_with_steps_waiting_for_retry(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create task with 2 steps
    let (task, steps) = create_test_task_with_steps(&pool, 2).await?;

    // Put first step in WaitingForRetry state
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[0],
        WorkflowStepState::WaitingForRetry,
    )
    .await?;

    // Keep second step in pending state
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[1],
        WorkflowStepState::Pending,
    )
    .await?;

    // Create task finalizer
    let step_enqueuer_service = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let finalizer = TaskFinalizer::new(system_context.clone(), step_enqueuer_service);

    // Test finalization - should respect waiting for retry state
    let result = finalizer.finalize_task(task.task_uuid).await?;

    assert_eq!(result.task_uuid, task.task_uuid);
    // Should not fail the task - waiting for retry is not a permanent failure
    assert!(!matches!(result.action, FinalizationAction::Failed));

    // Verify task is not in error state
    let task_state_machine = TaskStateMachine::for_task(
        task.task_uuid,
        pool.clone(),
        system_context.processor_uuid(),
    )
    .await?;
    let final_state = task_state_machine.current_state().await?;
    assert_ne!(final_state, TaskState::Error);

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_task_finalization_mixed_error_and_success_states(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create task with 4 steps
    let (task, steps) = create_test_task_with_steps(&pool, 4).await?;

    // Put first step in error state (permanent failure)
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[0],
        WorkflowStepState::Error,
    )
    .await?;

    // Put second step in complete state
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[1],
        WorkflowStepState::Complete,
    )
    .await?;

    // Put third step in waiting for retry state
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[2],
        WorkflowStepState::WaitingForRetry,
    )
    .await?;

    // Keep fourth step in pending state

    // Create task finalizer
    let step_enqueuer_service = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let finalizer = TaskFinalizer::new(system_context.clone(), step_enqueuer_service);

    // Test finalization - should fail task due to error state step
    let result = finalizer.finalize_task(task.task_uuid).await?;

    assert_eq!(result.task_uuid, task.task_uuid);
    assert!(matches!(result.action, FinalizationAction::Failed));

    // Verify task state is now Error
    let task_state_machine = TaskStateMachine::for_task(
        task.task_uuid,
        pool.clone(),
        system_context.processor_uuid(),
    )
    .await?;
    let final_state = task_state_machine.current_state().await?;
    assert_eq!(final_state, TaskState::Error);

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_task_finalization_all_steps_complete(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create task with 3 steps
    let (task, steps) = create_test_task_with_steps(&pool, 3).await?;

    // Put all steps in complete state
    for workflow_step in &steps {
        transition_step_to_state(
            &pool,
            system_context.clone(),
            workflow_step,
            WorkflowStepState::Complete,
        )
        .await?;
    }

    // Create task finalizer
    let step_enqueuer_service = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let finalizer = TaskFinalizer::new(system_context.clone(), step_enqueuer_service);

    // Test finalization - should complete task
    let result = finalizer.finalize_task(task.task_uuid).await?;

    assert_eq!(result.task_uuid, task.task_uuid);
    assert!(matches!(result.action, FinalizationAction::Completed));
    assert_eq!(result.completion_percentage, Some(100.0));

    // Verify task state is now Complete
    let task_state_machine = TaskStateMachine::for_task(
        task.task_uuid,
        pool.clone(),
        system_context.processor_uuid(),
    )
    .await?;
    let final_state = task_state_machine.current_state().await?;
    assert_eq!(final_state, TaskState::Complete);

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_error_handling_service_permanent_failure_classification(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create test step
    let (task, steps) = create_test_task_with_steps(&pool, 1).await?;
    let mut workflow_step = steps[0].clone();

    // Exhaust retries to make the error permanent
    workflow_step.attempts = Some(3);
    workflow_step.retry_limit = Some(3);
    sqlx::query!(
        "UPDATE tasker_workflow_steps SET attempts = $1, retry_limit = $2 WHERE workflow_step_uuid = $3",
        workflow_step.attempts,
        workflow_step.retry_limit,
        workflow_step.workflow_step_uuid
    )
    .execute(&pool)
    .await?;

    // Create error handling service
    let config = ErrorHandlingConfig::default();
    let backoff_calculator = BackoffCalculator::new(
        BackoffCalculatorConfig::default(),
        system_context.database_pool().clone(),
    );
    let error_service = ErrorHandlingService::new(config, backoff_calculator, system_context);

    // Test permanent error handling
    let error = tasker_shared::errors::OrchestrationError::StepExecutionFailed {
        step_uuid: workflow_step.workflow_step_uuid,
        task_uuid: Some(task.task_uuid),
        reason: "Permanent database connection failure".to_string(),
        error_code: Some("DATABASE_UNREACHABLE".to_string()),
        retry_after: None,
    };

    let result = error_service
        .handle_step_error(
            &workflow_step,
            &error,
            Some("Database unreachable".to_string()),
        )
        .await?;

    assert_eq!(result.step_uuid, workflow_step.workflow_step_uuid);
    // Since we exhausted retries, it should be marked as error (permanent failure)
    assert!(
        matches!(
            result.action,
            ErrorHandlingAction::MarkedAsError | ErrorHandlingAction::MarkedAsPermanentFailure
        ),
        "Expected MarkedAsError or MarkedAsPermanentFailure, got {:?}",
        result.action
    );
    assert_eq!(result.final_state, WorkflowStepState::Error);

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_error_handling_service_retryable_failure_classification(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create test step with retry configuration
    let (task, steps) = create_test_task_with_steps(&pool, 1).await?;
    let mut workflow_step = steps[0].clone();
    workflow_step.retry_limit = Some(3); // Allow retries
    workflow_step.attempts = Some(1); // First attempt

    // Update the step in database with retry configuration
    sqlx::query!(
        "UPDATE tasker_workflow_steps SET retry_limit = $1, attempts = $2 WHERE workflow_step_uuid = $3",
        workflow_step.retry_limit,
        workflow_step.attempts,
        workflow_step.workflow_step_uuid
    )
    .execute(&pool)
    .await?;

    // Create error handling service
    let config = ErrorHandlingConfig::default();
    let backoff_calculator = BackoffCalculator::new(
        BackoffCalculatorConfig::default(),
        system_context.database_pool().clone(),
    );
    let error_service = ErrorHandlingService::new(config, backoff_calculator, system_context);

    // Test retryable error handling
    let error = tasker_shared::errors::OrchestrationError::StepExecutionFailed {
        step_uuid: workflow_step.workflow_step_uuid,
        task_uuid: Some(task.task_uuid),
        reason: "Temporary network timeout".to_string(),
        error_code: Some("NETWORK_TIMEOUT".to_string()),
        retry_after: Some(std::time::Duration::from_secs(5)),
    };

    let result = error_service
        .handle_step_error(&workflow_step, &error, Some("Network timeout".to_string()))
        .await?;

    assert_eq!(result.step_uuid, workflow_step.workflow_step_uuid);
    assert!(matches!(
        result.action,
        ErrorHandlingAction::TransitionedToWaitingForRetry
    ));
    assert_eq!(result.final_state, WorkflowStepState::WaitingForRetry);
    assert!(result.backoff_applied);
    assert!(result.next_retry_at.is_some());

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_blocked_by_errors_detection(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create task with steps in error state
    let (task, steps) = create_test_task_with_steps(&pool, 2).await?;

    // Exhaust retries on first step so it's permanently blocked
    sqlx::query!(
        "UPDATE tasker_workflow_steps SET attempts = 3, retry_limit = 3 WHERE workflow_step_uuid = $1",
        steps[0].workflow_step_uuid
    )
    .execute(&pool)
    .await?;

    // Put first step in error state with exhausted retries
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[0],
        WorkflowStepState::Error,
    )
    .await?;

    // Create task finalizer
    let step_enqueuer_service = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let finalizer = TaskFinalizer::new(system_context.clone(), step_enqueuer_service);

    // Test blocked by errors detection
    let is_blocked = finalizer.blocked_by_errors(task.task_uuid).await?;
    assert!(is_blocked, "Task should be blocked by errors");

    // Test with task that has no error steps
    let (task2, _steps2) = create_test_task_with_steps(&pool, 2).await?;

    // Keep all steps in pending state
    let is_blocked2 = finalizer.blocked_by_errors(task2.task_uuid).await?;
    assert!(!is_blocked2, "Task should not be blocked by errors");

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_sql_step_readiness_excludes_error_states(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create task with steps
    let (task, steps) = create_test_task_with_steps(&pool, 3).await?;

    // Put first step in error state
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[0],
        WorkflowStepState::Error,
    )
    .await?;

    // Put second step in WaitingForRetry state
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[1],
        WorkflowStepState::WaitingForRetry,
    )
    .await?;

    // Keep third step in pending state

    // Test SQL step readiness function
    let readiness_results = sqlx::query!(
        "SELECT * FROM get_step_readiness_status($1)",
        task.task_uuid
    )
    .fetch_all(&pool)
    .await?;

    // Should return 3 results (all steps)
    assert_eq!(readiness_results.len(), 3);

    // Find results by step UUID
    let error_step_result = readiness_results
        .iter()
        .find(|r| r.workflow_step_uuid == Some(steps[0].workflow_step_uuid))
        .unwrap();
    let waiting_step_result = readiness_results
        .iter()
        .find(|r| r.workflow_step_uuid == Some(steps[1].workflow_step_uuid))
        .unwrap();
    let pending_step_result = readiness_results
        .iter()
        .find(|r| r.workflow_step_uuid == Some(steps[2].workflow_step_uuid))
        .unwrap();

    // Error state step should NOT be ready for execution
    assert_eq!(error_step_result.current_state, Some("error".to_string()));
    assert_eq!(error_step_result.ready_for_execution, Some(false));

    // WaitingForRetry step should be ready if backoff expired (or false if not expired)
    assert_eq!(
        waiting_step_result.current_state,
        Some("waiting_for_retry".to_string())
    );
    // Note: ready_for_execution depends on backoff timing

    // Pending step should be ready for execution
    assert_eq!(
        pending_step_result.current_state,
        Some("pending".to_string())
    );
    assert_eq!(pending_step_result.ready_for_execution, Some(true));

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_task_finalization_state_transitions_integrity(pool: PgPool) -> Result<()> {
    let system_context = create_test_system_context(pool.clone()).await?;

    // Create task and initialize it properly
    let (task, steps) = create_test_task_with_steps(&pool, 2).await?;

    // Initialize task through proper state transitions
    let mut task_state_machine = TaskStateMachine::for_task(
        task.task_uuid,
        pool.clone(),
        system_context.processor_uuid(),
    )
    .await?;

    // Start task processing
    task_state_machine.transition(TaskEvent::Start).await?;

    // Put one step in error state
    transition_step_to_state(
        &pool,
        system_context.clone(),
        &steps[0],
        WorkflowStepState::Error,
    )
    .await?;

    // Create task finalizer
    let step_enqueuer_service = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let finalizer = TaskFinalizer::new(system_context.clone(), step_enqueuer_service);

    // Test finalization maintains state transition integrity
    let result = finalizer.finalize_task(task.task_uuid).await?;

    assert_eq!(result.task_uuid, task.task_uuid);
    assert!(matches!(result.action, FinalizationAction::Failed));

    // Verify all state transitions are recorded in database
    let transitions = sqlx::query!(
        "SELECT from_state, to_state, metadata FROM tasker_task_transitions
         WHERE task_uuid = $1 ORDER BY created_at",
        task.task_uuid
    )
    .fetch_all(&pool)
    .await?;

    // Should have at least: pending -> active state, active -> error
    assert!(transitions.len() >= 2);

    // Final transition should be to error state
    let final_transition = transitions.last().unwrap();
    assert_eq!(final_transition.to_state, "error");

    Ok(())
}
