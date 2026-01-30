//! Workflow Step Edge Model Tests
//!
//! Tests for the WorkflowStepEdge model (DAG edge management) using SQLx native testing.
//! WorkflowStepEdge tracks dependency relationships between workflow steps,
//! ensuring the workflow forms a valid DAG without cycles.

use sqlx::PgPool;
use tasker_shared::models::core::workflow_step_edge::{NewWorkflowStepEdge, WorkflowStepEdge};
use tasker_shared::models::factories::base::SqlxFactory;
use tasker_shared::models::factories::{TaskFactory, WorkflowStepFactory};

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_edge_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create a task and two workflow steps
    let task = TaskFactory::new().create(&pool).await.unwrap();
    let step1 = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("edge_crud_step_1")
        .create(&pool)
        .await
        .unwrap();
    let step2 = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("edge_crud_step_2")
        .create(&pool)
        .await
        .unwrap();

    // Create edge: step1 -> step2
    let new_edge = NewWorkflowStepEdge {
        from_step_uuid: step1.workflow_step_uuid,
        to_step_uuid: step2.workflow_step_uuid,
        name: "provides".to_string(),
    };
    let edge = WorkflowStepEdge::create(&pool, new_edge).await?;
    assert_eq!(edge.from_step_uuid, step1.workflow_step_uuid);
    assert_eq!(edge.to_step_uuid, step2.workflow_step_uuid);
    assert_eq!(edge.name, "provides");

    // Find the edge by steps and name
    let found = WorkflowStepEdge::find_by_steps_and_name(
        &pool,
        step1.workflow_step_uuid,
        step2.workflow_step_uuid,
        "provides",
    )
    .await?;
    assert!(found.is_some());
    assert_eq!(
        found.unwrap().workflow_step_edge_uuid,
        edge.workflow_step_edge_uuid
    );

    // Delete the edge
    let deleted =
        WorkflowStepEdge::delete(&pool, step1.workflow_step_uuid, step2.workflow_step_uuid).await?;
    assert!(deleted);

    // Verify deletion
    let not_found = WorkflowStepEdge::find_by_steps_and_name(
        &pool,
        step1.workflow_step_uuid,
        step2.workflow_step_uuid,
        "provides",
    )
    .await?;
    assert!(not_found.is_none());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_edge_find_by_steps_and_name(pool: PgPool) -> sqlx::Result<()> {
    let task = TaskFactory::new().create(&pool).await.unwrap();
    let step_a = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("find_edge_a")
        .create(&pool)
        .await
        .unwrap();
    let step_b = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("find_edge_b")
        .create(&pool)
        .await
        .unwrap();

    // Create edge with specific name
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_b.workflow_step_uuid,
            name: "depends_on".to_string(),
        },
    )
    .await?;

    // Find with correct name
    let found = WorkflowStepEdge::find_by_steps_and_name(
        &pool,
        step_a.workflow_step_uuid,
        step_b.workflow_step_uuid,
        "depends_on",
    )
    .await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "depends_on");

    // Find with wrong name
    let not_found = WorkflowStepEdge::find_by_steps_and_name(
        &pool,
        step_a.workflow_step_uuid,
        step_b.workflow_step_uuid,
        "wrong_name",
    )
    .await?;
    assert!(not_found.is_none());

    // Find with reversed step order
    let not_found_reversed = WorkflowStepEdge::find_by_steps_and_name(
        &pool,
        step_b.workflow_step_uuid,
        step_a.workflow_step_uuid,
        "depends_on",
    )
    .await?;
    assert!(not_found_reversed.is_none());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_edge_find_dependencies(pool: PgPool) -> sqlx::Result<()> {
    // Create a chain: step_a -> step_b -> step_c
    let task = TaskFactory::new().create(&pool).await.unwrap();
    let step_a = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("dep_step_a")
        .create(&pool)
        .await
        .unwrap();
    let step_b = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("dep_step_b")
        .create(&pool)
        .await
        .unwrap();
    let step_c = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("dep_step_c")
        .create(&pool)
        .await
        .unwrap();

    // a -> b
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_b.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // b -> c
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_b.workflow_step_uuid,
            to_step_uuid: step_c.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // step_a has no dependencies (root)
    let a_deps = WorkflowStepEdge::find_dependencies(&pool, step_a.workflow_step_uuid).await?;
    assert!(a_deps.is_empty());

    // step_b depends on step_a
    let b_deps = WorkflowStepEdge::find_dependencies(&pool, step_b.workflow_step_uuid).await?;
    assert_eq!(b_deps.len(), 1);
    assert_eq!(b_deps[0], step_a.workflow_step_uuid);

    // step_c depends on step_b
    let c_deps = WorkflowStepEdge::find_dependencies(&pool, step_c.workflow_step_uuid).await?;
    assert_eq!(c_deps.len(), 1);
    assert_eq!(c_deps[0], step_b.workflow_step_uuid);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_edge_find_dependents(pool: PgPool) -> sqlx::Result<()> {
    // Create a fan-out: step_a -> step_b, step_a -> step_c
    let task = TaskFactory::new().create(&pool).await.unwrap();
    let step_a = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("fan_step_a")
        .create(&pool)
        .await
        .unwrap();
    let step_b = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("fan_step_b")
        .create(&pool)
        .await
        .unwrap();
    let step_c = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("fan_step_c")
        .create(&pool)
        .await
        .unwrap();

    // a -> b
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_b.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // a -> c
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_c.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // step_a has 2 dependents
    let a_dependents = WorkflowStepEdge::find_dependents(&pool, step_a.workflow_step_uuid).await?;
    assert_eq!(a_dependents.len(), 2);
    assert!(a_dependents.contains(&step_b.workflow_step_uuid));
    assert!(a_dependents.contains(&step_c.workflow_step_uuid));

    // step_b has no dependents (leaf)
    let b_dependents = WorkflowStepEdge::find_dependents(&pool, step_b.workflow_step_uuid).await?;
    assert!(b_dependents.is_empty());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_edge_find_by_task(pool: PgPool) -> sqlx::Result<()> {
    // Create a task with edges between its steps
    let task = TaskFactory::new().create(&pool).await.unwrap();
    let step1 = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("task_edge_step_1")
        .create(&pool)
        .await
        .unwrap();
    let step2 = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("task_edge_step_2")
        .create(&pool)
        .await
        .unwrap();
    let step3 = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("task_edge_step_3")
        .create(&pool)
        .await
        .unwrap();

    // step1 -> step2, step2 -> step3
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step1.workflow_step_uuid,
            to_step_uuid: step2.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step2.workflow_step_uuid,
            to_step_uuid: step3.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // Find all edges for this task
    let task_edges = WorkflowStepEdge::find_by_task(&pool, task.task_uuid).await?;
    assert_eq!(task_edges.len(), 2);

    // Create another task with edges - should not appear in first task's results
    let other_task = TaskFactory::new().create(&pool).await.unwrap();
    let other_step1 = WorkflowStepFactory::new()
        .for_task(other_task.task_uuid)
        .with_named_step("other_task_step_1")
        .create(&pool)
        .await
        .unwrap();
    let other_step2 = WorkflowStepFactory::new()
        .for_task(other_task.task_uuid)
        .with_named_step("other_task_step_2")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: other_step1.workflow_step_uuid,
            to_step_uuid: other_step2.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // First task should still have exactly 2 edges
    let task_edges_after = WorkflowStepEdge::find_by_task(&pool, task.task_uuid).await?;
    assert_eq!(task_edges_after.len(), 2);

    // Other task should have 1 edge
    let other_edges = WorkflowStepEdge::find_by_task(&pool, other_task.task_uuid).await?;
    assert_eq!(other_edges.len(), 1);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_edge_would_create_cycle(pool: PgPool) -> sqlx::Result<()> {
    // Create a chain: A -> B -> C
    let task = TaskFactory::new().create(&pool).await.unwrap();
    let step_a = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("cycle_step_a")
        .create(&pool)
        .await
        .unwrap();
    let step_b = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("cycle_step_b")
        .create(&pool)
        .await
        .unwrap();
    let step_c = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("cycle_step_c")
        .create(&pool)
        .await
        .unwrap();

    // Create edges A -> B, B -> C
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

    // Adding C -> A would create a cycle: A -> B -> C -> A
    let would_cycle = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_c.workflow_step_uuid,
        step_a.workflow_step_uuid,
    )
    .await?;
    assert!(would_cycle, "C -> A should create a cycle");

    // Adding C -> B would also create a cycle: B -> C -> B
    let would_cycle_short = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_c.workflow_step_uuid,
        step_b.workflow_step_uuid,
    )
    .await?;
    assert!(would_cycle_short, "C -> B should create a cycle");

    // Adding A -> C should NOT create a cycle (just a shortcut edge)
    let no_cycle = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_a.workflow_step_uuid,
        step_c.workflow_step_uuid,
    )
    .await?;
    assert!(!no_cycle, "A -> C should not create a cycle");

    // A new independent step D: adding D -> A should NOT create a cycle
    let step_d = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("cycle_step_d")
        .create(&pool)
        .await
        .unwrap();
    let no_cycle_new = WorkflowStepEdge::would_create_cycle(
        &pool,
        step_d.workflow_step_uuid,
        step_a.workflow_step_uuid,
    )
    .await?;
    assert!(!no_cycle_new, "D -> A should not create a cycle");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_edge_root_and_leaf_steps(pool: PgPool) -> sqlx::Result<()> {
    // Build a diamond DAG: A -> B, A -> C, B -> D, C -> D
    let task = TaskFactory::new().create(&pool).await.unwrap();
    let step_a = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("diamond_a")
        .create(&pool)
        .await
        .unwrap();
    let step_b = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("diamond_b")
        .create(&pool)
        .await
        .unwrap();
    let step_c = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("diamond_c")
        .create(&pool)
        .await
        .unwrap();
    let step_d = WorkflowStepFactory::new()
        .for_task(task.task_uuid)
        .with_named_step("diamond_d")
        .create(&pool)
        .await
        .unwrap();

    // A -> B
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_b.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // A -> C
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_a.workflow_step_uuid,
            to_step_uuid: step_c.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // B -> D
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_b.workflow_step_uuid,
            to_step_uuid: step_d.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // C -> D
    WorkflowStepEdge::create(
        &pool,
        NewWorkflowStepEdge {
            from_step_uuid: step_c.workflow_step_uuid,
            to_step_uuid: step_d.workflow_step_uuid,
            name: "provides".to_string(),
        },
    )
    .await?;

    // Root steps: only A (no incoming edges)
    let root_steps = WorkflowStepEdge::find_root_steps(&pool, task.task_uuid).await?;
    assert_eq!(root_steps.len(), 1);
    assert!(root_steps.contains(&step_a.workflow_step_uuid));

    // Leaf steps: only D (no outgoing edges)
    let leaf_steps = WorkflowStepEdge::find_leaf_steps(&pool, task.task_uuid).await?;
    assert_eq!(leaf_steps.len(), 1);
    assert!(leaf_steps.contains(&step_d.workflow_step_uuid));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_edge_delete_nonexistent(pool: PgPool) -> sqlx::Result<()> {
    // Deleting an edge between non-existent steps should return false
    let deleted =
        WorkflowStepEdge::delete(&pool, uuid::Uuid::now_v7(), uuid::Uuid::now_v7()).await?;
    assert!(!deleted);

    Ok(())
}
