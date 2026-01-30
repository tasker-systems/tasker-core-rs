//! Workflow Step Model Tests
//!
//! Tests for the WorkflowStep model using SQLx native testing

use serde_json::json;
use sqlx::PgPool;

use tasker_shared::models::{
    core::IdentityStrategy,
    named_step::{NamedStep, NewNamedStep},
    named_task::{NamedTask, NewNamedTask},
    task::{NewTask, Task},
    task_namespace::{NewTaskNamespace, TaskNamespace},
    workflow_step::{NewWorkflowStep, WorkflowStep},
};
use uuid::Uuid;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies
    let namespace = TaskNamespace::create(
        &pool,
        NewTaskNamespace {
            name: "test_namespace_workflow_step".to_string(),
            description: None,
        },
    )
    .await?;

    let named_task = NamedTask::create(
        &pool,
        NewNamedTask {
            name: "test_task_workflow_step".to_string(),
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
            context: Some(serde_json::json!({"test": "context"})),
            identity_hash: "test_hash_workflow_step".to_string(),
            priority: Some(5),
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
        },
    )
    .await?;

    // Create named_step directly without dependent_system_uuid
    let named_step = NamedStep::create(
        &pool,
        NewNamedStep {
            name: "test_step_workflow_step".to_string(),
            description: None,
        },
    )
    .await?;

    // Test creation
    let new_step = NewWorkflowStep {
        task_uuid: task.task_uuid,
        named_step_uuid: named_step.named_step_uuid,
        retryable: Some(true),
        max_attempts: Some(5),
        inputs: Some(json!({"param1": "value1", "param2": 42})),
    };

    let created = WorkflowStep::create(&pool, new_step).await?;
    assert_eq!(created.task_uuid, task.task_uuid);
    assert_eq!(created.named_step_uuid, named_step.named_step_uuid);
    assert!(created.retryable);
    assert_eq!(created.max_attempts, Some(5));
    assert!(!created.processed);
    assert!(!created.in_process);

    // Test find by ID
    let found = WorkflowStep::find_by_id(&pool, created.workflow_step_uuid)
        .await?
        .expect("Step not found");
    assert_eq!(found.workflow_step_uuid, created.workflow_step_uuid);

    // Test mark in process
    // Note: mark_in_process() only sets the in_process flag (a denormalized query optimization field).
    // attempts and last_attempted_at are managed by the step claim logic, not by mark_in_process().
    let mut step_to_process = found.clone();
    step_to_process.mark_in_process(&pool).await?;
    assert!(step_to_process.in_process);

    // Test mark processed with results
    let results = json!({"output": "success", "count": 10});
    step_to_process
        .mark_processed(&pool, Some(results.clone()))
        .await?;
    assert!(step_to_process.processed);
    assert!(!step_to_process.in_process);
    assert!(step_to_process.processed_at.is_some());
    assert_eq!(step_to_process.results, Some(results));

    // Test retry logic
    assert!(!step_to_process.has_exceeded_max_attempts());
    assert!(!step_to_process.is_processing_eligible()); // Already processed

    // Test inputs update
    let new_inputs = json!({"updated_param": "new_value"});
    step_to_process
        .update_inputs(&pool, new_inputs.clone())
        .await?;
    assert_eq!(step_to_process.inputs, Some(new_inputs));

    // Test deletion
    let deleted = WorkflowStep::delete(&pool, created.workflow_step_uuid).await?;
    assert!(deleted);

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[test]
fn test_max_attempts_logic() {
    let task_uuid = Uuid::now_v7();
    let workflow_step_uuid = Uuid::now_v7();
    let named_step_uuid = Uuid::now_v7();

    let mut step = WorkflowStep {
        workflow_step_uuid,
        task_uuid,
        named_step_uuid,
        retryable: true,
        max_attempts: Some(3),
        in_process: false,
        processed: false,
        processed_at: None,
        attempts: Some(2),
        last_attempted_at: None,
        backoff_request_seconds: None,
        inputs: None,
        results: None,
        checkpoint: None,
        created_at: chrono::Utc::now().naive_utc(),
        updated_at: chrono::Utc::now().naive_utc(),
    };

    // Not exceeded yet
    assert!(!step.has_exceeded_max_attempts());
    assert!(step.is_processing_eligible());

    // Exceed limit
    step.attempts = Some(3);
    assert!(step.has_exceeded_max_attempts());

    // In backoff - set both backoff_request_seconds and last_attempted_at to create valid backoff state
    step.attempts = Some(1);
    step.backoff_request_seconds = Some(60);
    step.last_attempted_at = Some(chrono::Utc::now().naive_utc()); // Set to now to ensure we're in backoff period
    assert!(!step.is_processing_eligible());
}

