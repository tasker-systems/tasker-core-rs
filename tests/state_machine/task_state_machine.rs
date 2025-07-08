//! Task State Machine Tests
//!
//! Tests for the task state machine component using SQLx native testing
//! for automatic database isolation.

use sqlx::PgPool;
use tasker_core::events::publisher::EventPublisher;
use tasker_core::models::Task;
use tasker_core::state_machine::events::TaskEvent;
use tasker_core::state_machine::states::TaskState;
use tasker_core::state_machine::task_state_machine::TaskStateMachine;

#[sqlx::test]
async fn test_state_transitions(pool: PgPool) -> sqlx::Result<()> {
    // Test valid transitions
    let sm = create_test_state_machine(pool);

    assert_eq!(
        sm.determine_target_state(TaskState::Pending, &TaskEvent::Start)
            .unwrap(),
        TaskState::InProgress
    );

    assert_eq!(
        sm.determine_target_state(TaskState::InProgress, &TaskEvent::Complete)
            .unwrap(),
        TaskState::Complete
    );

    assert_eq!(
        sm.determine_target_state(TaskState::InProgress, &TaskEvent::Fail("error".to_string()))
            .unwrap(),
        TaskState::Error
    );

    Ok(())
}

#[sqlx::test]
async fn test_invalid_transitions(pool: PgPool) -> sqlx::Result<()> {
    let sm = create_test_state_machine(pool);

    // Cannot start from complete state
    assert!(sm
        .determine_target_state(TaskState::Complete, &TaskEvent::Start)
        .is_err());

    // Cannot complete from pending state
    assert!(sm
        .determine_target_state(TaskState::Pending, &TaskEvent::Complete)
        .is_err());

    Ok(())
}

#[sqlx::test]
async fn test_state_machine_creation(pool: PgPool) -> sqlx::Result<()> {
    // Test that we can create a state machine instance
    let sm = create_test_state_machine(pool);

    // Basic validation that the state machine is created with expected values
    assert_eq!(sm.task_id(), 1);
    assert_eq!(sm.task().task_id, 1);
    assert_eq!(sm.task().named_task_id, 1);
    assert!(!sm.task().complete);

    Ok(())
}

#[sqlx::test]
async fn test_task_properties(pool: PgPool) -> sqlx::Result<()> {
    let sm = create_test_state_machine(pool);

    // Test task property access
    let task = sm.task();
    assert_eq!(task.initiator.as_deref(), Some("test"));
    assert_eq!(task.source_system.as_deref(), Some("test_system"));
    assert_eq!(task.reason.as_deref(), Some("test reason"));
    assert_eq!(task.identity_hash, "test_hash");

    Ok(())
}

fn create_test_state_machine(pool: PgPool) -> TaskStateMachine {
    use chrono::NaiveDateTime;

    // Use static timestamp instead of Utc::now()
    let static_timestamp =
        NaiveDateTime::parse_from_str("2023-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

    let task = Task {
        task_id: 1,
        named_task_id: 1,
        complete: false,
        requested_at: static_timestamp,
        initiator: Some("test".to_string()),
        source_system: Some("test_system".to_string()),
        reason: Some("test reason".to_string()),
        bypass_steps: None,
        tags: None,
        context: Some(serde_json::json!({})),
        identity_hash: "test_hash".to_string(),
        created_at: static_timestamp,
        updated_at: static_timestamp,
    };

    // Use the public constructor with provided pool
    TaskStateMachine::new(task, pool, EventPublisher::default())
}
