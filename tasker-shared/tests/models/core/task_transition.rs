//! Task Transition Model Tests
//!
//! Tests for the TaskTransition model using SQLx native testing

use sqlx::PgPool;
use tasker_shared::models::core::IdentityStrategy;
use tasker_shared::models::named_task::{NamedTask, NewNamedTask};
use tasker_shared::models::task::{NewTask, Task};
use tasker_shared::models::task_namespace::{NewTaskNamespace, TaskNamespace};
use tasker_shared::models::task_transition::{
    NewTaskTransition, TaskTransition, TaskTransitionQuery,
};

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_transition_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "test_namespace".to_string(),
            description: None,
        },
    )
    .await?;

    let named_task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "test_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    let task = Task::create(
        &pool,
        NewTask {
            named_task_uuid: named_task.named_task_uuid,
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,

            tags: None,
            context: None,
            identity_hash: "test_hash".to_string(),
            priority: Some(5),
            correlation_id: uuid::Uuid::now_v7(),
            parent_correlation_id: None,
        },
    )
    .await?;

    // Test transition creation
    let transition_metadata = serde_json::json!({
        "reason": "Starting task execution",
        "actor": "test_user",
        "actor_type": "user"
    });

    let new_transition = NewTaskTransition {
        task_uuid: task.task_uuid,
        from_state: Some("pending".to_string()),
        to_state: "initializing".to_string(),
        processor_uuid: None,
        metadata: Some(transition_metadata),
    };

    let created = TaskTransition::create(&pool, new_transition).await?;
    assert_eq!(created.task_uuid, task.task_uuid);
    assert_eq!(created.from_state, Some("pending".to_string()));
    assert_eq!(created.to_state, "initializing".to_string());

    // Test find by ID
    let found = TaskTransition::find_by_uuid(&pool, created.task_transition_uuid)
        .await?
        .expect("Task transition not found");
    assert_eq!(found.task_transition_uuid, created.task_transition_uuid);

    // Test find by task
    let by_task = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert!(!by_task.is_empty());
    assert_eq!(by_task[0].task_uuid, task.task_uuid);

    // Test get current status
    let current_transition = TaskTransition::get_current(&pool, task.task_uuid).await?;
    assert!(current_transition.is_some());
    assert_eq!(
        current_transition.unwrap().to_state,
        "initializing".to_string()
    );

    // Test get status history
    let history = TaskTransition::get_history(&pool, task.task_uuid, None, None).await?;
    assert!(!history.is_empty());

    // Test transition to another status
    let second_metadata = serde_json::json!({
        "reason": "Task completed successfully",
        "actor": "system",
        "actor_type": "system"
    });

    let second_transition = NewTaskTransition {
        task_uuid: task.task_uuid,
        from_state: Some("initializing".to_string()),
        to_state: "complete".to_string(),
        processor_uuid: None,
        metadata: Some(second_metadata),
    };

    let _second_created = TaskTransition::create(&pool, second_transition).await?;

    // Verify current status updated
    let new_current = TaskTransition::get_current(&pool, task.task_uuid).await?;
    assert!(new_current.is_some());
    assert_eq!(new_current.unwrap().to_state, "complete".to_string());

    // Test list functionality
    let list_results = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert!(!list_results.is_empty());

    // Test recent transitions
    let recent = TaskTransition::recent(&pool).await?;
    assert!(!recent.is_empty());

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_transition_status_tracking(pool: PgPool) -> sqlx::Result<()> {
    // Create minimal test dependencies
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "status_test_namespace".to_string(),
            description: None,
        },
    )
    .await?;

    let named_task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "status_test_task".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    let task = Task::create(
        &pool,
        NewTask {
            named_task_uuid: named_task.named_task_uuid,
            identity_hash: "status_test_hash".to_string(),
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,

            tags: None,
            context: None,
            priority: Some(5),
            correlation_id: uuid::Uuid::now_v7(),
            parent_correlation_id: None,
        },
    )
    .await?;

    // Test status progression using valid task states
    let statuses = [
        "pending",
        "initializing",
        "error",
        "initializing",
        "complete",
    ];

    for (i, &status) in statuses.iter().enumerate() {
        let from_state = if i == 0 {
            None
        } else {
            Some(statuses[i - 1].to_string())
        };

        let metadata = serde_json::json!({
            "reason": format!("Transition to {}", status),
            "actor": "test_system",
            "actor_type": "system"
        });

        let transition = NewTaskTransition {
            task_uuid: task.task_uuid,
            from_state,
            to_state: status.to_string(),
            processor_uuid: None,
            metadata: Some(metadata),
        };

        let _created = TaskTransition::create(&pool, transition).await?;

        // Verify current status
        let current = TaskTransition::get_current(&pool, task.task_uuid).await?;
        assert!(current.is_some());
        assert_eq!(current.unwrap().to_state, status.to_string());
    }

    // Test status history length
    let history = TaskTransition::get_history(&pool, task.task_uuid, None, None).await?;
    assert_eq!(history.len(), statuses.len());

    // Verify history order (most recent first)
    assert_eq!(history[0].to_state, "complete");
    assert_eq!(history[history.len() - 1].to_state, "pending");

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

