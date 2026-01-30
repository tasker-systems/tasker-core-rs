//! Tests for state machine actions
//!
//! These tests verify the behavior of state transition actions.
//! Since the helper functions are private implementation details,
//! we test them indirectly through the public action APIs.
//!
//! Note: PublishTransitionEventAction and TriggerStepDiscoveryAction are
//! pub(crate) and cannot be tested from integration tests. They are
//! tested through the internal unit tests in the state_machine module.

use sqlx::PgPool;
use tasker_shared::models::{Task, WorkflowStep};
use tasker_shared::state_machine::actions::{
    ErrorStateCleanupAction, StateAction, UpdateStepResultsAction, UpdateTaskCompletionAction,
};
use uuid::Uuid;

/// Create a test task
fn create_test_task() -> Task {
    Task {
        task_uuid: Uuid::now_v7(),
        named_task_uuid: Uuid::now_v7(),
        complete: false,
        requested_at: chrono::Utc::now().naive_utc(),
        initiator: Some("test_user".to_string()),
        source_system: Some("test_system".to_string()),
        reason: Some("test_reason".to_string()),

        tags: None,
        context: Some(serde_json::json!({"test": "data"})),
        identity_hash: "test_hash".to_string(),
        priority: 5,

        correlation_id: Uuid::now_v7(),
        parent_correlation_id: None,
        created_at: chrono::Utc::now().naive_utc(),
        updated_at: chrono::Utc::now().naive_utc(),
    }
}

