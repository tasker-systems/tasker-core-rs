//! Task Transition Model Tests
//!
//! Tests for the TaskTransition model using SQLx native testing

use sqlx::PgPool;
use tasker_core::models::named_task::{NamedTask, NewNamedTask};
use tasker_core::models::task::{NewTask, Task};
use tasker_core::models::task_namespace::{NewTaskNamespace, TaskNamespace};
use tasker_core::models::task_transition::{NewTaskTransition, TaskTransition};

#[sqlx::test]
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
            bypass_steps: None,
            tags: None,
            context: None,
            identity_hash: "test_hash".to_string(),
            priority: Some(5),
            claim_timeout_seconds: Some(300),
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
        to_state: "in_progress".to_string(),
        metadata: Some(transition_metadata),
    };

    let created = TaskTransition::create(&pool, new_transition).await?;
    assert_eq!(created.task_uuid, task.task_uuid);
    assert_eq!(created.from_state, Some("pending".to_string()));
    assert_eq!(created.to_state, "in_progress".to_string());

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
        "in_progress".to_string()
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
        from_state: Some("in_progress".to_string()),
        to_state: "complete".to_string(),
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

#[sqlx::test]
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
            bypass_steps: None,
            tags: None,
            context: None,
            priority: Some(5),
            claim_timeout_seconds: Some(300),
        },
    )
    .await?;

    // Test status progression
    let statuses = [
        "pending",
        "in_progress",
        "paused",
        "in_progress",
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
