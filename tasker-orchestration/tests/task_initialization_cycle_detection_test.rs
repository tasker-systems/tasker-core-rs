//! Task Initialization Cycle Detection Tests
//!
//! Tests for the cycle detection enforcement in WorkflowStepBuilder.
//! This ensures that workflows cannot create circular dependencies.
//!

use sqlx::PgPool;
use tasker_shared::models::{
    named_step::{NamedStep, NewNamedStep},
    named_task::{NamedTask, NewNamedTask},
    task::{NewTask, Task},
    task_namespace::{NewTaskNamespace, TaskNamespace},
    workflow_step::{NewWorkflowStep, WorkflowStep},
    workflow_step_edge::{NewWorkflowStepEdge, WorkflowStepEdge},
};
use uuid::Uuid;

/// Helper to create test task hierarchy
async fn create_test_task(pool: &PgPool, identity_hash: &str) -> sqlx::Result<Task> {
    let namespace = TaskNamespace::create(
        pool,
        NewTaskNamespace {
            name: format!("test_cycle_ns_{}", identity_hash),
            description: Some("Test namespace for cycle detection".to_string()),
        },
    )
    .await?;

    let named_task = NamedTask::create(
        pool,
        NewNamedTask {
            name: format!("test_cycle_task_{}", identity_hash),
            version: Some("1.0.0".to_string()),
            description: Some("Test task for cycle detection".to_string()),
            task_namespace_uuid: namespace.task_namespace_uuid,
            configuration: None,
        },
    )
    .await?;

    Task::create(
        pool,
        NewTask {
            named_task_uuid: named_task.named_task_uuid,
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            tags: None,
            context: Some(serde_json::json!({"test": "cycle_detection"})),
            identity_hash: identity_hash.to_string(),
            priority: Some(5),
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
        },
    )
    .await
}

/// Helper to create test workflow steps

async fn create_test_steps(
    pool: &PgPool,
    task_uuid: Uuid,
    step_names: &[&str],
) -> sqlx::Result<Vec<WorkflowStep>> {
    let mut workflow_steps = Vec::new();

    for name in step_names {
        // Create named step (no longer requires dependent_system)
        let named_step = NamedStep::create(
            pool,
            NewNamedStep {
                name: format!("{}_{}", name, task_uuid),
                description: Some(format!("Test step: {}", name)),
            },
        )
        .await?;

        let workflow_step = WorkflowStep::create(
            pool,
            NewWorkflowStep {
                task_uuid,
                named_step_uuid: named_step.named_step_uuid,
                retryable: Some(true),
                max_attempts: Some(3),
                inputs: None,
            },
        )
        .await?;

        workflow_steps.push(workflow_step);
    }

    Ok(workflow_steps)
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_direct_cycle_detection(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "direct_cycle").await?;
    let steps = create_test_steps(&pool, task.task_uuid, &["step_a", "step_b"]).await?;

    let step_a = &steps[0];
    let step_b = &steps[1];

    // Create edge A -> B
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_b.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // Check if B -> A would create a cycle (it should)
    let would_cycle = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_b.workflow_step_uuid,
        step_a.workflow_step_uuid,
    )
    .await?;

    assert!(would_cycle, "Expected cycle to be detected for B -> A");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_indirect_cycle_detection(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "indirect_cycle").await?;
    let steps = create_test_steps(&pool, task.task_uuid, &["step_a", "step_b", "step_c"]).await?;

    let step_a = &steps[0];
    let step_b = &steps[1];
    let step_c = &steps[2];

    // Create edges A -> B -> C
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_b.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_b.workflow_step_uuid,
            to_step_uuid: step_c.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // Check if C -> A would create a cycle (it should)
    let would_cycle = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_c.workflow_step_uuid,
        step_a.workflow_step_uuid,
    )
    .await?;

    assert!(would_cycle, "Expected cycle to be detected for C -> A");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_self_referencing_cycle(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "self_reference").await?;
    let steps = create_test_steps(&pool, task.task_uuid, &["step_a"]).await?;

    let step_a = &steps[0];

    // NOTE: The SQL function `would_create_cycle` doesn't detect self-loops (A -> A)
    // because the recursive CTE requires at least one edge to traverse.
    // Self-references are detected at the WorkflowStepBuilder level with a simple
    // equality check (from_uuid == to_uuid) before calling the SQL function.
    //
    // This test verifies the SQL function behavior - it returns false for self-loops,
    // which is expected and handled by the caller.
    let would_cycle = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_a.workflow_step_uuid,
        step_a.workflow_step_uuid,
    )
    .await?;

    assert!(
        !would_cycle,
        "SQL function does not detect self-loops (handled at WorkflowStepBuilder level)"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_valid_dag_no_cycle(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "valid_dag").await?;
    let steps = create_test_steps(&pool, task.task_uuid, &["step_a", "step_b", "step_c"]).await?;

    let step_a = &steps[0];
    let step_b = &steps[1];
    let step_c = &steps[2];

    // Create edges A -> B
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_b.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // Check if B -> C would create a cycle (it should NOT)
    let would_cycle = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_b.workflow_step_uuid,
        step_c.workflow_step_uuid,
    )
    .await?;

    assert!(!would_cycle, "Expected no cycle for valid DAG: A -> B -> C");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_no_cycle(pool: PgPool) -> sqlx::Result<()> {
    let task = create_test_task(&pool, "diamond_pattern").await?;
    let steps = create_test_steps(
        &pool,
        task.task_uuid,
        &["step_a", "step_b", "step_c", "step_d"],
    )
    .await?;

    let step_a = &steps[0];
    let step_b = &steps[1];
    let step_c = &steps[2];
    let step_d = &steps[3];

    // Create diamond pattern: A -> B, A -> C
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_b.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_c.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // Check if B -> D would create a cycle (it should NOT)
    let would_cycle_b_d = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_b.workflow_step_uuid,
        step_d.workflow_step_uuid,
    )
    .await?;

    assert!(
        !would_cycle_b_d,
        "Expected no cycle for diamond pattern: B -> D"
    );

    // Check if C -> D would create a cycle (it should NOT)
    let would_cycle_c_d = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_c.workflow_step_uuid,
        step_d.workflow_step_uuid,
    )
    .await?;

    assert!(
        !would_cycle_c_d,
        "Expected no cycle for diamond pattern: C -> D"
    );

    // Now create both edges to complete the diamond
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_b.workflow_step_uuid,
            to_step_uuid: step_d.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_c.workflow_step_uuid,
            to_step_uuid: step_d.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // Verify that D -> A would create a cycle
    let would_cycle_d_a = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_d.workflow_step_uuid,
        step_a.workflow_step_uuid,
    )
    .await?;

    assert!(
        would_cycle_d_a,
        "Expected cycle to be detected for D -> A after diamond completion"
    );

    Ok(())
}
