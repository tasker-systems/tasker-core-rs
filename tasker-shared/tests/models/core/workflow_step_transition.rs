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
            skippable: Some(false),
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