// =============================================================================
// Factory-based integration tests covering uncovered WorkflowStep methods
// =============================================================================

use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
use tasker_shared::models::factories::base::SqlxFactory;
use tasker_shared::models::factories::complex_workflows::ComplexWorkflowFactory;
use tasker_shared::models::factories::core::{TaskFactory, WorkflowStepFactory};
use tasker_shared::models::factories::relationships::WorkflowStepEdgeFactory;
use tasker_shared::models::factories::states::WorkflowStepTransitionFactory;

/// Test list_by_task and for_task by creating a task with 3 steps via factories
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_by_task(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    let step_a = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("list_by_task_a")
        .create(&pool)
        .await?;
    let step_b = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("list_by_task_b")
        .create(&pool)
        .await?;
    let step_c = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("list_by_task_c")
        .create(&pool)
        .await?;

    // list_by_task should return all 3 steps ordered by workflow_step_uuid
    let steps = WorkflowStep::list_by_task(&pool, task.task_uuid).await?;
    assert_eq!(steps.len(), 3);

    let step_uuids: Vec<Uuid> = steps.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(step_uuids.contains(&step_a.workflow_step_uuid));
    assert!(step_uuids.contains(&step_b.workflow_step_uuid));
    assert!(step_uuids.contains(&step_c.workflow_step_uuid));

    // for_task is an alias -- should return the same results
    let steps_via_alias = WorkflowStep::for_task(&pool, task.task_uuid).await?;
    assert_eq!(steps_via_alias.len(), 3);

    Ok(())
}

/// Test list_unprocessed and list_in_process after marking some steps in_process
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_unprocessed_and_in_process(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    // Create 3 steps
    let _step_a = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("unprocessed_step_a")
        .create(&pool)
        .await?;
    let mut step_b = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("unprocessed_step_b")
        .create(&pool)
        .await?;
    let _step_c = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("unprocessed_step_c")
        .create(&pool)
        .await?;

    // Mark step_b as in_process
    step_b.mark_in_process(&pool).await?;
    assert!(step_b.in_process);

    // list_unprocessed should return steps that are not processed AND not in_process
    let unprocessed = WorkflowStep::list_unprocessed(&pool).await?;
    let unprocessed_uuids: Vec<Uuid> = unprocessed.iter().map(|s| s.workflow_step_uuid).collect();
    // step_b should NOT be in unprocessed (it is in_process)
    assert!(!unprocessed_uuids.contains(&step_b.workflow_step_uuid));

    // list_in_process should include step_b
    let in_process = WorkflowStep::list_in_process(&pool).await?;
    let in_process_uuids: Vec<Uuid> = in_process.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(in_process_uuids.contains(&step_b.workflow_step_uuid));

    Ok(())
}

/// Test set_backoff and reset_for_retry
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_set_backoff_and_reset(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    let mut step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("backoff_step")
        .create(&pool)
        .await?;

    // Mark in process first
    step.mark_in_process(&pool).await?;
    assert!(step.in_process);

    // Set backoff -- should clear in_process and set backoff_request_seconds
    step.set_backoff(&pool, 30).await?;
    assert_eq!(step.backoff_request_seconds, Some(30));
    assert!(!step.in_process);

    // Verify via reload from DB
    let reloaded = WorkflowStep::find_by_id(&pool, step.workflow_step_uuid)
        .await?
        .expect("Step should exist");
    assert_eq!(reloaded.backoff_request_seconds, Some(30));
    assert!(!reloaded.in_process);

    // Reset for retry -- should clear processed, in_process, results, backoff
    step.reset_for_retry(&pool).await?;
    assert!(!step.in_process);
    assert!(!step.processed);
    assert!(step.processed_at.is_none());
    assert!(step.results.is_none());
    assert!(step.backoff_request_seconds.is_none());

    Ok(())
}

