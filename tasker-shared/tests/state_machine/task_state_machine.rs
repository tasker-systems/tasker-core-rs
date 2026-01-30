//! Task State Machine Tests
//!
//! Tests for the task state machine component using SQLx native testing
//! for automatic database isolation.

use sqlx::PgPool;
use tasker_shared::state_machine::events::TaskEvent;
use tasker_shared::state_machine::states::TaskState;
use tasker_shared::state_machine::task_state_machine::TaskStateMachine;
use uuid::Uuid;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_state_transitions(pool: PgPool) -> sqlx::Result<()> {
    // Test valid transitions using for_task constructor
    let sm = create_test_state_machine(pool.clone()).await;

    // Pending -> Initializing (via Start)
    assert_eq!(
        sm.determine_target_state(TaskState::Pending, &TaskEvent::Start)
            .unwrap(),
        TaskState::Initializing
    );

    // StepsInProcess -> Complete (via legacy Complete event)
    assert_eq!(
        sm.determine_target_state(TaskState::StepsInProcess, &TaskEvent::Complete)
            .unwrap(),
        TaskState::Complete
    );

    // StepsInProcess -> Error (via Fail event)
    assert_eq!(
        sm.determine_target_state(
            TaskState::StepsInProcess,
            &TaskEvent::Fail("error".to_string())
        )
        .unwrap(),
        TaskState::Error
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_invalid_transitions(pool: PgPool) -> sqlx::Result<()> {
    let sm = create_test_state_machine(pool.clone()).await;

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

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_state_machine_creation(pool: PgPool) -> sqlx::Result<()> {
    // Test that we can create a state machine instance
    let sm = create_test_state_machine(pool.clone()).await;

    // Basic validation that the state machine is created with expected values
    assert!(
        !sm.task_uuid().to_string().is_empty(),
        "Task UUID should not be empty"
    );
    assert!(
        !sm.task().task_uuid.to_string().is_empty(),
        "Task UUID should not be empty"
    );
    assert!(
        !sm.task().named_task_uuid.to_string().is_empty(),
        "Named task UUID should not be empty"
    );
    assert!(!sm.task().complete);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_properties(pool: PgPool) -> sqlx::Result<()> {
    let sm = create_test_state_machine(pool.clone()).await;

    // Test task property access - for_task creates a minimal task with placeholder values
    let task = sm.task();
    assert_eq!(task.identity_hash, "placeholder");
    assert_eq!(task.priority, 0);
    assert!(!task.complete);

    Ok(())
}

async fn create_test_state_machine(pool: PgPool) -> TaskStateMachine {
    let task_uuid = Uuid::now_v7();
    let processor_uuid = Uuid::now_v7();

    // Use the for_task constructor which takes simpler arguments
    TaskStateMachine::for_task(task_uuid, pool, processor_uuid)
        .await
        .expect("should create task state machine")
}
