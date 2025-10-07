use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test tree workflow with retry limit exhaustion on leaf
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_max_attempts_exhaustion(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Retry limit exhaustion blocks convergence");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "rust_e2e_tree",
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

    // **PHASE 2**: Complete 3 leaves successfully
    tracing::info!("📊 PHASE 2: Completing 3 leaves successfully");
    manager
        .complete_step(task_uuid, "tree_leaf_d", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_e", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_f", serde_json::json!({"result": 256}))
        .await?;

    // **PHASE 3**: Fail tree_leaf_g three times (exhaust retry limit)
    tracing::info!("📊 PHASE 3: Failing tree_leaf_g three times");
    manager
        .fail_step(task_uuid, "tree_leaf_g", "Leaf processing error")
        .await?;
    manager
        .fail_step(task_uuid, "tree_leaf_g", "Leaf processing error")
        .await?;
    manager
        .fail_step(task_uuid, "tree_leaf_g", "Leaf processing error")
        .await?;

    // Validate: leaf_g in terminal error, convergence blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let leaf_g = step_readiness
        .iter()
        .find(|s| s.name == "tree_leaf_g")
        .unwrap();
    assert_eq!(leaf_g.current_state.as_deref(), Some("error"));
    assert_eq!(leaf_g.attempts, 3, "Should have 3 attempts");
    assert!(!leaf_g.retry_eligible, "Should not be retry eligible");

    let convergence = step_readiness
        .iter()
        .find(|s| s.name == "tree_final_convergence")
        .unwrap();
    assert_eq!(convergence.total_parents, 4, "Convergence has 4 parents");
    assert_eq!(
        convergence.completed_parents, 3,
        "Only 3 leaves complete (d, e, f)"
    );
    assert!(
        !convergence.dependencies_satisfied,
        "Convergence dependencies not satisfied"
    );
    assert!(
        !convergence.ready_for_execution,
        "Convergence blocked by failed leaf"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 8, // total_steps
            6, // completed_steps (root, 2 branches, 3 leaves)
            0, // ready_steps (leaf_g failed, convergence blocked)
        )
        .await?;

    tracing::info!("✅ Leaf G retry limit exhausted, convergence blocked");
    tracing::info!("🎯 DELAYED GRATIFICATION: Retry limit exhaustion test complete");

    Ok(())
}