/// Test get_current_state with transitions
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_current_state(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let step = WorkflowStepFactory::new()
        .with_named_step("current_state_step")
        .create(&pool)
        .await?;

    // No transitions yet -- get_current_state returns None
    let state = step.get_current_state(&pool).await?;
    assert!(state.is_none());

    // Create a transition to "pending"
    WorkflowStepTransitionFactory::new()
        .for_workflow_step(step.workflow_step_uuid)
        .to_state("pending")
        .create(&pool)
        .await?;

    let state = step.get_current_state(&pool).await?;
    assert_eq!(state.as_deref(), Some("pending"));

    // Create another transition to "in_progress"
    WorkflowStepTransitionFactory::new()
        .for_workflow_step(step.workflow_step_uuid)
        .from_state("pending")
        .to_in_progress()
        .create(&pool)
        .await?;

    let state = step.get_current_state(&pool).await?;
    assert_eq!(state.as_deref(), Some("in_progress"));

    Ok(())
}

/// Test dependency graph operations with a diamond workflow (A -> B,C -> D)
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_dependency_graph_diamond(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let (task_uuid, step_uuids) = ComplexWorkflowFactory::new()
        .diamond()
        .create(&pool)
        .await?;
    assert_eq!(step_uuids.len(), 4);

    // step_uuids: [A, B, C, D]
    let step_a = WorkflowStep::find_by_id(&pool, step_uuids[0])
        .await?
        .expect("Step A");
    let step_b = WorkflowStep::find_by_id(&pool, step_uuids[1])
        .await?
        .expect("Step B");
    let step_d = WorkflowStep::find_by_id(&pool, step_uuids[3])
        .await?
        .expect("Step D");

    // A is a root step -- no dependencies
    let deps_a = step_a.get_dependencies(&pool).await?;
    assert_eq!(deps_a.len(), 0);
    assert!(step_a.root_step(&pool).await?);
    assert!(!step_a.leaf_step(&pool).await?);

    // A has 2 dependents: B and C
    let dependents_a = step_a.get_dependents(&pool).await?;
    assert_eq!(dependents_a.len(), 2);

    // B has 1 dependency: A
    let deps_b = step_b.get_dependencies(&pool).await?;
    assert_eq!(deps_b.len(), 1);
    assert_eq!(deps_b[0].workflow_step_uuid, step_a.workflow_step_uuid);
    assert!(!step_b.root_step(&pool).await?);
    assert!(!step_b.leaf_step(&pool).await?);

    // D is a leaf step -- no dependents
    let dependents_d = step_d.get_dependents(&pool).await?;
    assert_eq!(dependents_d.len(), 0);
    assert!(step_d.leaf_step(&pool).await?);
    assert!(!step_d.root_step(&pool).await?);

    // D has 2 dependencies: B and C
    let deps_d = step_d.get_dependencies(&pool).await?;
    assert_eq!(deps_d.len(), 2);

    // All steps belong to the same task
    for uuid in &step_uuids {
        let s = WorkflowStep::find_by_id(&pool, *uuid)
            .await?
            .expect("Step must exist");
        assert_eq!(s.task_uuid, task_uuid);
    }

    Ok(())
}

/// Test state scopes: completed(), failed(), pending(), by_current_state()
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_state_scopes(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    // Create steps with different states
    let step_complete = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("scope_complete")
        .create(&pool)
        .await?;

    let step_error = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("scope_error")
        .create(&pool)
        .await?;

    let step_pending = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("scope_pending")
        .create(&pool)
        .await?;

    // Set up transitions for complete step
    WorkflowStepTransitionFactory::create_complete_lifecycle(
        step_complete.workflow_step_uuid,
        &pool,
    )
    .await?;

    // Set up transitions for error step
    WorkflowStepTransitionFactory::create_failed_lifecycle(
        step_error.workflow_step_uuid,
        "test failure",
        &pool,
    )
    .await?;

    // Pending step has no transitions -- it will show up in pending() scope

    // Test completed() scope
    let completed = WorkflowStep::completed(&pool).await?;
    let completed_uuids: Vec<Uuid> = completed.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(completed_uuids.contains(&step_complete.workflow_step_uuid));
    assert!(!completed_uuids.contains(&step_error.workflow_step_uuid));
    assert!(!completed_uuids.contains(&step_pending.workflow_step_uuid));

    // Test failed() scope
    let failed = WorkflowStep::failed(&pool).await?;
    let failed_uuids: Vec<Uuid> = failed.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(failed_uuids.contains(&step_error.workflow_step_uuid));
    assert!(!failed_uuids.contains(&step_complete.workflow_step_uuid));

    // Test pending() scope
    let pending = WorkflowStep::pending(&pool).await?;
    let pending_uuids: Vec<Uuid> = pending.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(pending_uuids.contains(&step_pending.workflow_step_uuid));

    // Test by_current_state() with "complete" filter
    let by_complete = WorkflowStep::by_current_state(&pool, Some("complete")).await?;
    let by_complete_uuids: Vec<Uuid> = by_complete.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(by_complete_uuids.contains(&step_complete.workflow_step_uuid));

    // Test by_current_state() with "error" filter
    let by_error = WorkflowStep::by_current_state(&pool, Some("error")).await?;
    let by_error_uuids: Vec<Uuid> = by_error.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(by_error_uuids.contains(&step_error.workflow_step_uuid));

    // Test by_current_state() with None (returns all)
    let all_steps = WorkflowStep::by_current_state(&pool, None).await?;
    assert!(all_steps.len() >= 3);

    Ok(())
}

