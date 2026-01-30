//! Step State Machine Tests
//!
//! Tests for the step state machine transitions and states.
//! Since StepStateMachine::new requires Arc<SystemContext> which is complex
//! to construct in tests, we test step transition logic through:
//! 1. State properties and state string conversions
//! 2. Event properties and serialization
//! 3. Step state lifecycle validation

use serde_json::json;
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::states::WorkflowStepState;

#[test]
fn test_step_state_transitions_pending_to_enqueued() {
    // Pending -> Enqueued is a valid transition via Enqueue event
    let from = WorkflowStepState::Pending;
    let event = StepEvent::Enqueue;

    assert!(!from.is_terminal(), "Pending should not be terminal");
    assert_eq!(from.as_str(), "pending");
    assert_eq!(event.event_type(), "enqueue");
}

#[test]
fn test_step_state_transitions_enqueued_to_in_progress() {
    // Enqueued -> InProgress is valid via Start event
    let from = WorkflowStepState::Enqueued;
    let event = StepEvent::Start;

    assert!(!from.is_terminal());
    assert!(
        from.is_ready_for_claiming(),
        "Enqueued should be ready for claiming"
    );
    assert_eq!(event.event_type(), "start");
}

#[test]
fn test_step_state_transitions_in_progress_to_complete() {
    let from = WorkflowStepState::InProgress;
    let to = WorkflowStepState::Complete;

    assert!(from.is_active(), "InProgress should be active");
    assert!(to.is_terminal(), "Complete should be terminal");
    assert!(
        to.satisfies_dependencies(),
        "Complete should satisfy dependencies"
    );
}

#[test]
fn test_step_state_transitions_in_progress_to_error() {
    let from = WorkflowStepState::InProgress;
    let to = WorkflowStepState::Error;
    let event = StepEvent::Fail("error".to_string());

    assert!(from.is_active());
    assert!(to.is_error());
    assert!(!to.is_terminal(), "Error should not be terminal");
    assert_eq!(event.error_message(), Some("error"));
}

#[test]
fn test_step_state_transitions_error_to_pending_via_retry() {
    let from = WorkflowStepState::Error;
    let event = StepEvent::Retry;

    assert!(from.is_error());
    assert!(
        event.allows_retry(),
        "Retry should be allowed from error state"
    );
    assert_eq!(event.event_type(), "retry");
}

#[test]
fn test_enqueued_for_orchestration_transitions() {
    // TAS-41: Test the EnqueuedForOrchestration state properties
    let enqueued_for_orch = WorkflowStepState::EnqueuedForOrchestration;
    assert!(
        !enqueued_for_orch.is_terminal(),
        "EnqueuedForOrchestration should NOT be terminal"
    );
    assert!(
        !enqueued_for_orch.satisfies_dependencies(),
        "EnqueuedForOrchestration should NOT satisfy dependencies"
    );
    assert!(
        enqueued_for_orch.is_in_processing_pipeline(),
        "EnqueuedForOrchestration should be in processing pipeline"
    );

    // Enqueue for orchestration event
    let event = StepEvent::EnqueueForOrchestration(Some(json!({"success": true})));
    assert_eq!(event.event_type(), "enqueue_for_orchestration");
    assert!(event.results().is_some(), "Should have results");
}

#[test]
fn test_step_invalid_transitions_from_terminal_state() {
    // Complete is terminal - no further transitions should be valid
    let complete = WorkflowStepState::Complete;
    assert!(complete.is_terminal());
    assert!(!complete.is_active());
    assert!(!complete.is_error());
}

#[test]
fn test_step_completion_with_results() {
    let results = json!({"processed": 42, "status": "success"});
    let event = StepEvent::Complete(Some(results.clone()));

    assert_eq!(event.event_type(), "complete");
    assert!(event.is_terminal(), "Complete event should be terminal");
    assert_eq!(
        event.results().unwrap(),
        &results,
        "Event should carry the results"
    );
}

#[test]
fn test_step_cancel_from_various_states() {
    let cancel_event = StepEvent::Cancel;
    assert!(cancel_event.is_terminal(), "Cancel should be terminal");
    assert_eq!(cancel_event.event_type(), "cancel");

    let cancelled = WorkflowStepState::Cancelled;
    assert!(
        cancelled.is_terminal(),
        "Cancelled state should be terminal"
    );
    assert!(!cancelled.satisfies_dependencies());
}

#[test]
fn test_step_waiting_for_retry_state() {
    let waiting = WorkflowStepState::WaitingForRetry;
    assert!(!waiting.is_terminal());
    assert!(!waiting.is_active());
    assert!(!waiting.is_error());

    let event = StepEvent::WaitForRetry("backoff reason".to_string());
    assert_eq!(event.event_type(), "wait_for_retry");
    assert_eq!(event.error_message(), Some("backoff reason"));
}

#[test]
fn test_step_resolved_manually_state() {
    let resolved = WorkflowStepState::ResolvedManually;
    assert!(
        resolved.is_terminal(),
        "ResolvedManually should be terminal"
    );
    assert!(
        resolved.satisfies_dependencies(),
        "ResolvedManually should satisfy dependencies"
    );
}

#[test]
fn test_step_workflow_step_construction() {
    use chrono::NaiveDateTime;
    use tasker_shared::models::WorkflowStep;
    use uuid::Uuid;

    // Use static timestamp instead of dynamic timestamp
    let static_timestamp =
        NaiveDateTime::parse_from_str("2023-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

    // Verify WorkflowStep can be constructed without the removed skippable field
    let step = WorkflowStep {
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
        inputs: Some(json!({})),
        results: Some(json!({})),
        checkpoint: None,
        created_at: static_timestamp,
        updated_at: static_timestamp,
    };

    assert!(!step.in_process);
    assert!(!step.processed);
    assert!(!step.has_exceeded_max_attempts());
    assert!(step.is_processing_eligible());
}
