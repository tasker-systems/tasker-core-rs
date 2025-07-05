use tasker_core::state_machine::errors::*;

#[test]
fn test_error_chain() {
    let guard_err = dependencies_not_met("All steps must be complete");
    let sm_err: StateMachineError = guard_err.into();

    match sm_err {
        StateMachineError::GuardFailed { reason } => {
            assert!(reason.contains("Dependencies not satisfied"));
        }
        _ => panic!("Expected GuardFailed error"),
    }
}

#[test]
fn test_error_messages() {
    let err = PersistenceError::StateResolutionFailed { entity_id: 123 };
    assert_eq!(err.to_string(), "Failed to resolve current state: 123");

    let err = ActionError::EventPublishFailed {
        event_name: "task.completed".to_string(),
    };
    assert_eq!(err.to_string(), "Event publishing failed: task.completed");
}
