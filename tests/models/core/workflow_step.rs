//! Workflow Step Model Tests
//!
//! Tests for the WorkflowStep model using SQLx native testing

use serde_json::json;
use sqlx::PgPool;
use tasker_core::models::{
    dependent_system::{DependentSystem, NewDependentSystem},
    named_step::{NamedStep, NewNamedStep},
    named_task::{NamedTask, NewNamedTask},
    task::{NewTask, Task},
    task_namespace::{NewTaskNamespace, TaskNamespace},
    workflow_step::{NewWorkflowStep, WorkflowStep},
};

#[sqlx::test]
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
            task_namespace_id: namespace.task_namespace_id as i64,
            configuration: None,
        },
    )
    .await?;

    let task = Task::create(
        &pool,
        NewTask {
            named_task_id: named_task.named_task_id,
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: Some(serde_json::json!({"test": "context"})),
            identity_hash: "test_hash_workflow_step".to_string(),
        },
    )
    .await?;

    let dependent_system = DependentSystem::create(
        &pool,
        NewDependentSystem {
            name: "test_system_workflow_step".to_string(),
            description: None,
        },
    )
    .await?;

    let named_step = NamedStep::create(
        &pool,
        NewNamedStep {
            dependent_system_id: dependent_system.dependent_system_id,
            name: "test_step_workflow_step".to_string(),
            description: None,
        },
    )
    .await?;

    // Test creation
    let new_step = NewWorkflowStep {
        task_id: task.task_id,
        named_step_id: named_step.named_step_id,
        retryable: Some(true),
        retry_limit: Some(5),
        inputs: Some(json!({"param1": "value1", "param2": 42})),
        skippable: Some(false),
    };

    let created = WorkflowStep::create(&pool, new_step).await?;
    assert_eq!(created.task_id, task.task_id);
    assert_eq!(created.named_step_id, named_step.named_step_id);
    assert!(created.retryable);
    assert_eq!(created.retry_limit, Some(5));
    assert!(!created.processed);
    assert!(!created.in_process);

    // Test find by ID
    let found = WorkflowStep::find_by_id(&pool, created.workflow_step_id)
        .await?
        .expect("Step not found");
    assert_eq!(found.workflow_step_id, created.workflow_step_id);

    // Test mark in process
    let mut step_to_process = found.clone();
    step_to_process.mark_in_process(&pool).await?;
    assert!(step_to_process.in_process);
    assert!(step_to_process.last_attempted_at.is_some());
    assert_eq!(step_to_process.attempts, Some(1));

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
    assert!(!step_to_process.has_exceeded_retry_limit());
    assert!(!step_to_process.is_ready_for_processing()); // Already processed

    // Test inputs update
    let new_inputs = json!({"updated_param": "new_value"});
    step_to_process
        .update_inputs(&pool, new_inputs.clone())
        .await?;
    assert_eq!(step_to_process.inputs, Some(new_inputs));

    // Test deletion
    let deleted = WorkflowStep::delete(&pool, created.workflow_step_id).await?;
    assert!(deleted);

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[test]
fn test_retry_limit_logic() {
    let mut step = WorkflowStep {
        workflow_step_id: 1,
        task_id: 1,
        named_step_id: 1,
        retryable: true,
        retry_limit: Some(3),
        in_process: false,
        processed: false,
        processed_at: None,
        attempts: Some(2),
        last_attempted_at: None,
        backoff_request_seconds: None,
        inputs: None,
        results: None,
        skippable: false,
        created_at: chrono::Utc::now().naive_utc(),
        updated_at: chrono::Utc::now().naive_utc(),
    };

    // Not exceeded yet
    assert!(!step.has_exceeded_retry_limit());
    assert!(step.is_ready_for_processing());

    // Exceed limit
    step.attempts = Some(3);
    assert!(step.has_exceeded_retry_limit());

    // In backoff
    step.attempts = Some(1);
    step.backoff_request_seconds = Some(60);
    assert!(!step.is_ready_for_processing());
}
