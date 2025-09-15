use tasker_shared::state_machine::errors::*;
use uuid::Uuid;

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
    let test_uuid = Uuid::now_v7();
    let err = PersistenceError::StateResolutionFailed {
        entity_id: test_uuid.to_string(),
    };
    assert_eq!(
        err.to_string(),
        format!("Failed to resolve current state: {test_uuid}")
    );

    let err = ActionError::EventPublishFailed {
        event_name: "task.completed".to_string(),
    };
    assert_eq!(err.to_string(), "Event publishing failed: task.completed");
}