/// Test completed_since and failed_since with known timestamps
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_time_scopes(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // Record a timestamp before creating any data
    let before_time = chrono::Utc::now();

    let task = TaskFactory::new().create(&pool).await?;

    let step_complete = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("time_scope_complete")
        .create(&pool)
        .await?;

    let step_error = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("time_scope_error")
        .create(&pool)
        .await?;

    // Create complete lifecycle
    WorkflowStepTransitionFactory::create_complete_lifecycle(
        step_complete.workflow_step_uuid,
        &pool,
    )
    .await?;

    // Create failed lifecycle
    WorkflowStepTransitionFactory::create_failed_lifecycle(
        step_error.workflow_step_uuid,
        "timeout error",
        &pool,
    )
    .await?;

    // completed_since before_time should include step_complete
    let completed_since = WorkflowStep::completed_since(&pool, before_time).await?;
    let completed_uuids: Vec<Uuid> = completed_since
        .iter()
        .map(|s| s.workflow_step_uuid)
        .collect();
    assert!(completed_uuids.contains(&step_complete.workflow_step_uuid));
    assert!(!completed_uuids.contains(&step_error.workflow_step_uuid));

    // failed_since before_time should include step_error
    let failed_since = WorkflowStep::failed_since(&pool, before_time).await?;
    let failed_uuids: Vec<Uuid> = failed_since.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(failed_uuids.contains(&step_error.workflow_step_uuid));
    assert!(!failed_uuids.contains(&step_complete.workflow_step_uuid));

    // Using a future timestamp should return empty
    let future_time = chrono::Utc::now() + chrono::Duration::hours(1);
    let empty_completed = WorkflowStep::completed_since(&pool, future_time).await?;
    assert!(empty_completed.is_empty());
    let empty_failed = WorkflowStep::failed_since(&pool, future_time).await?;
    assert!(empty_failed.is_empty());

    Ok(())
}

/// Test state check methods: complete(), in_progress(), is_pending(), in_error(), status()
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_state_check_methods(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    // Step with no transitions -- should be pending
    let step_no_transition = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("check_no_transition")
        .create(&pool)
        .await?;

    assert!(step_no_transition.is_pending(&pool).await?);
    assert!(!step_no_transition.in_progress(&pool).await?);
    assert!(!step_no_transition.complete(&pool).await?);
    assert!(!step_no_transition.in_error(&pool).await?);
    let status_pending = step_no_transition.status(&pool).await?;
    assert_eq!(status_pending, "pending");

    // Step with complete lifecycle
    let step_complete = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("check_complete")
        .create(&pool)
        .await?;
    WorkflowStepTransitionFactory::create_complete_lifecycle(
        step_complete.workflow_step_uuid,
        &pool,
    )
    .await?;

    assert!(step_complete.complete(&pool).await?);
    assert!(!step_complete.is_pending(&pool).await?);
    assert!(!step_complete.in_progress(&pool).await?);
    let status_complete = step_complete.status(&pool).await?;
    assert_eq!(status_complete, "complete");

    // Step with error lifecycle
    let step_error = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("check_error")
        .create(&pool)
        .await?;
    WorkflowStepTransitionFactory::create_failed_lifecycle(
        step_error.workflow_step_uuid,
        "check error message",
        &pool,
    )
    .await?;

    assert!(step_error.in_error(&pool).await?);
    assert!(!step_error.complete(&pool).await?);
    assert!(!step_error.is_pending(&pool).await?);
    let status_error = step_error.status(&pool).await?;
    assert_eq!(status_error, "error");

    // Step with only in_progress transition
    let step_in_progress = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("check_in_progress")
        .create(&pool)
        .await?;
    WorkflowStepTransitionFactory::new()
        .for_workflow_step(step_in_progress.workflow_step_uuid)
        .to_in_progress()
        .create(&pool)
        .await?;

    assert!(step_in_progress.in_progress(&pool).await?);
    assert!(!step_in_progress.is_pending(&pool).await?);
    assert!(!step_in_progress.complete(&pool).await?);

    Ok(())
}