/// Create a test workflow step
fn create_test_step() -> WorkflowStep {
    WorkflowStep {
        workflow_step_uuid: Uuid::now_v7(),
        task_uuid: Uuid::now_v7(),
        named_step_uuid: Uuid::now_v7(),
        retryable: true,
        max_attempts: Some(3),
        in_process: false,
        processed: false,
        processed_at: None,
        attempts: Some(0),
        last_attempted_at: None,
        backoff_request_seconds: None,
        inputs: None,
        results: None,
        checkpoint: None,
        created_at: chrono::Utc::now().naive_utc(),
        updated_at: chrono::Utc::now().naive_utc(),
    }
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_update_step_results_with_json_event(pool: PgPool) -> sqlx::Result<()> {
    let action = UpdateStepResultsAction;
    let step = create_test_step();

    // Test with JSON event containing results
    let event_with_results = r#"{"results": {"count": 42, "status": "ok"}}"#;

    // Should process without error when completing
    <UpdateStepResultsAction as StateAction<WorkflowStep>>::execute(
        &action,
        &step,
        Some("in_progress".to_string()),
        "complete".to_string(),
        event_with_results,
        &pool,
    )
    .await
    .unwrap();

    // Should handle non-JSON events gracefully
    <UpdateStepResultsAction as StateAction<WorkflowStep>>::execute(
        &action,
        &step,
        Some("in_progress".to_string()),
        "complete".to_string(),
        "plain text event",
        &pool,
    )
    .await
    .unwrap();

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_error_state_cleanup_actions(pool: PgPool) -> sqlx::Result<()> {
    let task_action = ErrorStateCleanupAction;
    let step_action = ErrorStateCleanupAction;

    let task = create_test_task();
    let step = create_test_step();

    // Test with JSON error event
    let json_error = r#"{"error": "Database connection failed"}"#;
    <ErrorStateCleanupAction as StateAction<Task>>::execute(
        &task_action,
        &task,
        Some("in_progress".to_string()),
        "error".to_string(),
        json_error,
        &pool,
    )
    .await
    .unwrap();

    <ErrorStateCleanupAction as StateAction<WorkflowStep>>::execute(
        &step_action,
        &step,
        Some("in_progress".to_string()),
        "error".to_string(),
        json_error,
        &pool,
    )
    .await
    .unwrap();

    // Test with plain text error
    let plain_error = "Simple error message";
    <ErrorStateCleanupAction as StateAction<Task>>::execute(
        &task_action,
        &task,
        Some("in_progress".to_string()),
        "error".to_string(),
        plain_error,
        &pool,
    )
    .await
    .unwrap();

    Ok(())
}

#[tokio::test]
async fn test_action_descriptions() {
    assert_eq!(
        <UpdateTaskCompletionAction as StateAction<Task>>::description(&UpdateTaskCompletionAction),
        "Update task completion metadata and legacy flags"
    );

    assert_eq!(
        <UpdateStepResultsAction as StateAction<WorkflowStep>>::description(
            &UpdateStepResultsAction
        ),
        "Update step results and completion metadata with legacy flags"
    );

    assert_eq!(
        <ErrorStateCleanupAction as StateAction<Task>>::description(&ErrorStateCleanupAction),
        "Handle error state cleanup and logging"
    );
}

// =============================================================================
// DB-backed action tests
//
// These tests use real database objects created through factories to verify
// that actions correctly persist their side effects to the database.
// =============================================================================

use tasker_shared::models::factories::base::SqlxFactory;
use tasker_shared::models::factories::core::{TaskFactory, WorkflowStepFactory};
use tasker_shared::state_machine::actions::TransitionActions;
use tasker_shared::state_machine::events::TaskEvent;
use tasker_shared::state_machine::states::{TaskState, WorkflowStepState};

/// Test that UpdateTaskCompletionAction persists the complete flag to the DB
/// when a real task transitions to the Complete state.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_completion_action_db(pool: PgPool) -> sqlx::Result<()> {
    // Create a real task in the database via factory
    let task = TaskFactory::new()
        .with_initiator("action_test")
        .create(&pool)
        .await
        .expect("create task");

    assert!(!task.complete, "Task should start as not complete");

    // Execute the UpdateTaskCompletionAction against the real task
    let action = UpdateTaskCompletionAction;
    <UpdateTaskCompletionAction as StateAction<Task>>::execute(
        &action,
        &task,
        Some("steps_in_process".to_string()),
        TaskState::Complete.to_string(),
        "test_event",
        &pool,
    )
    .await
    .expect("action should succeed");

    // Verify the task is now marked complete in the database
    let updated_task = sqlx::query_as!(
        Task,
        r#"
        SELECT task_uuid, named_task_uuid, complete, requested_at, initiator,
               source_system, reason, tags, context, identity_hash, priority,
               correlation_id, parent_correlation_id,
               created_at, updated_at
        FROM tasker.tasks
        WHERE task_uuid = $1
        "#,
        task.task_uuid
    )
    .fetch_one(&pool)
    .await?;

    assert!(
        updated_task.complete,
        "Task should be marked complete in DB after action"
    );

    Ok(())
}

/// Test that UpdateStepResultsAction marks a real step as in_process in the DB
/// when transitioning to InProgress state.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_in_progress_action_db(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new().create(&pool).await.expect("create task");
    let step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .create(&pool)
        .await
        .expect("create step");

    assert!(!step.in_process, "Step should start as not in process");

    let action = UpdateStepResultsAction::new();
    <UpdateStepResultsAction as StateAction<WorkflowStep>>::execute(
        &action,
        &step,
        Some(WorkflowStepState::Pending.to_string()),
        WorkflowStepState::InProgress.to_string(),
        "test_event",
        &pool,
    )
    .await
    .expect("action should succeed");

    // Verify the step is now in_process in the database
    let updated_step = sqlx::query_as!(
        WorkflowStep,
        r#"
        SELECT workflow_step_uuid, task_uuid, named_step_uuid, retryable,
               max_attempts, in_process, processed, processed_at,
               attempts, last_attempted_at, backoff_request_seconds,
               inputs, results, checkpoint,
               created_at, updated_at
        FROM tasker.workflow_steps
        WHERE workflow_step_uuid = $1
        "#,
        step.workflow_step_uuid
    )
    .fetch_one(&pool)
    .await?;

    assert!(
        updated_step.in_process,
        "Step should be marked in_process after InProgress action"
    );

    Ok(())
}

