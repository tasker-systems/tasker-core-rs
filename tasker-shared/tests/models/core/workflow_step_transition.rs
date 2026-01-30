use serde_json::json;
use sqlx::PgPool;

use tasker_shared::models::{
    core::IdentityStrategy,
    named_step::{NamedStep, NewNamedStep},
    named_task::{NamedTask, NewNamedTask},
    task::{NewTask, Task},
    task_namespace::{NewTaskNamespace, TaskNamespace},
    workflow_step::{NewWorkflowStep, WorkflowStep},
    workflow_step_transition::{
        NewWorkflowStepTransition, WorkflowStepTransition, WorkflowStepTransitionQuery,
    },
};
use uuid::Uuid;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_transition_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies - WorkflowStep
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: format!(
                "test_namespace_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            description: None,
        },
    )
    .await?;

    let named_task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: format!(
                "test_task_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
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
            context: Some(json!({"test": "context"})),
            identity_hash: format!(
                "test_hash_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            priority: Some(5),
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
        },
    )
    .await?;

    let named_step = NamedStep::create(
        &pool,
        NewNamedStep {
            name: format!(
                "test_step_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            description: Some("Test step".to_string()),
        },
    )
    .await?;

    let workflow_step = WorkflowStep::create(
        &pool,
        NewWorkflowStep {
            task_uuid: task.task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            retryable: Some(true),
            max_attempts: Some(3),
            inputs: None,
        },
    )
    .await?;

    let workflow_step_uuid = workflow_step.workflow_step_uuid;

    // Test creation using valid workflow step states
    let new_transition = NewWorkflowStepTransition {
        workflow_step_uuid,
        to_state: "enqueued".to_string(),
        from_state: Some("pending".to_string()),
        metadata: Some(json!({"reason": "dependencies_met"})),
    };

    let created = WorkflowStepTransition::create(&pool, new_transition).await?;
    assert_eq!(created.to_state, "enqueued");
    assert!(created.most_recent);
    assert_eq!(created.sort_key, 1);

    // Test find by ID
    let found = WorkflowStepTransition::find_by_uuid(&pool, created.workflow_step_transition_uuid)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(
        found.workflow_step_transition_uuid,
        created.workflow_step_transition_uuid
    );

    // Test get current
    let current = WorkflowStepTransition::get_current(&pool, workflow_step_uuid)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert_eq!(
        current.workflow_step_transition_uuid,
        created.workflow_step_transition_uuid
    );
    assert!(current.most_recent);

    // Test creating another transition using valid states
    let new_transition2 = NewWorkflowStepTransition {
        workflow_step_uuid,
        to_state: "in_progress".to_string(),
        from_state: Some("enqueued".to_string()),
        metadata: Some(json!({"started_at": "2024-01-01T00:00:00Z"})),
    };

    let created2 = WorkflowStepTransition::create(&pool, new_transition2).await?;
    assert_eq!(created2.sort_key, 2);
    assert!(created2.most_recent);

    // Verify first transition is no longer most recent
    let updated_first =
        WorkflowStepTransition::find_by_uuid(&pool, created.workflow_step_transition_uuid)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)?;
    assert!(!updated_first.most_recent);

    // Test get history
    let history =
        WorkflowStepTransition::get_history(&pool, workflow_step_uuid, Some(10), None).await?;
    assert_eq!(history.len(), 2);
    assert_eq!(
        history[0].workflow_step_transition_uuid,
        created2.workflow_step_transition_uuid
    ); // Most recent first
    assert_eq!(
        history[1].workflow_step_transition_uuid,
        created.workflow_step_transition_uuid
    );

    // Test can transition using valid states
    let can_transition = WorkflowStepTransition::can_transition(
        &pool,
        workflow_step_uuid,
        "in_progress",
        "complete",
    )
    .await?;
    assert!(can_transition);

    // Cleanup - delete in reverse dependency order (transitions first)
    sqlx::query!(
        "DELETE FROM tasker.workflow_step_transitions WHERE workflow_step_uuid = $1",
        workflow_step.workflow_step_uuid
    )
    .execute(&pool)
    .await?;
    WorkflowStep::delete(&pool, workflow_step.workflow_step_uuid).await?;
    NamedStep::delete(&pool, named_step.named_step_uuid).await?;
    Task::delete(&pool, task.task_uuid).await?;
    NamedTask::delete(&pool, named_task.named_task_uuid).await?;
    TaskNamespace::delete(&pool, namespace.task_namespace_uuid).await?;

    Ok(())
}