/// Test tree workflow recovery after branch retry
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_recovery_after_retry(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Recovery after branch retry");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "rust_e2e_tree",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete tree_root
    tracing::info!("📊 PHASE 1: Completing tree_root");
    manager
        .complete_step(task_uuid, "tree_root", serde_json::json!({"result": 4}))
        .await?;

    // **PHASE 2**: Complete branch_right, fail branch_left twice
    tracing::info!("📊 PHASE 2: Completing branch_right, failing branch_left twice");
    manager
        .complete_step(
            task_uuid,
            "tree_branch_right",
            serde_json::json!({"result": 16}),
        )
        .await?;
    manager
        .fail_step(task_uuid, "tree_branch_left", "Transient branch error")
        .await?;
    manager
        .fail_step(task_uuid, "tree_branch_left", "Transient branch error")
        .await?;

    // Validate: branch_left in error with retry eligibility, its leaves blocked
    let step_readiness_mid = manager.get_step_readiness_status(task_uuid).await?;

    let branch_left_mid = step_readiness_mid
        .iter()
        .find(|s| s.name == "tree_branch_left")
        .unwrap();
    assert_eq!(branch_left_mid.attempts, 2, "Should have 2 attempts");
    assert!(
        branch_left_mid.retry_eligible,
        "Should still be retry eligible"
    );

    // Left branch leaves (d, e) should be blocked
    for leaf_name in ["tree_leaf_d", "tree_leaf_e"] {
        let leaf = step_readiness_mid
            .iter()
            .find(|s| s.name == leaf_name)
            .unwrap();
        assert!(
            !leaf.dependencies_satisfied,
            "{} dependencies not satisfied",
            leaf_name
        );
        assert!(!leaf.ready_for_execution, "{} not ready", leaf_name);
    }

    // Right branch leaves (f, g) should be ready
    for leaf_name in ["tree_leaf_f", "tree_leaf_g"] {
        let leaf = step_readiness_mid
            .iter()
            .find(|s| s.name == leaf_name)
            .unwrap();
        assert!(
            leaf.dependencies_satisfied,
            "{} dependencies satisfied",
            leaf_name
        );
        assert!(leaf.ready_for_execution, "{} ready", leaf_name);
    }

    // **PHASE 3**: Recover branch_left on third attempt
    tracing::info!("📊 PHASE 3: Recovering tree_branch_left on third attempt");
    manager
        .complete_step(
            task_uuid,
            "tree_branch_left",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // Validate: branch_left complete with 3 attempts, its leaves now ready
    let step_readiness_after = manager.get_step_readiness_status(task_uuid).await?;

    let branch_left_after = step_readiness_after
        .iter()
        .find(|s| s.name == "tree_branch_left")
        .unwrap();
    assert_eq!(
        branch_left_after.current_state.as_deref(),
        Some("complete"),
        "Branch left should be complete"
    );
    assert_eq!(
        branch_left_after.attempts, 3,
        "Should have 3 total attempts"
    );

    // All 4 leaves should now be ready
    for leaf_name in ["tree_leaf_d", "tree_leaf_e", "tree_leaf_f", "tree_leaf_g"] {
        let leaf = step_readiness_after
            .iter()
            .find(|s| s.name == leaf_name)
            .unwrap();
        assert!(
            leaf.dependencies_satisfied,
            "{} dependencies satisfied",
            leaf_name
        );
        assert!(leaf.ready_for_execution, "{} ready", leaf_name);
    }

    manager
        .validate_task_execution_context(
            task_uuid, 8, // total_steps
            3, // completed_steps (root, both branches)
            4, // ready_steps (all 4 leaves now ready)
        )
        .await?;

    // **PHASE 4**: Complete remaining steps to verify full workflow recovery
    tracing::info!("📊 PHASE 4: Completing all leaves and convergence");
    manager
        .complete_step(task_uuid, "tree_leaf_d", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_e", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_f", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_g", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(
            task_uuid,
            "tree_final_convergence",
            serde_json::json!({"result": 4294967296u64}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 8, 8, 0)
        .await?;

    tracing::info!("✅ Tree workflow recovered after branch retry and completed successfully");
    tracing::info!("🎯 DELAYED GRATIFICATION: Recovery after retry test complete");

    Ok(())
}

/// Test tree workflow with single leaf failure blocking convergence
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_leaf_failure_blocks_convergence(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Single leaf failure blocks convergence");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "rust_e2e_tree",
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

    // **PHASE 2**: Complete 3 leaves, fail 1 leaf
    tracing::info!("📊 PHASE 2: Completing 3 leaves, failing leaf_e");
    manager
        .complete_step(task_uuid, "tree_leaf_d", serde_json::json!({"result": 256}))
        .await?;
    manager
        .fail_step(task_uuid, "tree_leaf_e", "Leaf processing failure")
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_f", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_g", serde_json::json!({"result": 256}))
        .await?;

    // **VALIDATION**: Verify convergence waits for all 4 leaves
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let leaf_e = step_readiness
        .iter()
        .find(|s| s.name == "tree_leaf_e")
        .unwrap();
    assert_eq!(
        leaf_e.current_state.as_deref(),
        Some("error"),
        "Leaf E should be in error"
    );
    assert!(leaf_e.retry_eligible, "Leaf E should be retry eligible");

    let convergence = step_readiness
        .iter()
        .find(|s| s.name == "tree_final_convergence")
        .unwrap();
    assert_eq!(
        convergence.total_parents, 4,
        "Convergence needs all 4 leaves"
    );
    assert_eq!(
        convergence.completed_parents, 3,
        "Only 3 leaves complete (d, f, g)"
    );
    assert!(
        !convergence.dependencies_satisfied,
        "Convergence dependencies not satisfied - needs all 4 leaves"
    );
    assert!(
        !convergence.ready_for_execution,
        "Convergence not ready - waiting for leaf_e"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 8, // total_steps
            6, // completed_steps (root, 2 branches, 3 leaves)
            0, // ready_steps (leaf_e in error, convergence blocked)
        )
        .await?;

    tracing::info!("✅ Convergence correctly waits for all 4 leaves");
    tracing::info!("🎯 DELAYED GRATIFICATION: Leaf failure blocks convergence test complete");

    Ok(())
}

/// Test tree workflow with multiple leaf failures and recovery
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_multiple_leaf_failures(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Multiple leaf failures with recovery");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "rust_e2e_tree",
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

    // **PHASE 2**: Complete 2 leaves, fail 2 leaves
    tracing::info!("📊 PHASE 2: Completing 2 leaves, failing 2 leaves");
    manager
        .complete_step(task_uuid, "tree_leaf_d", serde_json::json!({"result": 256}))
        .await?;
    manager
        .fail_step(task_uuid, "tree_leaf_e", "Transient leaf error")
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_f", serde_json::json!({"result": 256}))
        .await?;
    manager
        .fail_step(task_uuid, "tree_leaf_g", "Transient leaf error")
        .await?;

    // **VALIDATION 1**: Verify both failures block convergence
    let step_readiness_blocked = manager.get_step_readiness_status(task_uuid).await?;

    for leaf_name in ["tree_leaf_e", "tree_leaf_g"] {
        let leaf = step_readiness_blocked
            .iter()
            .find(|s| s.name == leaf_name)
            .unwrap();
        assert_eq!(
            leaf.current_state.as_deref(),
            Some("error"),
            "{} should be in error",
            leaf_name
        );
        assert!(
            leaf.retry_eligible,
            "{} should be retry eligible",
            leaf_name
        );
    }

    let convergence_blocked = step_readiness_blocked
        .iter()
        .find(|s| s.name == "tree_final_convergence")
        .unwrap();
    assert_eq!(
        convergence_blocked.completed_parents, 2,
        "Only 2 leaves complete (d, f)"
    );
    assert!(
        !convergence_blocked.dependencies_satisfied,
        "Convergence blocked by 2 failed leaves"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 8, // total_steps
            5, // completed_steps (root, 2 branches, 2 leaves)
            0, // ready_steps (2 leaves in error, convergence blocked)
        )
        .await?;

    tracing::info!("✅ SQL functions correctly show multiple failures blocking convergence");

    // **PHASE 3**: Recover both failed leaves
    tracing::info!("📊 PHASE 3: Recovering both failed leaves");
    manager
        .complete_step(task_uuid, "tree_leaf_e", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_g", serde_json::json!({"result": 256}))
        .await?;

    // **VALIDATION 2**: Verify convergence now ready
    let step_readiness_recovered = manager.get_step_readiness_status(task_uuid).await?;

    for leaf_name in ["tree_leaf_d", "tree_leaf_e", "tree_leaf_f", "tree_leaf_g"] {
        let leaf = step_readiness_recovered
            .iter()
            .find(|s| s.name == leaf_name)
            .unwrap();
        assert_eq!(
            leaf.current_state.as_deref(),
            Some("complete"),
            "{} should be complete",
            leaf_name
        );
    }

    let convergence_recovered = step_readiness_recovered
        .iter()
        .find(|s| s.name == "tree_final_convergence")
        .unwrap();
    assert_eq!(
        convergence_recovered.completed_parents, 4,
        "All 4 leaves complete"
    );
    assert!(
        convergence_recovered.dependencies_satisfied,
        "Convergence dependencies satisfied"
    );
    assert!(
        convergence_recovered.ready_for_execution,
        "Convergence ready"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 8, // total_steps
            7, // completed_steps (root, 2 branches, 4 leaves)
            1, // ready_steps (convergence now ready)
        )
        .await?;

    // **PHASE 4**: Complete convergence
    tracing::info!("📊 PHASE 4: Completing tree_final_convergence");
    manager
        .complete_step(
            task_uuid,
            "tree_final_convergence",
            serde_json::json!({"result": 4294967296u64}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 8, 8, 0)
        .await?;

    tracing::info!("✅ Tree workflow completed after multiple leaf recoveries");
    tracing::info!("🎯 DELAYED GRATIFICATION: Multiple leaf failures test complete");

    Ok(())
}

/// Test tree workflow with branch transient failure and eventual convergence
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_branch_transient_failure_eventual_convergence(
    pool: PgPool,
) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Branch transient failure with eventual convergence");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "rust_e2e_tree",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete tree_root
    tracing::info!("📊 PHASE 1: Completing tree_root");
    manager
        .complete_step(task_uuid, "tree_root", serde_json::json!({"result": 4}))
        .await?;

    // **PHASE 2**: Complete branch_left, fail branch_right
    tracing::info!("📊 PHASE 2: Completing branch_left, failing branch_right");
    manager
        .complete_step(
            task_uuid,
            "tree_branch_left",
            serde_json::json!({"result": 16}),
        )
        .await?;
    manager
        .fail_step(
            task_uuid,
            "tree_branch_right",
            "Transient branch network error",
        )
        .await?;

    // **VALIDATION 1**: Verify branch failure and downstream blocking
    let step_readiness_blocked = manager.get_step_readiness_status(task_uuid).await?;

    let branch_right_blocked = step_readiness_blocked
        .iter()
        .find(|s| s.name == "tree_branch_right")
        .unwrap();
    assert_eq!(
        branch_right_blocked.current_state.as_deref(),
        Some("error"),
        "Branch right should be in error"
    );
    assert!(
        branch_right_blocked.retry_eligible,
        "Branch right should be retry eligible"
    );

    // Left branch leaves should be ready
    for leaf_name in ["tree_leaf_d", "tree_leaf_e"] {
        let leaf = step_readiness_blocked
            .iter()
            .find(|s| s.name == leaf_name)
            .unwrap();
        assert!(
            leaf.dependencies_satisfied,
            "{} dependencies satisfied",
            leaf_name
        );
        assert!(leaf.ready_for_execution, "{} ready", leaf_name);
    }

    // Right branch leaves should be blocked
    for leaf_name in ["tree_leaf_f", "tree_leaf_g"] {
        let leaf = step_readiness_blocked
            .iter()
            .find(|s| s.name == leaf_name)
            .unwrap();
        assert!(
            !leaf.dependencies_satisfied,
            "{} dependencies not satisfied",
            leaf_name
        );
        assert!(!leaf.ready_for_execution, "{} not ready", leaf_name);
    }

    manager
        .validate_task_execution_context(
            task_uuid, 8, // total_steps
            2, // completed_steps (root, branch_left)
            2, // ready_steps (leaf_d, leaf_e ready; branch_right failed blocks f, g)
        )
        .await?;

    tracing::info!("✅ SQL functions correctly show branch failure and downstream blocking");

    // **PHASE 3**: Recover branch_right
    tracing::info!("📊 PHASE 3: Recovering tree_branch_right");
    manager
        .complete_step(
            task_uuid,
            "tree_branch_right",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // **VALIDATION 2**: Verify all leaves now ready
    let step_readiness_recovered = manager.get_step_readiness_status(task_uuid).await?;

    for leaf_name in ["tree_leaf_d", "tree_leaf_e", "tree_leaf_f", "tree_leaf_g"] {
        let leaf = step_readiness_recovered
            .iter()
            .find(|s| s.name == leaf_name)
            .unwrap();
        assert!(
            leaf.dependencies_satisfied,
            "{} dependencies satisfied",
            leaf_name
        );
        assert!(leaf.ready_for_execution, "{} ready", leaf_name);
    }

    manager
        .validate_task_execution_context(
            task_uuid, 8, // total_steps
            3, // completed_steps (root, both branches)
            4, // ready_steps (all 4 leaves now ready)
        )
        .await?;

    tracing::info!("✅ SQL functions correctly show recovery and unblocked leaves");

    // **PHASE 4**: Complete all leaves and convergence
    tracing::info!("📊 PHASE 4: Completing all leaves and convergence");
    manager
        .complete_step(task_uuid, "tree_leaf_d", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_e", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_f", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(task_uuid, "tree_leaf_g", serde_json::json!({"result": 256}))
        .await?;
    manager
        .complete_step(
            task_uuid,
            "tree_final_convergence",
            serde_json::json!({"result": 4294967296u64}),
        )
        .await?;

    // **VALIDATION 3**: Verify complete workflow
    manager
        .validate_task_execution_context(task_uuid, 8, 8, 0)
        .await?;

    let final_readiness = manager.get_step_readiness_status(task_uuid).await?;
    for step in &final_readiness {
        assert_eq!(
            step.current_state.as_deref(),
            Some("complete"),
            "Step {} should be complete",
            step.name
        );
    }

    tracing::info!("✅ Tree workflow completed successfully after branch recovery");
    tracing::info!(
        "🎯 DELAYED GRATIFICATION: Branch transient failure with convergence test complete"
    );

    Ok(())
}
