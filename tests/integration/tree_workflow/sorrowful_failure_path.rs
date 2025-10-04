use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test tree workflow with root failure blocking entire tree
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_root_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Root failure blocks entire tree");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "tree_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail tree_root permanently
    tracing::info!("📊 PHASE 1: Failing tree_root permanently");
    for _ in 0..3 {
        manager
            .fail_step(task_uuid, "tree_root", "Root validation error")
            .await?;
    }

    // Validate: root in terminal error, entire tree blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let root = step_readiness
        .iter()
        .find(|s| s.name == "tree_root")
        .unwrap();
    assert_eq!(root.current_state.as_deref(), Some("error"));
    assert_eq!(root.attempts, 3);
    assert!(!root.retry_eligible);

    // All descendant steps remain pending, never executable
    for step_name in [
        "tree_branch_left",
        "tree_branch_right",
        "tree_leaf_d",
        "tree_leaf_e",
        "tree_leaf_f",
        "tree_leaf_g",
        "tree_final_convergence",
    ] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert_eq!(step.current_state.as_deref(), Some("pending"));
        assert!(!step.dependencies_satisfied);
        assert!(!step.ready_for_execution);
    }

    manager
        .validate_task_execution_context(task_uuid, 8, 0, 0)
        .await?;

    tracing::info!("✅ Root failure permanently blocks entire tree");
    tracing::info!("🎯 SORROWFUL FAILURE: Root failure test complete");

    Ok(())
}

/// Test tree workflow with branch permanent failure blocking leaves
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_branch_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Branch permanent failure blocks leaves");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "tree_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete tree_root
    tracing::info!("📊 PHASE 1: Completing tree_root");
    manager
        .complete_step(task_uuid, "tree_root", serde_json::json!({"result": 4}))
        .await?;

    // **PHASE 2**: Complete branch_right, fail branch_left permanently
    tracing::info!("📊 PHASE 2: Completing branch_right, failing branch_left");
    manager
        .complete_step(
            task_uuid,
            "tree_branch_right",
            serde_json::json!({"result": 16}),
        )
        .await?;
    for _ in 0..3 {
        manager
            .fail_step(task_uuid, "tree_branch_left", "Branch processing error")
            .await?;
    }

    // Validate: branch_left failed, its leaves blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let branch_left = step_readiness
        .iter()
        .find(|s| s.name == "tree_branch_left")
        .unwrap();
    assert_eq!(branch_left.current_state.as_deref(), Some("error"));
    assert!(!branch_left.retry_eligible);

    // Left branch leaves (d, e) blocked
    for leaf_name in ["tree_leaf_d", "tree_leaf_e"] {
        let leaf = step_readiness.iter().find(|s| s.name == leaf_name).unwrap();
        assert_eq!(leaf.current_state.as_deref(), Some("pending"));
        assert!(!leaf.dependencies_satisfied);
    }

    // Right branch leaves (f, g) ready
    for leaf_name in ["tree_leaf_f", "tree_leaf_g"] {
        let leaf = step_readiness.iter().find(|s| s.name == leaf_name).unwrap();
        assert!(leaf.dependencies_satisfied);
        assert!(leaf.ready_for_execution);
    }

    manager
        .validate_task_execution_context(task_uuid, 8, 2, 2)
        .await?;

    tracing::info!("✅ Branch left failure blocks its leaves, branch right leaves ready");
    tracing::info!("🎯 SORROWFUL FAILURE: Branch permanent failure test complete");

    Ok(())
}