/// Test retry_eligible, can_retry_now, retry_exhausted, has_retry_attempts
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_retry_methods(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    // Create a retryable step and put it in error state
    let step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("retry_step")
        .create(&pool)
        .await?;

    // Move to error state via transitions
    WorkflowStepTransitionFactory::create_failed_lifecycle(
        step.workflow_step_uuid,
        "transient failure",
        &pool,
    )
    .await?;

    // Step is retryable (default), in error, and has not exceeded max_attempts
    assert!(step.retryable);
    assert!(!step.has_exceeded_max_attempts());
    assert!(!step.retry_exhausted());
    assert!(step.retry_eligible(&pool).await?);
    assert!(step.can_retry_now(&pool).await?);
    // has_retry_attempts is false because attempts is 0 (factory default)
    assert!(!step.has_retry_attempts());

    // Create a non-retryable step in error state
    let mut step_non_retryable = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("non_retry_step")
        .create(&pool)
        .await?;

    // Manually set retryable to false via direct SQL (factory creates retryable=true)
    sqlx::query!(
        "UPDATE tasker.workflow_steps SET retryable = false WHERE workflow_step_uuid = $1::uuid",
        step_non_retryable.workflow_step_uuid
    )
    .execute(&pool)
    .await?;
    step_non_retryable.retryable = false;

    WorkflowStepTransitionFactory::create_failed_lifecycle(
        step_non_retryable.workflow_step_uuid,
        "permanent failure",
        &pool,
    )
    .await?;

    assert!(!step_non_retryable.retry_eligible(&pool).await?);
    assert!(!step_non_retryable.can_retry_now(&pool).await?);

    Ok(())
}

/// Test dependencies_met and count_unmet_dependencies using a diamond workflow
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_dependencies_met(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let (_task_uuid, step_uuids) = ComplexWorkflowFactory::new()
        .diamond()
        .create(&pool)
        .await?;

    // Diamond: A -> B, A -> C, B -> D, C -> D
    let step_a = WorkflowStep::find_by_id(&pool, step_uuids[0])
        .await?
        .expect("Step A");
    let step_b = WorkflowStep::find_by_id(&pool, step_uuids[1])
        .await?
        .expect("Step B");
    let step_d = WorkflowStep::find_by_id(&pool, step_uuids[3])
        .await?
        .expect("Step D");

    // A is a root -- dependencies are automatically met (no parents)
    assert!(step_a.dependencies_met(&pool).await?);
    assert_eq!(step_a.count_unmet_dependencies(&pool).await?, 0);

    // B depends on A -- A is not complete yet, so dependencies NOT met
    assert!(!step_b.dependencies_met(&pool).await?);
    assert_eq!(step_b.count_unmet_dependencies(&pool).await?, 1);

    // D depends on B and C -- both not complete, 2 unmet
    assert!(!step_d.dependencies_met(&pool).await?);
    assert_eq!(step_d.count_unmet_dependencies(&pool).await?, 2);

    // Complete step A via transition
    WorkflowStepTransitionFactory::create_complete_lifecycle(step_a.workflow_step_uuid, &pool)
        .await?;

    // Now B's dependencies should be met
    assert!(step_b.dependencies_met(&pool).await?);
    assert_eq!(step_b.count_unmet_dependencies(&pool).await?, 0);

    // D still has 2 unmet (B and C not complete yet)
    assert!(!step_d.dependencies_met(&pool).await?);
    assert_eq!(step_d.count_unmet_dependencies(&pool).await?, 2);

    // Complete step B
    WorkflowStepTransitionFactory::create_complete_lifecycle(step_b.workflow_step_uuid, &pool)
        .await?;

    // D still has 1 unmet (C not complete yet)
    assert!(!step_d.dependencies_met(&pool).await?);
    assert_eq!(step_d.count_unmet_dependencies(&pool).await?, 1);

    // Complete step C
    let step_c = WorkflowStep::find_by_id(&pool, step_uuids[2])
        .await?
        .expect("Step C");
    WorkflowStepTransitionFactory::create_complete_lifecycle(step_c.workflow_step_uuid, &pool)
        .await?;

    // Now D's dependencies are all met
    assert!(step_d.dependencies_met(&pool).await?);
    assert_eq!(step_d.count_unmet_dependencies(&pool).await?, 0);

    Ok(())
}