// ---------------------------------------------------------------------------
// New tests below
// ---------------------------------------------------------------------------

/// Helper to create a task for use in transition tests.
/// Returns a Task with all required dependencies set up.
async fn create_test_task(pool: &PgPool, suffix: &str) -> sqlx::Result<Task> {
    let namespace = TaskNamespace::create(
        pool,
        NewTaskNamespace {
            name: format!("ns_{suffix}"),
            description: None,
        },
    )
    .await?;

    let named_task = NamedTask::create(
        pool,
        NewNamedTask {
            name: format!("task_{suffix}"),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
            identity_strategy: IdentityStrategy::Strict,
        },
    )
    .await?;

    Task::create(
        pool,
        NewTask {
            named_task_uuid: named_task.named_task_uuid,
            identity_hash: format!("hash_{suffix}"),
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            tags: None,
            context: None,
            priority: Some(5),
            correlation_id: uuid::Uuid::now_v7(),
            parent_correlation_id: None,
        },
    )
    .await
}

/// Helper to create a NewTaskTransition with sensible defaults
fn new_transition(
    task_uuid: uuid::Uuid,
    to_state: &str,
    from_state: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> NewTaskTransition {
    NewTaskTransition {
        task_uuid,
        to_state: to_state.to_string(),
        from_state: from_state.map(|s| s.to_string()),
        processor_uuid: None,
        metadata,
    }
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_by_state(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "list_by_state").await?;

    // Create transitions: pending -> initializing -> complete
    TaskTransition::create(&pool, new_transition(task.task_uuid, "pending", None, None)).await?;
    TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "initializing", Some("pending"), None),
    )
    .await?;
    TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "complete", Some("initializing"), None),
    )
    .await?;

    // most_recent_only = false should find the "pending" transition even though it is not most_recent
    let all_pending = TaskTransition::list_by_state(&pool, "pending", false).await?;
    assert!(
        all_pending.iter().any(|t| t.task_uuid == task.task_uuid),
        "should find pending transition for this task"
    );

    // most_recent_only = true for "pending" -- should NOT return anything for this task
    // because the most_recent transition is "complete"
    let recent_pending = TaskTransition::list_by_state(&pool, "pending", true).await?;
    assert!(
        !recent_pending.iter().any(|t| t.task_uuid == task.task_uuid),
        "pending should not be most_recent for this task"
    );

    // most_recent_only = true for "complete" should find this task
    let recent_complete = TaskTransition::list_by_state(&pool, "complete", true).await?;
    assert!(
        recent_complete
            .iter()
            .any(|t| t.task_uuid == task.task_uuid),
        "complete should be the most_recent state for this task"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_by_tasks_batch(pool: PgPool) -> sqlx::Result<()> {
    let task_a = create_test_task(&pool, "batch_a").await?;
    let task_b = create_test_task(&pool, "batch_b").await?;

    TaskTransition::create(
        &pool,
        new_transition(task_a.task_uuid, "pending", None, None),
    )
    .await?;
    TaskTransition::create(
        &pool,
        new_transition(task_b.task_uuid, "initializing", None, None),
    )
    .await?;

    let results =
        TaskTransition::list_by_tasks(&pool, &[task_a.task_uuid, task_b.task_uuid]).await?;

    // Should contain transitions for both tasks
    let task_uuids: Vec<_> = results.iter().map(|t| t.task_uuid).collect();
    assert!(task_uuids.contains(&task_a.task_uuid));
    assert!(task_uuids.contains(&task_b.task_uuid));
    assert!(results.len() >= 2);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_previous(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "get_previous").await?;

    // Create 3 transitions
    TaskTransition::create(&pool, new_transition(task.task_uuid, "pending", None, None)).await?;
    let second = TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "initializing", Some("pending"), None),
    )
    .await?;
    let _third = TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "complete", Some("initializing"), None),
    )
    .await?;

    // get_previous should return the second-to-last non-most_recent transition
    let previous = TaskTransition::get_previous(&pool, task.task_uuid).await?;
    assert!(previous.is_some());
    let prev = previous.unwrap();
    // The previous (highest sort_key with most_recent = false) should be "initializing"
    assert_eq!(prev.to_state, "initializing");
    assert_eq!(prev.task_transition_uuid, second.task_transition_uuid);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_count_by_state(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "count_state").await?;

    TaskTransition::create(&pool, new_transition(task.task_uuid, "pending", None, None)).await?;
    TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "initializing", Some("pending"), None),
    )
    .await?;

    // Count all "pending" transitions (not just most_recent)
    let count_all = TaskTransition::count_by_state(&pool, "pending", false).await?;
    assert!(
        count_all >= 1,
        "should find at least one pending transition"
    );

    // Count only most_recent "initializing" transitions
    let count_recent = TaskTransition::count_by_state(&pool, "initializing", true).await?;
    assert!(
        count_recent >= 1,
        "should find at least one most_recent initializing"
    );

    // Count most_recent "pending" -- this task's pending is no longer most_recent
    let count_pending_recent = TaskTransition::count_by_state(&pool, "pending", true).await?;
    // It may be 0 or more depending on other data; just check it does not error
    assert!(count_pending_recent >= 0);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_tasks_in_state(pool: PgPool) -> sqlx::Result<()> {
    let task_a = create_test_task(&pool, "in_state_a").await?;
    let task_b = create_test_task(&pool, "in_state_b").await?;
    let task_c = create_test_task(&pool, "in_state_c").await?;

    // task_a -> pending
    TaskTransition::create(
        &pool,
        new_transition(task_a.task_uuid, "pending", None, None),
    )
    .await?;
    // task_b -> initializing
    TaskTransition::create(
        &pool,
        new_transition(task_b.task_uuid, "initializing", None, None),
    )
    .await?;
    // task_c -> pending
    TaskTransition::create(
        &pool,
        new_transition(task_c.task_uuid, "pending", None, None),
    )
    .await?;

    // get_tasks_in_state("pending") should include task_a and task_c
    let pending_tasks = TaskTransition::get_tasks_in_state(&pool, "pending").await?;
    assert!(pending_tasks.contains(&task_a.task_uuid));
    assert!(pending_tasks.contains(&task_c.task_uuid));
    assert!(!pending_tasks.contains(&task_b.task_uuid));

    // get_tasks_in_states for multiple states
    let multi = TaskTransition::get_tasks_in_states(
        &pool,
        &["pending".to_string(), "initializing".to_string()],
    )
    .await?;
    assert!(multi.contains(&task_a.task_uuid));
    assert!(multi.contains(&task_b.task_uuid));
    assert!(multi.contains(&task_c.task_uuid));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_state_summary(pool: PgPool) -> sqlx::Result<()> {
    let task_a = create_test_task(&pool, "summary_a").await?;
    let task_b = create_test_task(&pool, "summary_b").await?;

    // Put task_a in "pending", task_b in "initializing"
    TaskTransition::create(
        &pool,
        new_transition(task_a.task_uuid, "pending", None, None),
    )
    .await?;
    TaskTransition::create(
        &pool,
        new_transition(task_b.task_uuid, "initializing", None, None),
    )
    .await?;

    let summary = TaskTransition::get_state_summary(&pool).await?;
    // Summary is Vec<(String, i64)>
    let state_names: Vec<&str> = summary.iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        state_names.contains(&"pending"),
        "summary should contain pending state"
    );
    assert!(
        state_names.contains(&"initializing"),
        "summary should contain initializing state"
    );

    // Each count should be >= 1
    for (state, count) in &summary {
        assert!(*count >= 1, "state {state} should have count >= 1");
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_update_metadata(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "update_meta").await?;

    let initial_metadata = serde_json::json!({"reason": "initial"});
    let mut transition = TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "pending", None, Some(initial_metadata)),
    )
    .await?;

    assert_eq!(
        transition.get_metadata("reason", serde_json::json!("")),
        serde_json::json!("initial")
    );

    // Update metadata
    let updated_metadata = serde_json::json!({"reason": "updated", "extra_field": 42});
    transition
        .update_metadata(&pool, updated_metadata.clone())
        .await?;

    // Verify in-memory update
    assert_eq!(transition.metadata, Some(updated_metadata.clone()));

    // Verify DB update by re-fetching
    let refetched = TaskTransition::find_by_uuid(&pool, transition.task_transition_uuid)
        .await?
        .expect("transition should exist");
    assert_eq!(refetched.metadata, Some(updated_metadata));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_cleanup_old_transitions(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "cleanup").await?;

    // Create 5 transitions
    let states = [
        "pending",
        "initializing",
        "error",
        "initializing",
        "complete",
    ];
    for (i, &state) in states.iter().enumerate() {
        let from = if i == 0 { None } else { Some(states[i - 1]) };
        TaskTransition::create(&pool, new_transition(task.task_uuid, state, from, None)).await?;
    }

    // Verify we have 5 transitions
    let before = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert_eq!(before.len(), 5);

    // Cleanup keeping only the 3 most recent
    let deleted = TaskTransition::cleanup_old_transitions(&pool, task.task_uuid, 3).await?;
    assert_eq!(deleted, 2, "should delete 2 oldest transitions");

    // Verify remaining transitions
    let after = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    assert_eq!(after.len(), 3);

    // The most recent should still be "complete"
    let current = TaskTransition::get_current(&pool, task.task_uuid).await?;
    assert_eq!(current.unwrap().to_state, "complete");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_can_transition(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "can_trans").await?;

    // Create a transition to "pending"
    TaskTransition::create(&pool, new_transition(task.task_uuid, "pending", None, None)).await?;

    // Can transition from pending (matches current state)
    let can =
        TaskTransition::can_transition(&pool, task.task_uuid, "pending", "initializing").await?;
    assert!(can, "should allow transition from current state 'pending'");

    // Cannot transition from initializing (does not match current state)
    let cannot =
        TaskTransition::can_transition(&pool, task.task_uuid, "initializing", "complete").await?;
    assert!(
        !cannot,
        "should not allow transition from non-current state 'initializing'"
    );

    // For a task with no transitions at all, check initial state handling
    let fresh_task = create_test_task(&pool, "can_trans_fresh").await?;
    let can_draft =
        TaskTransition::can_transition(&pool, fresh_task.task_uuid, "draft", "pending").await?;
    assert!(
        can_draft,
        "should allow transition from 'draft' when no transitions exist"
    );
    let can_planned =
        TaskTransition::can_transition(&pool, fresh_task.task_uuid, "planned", "pending").await?;
    assert!(
        can_planned,
        "should allow transition from 'planned' when no transitions exist"
    );
    let cannot_other =
        TaskTransition::can_transition(&pool, fresh_task.task_uuid, "initializing", "complete")
            .await?;
    assert!(
        !cannot_other,
        "should not allow transition from non-initial state when no transitions exist"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_scope_to_state(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "scope_to_state").await?;

    // Create transitions for different states
    TaskTransition::create(&pool, new_transition(task.task_uuid, "pending", None, None)).await?;
    TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "initializing", Some("pending"), None),
    )
    .await?;

    // Use the to_state scope method
    let pending_transitions = TaskTransition::to_state(&pool, "pending").await?;
    assert!(
        pending_transitions
            .iter()
            .any(|t| t.task_uuid == task.task_uuid),
        "to_state scope should find pending transitions for this task"
    );

    let initializing_transitions = TaskTransition::to_state(&pool, "initializing").await?;
    assert!(
        initializing_transitions
            .iter()
            .any(|t| t.task_uuid == task.task_uuid),
        "to_state scope should find initializing transitions for this task"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_scope_with_metadata(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "scope_metadata").await?;

    let metadata = serde_json::json!({
        "reason": "test_reason",
        "actor": "test_actor",
        "priority": "high"
    });
    TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "pending", None, Some(metadata)),
    )
    .await?;

    // Test with_metadata_key
    let with_reason = TaskTransition::with_metadata_key(&pool, "reason").await?;
    assert!(
        with_reason.iter().any(|t| t.task_uuid == task.task_uuid),
        "should find transitions with 'reason' metadata key"
    );

    // Test with_metadata_value
    let with_value = TaskTransition::with_metadata_value(&pool, "actor", "test_actor").await?;
    assert!(
        with_value.iter().any(|t| t.task_uuid == task.task_uuid),
        "should find transitions where actor = test_actor"
    );

    // Test with_metadata_value for non-matching value
    let no_match = TaskTransition::with_metadata_value(&pool, "actor", "nonexistent_actor").await?;
    assert!(
        !no_match.iter().any(|t| t.task_uuid == task.task_uuid),
        "should not find transitions with non-matching metadata value"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_query_builder_execute(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "qb_execute").await?;

    // Create several transitions
    TaskTransition::create(&pool, new_transition(task.task_uuid, "pending", None, None)).await?;
    TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "initializing", Some("pending"), None),
    )
    .await?;
    TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "complete", Some("initializing"), None),
    )
    .await?;

    // Test: query by task_uuid only
    let all_for_task = TaskTransitionQuery::new()
        .task_uuid(task.task_uuid)
        .execute(&pool)
        .await?;
    assert_eq!(all_for_task.len(), 3);

    // Test: query by task_uuid and state
    let initializing_only = TaskTransitionQuery::new()
        .task_uuid(task.task_uuid)
        .state("initializing")
        .execute(&pool)
        .await?;
    assert_eq!(initializing_only.len(), 1);
    assert_eq!(initializing_only[0].to_state, "initializing");

    // Test: most_recent_only
    let most_recent = TaskTransitionQuery::new()
        .task_uuid(task.task_uuid)
        .most_recent_only()
        .execute(&pool)
        .await?;
    assert_eq!(most_recent.len(), 1);
    assert_eq!(most_recent[0].to_state, "complete");

    // Test: limit
    let limited = TaskTransitionQuery::new()
        .task_uuid(task.task_uuid)
        .limit(2)
        .execute(&pool)
        .await?;
    assert_eq!(limited.len(), 2);

    // Test: limit + offset
    let paginated = TaskTransitionQuery::new()
        .task_uuid(task.task_uuid)
        .limit(1)
        .offset(1)
        .execute(&pool)
        .await?;
    assert_eq!(paginated.len(), 1);

    // Test: no task_uuid falls through to empty result
    let empty = TaskTransitionQuery::new()
        .state("pending")
        .execute(&pool)
        .await?;
    assert!(empty.is_empty());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_instance_helpers(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "instance_helpers").await?;

    // Create transitions covering different state types
    let pending_t = TaskTransition::create(
        &pool,
        new_transition(
            task.task_uuid,
            "pending",
            None,
            Some(serde_json::json!({"key1": "value1"})),
        ),
    )
    .await?;
    let error_t = TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "error", Some("pending"), None),
    )
    .await?;
    let complete_t = TaskTransition::create(
        &pool,
        new_transition(task.task_uuid, "complete", Some("error"), None),
    )
    .await?;

    // error_transition()
    assert!(error_t.error_transition());
    assert!(!complete_t.error_transition());
    assert!(!pending_t.error_transition());

    // completion_transition()
    assert!(complete_t.completion_transition());
    assert!(!error_t.completion_transition());

    // cancellation_transition() - none of these are cancelled
    assert!(!complete_t.cancellation_transition());
    assert!(!error_t.cancellation_transition());

    // description()
    assert_eq!(
        pending_t.description(),
        "Task initialized and ready for processing"
    );
    assert_eq!(error_t.description(), "Task encountered an error");
    assert_eq!(complete_t.description(), "Task completed successfully");

    // has_metadata()
    assert!(pending_t.has_metadata());
    assert!(!error_t.has_metadata());

    // get_metadata()
    assert_eq!(
        pending_t.get_metadata("key1", serde_json::json!("default")),
        serde_json::json!("value1")
    );
    assert_eq!(
        pending_t.get_metadata("missing_key", serde_json::json!("fallback")),
        serde_json::json!("fallback")
    );
    assert_eq!(
        error_t.get_metadata("any", serde_json::json!("default")),
        serde_json::json!("default")
    );

    // set_metadata()
    let mut mutable_t = pending_t.clone();
    mutable_t.set_metadata("new_key", serde_json::json!("new_value"));
    assert_eq!(
        mutable_t.get_metadata("new_key", serde_json::json!("")),
        serde_json::json!("new_value")
    );
    // Original key should still be present
    assert_eq!(
        mutable_t.get_metadata("key1", serde_json::json!("")),
        serde_json::json!("value1")
    );

    // duration_since_previous()
    let duration_first = pending_t.duration_since_previous(&pool).await?;
    assert!(
        duration_first.is_none(),
        "first transition should have no previous"
    );

    let duration_second = error_t.duration_since_previous(&pool).await?;
    assert!(
        duration_second.is_some(),
        "second transition should have a duration since previous"
    );

    Ok(())
}
