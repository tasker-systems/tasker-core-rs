//! State Machine Persistence Tests
//!
//! Tests for the state machine persistence component using SQLx native testing
//! for automatic database isolation.

#[test]
fn test_metadata_creation() {
    let metadata = serde_json::json!({
        "event": "start",
        "timestamp": "2024-01-01T00:00:00Z",
    });

    assert_eq!(metadata["event"], "start");
    assert!(metadata["timestamp"].is_string());
}

#[test]
fn test_idempotent_logic() {
    // Test the logical behavior of idempotent operations
    // This tests the concept without requiring database setup

    // Mock behavior: if current state equals target state, no transition needed
    let current_state = Some("in_progress".to_string());
    let target_state = "in_progress".to_string();

    let should_transition = current_state.as_ref() != Some(&target_state);
    assert!(
        !should_transition,
        "Should not transition when already in target state"
    );

    // Test different states - should transition
    let current_state = Some("pending".to_string());
    let target_state = "in_progress".to_string();

    let should_transition = current_state.as_ref() != Some(&target_state);
    assert!(should_transition, "Should transition when states differ");

    // Test no current state - should transition
    let current_state: Option<String> = None;
    let target_state = "in_progress".to_string();

    let should_transition = current_state.as_ref() != Some(&target_state);
    assert!(should_transition, "Should transition when no current state");
}

// =============================================================================
// DB-backed persistence tests
//
// These tests exercise the TransitionPersistence trait implementations
// (TaskTransitionPersistence and StepTransitionPersistence) against a real
// PostgreSQL database, verifying transitions are correctly persisted and
// can be resolved back.
// =============================================================================

use sqlx::PgPool;
use tasker_shared::models::factories::base::SqlxFactory;
use tasker_shared::models::factories::core::{TaskFactory, WorkflowStepFactory};
use tasker_shared::models::{TaskTransition, WorkflowStepTransition};
use tasker_shared::state_machine::persistence::{
    idempotent_transition, StepTransitionPersistence, TaskTransitionPersistence,
    TransitionPersistence,
};

/// Test persisting a task transition via TaskTransitionPersistence and resolving it back.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_transition_persist_and_resolve(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new()
        .with_initiator("persistence_test")
        .create(&pool)
        .await
        .expect("create task");

    let persistence = TaskTransitionPersistence;

    // Persist a transition from None -> pending
    persistence
        .persist_transition(
            &task,
            None,
            "pending".to_string(),
            "start",
            Some(serde_json::json!({"test": true})),
            &pool,
        )
        .await
        .expect("persist transition should succeed");

    // Resolve the current state
    let current_state = persistence
        .resolve_current_state(task.task_uuid, &pool)
        .await
        .expect("resolve state should succeed");

    assert_eq!(
        current_state,
        Some("pending".to_string()),
        "Current state should be 'pending' after persisting"
    );

    // Verify the transition was recorded in the database
    let transitions = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert_eq!(transitions.len(), 1, "Should have exactly one transition");
    assert_eq!(transitions[0].to_state, "pending");
    assert!(
        transitions[0].most_recent,
        "Should be marked as most recent"
    );
    assert!(
        transitions[0].metadata.is_some(),
        "Metadata should be present"
    );

    Ok(())
}

/// Test persisting a step transition via StepTransitionPersistence and resolving it back.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_transition_persist_and_resolve(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new().create(&pool).await.expect("create task");
    let step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .create(&pool)
        .await
        .expect("create step");

    let persistence = StepTransitionPersistence;

    // Persist a transition from None -> pending
    persistence
        .persist_transition(&step, None, "pending".to_string(), "enqueue", None, &pool)
        .await
        .expect("persist step transition should succeed");

    // Resolve the current state
    let current_state = persistence
        .resolve_current_state(step.workflow_step_uuid, &pool)
        .await
        .expect("resolve step state should succeed");

    assert_eq!(
        current_state,
        Some("pending".to_string()),
        "Current step state should be 'pending'"
    );

    // Verify the transition was recorded in the database
    let transitions =
        WorkflowStepTransition::list_by_workflow_step(&pool, step.workflow_step_uuid).await?;
    assert_eq!(
        transitions.len(),
        1,
        "Should have exactly one step transition"
    );
    assert_eq!(transitions[0].to_state, "pending");
    assert!(transitions[0].most_recent);

    Ok(())
}