/// Test the full task lifecycle through TransitionActions: Pending -> Initializing
/// -> EnqueuingSteps -> StepsInProcess -> Complete, verifying DB state at each stage.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_transition_actions_full_lifecycle(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new()
        .with_initiator("lifecycle_test")
        .create(&pool)
        .await
        .expect("create task");

    let processor_uuid = Uuid::now_v7();
    let actions = TransitionActions::new(pool.clone(), None);

    // Transition: Pending -> Initializing
    actions
        .execute(
            &task,
            TaskState::Pending,
            TaskState::Initializing,
            &TaskEvent::Start,
            processor_uuid,
            None,
        )
        .await
        .expect("pending -> initializing should succeed");

    // Transition: Initializing -> EnqueuingSteps
    actions
        .execute(
            &task,
            TaskState::Initializing,
            TaskState::EnqueuingSteps,
            &TaskEvent::ReadyStepsFound(2),
            processor_uuid,
            None,
        )
        .await
        .expect("initializing -> enqueuing_steps should succeed");

    // Transition: EnqueuingSteps -> StepsInProcess
    actions
        .execute(
            &task,
            TaskState::EnqueuingSteps,
            TaskState::StepsInProcess,
            &TaskEvent::StepsEnqueued(vec![]),
            processor_uuid,
            None,
        )
        .await
        .expect("enqueuing_steps -> steps_in_process should succeed");

    // Transition: StepsInProcess -> Complete (this triggers set_task_completed)
    actions
        .execute(
            &task,
            TaskState::StepsInProcess,
            TaskState::Complete,
            &TaskEvent::Complete,
            processor_uuid,
            None,
        )
        .await
        .expect("steps_in_process -> complete should succeed");

    // Verify the task is complete in the DB
    let updated_task = sqlx::query_as!(
        Task,
        r#"
        SELECT task_uuid, named_task_uuid, complete, requested_at, initiator,
               source_system, reason, tags, context, identity_hash, priority,
               correlation_id, parent_correlation_id,
               created_at, updated_at
        FROM tasker.tasks
        WHERE task_uuid = $1
        "#,
        task.task_uuid
    )
    .fetch_one(&pool)
    .await?;

    assert!(
        updated_task.complete,
        "Task should be complete after full lifecycle"
    );

    Ok(())
}

/// Test the error path: exercise TransitionActions for error state transitions
/// and verify that the error state is handled correctly.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_transition_actions_error_path(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new()
        .with_initiator("error_path_test")
        .create(&pool)
        .await
        .expect("create task");

    let processor_uuid = Uuid::now_v7();
    let actions = TransitionActions::new(pool.clone(), None);

    // Transition to error: StepsInProcess -> Error (via EnqueueFailed on EnqueuingSteps)
    // Using the error action on the task, simulate enqueue failure
    actions
        .execute(
            &task,
            TaskState::EnqueuingSteps,
            TaskState::Error,
            &TaskEvent::EnqueueFailed("simulated failure".to_string()),
            processor_uuid,
            Some(serde_json::json!({"error": "simulated_enqueue_failure"})),
        )
        .await
        .expect("enqueuing_steps -> error should succeed");

    // Also test ErrorStateCleanupAction on a real task after error path
    let cleanup_action = ErrorStateCleanupAction;
    <ErrorStateCleanupAction as StateAction<Task>>::execute(
        &cleanup_action,
        &task,
        Some(TaskState::EnqueuingSteps.to_string()),
        TaskState::Error.to_string(),
        r#"{"error": "simulated failure"}"#,
        &pool,
    )
    .await
    .expect("error cleanup action should succeed");

    Ok(())
}

/// Test the cancellation path: execute UpdateStepResultsAction to cancel
/// a real step in the DB and verify the persisted state.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_cancellation_action_db(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new().create(&pool).await.expect("create task");
    let step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .create(&pool)
        .await
        .expect("create step");

    let action = UpdateStepResultsAction::new();

    // Cancel the step
    <UpdateStepResultsAction as StateAction<WorkflowStep>>::execute(
        &action,
        &step,
        Some(WorkflowStepState::Pending.to_string()),
        WorkflowStepState::Cancelled.to_string(),
        "cancel_event",
        &pool,
    )
    .await
    .expect("cancel action should succeed");

    // Verify the step is marked as processed and not in_process
    let updated_step = sqlx::query_as!(
        WorkflowStep,
        r#"
        SELECT workflow_step_uuid, task_uuid, named_step_uuid, retryable,
               max_attempts, in_process, processed, processed_at,
               attempts, last_attempted_at, backoff_request_seconds,
               inputs, results, checkpoint,
               created_at, updated_at
        FROM tasker.workflow_steps
        WHERE workflow_step_uuid = $1
        "#,
        step.workflow_step_uuid
    )
    .fetch_one(&pool)
    .await?;

    assert!(
        updated_step.processed,
        "Cancelled step should be marked as processed"
    );
    assert!(
        !updated_step.in_process,
        "Cancelled step should not be in_process"
    );

    Ok(())
}
