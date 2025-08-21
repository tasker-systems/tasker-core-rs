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
fn test_step_state_dependency_satisfaction() {
    assert!(WorkflowStepState::Complete.satisfies_dependencies());
    assert!(WorkflowStepState::ResolvedManually.satisfies_dependencies());
    assert!(!WorkflowStepState::Pending.satisfies_dependencies());
    assert!(!WorkflowStepState::InProgress.satisfies_dependencies());
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
}

#[test]
fn test_state_serde() {
    let task_state = TaskState::InProgress;
    let json = serde_json::to_string(&task_state).unwrap();
    assert_eq!(json, "\"in_progress\"");

    let parsed: TaskState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, task_state);
}