/// Test that persisting multiple transitions increments the sort_key correctly.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_persistence_sort_key_increment(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new().create(&pool).await.expect("create task");

    let persistence = TaskTransitionPersistence;

    // Persist three transitions in sequence
    let states = vec![
        (None, "pending"),
        (Some("pending".to_string()), "initializing"),
        (Some("initializing".to_string()), "enqueuing_steps"),
    ];

    for (from_state, to_state) in &states {
        persistence
            .persist_transition(
                &task,
                from_state.clone(),
                to_state.to_string(),
                "test_event",
                None,
                &pool,
            )
            .await
            .expect("persist transition should succeed");
    }

    // Verify sort keys are sequential
    let transitions = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert_eq!(transitions.len(), 3, "Should have three transitions");

    // Transitions are ordered by sort_key ASC from list_by_task
    assert_eq!(transitions[0].sort_key, 1, "First sort_key should be 1");
    assert_eq!(transitions[1].sort_key, 2, "Second sort_key should be 2");
    assert_eq!(transitions[2].sort_key, 3, "Third sort_key should be 3");

    // Verify only the last transition is most_recent
    assert!(
        !transitions[0].most_recent,
        "First transition should not be most_recent"
    );
    assert!(
        !transitions[1].most_recent,
        "Second transition should not be most_recent"
    );
    assert!(
        transitions[2].most_recent,
        "Third transition should be most_recent"
    );

    // Verify get_next_sort_key returns the correct value
    let next_key = persistence
        .get_next_sort_key(task.task_uuid, &pool)
        .await
        .expect("get_next_sort_key should succeed");
    assert_eq!(
        next_key, 4,
        "Next sort key should be 4 after three transitions"
    );

    Ok(())
}

/// Test persisting transitions with rich metadata and verifying the
/// serialization round-trip preserves the data correctly.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_persistence_with_metadata(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new().create(&pool).await.expect("create task");

    let persistence = TaskTransitionPersistence;

    // Create metadata with rich structure
    let metadata = serde_json::json!({
        "event": "start",
        "processor": "test-worker-01",
        "context": {
            "order_id": 12345,
            "priority": "high",
            "tags": ["urgent", "billing"]
        },
        "timing": {
            "enqueued_at": "2024-01-01T00:00:00Z",
            "started_at": "2024-01-01T00:01:00Z"
        }
    });

    persistence
        .persist_transition(
            &task,
            None,
            "pending".to_string(),
            "start",
            Some(metadata.clone()),
            &pool,
        )
        .await
        .expect("persist with metadata should succeed");

    // Read back the transition and verify metadata round-trip
    let transitions = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert_eq!(transitions.len(), 1);

    let stored_metadata = transitions[0]
        .metadata
        .as_ref()
        .expect("metadata should be stored");

    // The persistence layer wraps user metadata, but it should contain the event key
    // since TaskTransitionPersistence uses the provided metadata or constructs one with
    // "event" and "timestamp" keys
    assert!(
        stored_metadata.is_object(),
        "Stored metadata should be a JSON object"
    );

    // Since persist_transition uses metadata.unwrap_or_else which constructs default,
    // when we pass Some(metadata), the metadata is directly used as transition_metadata
    assert_eq!(
        stored_metadata["event"], "start",
        "Event should be preserved in metadata"
    );
    assert_eq!(
        stored_metadata["processor"], "test-worker-01",
        "Processor should be preserved in metadata"
    );
    assert_eq!(
        stored_metadata["context"]["order_id"], 12345,
        "Nested context should be preserved"
    );

    Ok(())
}

/// Test the idempotent_transition helper function: if the entity is already
/// in the target state, no new transition should be created.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_persistence_idempotent_check(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new().create(&pool).await.expect("create task");

    let persistence = TaskTransitionPersistence;

    // First transition: None -> pending (should create)
    let transitioned = idempotent_transition(
        &persistence,
        &task,
        task.task_uuid,
        "pending".to_string(),
        "start",
        None,
        &pool,
    )
    .await
    .expect("first idempotent transition should succeed");
    assert!(transitioned, "First transition should actually occur");

    // Second transition: pending -> pending (same state, should be idempotent)
    let transitioned = idempotent_transition(
        &persistence,
        &task,
        task.task_uuid,
        "pending".to_string(),
        "start",
        None,
        &pool,
    )
    .await
    .expect("idempotent transition should succeed");
    assert!(
        !transitioned,
        "Second transition to same state should be idempotent (no transition)"
    );

    // Verify only one transition exists
    let transitions = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert_eq!(
        transitions.len(),
        1,
        "Idempotent call should not create a second transition"
    );

    // Third transition: pending -> initializing (different state, should create)
    let transitioned = idempotent_transition(
        &persistence,
        &task,
        task.task_uuid,
        "initializing".to_string(),
        "ready_steps_found",
        None,
        &pool,
    )
    .await
    .expect("transition to different state should succeed");
    assert!(transitioned, "Transition to a different state should occur");

    // Now there should be two transitions
    let transitions = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert_eq!(
        transitions.len(),
        2,
        "Should have two transitions after non-idempotent call"
    );

    Ok(())
}
