//! Step State Machine Tests
//!
//! Tests for the step state machine implementation using SQLx native testing
//! for automatic database isolation.

use serde_json::json;
use sqlx::PgPool;
use tasker_shared::events::publisher::EventPublisher;
use tasker_shared::models::WorkflowStep;
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::states::WorkflowStepState;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use uuid::Uuid;

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_step_state_transitions(pool: PgPool) -> sqlx::Result<()> {
    // Test valid transitions
    let sm = create_test_step_state_machine(pool);

    assert_eq!(
        sm.determine_target_state(WorkflowStepState::Pending, &StepEvent::Start)
            .unwrap(),
        WorkflowStepState::InProgress
    );

    assert_eq!(
        sm.determine_target_state(WorkflowStepState::InProgress, &StepEvent::Complete(None))
            .unwrap(),
        WorkflowStepState::Complete
    );

    assert_eq!(
        sm.determine_target_state(
            WorkflowStepState::InProgress,
            &StepEvent::Fail("error".to_string())
        )
        .unwrap(),
        WorkflowStepState::Error
    );

    assert_eq!(
        sm.determine_target_state(WorkflowStepState::Error, &StepEvent::Retry)
            .unwrap(),
        WorkflowStepState::Pending
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_enqueued_for_orchestration_transitions(pool: PgPool) -> sqlx::Result<()> {
    // TAS-41: Test the new EnqueuedForOrchestration state transitions
    let sm = create_test_step_state_machine(pool);

    // Worker transitions InProgress to EnqueuedForOrchestration
    assert_eq!(
        sm.determine_target_state(
            WorkflowStepState::InProgress,
            &StepEvent::EnqueueForOrchestration(Some(json!({"success": true})))
        )
        .unwrap(),
        WorkflowStepState::EnqueuedForOrchestration
    );

    // Orchestration transitions EnqueuedForOrchestration to Complete
    assert_eq!(
        sm.determine_target_state(
            WorkflowStepState::EnqueuedForOrchestration,
            &StepEvent::Complete(None)
        )
        .unwrap(),
        WorkflowStepState::Complete
    );

    // Orchestration transitions EnqueuedForOrchestration to Error
    assert_eq!(
        sm.determine_target_state(
            WorkflowStepState::EnqueuedForOrchestration,
            &StepEvent::Fail("orchestration error".to_string())
        )
        .unwrap(),
        WorkflowStepState::Error
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_step_invalid_transitions(pool: PgPool) -> sqlx::Result<()> {
    let sm = create_test_step_state_machine(pool);

    // Cannot start from complete state
    assert!(sm
        .determine_target_state(WorkflowStepState::Complete, &StepEvent::Start)
        .is_err());

    // Cannot retry from pending state
    assert!(sm
        .determine_target_state(WorkflowStepState::Pending, &StepEvent::Retry)
        .is_err());

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_step_completion_with_results(pool: PgPool) -> sqlx::Result<()> {
    let sm = create_test_step_state_machine(pool);
    let results = json!({"processed": 42, "status": "success"});

    assert_eq!(
        sm.determine_target_state(
            WorkflowStepState::InProgress,
            &StepEvent::Complete(Some(results))
        )
        .unwrap(),
        WorkflowStepState::Complete
    );

    Ok(())
}

fn create_test_step_state_machine(pool: PgPool) -> StepStateMachine {
    use chrono::NaiveDateTime;

    // Use static timestamp instead of dynamic timestamp
    let static_timestamp =
        NaiveDateTime::parse_from_str("2023-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

    let step = WorkflowStep {
        workflow_step_uuid: Uuid::now_v7(),
        task_uuid: Uuid::now_v7(),
        named_step_uuid: Uuid::now_v7(),
        retryable: true,
        retry_limit: Some(3),
        in_process: false,
        processed: false,
        processed_at: None,
        attempts: Some(0),
        last_attempted_at: None,
        backoff_request_seconds: None,
        inputs: Some(json!({})),
        results: Some(json!({})),
        skippable: false,
        created_at: static_timestamp,
        updated_at: static_timestamp,
    };

    // Use the public constructor with provided pool
    StepStateMachine::new(step, pool, EventPublisher::default())
}