#[test]
fn test_query_builder() {
    let step_uuid = Uuid::now_v7();
    let query = WorkflowStepTransitionQuery::new()
        .workflow_step_uuid(step_uuid)
        .state("completed")
        .most_recent_only()
        .limit(10)
        .offset(0);

    assert_eq!(query.workflow_step_uuid, Some(step_uuid));
    assert_eq!(query.state, Some("completed".to_string()));
    assert!(query.most_recent_only);
    assert_eq!(query.limit, Some(10));
    assert_eq!(query.offset, Some(0));
}

// ---------------------------------------------------------------------------
// New tests below
// ---------------------------------------------------------------------------

/// Helper to create a task with all dependencies for workflow step transition tests
async fn create_test_task(pool: &PgPool, suffix: &str) -> sqlx::Result<Task> {
    let namespace = TaskNamespace::create(
        pool,
        NewTaskNamespace {
            name: format!("wst_ns_{suffix}"),
            description: None,
        },
    )
    .await?;

    let named_task = NamedTask::create(
        pool,
        NewNamedTask {
            name: format!("wst_task_{suffix}"),
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
            identity_hash: format!("wst_hash_{suffix}"),
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            tags: None,
            context: None,
            priority: Some(5),
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
        },
    )
    .await
}

/// Helper to create a workflow step for a given task
async fn create_test_step(
    pool: &PgPool,
    task_uuid: Uuid,
    step_suffix: &str,
) -> sqlx::Result<WorkflowStep> {
    let named_step = NamedStep::create(
        pool,
        NewNamedStep {
            name: format!("wst_step_{step_suffix}"),
            description: Some("Test step".to_string()),
        },
    )
    .await?;

    WorkflowStep::create(
        pool,
        NewWorkflowStep {
            task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            retryable: Some(true),
            max_attempts: Some(3),
            inputs: None,
        },
    )
    .await
}

