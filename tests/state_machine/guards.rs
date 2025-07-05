use tasker_core::state_machine::guards::*;
use tasker_core::state_machine::states::TaskState;

// Note: These tests would require a test database setup
// For now, we're just testing the structure

#[test]
fn test_guard_descriptions() {
    assert_eq!(
        AllStepsCompleteGuard.description(),
        "All workflow steps must be complete"
    );
    assert_eq!(
        StepDependenciesMetGuard.description(),
        "All step dependencies must be satisfied"
    );
    assert_eq!(
        TaskNotInProgressGuard.description(),
        "Task must not already be in progress"
    );
}

#[tokio::test]
async fn test_state_parsing() {
    // Test that state parsing works correctly
    let valid_state = "complete".parse::<TaskState>().unwrap();
    assert_eq!(valid_state, TaskState::Complete);

    let invalid_state = "invalid_state".parse::<TaskState>();
    assert!(invalid_state.is_err());
}