/// Test tree workflow with leaf permanent failure blocking convergence
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_leaf_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Leaf permanent failure blocks convergence");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "tree_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete root and both branches
    tracing::info!("📊 PHASE 1: Completing tree_root and both branches");
    manager
        .complete_step(task_uuid, "tree_root", serde_json::json!({"result": 4}))
        .await?;
    manager
        .complete_step(
            task_uuid,
            "tree_branch_left",
            serde_json::json!({"result": 16}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "tree_branch_right",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // **PHASE 2**: Complete 3 leaves, fail 1 leaf permanently
    tracing::info!("📊 PHASE 2: Completing 3 leaves, failing leaf_d permanently");
    for _ in 0..3 {
        manager
            .fail_step(task_uuid, "tree_leaf_d", "Leaf processing error")
            .await?;
    }
    manager
        .complete_step(task_uuid, "tree_leaf_e", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_f", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_g", serde_json::json!({"result": 256}))
        .await?;

    // Validate: leaf_d in terminal error, convergence permanently blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let leaf_d = step_readiness
        .iter()
        .find(|s| s.name == "tree_leaf_d")
        .unwrap();
    assert_eq!(leaf_d.current_state.as_deref(), Some("error"));
    assert!(!leaf_d.retry_eligible);

    let convergence = step_readiness
        .iter()
        .find(|s| s.name == "tree_final_convergence")
        .unwrap();
    assert_eq!(convergence.completed_parents, 3);
    assert!(!convergence.dependencies_satisfied);
    assert!(!convergence.ready_for_execution);

    manager
        .validate_task_execution_context(task_uuid, 8, 6, 0)
        .await?;

    tracing::info!("✅ Leaf D permanent failure blocks convergence");
    tracing::info!("🎯 SORROWFUL FAILURE: Leaf permanent failure test complete");

    Ok(())
}

/// Test tree workflow with mixed states and SQL accuracy
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_mixed_states_sql_accuracy(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Mixed states SQL accuracy");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "tree_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete root
    manager
        .complete_step(task_uuid, "tree_root", serde_json::json!({"result": 4}))
        .await?;

    // **PHASE 2**: Fail branch_left (leave in error)
    manager
        .fail_step(task_uuid, "tree_branch_left", "Transient error")
        .await?;

    // **PHASE 3**: Complete branch_right and 2 leaves
    manager
        .complete_step(
            task_uuid,
            "tree_branch_right",
            serde_json::json!({"result": 16}),
        )
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_f", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_g", serde_json::json!({"result": 256}))
        .await?;

    // **VALIDATION**: Mixed state verification
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    // Root: complete
    let root = step_readiness
        .iter()
        .find(|s| s.name == "tree_root")
        .unwrap();
    assert_eq!(root.current_state.as_deref(), Some("complete"));

    // Branch left: error (with retry eligibility)
    let branch_left = step_readiness
        .iter()
        .find(|s| s.name == "tree_branch_left")
        .unwrap();
    assert_eq!(branch_left.current_state.as_deref(), Some("error"));
    assert!(branch_left.retry_eligible);

    // Branch right: complete
    let branch_right = step_readiness
        .iter()
        .find(|s| s.name == "tree_branch_right")
        .unwrap();
    assert_eq!(branch_right.current_state.as_deref(), Some("complete"));

    // Left leaves: pending (blocked by branch_left error)
    for leaf_name in ["tree_leaf_d", "tree_leaf_e"] {
        let leaf = step_readiness.iter().find(|s| s.name == leaf_name).unwrap();
        assert_eq!(leaf.current_state.as_deref(), Some("pending"));
        assert!(!leaf.dependencies_satisfied);
    }

    // Right leaves: complete
    for leaf_name in ["tree_leaf_f", "tree_leaf_g"] {
        let leaf = step_readiness.iter().find(|s| s.name == leaf_name).unwrap();
        assert_eq!(leaf.current_state.as_deref(), Some("complete"));
    }

    // Convergence: pending (blocked by incomplete left leaves)
    let convergence = step_readiness
        .iter()
        .find(|s| s.name == "tree_final_convergence")
        .unwrap();
    assert_eq!(convergence.current_state.as_deref(), Some("pending"));
    assert_eq!(convergence.completed_parents, 2);
    assert!(!convergence.dependencies_satisfied);

    manager
        .validate_task_execution_context(task_uuid, 8, 4, 0)
        .await?;

    tracing::info!("✅ SQL functions accurately reflect complex mixed state tree");
    tracing::info!("🎯 SORROWFUL FAILURE: Mixed states SQL accuracy test complete");

    Ok(())
}
