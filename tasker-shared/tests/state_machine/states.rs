use tasker_shared::state_machine::states::{TaskState, WorkflowStepState};

#[test]
fn test_task_state_terminal_check() {
    assert!(TaskState::Complete.is_terminal());
    assert!(TaskState::Cancelled.is_terminal());
    assert!(TaskState::ResolvedManually.is_terminal());
    assert!(!TaskState::Pending.is_terminal());
    assert!(!TaskState::InProgress.is_terminal());
    assert!(!TaskState::Error.is_terminal());
}

#[test]
fn test_workflow_step_state_terminal_check() {
    // Terminal states
    assert!(WorkflowStepState::Complete.is_terminal());
    assert!(WorkflowStepState::Cancelled.is_terminal());
    assert!(WorkflowStepState::ResolvedManually.is_terminal());
    
    // Non-terminal states
    assert!(!WorkflowStepState::Pending.is_terminal());
    assert!(!WorkflowStepState::Enqueued.is_terminal());
    assert!(!WorkflowStepState::InProgress.is_terminal());
    // TAS-41: EnqueuedForOrchestration is NOT terminal - orchestration can still process it
    assert!(!WorkflowStepState::EnqueuedForOrchestration.is_terminal());
    assert!(!WorkflowStepState::Error.is_terminal());
}

#[test]
fn test_step_state_dependency_satisfaction() {
    assert!(WorkflowStepState::Complete.satisfies_dependencies());
    assert!(WorkflowStepState::ResolvedManually.satisfies_dependencies());
    assert!(!WorkflowStepState::Pending.satisfies_dependencies());
    assert!(!WorkflowStepState::Enqueued.satisfies_dependencies());
    assert!(!WorkflowStepState::InProgress.satisfies_dependencies());
    // TAS-41: EnqueuedForOrchestration should NOT satisfy dependencies
    assert!(!WorkflowStepState::EnqueuedForOrchestration.satisfies_dependencies());
    assert!(!WorkflowStepState::Error.satisfies_dependencies());
    assert!(!WorkflowStepState::Cancelled.satisfies_dependencies());
}

#[test]
fn test_state_string_conversion() {
    assert_eq!(TaskState::InProgress.to_string(), "in_progress");
    assert_eq!(
        "complete".parse::<TaskState>().unwrap(),
        TaskState::Complete
    );

    assert_eq!(WorkflowStepState::Error.to_string(), "error");
    assert_eq!(
        "resolved_manually".parse::<WorkflowStepState>().unwrap(),
        WorkflowStepState::ResolvedManually
    );
    
    // TAS-41: Test EnqueuedForOrchestration string conversion
    assert_eq!(
        WorkflowStepState::EnqueuedForOrchestration.to_string(),
        "enqueued_for_orchestration"
    );
    assert_eq!(
        "enqueued_for_orchestration".parse::<WorkflowStepState>().unwrap(),
        WorkflowStepState::EnqueuedForOrchestration
    );
}

#[test]
fn test_state_serde() {
    let task_state = TaskState::InProgress;
    let json = serde_json::to_string(&task_state).unwrap();
    assert_eq!(json, "\"in_progress\"");

    let parsed: TaskState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, task_state);
}