/// Test guard delegation methods: not_in_progress, can_be_retried, can_be_enqueued_for_orchestration
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_guard_delegation(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    // Step with no transition -- not_in_progress should be true
    let step_no_state = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("guard_no_state")
        .create(&pool)
        .await?;

    assert!(step_no_state.not_in_progress(&pool).await?);
    assert!(!step_no_state.can_be_retried(&pool).await?);
    assert!(
        !step_no_state
            .can_be_enqueued_for_orchestration(&pool)
            .await?
    );

    // Step in in_progress state
    let step_ip = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("guard_in_progress")
        .create(&pool)
        .await?;
    WorkflowStepTransitionFactory::new()
        .for_workflow_step(step_ip.workflow_step_uuid)
        .to_in_progress()
        .create(&pool)
        .await?;

    assert!(!step_ip.not_in_progress(&pool).await?);
    assert!(!step_ip.can_be_retried(&pool).await?);
    assert!(step_ip.can_be_enqueued_for_orchestration(&pool).await?);

    // Step in error state
    let step_err = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("guard_error")
        .create(&pool)
        .await?;
    WorkflowStepTransitionFactory::create_failed_lifecycle(
        step_err.workflow_step_uuid,
        "guard test error",
        &pool,
    )
    .await?;

    assert!(step_err.not_in_progress(&pool).await?);
    assert!(step_err.can_be_retried(&pool).await?);
    assert!(!step_err.can_be_enqueued_for_orchestration(&pool).await?);

    // Step in complete state
    let step_done = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("guard_complete")
        .create(&pool)
        .await?;
    WorkflowStepTransitionFactory::create_complete_lifecycle(step_done.workflow_step_uuid, &pool)
        .await?;

    assert!(step_done.not_in_progress(&pool).await?);
    assert!(!step_done.can_be_retried(&pool).await?);
    assert!(!step_done.can_be_enqueued_for_orchestration(&pool).await?);

    Ok(())
}

/// Test find_step_by_name
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_find_step_by_name(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    let step_alpha = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("alpha_step")
        .create(&pool)
        .await?;
    let step_beta = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("beta_step")
        .create(&pool)
        .await?;

    // Find step by name
    let found_alpha = WorkflowStep::find_step_by_name(&pool, task.task_uuid, "alpha_step")
        .await?
        .expect("alpha_step should exist");
    assert_eq!(
        found_alpha.workflow_step_uuid,
        step_alpha.workflow_step_uuid
    );

    let found_beta = WorkflowStep::find_step_by_name(&pool, task.task_uuid, "beta_step")
        .await?
        .expect("beta_step should exist");
    assert_eq!(found_beta.workflow_step_uuid, step_beta.workflow_step_uuid);

    // Non-existent name should return None
    let not_found =
        WorkflowStep::find_step_by_name(&pool, task.task_uuid, "nonexistent_step").await?;
    assert!(not_found.is_none());

    Ok(())
}

/// Test task_completion_stats with mixed-state steps
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_completion_stats(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new()
        .with_state_transitions()
        .create(&pool)
        .await?;

    // Create 3 steps: one complete, one failed, one pending
    let step_complete = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("stats_complete")
        .create(&pool)
        .await?;
    let step_error = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("stats_error")
        .create(&pool)
        .await?;
    let _step_pending = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("stats_pending")
        .create(&pool)
        .await?;

    // Set states via transitions
    WorkflowStepTransitionFactory::create_complete_lifecycle(
        step_complete.workflow_step_uuid,
        &pool,
    )
    .await?;
    WorkflowStepTransitionFactory::create_failed_lifecycle(
        step_error.workflow_step_uuid,
        "stats test error",
        &pool,
    )
    .await?;
    // Pending step gets a pending transition
    WorkflowStepTransitionFactory::new()
        .for_workflow_step(_step_pending.workflow_step_uuid)
        .to_state("pending")
        .create(&pool)
        .await?;

    let stats = WorkflowStep::task_completion_stats(&pool, task.task_uuid).await?;

    assert_eq!(stats.total_steps, 3);
    assert_eq!(stats.completed_steps, 1);
    assert_eq!(stats.failed_steps, 1);
    assert_eq!(stats.pending_steps, 1);
    assert!(!stats.all_complete);

    Ok(())
}

