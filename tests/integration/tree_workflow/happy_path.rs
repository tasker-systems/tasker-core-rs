use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test tree workflow initialization and setup
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_initialization(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 TREE WORKFLOW: Initialization and setup");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "tree_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Validate: 8 steps total, 0 completed, 1 ready (tree_root)
    manager
        .validate_task_execution_context(
            init_result.task_uuid,
            8, // total_steps
            0, // completed_steps
            1, // ready_steps (only root)
        )
        .await?;

    let ready_count = manager.count_ready_steps(init_result.task_uuid).await?;
    assert_eq!(ready_count, 1, "Only tree_root should be ready");

    tracing::info!("🎯 TREE WORKFLOW: Initialization test complete");

    Ok(())
}

/// Test tree workflow with hierarchical dependency structure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_hierarchical_dependencies(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 TREE WORKFLOW: Hierarchical dependency validation");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "hierarchical_tree",
        "tree_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get all step readiness information
    let step_readiness = manager
        .get_step_readiness_status(init_result.task_uuid)
        .await?;

    assert_eq!(step_readiness.len(), 8, "Should have 8 steps");

    // Validate tree structure:
    // Root (tree_root): no parents
    // Branches (branch_left, branch_right): 1 parent (root)
    // Leaves (leaf_d, leaf_e, leaf_f, leaf_g): 1 parent each (respective branch)
    // Convergence (final_convergence): 4 parents (all leaves)

    for step in &step_readiness {
        tracing::info!(
            step = %step.name,
            ready = step.ready_for_execution,
            total_parents = step.total_parents,
            completed_parents = step.completed_parents,
            dependencies_satisfied = step.dependencies_satisfied,
            current_state = ?step.current_state,
            "📊 Step analysis"
        );

        match step.name.as_str() {
            "tree_root" => {
                assert_eq!(step.total_parents, 0, "Root has no dependencies");
                assert!(step.dependencies_satisfied, "Root dependencies satisfied");
                assert!(step.ready_for_execution, "Root should be ready");
            }
            "tree_branch_left" | "tree_branch_right" => {
                assert_eq!(step.total_parents, 1, "Branches depend on root");
                assert!(
                    !step.dependencies_satisfied,
                    "Branch dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Branches not ready yet");
            }
            "tree_leaf_d" | "tree_leaf_e" | "tree_leaf_f" | "tree_leaf_g" => {
                assert_eq!(step.total_parents, 1, "Leaves depend on 1 branch");
                assert!(
                    !step.dependencies_satisfied,
                    "Leaf dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Leaves not ready yet");
            }
            "tree_final_convergence" => {
                assert_eq!(step.total_parents, 4, "Convergence depends on all 4 leaves");
                assert!(
                    !step.dependencies_satisfied,
                    "Convergence dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Convergence not ready yet");
            }
            _ => panic!("Unexpected step: {}", step.name),
        }
    }

    tracing::info!("🎯 TREE WORKFLOW: Hierarchical dependency test complete");

    Ok(())
}

/// Test complete tree workflow execution with hierarchical parallelization
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_tree_workflow_complete_execution(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 TREE WORKFLOW: Complete execution with parallelization");

    let manager = LifecycleTestManager::new(pool).await?;

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

    // Validate: Both branches should now be ready (parallel execution)
    manager
        .validate_task_execution_context(task_uuid, 8, 1, 2)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "tree_branch_left", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "tree_branch_right", true, 1, 1)
        .await?;

    tracing::info!("✅ Root complete, both branches now ready for parallel execution");

    // **PHASE 2**: Complete both branches (parallel execution)
    tracing::info!("📊 PHASE 2: Completing tree_branch_left and tree_branch_right");
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

    // Validate: All 4 leaves should now be ready (parallel execution)
    manager
        .validate_task_execution_context(task_uuid, 8, 3, 4)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "tree_leaf_d", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "tree_leaf_e", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "tree_leaf_f", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "tree_leaf_g", true, 1, 1)
        .await?;

    tracing::info!("✅ Both branches complete, all 4 leaves now ready for parallel execution");

    // **PHASE 3**: Complete all 4 leaves (parallel execution)
    tracing::info!("📊 PHASE 3: Completing all 4 leaves");
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

    // Validate: Final convergence should now be ready
    manager
        .validate_task_execution_context(task_uuid, 8, 7, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "tree_final_convergence", true, 4, 4)
        .await?;

    tracing::info!("✅ All 4 leaves complete, convergence step now ready");

    // **PHASE 4**: Complete tree_final_convergence
    tracing::info!("📊 PHASE 4: Completing tree_final_convergence");
    manager
        .complete_step(
            task_uuid,
            "tree_final_convergence",
            serde_json::json!({"result": 4294967296u64}),
        )
        .await?;

    // Validate: All steps complete
    manager
        .validate_task_execution_context(task_uuid, 8, 8, 0)
        .await?;
    assert_eq!(manager.count_ready_steps(task_uuid).await?, 0);

    tracing::info!("✅ All steps complete - tree workflow execution successful");

    // **PHASE 5**: Final state validation
    tracing::info!("📊 PHASE 5: Final state validation");
    let final_readiness = manager.get_step_readiness_status(task_uuid).await?;

    for step in &final_readiness {
        assert_eq!(
            step.current_state.as_deref(),
            Some("complete"),
            "Step {} should be in complete state",
            step.name
        );
    }

    tracing::info!("🎯 TREE WORKFLOW: Complete execution test successful");

    Ok(())
}
