//! Integration test for legacy boolean flag updates during state transitions
//!
//! This test verifies that when state machines transition states,
//! the legacy boolean flags are properly updated in the database.

use tasker_core::models::core::task::Task;
use tasker_core::models::core::workflow_step::WorkflowStep;
use tasker_core::state_machine::task_state_machine::TaskStateMachine;
use tasker_core::state_machine::step_state_machine::StepStateMachine;
use tasker_core::state_machine::events::{TaskEvent, StepEvent};
use tasker_core::state_machine::states::{TaskState, WorkflowStepState};
use tasker_core::events::publisher::EventPublisher;

/// Test that Task.complete is set when task transitions to complete state
#[sqlx::test]
async fn test_task_complete_flag_updated_on_state_transition(pool: sqlx::PgPool) {
    // Create a test task
    let task = Task {
        task_id: 1,
        named_task_id: "test_task".to_string(),
        task_type: "test".to_string(),
        context: Some(serde_json::json!({"test": "data"})),
        current_user_id: Some(1),
        current_user_type: Some("User".to_string()),
        namespace: Some("test".to_string()),
        complete: false, // Initially not complete
        ..Default::default()
    };

    // Create state machine
    let event_publisher = EventPublisher::default();
    let mut state_machine = TaskStateMachine::new(task.clone(), pool.clone(), event_publisher);

    // Transition to complete state
    let result = state_machine.transition(TaskEvent::Complete).await;
    
    // Verify transition succeeded
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), TaskState::Complete);
    
    // Verify the task.complete flag was updated in the database
    let updated_task = Task::find_by_id(&pool, task.task_id).await.unwrap().unwrap();
    assert_eq!(updated_task.complete, true, "Task.complete should be true after completing state transition");
}

/// Test that WorkflowStep.in_process is set when step transitions to in_progress state
#[sqlx::test]
async fn test_workflow_step_in_process_flag_updated_on_state_transition(pool: sqlx::PgPool) {
    // Create a test workflow step
    let workflow_step = WorkflowStep {
        workflow_step_id: 1,
        task_id: 1,
        named_step_id: "test_step".to_string(),
        step_type: "test".to_string(),
        in_process: false, // Initially not in process
        processed: false,
        processed_at: None,
        ..Default::default()
    };

    // Create state machine
    let event_publisher = EventPublisher::default();
    let mut state_machine = StepStateMachine::new(workflow_step.clone(), pool.clone(), event_publisher);

    // Transition to in_progress state
    let result = state_machine.transition(StepEvent::Start).await;
    
    // Verify transition succeeded
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), WorkflowStepState::InProgress);
    
    // Verify the workflow_step.in_process flag was updated in the database
    let updated_step = WorkflowStep::find_by_id(&pool, workflow_step.workflow_step_id).await.unwrap().unwrap();
    assert_eq!(updated_step.in_process, true, "WorkflowStep.in_process should be true after transitioning to in_progress state");
}

/// Test that WorkflowStep.processed and processed_at are set when step transitions to complete state
#[sqlx::test]
async fn test_workflow_step_processed_flag_updated_on_state_transition(pool: sqlx::PgPool) {
    // Create a test workflow step
    let workflow_step = WorkflowStep {
        workflow_step_id: 1,
        task_id: 1,
        named_step_id: "test_step".to_string(),
        step_type: "test".to_string(),
        in_process: false,
        processed: false, // Initially not processed
        processed_at: None,
        ..Default::default()
    };

    // Create state machine
    let event_publisher = EventPublisher::default();
    let mut state_machine = StepStateMachine::new(workflow_step.clone(), pool.clone(), event_publisher);

    // Transition to complete state with results
    let results = Some(serde_json::json!({"result": "success"}));
    let result = state_machine.transition(StepEvent::Complete(results.clone())).await;
    
    // Verify transition succeeded
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), WorkflowStepState::Complete);
    
    // Verify the workflow_step.processed and processed_at flags were updated in the database
    let updated_step = WorkflowStep::find_by_id(&pool, workflow_step.workflow_step_id).await.unwrap().unwrap();
    assert_eq!(updated_step.processed, true, "WorkflowStep.processed should be true after transitioning to complete state");
    assert!(updated_step.processed_at.is_some(), "WorkflowStep.processed_at should be set after transitioning to complete state");
}