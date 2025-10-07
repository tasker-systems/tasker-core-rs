use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test complex DAG workflow initialization and setup
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_initialization(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 COMPLEX DAG: Initialization and setup");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "rust_e2e_mixed_dag",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Validate: 7 steps total, 0 completed, 1 ready (dag_init)
    manager
        .validate_task_execution_context(
            init_result.task_uuid,
            7, // total_steps
            0, // completed_steps
            1, // ready_steps (only init)
        )
        .await?;

    let ready_count = manager.count_ready_steps(init_result.task_uuid).await?;
    assert_eq!(ready_count, 1, "Only dag_init should be ready");

    tracing::info!("🎯 COMPLEX DAG: Initialization test complete");

    Ok(())
}

/// Test complex DAG workflow with mixed dependency structure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_mixed_dependency_structure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 COMPLEX DAG: Mixed dependency validation");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "rust_e2e_mixed_dag",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get all step readiness information
    let step_readiness = manager
        .get_step_readiness_status(init_result.task_uuid)
        .await?;

    assert_eq!(step_readiness.len(), 7, "Should have 7 steps");

    // Validate complex DAG structure:
    // Init (dag_init): no parents
    // Processing (process_left, process_right): 1 parent each (init)
    // Validate (dag_validate): 2 parents (both process steps) - convergence
    // Transform (dag_transform): 1 parent (process_left) - linear
    // Analyze (dag_analyze): 1 parent (process_right) - linear
    // Finalize (dag_finalize): 3 parents (validate, transform, analyze) - 3-way convergence

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
            "dag_init" => {
                assert_eq!(step.total_parents, 0, "Init has no dependencies");
                assert!(step.dependencies_satisfied, "Init dependencies satisfied");
                assert!(step.ready_for_execution, "Init should be ready");
            }
            "dag_process_left" | "dag_process_right" => {
                assert_eq!(step.total_parents, 1, "Process steps depend on init");
                assert!(
                    !step.dependencies_satisfied,
                    "Process dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Process steps not ready yet");
            }
            "dag_validate" => {
                assert_eq!(
                    step.total_parents, 2,
                    "Validate depends on both process steps"
                );
                assert!(
                    !step.dependencies_satisfied,
                    "Validate dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Validate not ready yet");
            }
            "dag_transform" => {
                assert_eq!(
                    step.total_parents, 1,
                    "Transform depends on process_left only"
                );
                assert!(
                    !step.dependencies_satisfied,
                    "Transform dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Transform not ready yet");
            }
            "dag_analyze" => {
                assert_eq!(
                    step.total_parents, 1,
                    "Analyze depends on process_right only"
                );
                assert!(
                    !step.dependencies_satisfied,
                    "Analyze dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Analyze not ready yet");
            }
            "dag_finalize" => {
                assert_eq!(
                    step.total_parents, 3,
                    "Finalize depends on validate, transform, analyze"
                );
                assert!(
                    !step.dependencies_satisfied,
                    "Finalize dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Finalize not ready yet");
            }
            _ => panic!("Unexpected step: {}", step.name),
        }
    }

    tracing::info!("🎯 COMPLEX DAG: Mixed dependency test complete");

    Ok(())
}

/// Test complete complex DAG workflow execution with mixed parallelization
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_complete_execution(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 COMPLEX DAG: Complete execution with mixed parallelization");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "rust_e2e_mixed_dag",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete dag_init (2 → 4)
    tracing::info!("📊 PHASE 1: Completing dag_init");
    manager
        .complete_step(task_uuid, "dag_init", serde_json::json!({"result": 4}))
        .await?;

    // Validate: Both process steps should now be ready (parallel execution)
    manager
        .validate_task_execution_context(task_uuid, 7, 1, 2)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "dag_process_left", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "dag_process_right", true, 1, 1)
        .await?;

    tracing::info!("✅ Init complete, both process steps now ready for parallel execution");

    // **PHASE 2**: Complete both process steps (parallel execution: 4 → 16)
    tracing::info!("📊 PHASE 2: Completing dag_process_left and dag_process_right");
    manager
        .complete_step(
            task_uuid,
            "dag_process_left",
            serde_json::json!({"result": 16}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "dag_process_right",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // Validate: All 3 next steps should now be ready (parallel execution)
    // - dag_validate (depends on both process steps)
    // - dag_transform (depends on process_left)
    // - dag_analyze (depends on process_right)
    manager
        .validate_task_execution_context(task_uuid, 7, 3, 3)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "dag_validate", true, 2, 2)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "dag_transform", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "dag_analyze", true, 1, 1)
        .await?;

    tracing::info!(
        "✅ Both process steps complete, validate/transform/analyze ready for parallel execution"
    );

    // **PHASE 3**: Complete validate, transform, analyze (parallel execution)
    tracing::info!("📊 PHASE 3: Completing dag_validate, dag_transform, dag_analyze");

    // dag_validate: (16 × 16)² = 256² = 65536
    manager
        .complete_step(
            task_uuid,
            "dag_validate",
            serde_json::json!({"result": 65536}),
        )
        .await?;

    // dag_transform: 16² = 256
    manager
        .complete_step(
            task_uuid,
            "dag_transform",
            serde_json::json!({"result": 256}),
        )
        .await?;

    // dag_analyze: 16² = 256
    manager
        .complete_step(task_uuid, "dag_analyze", serde_json::json!({"result": 256}))
        .await?;

    // Validate: Final convergence should now be ready
    manager
        .validate_task_execution_context(task_uuid, 7, 6, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "dag_finalize", true, 3, 3)
        .await?;

    tracing::info!("✅ All 3 steps complete, finalize step now ready for 3-way convergence");

    // **PHASE 4**: Complete dag_finalize (3-way convergence)
    // (65536 × 256 × 256) = 4294967296 (simplified to fit u64)
    tracing::info!("📊 PHASE 4: Completing dag_finalize (3-way convergence)");
    manager
        .complete_step(
            task_uuid,
            "dag_finalize",
            serde_json::json!({"result": 4294967296u64}),
        )
        .await?;

    // Validate: All steps complete
    manager
        .validate_task_execution_context(task_uuid, 7, 7, 0)
        .await?;
    assert_eq!(manager.count_ready_steps(task_uuid).await?, 0);

    tracing::info!("✅ All steps complete - complex DAG workflow execution successful");

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

    tracing::info!("🎯 COMPLEX DAG: Complete execution test successful");

    Ok(())
}