/// Test get_viable_steps using a diamond workflow with partial completion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_viable_steps(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let (task_uuid, step_uuids) = ComplexWorkflowFactory::new()
        .diamond()
        .create(&pool)
        .await?;

    // Diamond: A -> B, A -> C, B -> D, C -> D
    // With no transitions, only root step A should be viable
    // Set up pending transitions for all steps
    for uuid in &step_uuids {
        WorkflowStepTransitionFactory::new()
            .for_workflow_step(*uuid)
            .to_state("pending")
            .create(&pool)
            .await?;
    }

    let viable = WorkflowStep::get_viable_steps(&pool, task_uuid, json!({})).await?;
    // Only root step A should be ready (its deps are met since it has none)
    let viable_uuids: Vec<Uuid> = viable.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(
        viable_uuids.contains(&step_uuids[0]),
        "Root step A should be viable"
    );
    // B and C should NOT be viable (A not complete)
    assert!(!viable_uuids.contains(&step_uuids[1]));
    assert!(!viable_uuids.contains(&step_uuids[2]));
    assert!(!viable_uuids.contains(&step_uuids[3]));

    Ok(())
}

/// Test WorkflowStepWithName::find_by_id and find_by_ids
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_with_name_queries(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    let step_one = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("with_name_one")
        .create(&pool)
        .await?;
    let step_two = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("with_name_two")
        .create(&pool)
        .await?;

    // find_by_id should return the step with its name
    let found = WorkflowStepWithName::find_by_id(&pool, step_one.workflow_step_uuid)
        .await?
        .expect("WorkflowStepWithName should exist");
    assert_eq!(found.workflow_step_uuid, step_one.workflow_step_uuid);
    assert_eq!(found.name, "with_name_one");
    // template_step_name should equal name when no __template_step_name in inputs
    assert_eq!(found.template_step_name, "with_name_one");

    // find_by_ids should return both steps
    let found_all = WorkflowStepWithName::find_by_ids(
        &pool,
        &[step_one.workflow_step_uuid, step_two.workflow_step_uuid],
    )
    .await?;
    assert_eq!(found_all.len(), 2);

    let names: Vec<&str> = found_all.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"with_name_one"));
    assert!(names.contains(&"with_name_two"));

    // find_by_id with non-existent UUID should return None
    let not_found = WorkflowStepWithName::find_by_id(&pool, Uuid::now_v7()).await?;
    assert!(not_found.is_none());

    Ok(())
}

/// Test get_correlation_id method (TAS-157 JOIN optimization)
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_correlation_id(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    let step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("correlation_step")
        .create(&pool)
        .await?;

    // get_correlation_id should return the task's correlation_id
    let correlation_id = WorkflowStep::get_correlation_id(&pool, step.workflow_step_uuid).await?;
    assert!(
        correlation_id.is_some(),
        "correlation_id should be Some for existing step"
    );
    assert_eq!(correlation_id.unwrap(), task.correlation_id);

    // Non-existent step should return None
    let none_result = WorkflowStep::get_correlation_id(&pool, Uuid::now_v7()).await?;
    assert!(none_result.is_none());

    Ok(())
}