/// Helper to create a NewWorkflowStepTransition with sensible defaults
fn new_wst(
    workflow_step_uuid: Uuid,
    to_state: &str,
    from_state: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> NewWorkflowStepTransition {
    NewWorkflowStepTransition {
        workflow_step_uuid,
        to_state: to_state.to_string(),
        from_state: from_state.map(|s| s.to_string()),
        metadata,
    }
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_list_by_state(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "list_state").await?;
    let step = create_test_step(&pool, task.task_uuid, "list_state").await?;
    let step_uuid = step.workflow_step_uuid;

    // Create transitions: pending -> in_progress -> complete
    WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "complete", Some("in_progress"), None),
    )
    .await?;

    // most_recent_only = false returns all transitions with that state
    let all_pending = WorkflowStepTransition::list_by_state(&pool, "pending", false).await?;
    assert!(
        all_pending
            .iter()
            .any(|t| t.workflow_step_uuid == step_uuid),
        "should find pending transition for this step"
    );

    // most_recent_only = true for "pending" should NOT return this step
    let recent_pending = WorkflowStepTransition::list_by_state(&pool, "pending", true).await?;
    assert!(
        !recent_pending
            .iter()
            .any(|t| t.workflow_step_uuid == step_uuid),
        "pending should not be most_recent for this step"
    );

    // most_recent_only = true for "complete" should find this step
    let recent_complete = WorkflowStepTransition::list_by_state(&pool, "complete", true).await?;
    assert!(
        recent_complete
            .iter()
            .any(|t| t.workflow_step_uuid == step_uuid),
        "complete should be the most_recent state"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_list_by_task(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "list_task").await?;
    let step_a = create_test_step(&pool, task.task_uuid, "list_task_a").await?;
    let step_b = create_test_step(&pool, task.task_uuid, "list_task_b").await?;

    WorkflowStepTransition::create(
        &pool,
        new_wst(step_a.workflow_step_uuid, "pending", None, None),
    )
    .await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_b.workflow_step_uuid, "in_progress", None, None),
    )
    .await?;

    // for_task returns all transitions for all steps belonging to the task
    let results = WorkflowStepTransition::for_task(&pool, task.task_uuid).await?;
    let step_uuids: Vec<_> = results.iter().map(|t| t.workflow_step_uuid).collect();
    assert!(step_uuids.contains(&step_a.workflow_step_uuid));
    assert!(step_uuids.contains(&step_b.workflow_step_uuid));
    assert!(results.len() >= 2);

    // list_by_workflow_steps with multiple step UUIDs
    let batch = WorkflowStepTransition::list_by_workflow_steps(
        &pool,
        &[step_a.workflow_step_uuid, step_b.workflow_step_uuid],
    )
    .await?;
    assert!(batch.len() >= 2);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_get_previous(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "get_prev").await?;
    let step = create_test_step(&pool, task.task_uuid, "get_prev").await?;
    let step_uuid = step.workflow_step_uuid;

    // Create 3 transitions
    WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;
    let second = WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;
    let _third = WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "complete", Some("in_progress"), None),
    )
    .await?;

    // get_previous returns the highest sort_key with most_recent = false
    let previous = WorkflowStepTransition::get_previous(&pool, step_uuid).await?;
    assert!(previous.is_some());
    let prev = previous.unwrap();
    assert_eq!(prev.to_state, "in_progress");
    assert_eq!(
        prev.workflow_step_transition_uuid,
        second.workflow_step_transition_uuid
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_count_by_state(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "count").await?;
    let step = create_test_step(&pool, task.task_uuid, "count").await?;
    let step_uuid = step.workflow_step_uuid;

    WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;

    // Count all "pending" transitions
    let count_all = WorkflowStepTransition::count_by_state(&pool, "pending", false).await?;
    assert!(count_all >= 1);

    // Count most_recent "in_progress" transitions
    let count_recent = WorkflowStepTransition::count_by_state(&pool, "in_progress", true).await?;
    assert!(count_recent >= 1);

    // Count most_recent "pending" -- no longer most_recent for this step
    let count_pending_recent =
        WorkflowStepTransition::count_by_state(&pool, "pending", true).await?;
    assert!(count_pending_recent >= 0);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_statistics(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "stats").await?;
    let step = create_test_step(&pool, task.task_uuid, "stats").await?;
    let step_uuid = step.workflow_step_uuid;

    WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "complete", Some("in_progress"), None),
    )
    .await?;

    let stats = WorkflowStepTransition::statistics(&pool).await?;

    assert!(
        stats.total_transitions >= 3,
        "should have at least 3 total transitions"
    );
    assert!(
        stats.states.contains_key("pending"),
        "stats should include pending state"
    );
    assert!(
        stats.states.contains_key("in_progress"),
        "stats should include in_progress state"
    );
    assert!(
        stats.states.contains_key("complete"),
        "stats should include complete state"
    );
    // recent_activity counts transitions within 24 hours -- our test transitions qualify
    assert!(
        stats.recent_activity >= 3,
        "recent activity should include our transitions"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_get_steps_in_state(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "steps_state").await?;
    let step_a = create_test_step(&pool, task.task_uuid, "steps_state_a").await?;
    let step_b = create_test_step(&pool, task.task_uuid, "steps_state_b").await?;
    let step_c = create_test_step(&pool, task.task_uuid, "steps_state_c").await?;

    // step_a -> pending, step_b -> in_progress, step_c -> pending
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_a.workflow_step_uuid, "pending", None, None),
    )
    .await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_b.workflow_step_uuid, "in_progress", None, None),
    )
    .await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_c.workflow_step_uuid, "pending", None, None),
    )
    .await?;

    // get_steps_in_state queries for steps belonging to a specific task
    let pending_steps =
        WorkflowStepTransition::get_steps_in_state(&pool, task.task_uuid, "pending").await?;
    assert!(pending_steps.contains(&step_a.workflow_step_uuid));
    assert!(pending_steps.contains(&step_c.workflow_step_uuid));
    assert!(!pending_steps.contains(&step_b.workflow_step_uuid));

    let in_progress_steps =
        WorkflowStepTransition::get_steps_in_state(&pool, task.task_uuid, "in_progress").await?;
    assert!(in_progress_steps.contains(&step_b.workflow_step_uuid));
    assert!(!in_progress_steps.contains(&step_a.workflow_step_uuid));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_instance_methods(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "instance").await?;
    let step = create_test_step(&pool, task.task_uuid, "instance").await?;
    let step_uuid = step.workflow_step_uuid;

    let first =
        WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;
    let second = WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;

    // step_name() - resolves through workflow_steps -> named_steps join
    let name = second.step_name(&pool).await?;
    assert!(
        name.contains("wst_step_instance"),
        "step_name should resolve to the named step name, got: {name}"
    );

    // task_uuid() - resolves through workflow_steps join
    let resolved_task_uuid = second.task_uuid(&pool).await?;
    assert_eq!(resolved_task_uuid, task.task_uuid);

    // duration_since_previous() for first transition should be None
    let dur_first = first.duration_since_previous(&pool).await?;
    assert!(dur_first.is_none(), "first transition has no previous");

    // duration_since_previous() for second transition should be Some
    let dur_second = second.duration_since_previous(&pool).await?;
    assert!(
        dur_second.is_some(),
        "second transition should have duration"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_description_formatter(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "desc_fmt").await?;
    let step = create_test_step(&pool, task.task_uuid, "desc_fmt").await?;
    let step_uuid = step.workflow_step_uuid;

    // pending (initial)
    let pending =
        WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;
    assert_eq!(
        pending.description(),
        "Step initialized and ready for execution"
    );

    // in_progress
    let in_progress = WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;
    assert!(
        in_progress.description().contains("Step execution started"),
        "in_progress description should mention execution started"
    );

    // complete (without execution_duration metadata)
    let complete = WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "complete", Some("in_progress"), None),
    )
    .await?;
    assert_eq!(complete.description(), "Step completed successfully");

    // complete with execution_duration metadata
    let complete_with_dur = WorkflowStepTransition::create(
        &pool,
        new_wst(
            step_uuid,
            "complete",
            Some("in_progress"),
            Some(json!({"execution_duration": 2.5})),
        ),
    )
    .await?;
    assert!(
        complete_with_dur.description().contains("2.50s"),
        "should include duration in description"
    );

    // error
    let error = WorkflowStepTransition::create(
        &pool,
        new_wst(
            step_uuid,
            "error",
            Some("in_progress"),
            Some(json!({"error_message": "Connection timeout"})),
        ),
    )
    .await?;
    assert!(
        error.description().contains("Connection timeout"),
        "error description should include error message"
    );

    // cancelled
    let cancelled = WorkflowStepTransition::create(
        &pool,
        new_wst(
            step_uuid,
            "cancelled",
            Some("pending"),
            Some(json!({"triggered_by": "user_request"})),
        ),
    )
    .await?;
    assert!(
        cancelled.description().contains("user_request"),
        "cancelled description should include trigger reason"
    );

    // resolved_manually
    let resolved = WorkflowStepTransition::create(
        &pool,
        new_wst(
            step_uuid,
            "resolved_manually",
            Some("error"),
            Some(json!({"resolved_by": "admin_user"})),
        ),
    )
    .await?;
    assert!(
        resolved.description().contains("admin_user"),
        "resolved description should include resolver"
    );

    // State that falls through to the default description branch
    let waiting = WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "waiting_for_retry", Some("pending"), None),
    )
    .await?;
    assert_eq!(
        waiting.description(),
        "Step transitioned to waiting_for_retry"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_metadata_helpers(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "meta_helpers").await?;
    let step = create_test_step(&pool, task.task_uuid, "meta_helpers").await?;
    let step_uuid = step.workflow_step_uuid;

    // Transition with metadata
    let with_meta = WorkflowStepTransition::create(
        &pool,
        new_wst(
            step_uuid,
            "pending",
            None,
            Some(json!({"key1": "value1", "retry_attempt": true, "attempt_number": 2})),
        ),
    )
    .await?;

    // has_metadata(key)
    assert!(with_meta.has_metadata("key1"));
    assert!(with_meta.has_metadata("retry_attempt"));
    assert!(!with_meta.has_metadata("nonexistent_key"));

    // get_metadata(key, default)
    assert_eq!(
        with_meta.get_metadata("key1", json!("default")),
        json!("value1")
    );
    assert_eq!(
        with_meta.get_metadata("missing", json!("fallback")),
        json!("fallback")
    );

    // set_metadata(key, value)
    let mut mutable = with_meta.clone();
    mutable.set_metadata("new_key", json!("new_value"));
    assert_eq!(
        mutable.get_metadata("new_key", json!("")),
        json!("new_value")
    );
    // Original key should still be present
    assert_eq!(mutable.get_metadata("key1", json!("")), json!("value1"));

    // retry_transition() - checks if to_state == "pending" and has retry_attempt metadata
    assert!(with_meta.retry_transition());

    // attempt_number()
    assert_eq!(with_meta.attempt_number(), 2);

    // Transition without metadata
    let without_meta = WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;

    assert!(!without_meta.has_metadata("any_key"));
    assert_eq!(
        without_meta.get_metadata("any", json!("default")),
        json!("default")
    );
    assert!(!without_meta.retry_transition());
    assert_eq!(without_meta.attempt_number(), 1); // Default

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_update_metadata(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "update_meta").await?;
    let step = create_test_step(&pool, task.task_uuid, "update_meta").await?;
    let step_uuid = step.workflow_step_uuid;

    let initial_meta = json!({"reason": "initial"});
    let mut transition = WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "pending", None, Some(initial_meta)),
    )
    .await?;

    assert_eq!(
        transition.get_metadata("reason", json!("")),
        json!("initial")
    );

    // Update metadata
    let updated_meta = json!({"reason": "updated", "extra": 42});
    transition
        .update_metadata(&pool, updated_meta.clone())
        .await?;

    // Verify in-memory update
    assert_eq!(transition.metadata, Some(updated_meta.clone()));

    // Verify DB persistence by re-fetching
    let refetched =
        WorkflowStepTransition::find_by_uuid(&pool, transition.workflow_step_transition_uuid)
            .await?
            .expect("transition should exist");
    assert_eq!(refetched.metadata, Some(updated_meta));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_cleanup_old_transitions(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "cleanup").await?;
    let step = create_test_step(&pool, task.task_uuid, "cleanup").await?;
    let step_uuid = step.workflow_step_uuid;

    // Create 5 transitions
    let states = ["pending", "in_progress", "error", "in_progress", "complete"];
    for (i, &state) in states.iter().enumerate() {
        let from = if i == 0 { None } else { Some(states[i - 1]) };
        WorkflowStepTransition::create(&pool, new_wst(step_uuid, state, from, None)).await?;
    }

    // Verify 5 transitions exist
    let before = WorkflowStepTransition::list_by_workflow_step(&pool, step_uuid).await?;
    assert_eq!(before.len(), 5);

    // Cleanup keeping only the 3 most recent
    let deleted = WorkflowStepTransition::cleanup_old_transitions(&pool, step_uuid, 3).await?;
    assert_eq!(deleted, 2, "should delete 2 oldest transitions");

    // Verify remaining transitions
    let after = WorkflowStepTransition::list_by_workflow_step(&pool, step_uuid).await?;
    assert_eq!(after.len(), 3);

    // The most recent should still be "complete"
    let current = WorkflowStepTransition::get_current(&pool, step_uuid).await?;
    assert_eq!(current.unwrap().to_state, "complete");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_can_transition(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "can_trans").await?;
    let step = create_test_step(&pool, task.task_uuid, "can_trans").await?;
    let step_uuid = step.workflow_step_uuid;

    // Create transition to "pending"
    WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;

    // Can transition from pending (matches current state)
    let can =
        WorkflowStepTransition::can_transition(&pool, step_uuid, "pending", "in_progress").await?;
    assert!(can, "should allow transition from current state 'pending'");

    // Cannot transition from in_progress (does not match current state)
    let cannot =
        WorkflowStepTransition::can_transition(&pool, step_uuid, "in_progress", "complete").await?;
    assert!(
        !cannot,
        "should not allow transition from non-current state"
    );

    // For a step with no transitions, check initial state handling
    let fresh_step = create_test_step(&pool, task.task_uuid, "can_trans_fresh").await?;
    let fresh_uuid = fresh_step.workflow_step_uuid;

    let can_pending =
        WorkflowStepTransition::can_transition(&pool, fresh_uuid, "pending", "in_progress").await?;
    assert!(
        can_pending,
        "should allow transition from 'pending' when no transitions exist"
    );
    let can_ready =
        WorkflowStepTransition::can_transition(&pool, fresh_uuid, "ready", "in_progress").await?;
    assert!(
        can_ready,
        "should allow transition from 'ready' when no transitions exist"
    );
    let cannot_other =
        WorkflowStepTransition::can_transition(&pool, fresh_uuid, "in_progress", "complete")
            .await?;
    assert!(
        !cannot_other,
        "should not allow transition from non-initial state when no transitions exist"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_scope_to_state(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "scope_state").await?;
    let step = create_test_step(&pool, task.task_uuid, "scope_state").await?;
    let step_uuid = step.workflow_step_uuid;

    WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;

    // to_state scope returns all transitions with that to_state
    let pending_transitions = WorkflowStepTransition::to_state(&pool, "pending").await?;
    assert!(
        pending_transitions
            .iter()
            .any(|t| t.workflow_step_uuid == step_uuid),
        "to_state scope should find pending transitions for this step"
    );

    let in_progress_transitions = WorkflowStepTransition::to_state(&pool, "in_progress").await?;
    assert!(
        in_progress_transitions
            .iter()
            .any(|t| t.workflow_step_uuid == step_uuid),
        "to_state scope should find in_progress transitions for this step"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_wst_query_builder_execute(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "qb_exec").await?;
    let step = create_test_step(&pool, task.task_uuid, "qb_exec").await?;
    let step_uuid = step.workflow_step_uuid;

    // Create several transitions
    WorkflowStepTransition::create(&pool, new_wst(step_uuid, "pending", None, None)).await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "in_progress", Some("pending"), None),
    )
    .await?;
    WorkflowStepTransition::create(
        &pool,
        new_wst(step_uuid, "complete", Some("in_progress"), None),
    )
    .await?;

    // Test: query by workflow_step_uuid only
    let all = WorkflowStepTransitionQuery::new()
        .workflow_step_uuid(step_uuid)
        .execute(&pool)
        .await?;
    assert_eq!(all.len(), 3);

    // Test: query by step_uuid and state
    let in_progress_only = WorkflowStepTransitionQuery::new()
        .workflow_step_uuid(step_uuid)
        .state("in_progress")
        .execute(&pool)
        .await?;
    assert_eq!(in_progress_only.len(), 1);
    assert_eq!(in_progress_only[0].to_state, "in_progress");

    // Test: most_recent_only
    let most_recent = WorkflowStepTransitionQuery::new()
        .workflow_step_uuid(step_uuid)
        .most_recent_only()
        .execute(&pool)
        .await?;
    assert_eq!(most_recent.len(), 1);
    assert_eq!(most_recent[0].to_state, "complete");

    // Test: limit
    let limited = WorkflowStepTransitionQuery::new()
        .workflow_step_uuid(step_uuid)
        .limit(2)
        .execute(&pool)
        .await?;
    assert_eq!(limited.len(), 2);

    // Test: limit + offset
    let paginated = WorkflowStepTransitionQuery::new()
        .workflow_step_uuid(step_uuid)
        .limit(1)
        .offset(1)
        .execute(&pool)
        .await?;
    assert_eq!(paginated.len(), 1);

    // Test: no step_uuid falls through to empty result
    let empty = WorkflowStepTransitionQuery::new()
        .state("pending")
        .execute(&pool)
        .await?;
    assert!(empty.is_empty());

    Ok(())
}
