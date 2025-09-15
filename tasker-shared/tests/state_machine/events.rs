//! State Machine Events Tests
//!
//! Unit tests for task and step events that trigger state transitions.
//! These tests verify event properties, serialization, and business logic.

use serde_json::json;
use tasker_shared::state_machine::events::{StepEvent, TaskEvent};

#[test]
fn test_task_event_properties() {
    let start = TaskEvent::Start;
    assert_eq!(start.event_type(), "start");
    assert!(!start.is_terminal());

    let complete = TaskEvent::Complete;
    assert_eq!(complete.event_type(), "complete");
    assert!(complete.is_terminal());

    let fail = TaskEvent::fail_with_error("Database error");
    assert_eq!(fail.event_type(), "fail");
    assert_eq!(fail.error_message(), Some("Database error"));
    assert!(!fail.is_terminal());
}

#[test]
fn test_step_event_properties() {
    let start = StepEvent::Start;
    assert_eq!(start.event_type(), "start");
    assert!(!start.is_terminal());

    let results = json!({"processed": 42, "status": "ok"});
    let complete = StepEvent::complete_with_results(results.clone());
    assert_eq!(complete.event_type(), "complete");
    assert_eq!(complete.results(), Some(&results));
    assert!(complete.is_terminal());

    let retry = StepEvent::Retry;
    assert!(retry.allows_retry());
    assert!(!retry.is_terminal());
}

#[test]
fn test_event_serde() {
    let task_event = TaskEvent::fail_with_error("Network timeout");
    let json = serde_json::to_string(&task_event).unwrap();
    let parsed: TaskEvent = serde_json::from_str(&json).unwrap();

    match parsed {
        TaskEvent::Fail(msg) => assert_eq!(msg, "Network timeout"),
        _ => panic!("Expected Fail event"),
    }

    let step_event = StepEvent::complete_with_results(json!({"count": 5}));
    let json = serde_json::to_string(&step_event).unwrap();
    let parsed: StepEvent = serde_json::from_str(&json).unwrap();

    match parsed {
        StepEvent::Complete(Some(results)) => {
            assert_eq!(results["count"], 5);
        }
        _ => panic!("Expected Complete event with results"),
    }
}