/// Test list_by_named_step to find steps sharing the same named step across tasks
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_by_named_step(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // Create two tasks, each with a step using the same named step
    let task_1 = TaskFactory::new().create(&pool).await?;
    let task_2 = TaskFactory::new().create(&pool).await?;

    let step_1 = WorkflowStepFactory::new()
        .for_task(task_1.task_uuid)
        .with_named_step("shared_named_step")
        .create(&pool)
        .await?;
    let step_2 = WorkflowStepFactory::new()
        .for_task(task_2.task_uuid)
        .with_named_step("shared_named_step")
        .create(&pool)
        .await?;

    // Both steps share the same named_step_uuid
    assert_eq!(step_1.named_step_uuid, step_2.named_step_uuid);

    // list_by_named_step should return both steps
    let steps = WorkflowStep::list_by_named_step(&pool, step_1.named_step_uuid).await?;
    assert!(steps.len() >= 2);

    let step_uuids: Vec<Uuid> = steps.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(step_uuids.contains(&step_1.workflow_step_uuid));
    assert!(step_uuids.contains(&step_2.workflow_step_uuid));

    // Steps belong to different tasks
    let task_uuids: Vec<Uuid> = steps.iter().map(|s| s.task_uuid).collect();
    assert!(task_uuids.contains(&task_1.task_uuid));
    assert!(task_uuids.contains(&task_2.task_uuid));

    Ok(())
}

/// Test ready_status and ready methods
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_ready_status(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    // Step with no transitions -- ready_status should be true (not in terminal failure state)
    let step_pending = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("ready_pending")
        .create(&pool)
        .await?;

    assert!(step_pending.ready_status(&pool).await?);

    // Step in error state -- ready_status should be false
    let step_err = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("ready_error")
        .create(&pool)
        .await?;
    WorkflowStepTransitionFactory::create_failed_lifecycle(
        step_err.workflow_step_uuid,
        "ready_status test",
        &pool,
    )
    .await?;

    assert!(!step_err.ready_status(&pool).await?);

    // Step in complete state -- ready_status should be true (not a failure state)
    let step_done = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("ready_complete")
        .create(&pool)
        .await?;
    WorkflowStepTransitionFactory::create_complete_lifecycle(step_done.workflow_step_uuid, &pool)
        .await?;

    assert!(step_done.ready_status(&pool).await?);

    Ok(())
}

/// Test mark_processed and reload
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_mark_processed_and_reload(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    let mut step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("process_reload_step")
        .create(&pool)
        .await?;

    // Mark in process
    step.mark_in_process(&pool).await?;
    assert!(step.in_process);

    // Mark processed with results
    let results = json!({"outcome": "success", "processed_count": 42});
    step.mark_processed(&pool, Some(results.clone())).await?;
    assert!(step.processed);
    assert!(!step.in_process);
    assert!(step.processed_at.is_some());
    assert_eq!(step.results, Some(results.clone()));

    // Reload from database and verify
    step.reload(&pool).await?;
    assert!(step.processed);
    assert!(!step.in_process);
    assert_eq!(step.results, Some(results));

    Ok(())
}

/// Test for_tasks_since scope
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_for_tasks_since(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let before_time = chrono::Utc::now();

    let task = TaskFactory::new().create(&pool).await?;

    let step = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("tasks_since_step")
        .create(&pool)
        .await?;

    // for_tasks_since should return steps from tasks created after before_time
    let steps_since = WorkflowStep::for_tasks_since(&pool, before_time).await?;
    let step_uuids: Vec<Uuid> = steps_since.iter().map(|s| s.workflow_step_uuid).collect();
    assert!(step_uuids.contains(&step.workflow_step_uuid));

    // Future timestamp should return empty
    let future_time = chrono::Utc::now() + chrono::Duration::hours(1);
    let empty = WorkflowStep::for_tasks_since(&pool, future_time).await?;
    assert!(empty.is_empty());

    Ok(())
}

/// Test get_dependencies_with_names
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_dependencies_with_names(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let task = TaskFactory::new().create(&pool).await?;

    let step_parent = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("deps_parent_step")
        .create(&pool)
        .await?;
    let step_child = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("deps_child_step")
        .create(&pool)
        .await?;

    // Create edge: parent -> child
    WorkflowStepEdgeFactory::new()
        .with_from_step(step_parent.workflow_step_uuid)
        .with_to_step(step_child.workflow_step_uuid)
        .provides()
        .create(&pool)
        .await?;

    // get_dependencies_with_names on child should return parent with name
    let deps = step_child.get_dependencies_with_names(&pool).await?;
    assert_eq!(deps.len(), 1);
    let (dep_step, dep_name) = &deps[0];
    assert_eq!(dep_step.workflow_step_uuid, step_parent.workflow_step_uuid);
    assert_eq!(dep_name, "deps_parent_step");

    // Parent should have no dependencies with names
    let parent_deps = step_parent.get_dependencies_with_names(&pool).await?;
    assert!(parent_deps.is_empty());

    Ok(())
}
