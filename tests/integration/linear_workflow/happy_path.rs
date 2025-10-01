use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test linear workflow initialization and setup
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_linear_workflow_initialization(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 LINEAR WORKFLOW: Initialization and setup");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "linear_workflow",
        serde_json::json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Validate: 4 steps total, 0 completed, 1 ready (linear_step_1)
    manager
        .validate_task_execution_context(
            init_result.task_uuid,
            4, // total_steps
            0, // completed_steps
            1, // ready_steps (only step_1)
        )
        .await?;

    let ready_count = manager.count_ready_steps(init_result.task_uuid).await?;
    assert_eq!(ready_count, 1, "Only linear_step_1 should be ready");

    tracing::info!("🎯 LINEAR WORKFLOW: Initialization test complete");

    Ok(())
}

/// Test linear workflow with strict sequential dependency chain
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_linear_workflow_dependency_chain(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 LINEAR WORKFLOW: Dependency chain validation");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "linear_workflow",
        serde_json::json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get all step readiness information
    let step_readiness = manager
        .get_step_readiness_status(init_result.task_uuid)
        .await?;

    assert_eq!(step_readiness.len(), 4, "Should have 4 steps");

    // Validate dependency structure
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
            "linear_step_1" => {
                assert_eq!(step.total_parents, 0, "Step 1 has no dependencies");
                assert!(step.dependencies_satisfied, "Step 1 dependencies satisfied");
                assert!(step.ready_for_execution, "Step 1 should be ready");
            }
            "linear_step_2" => {
                assert_eq!(step.total_parents, 1, "Step 2 depends on step 1");
                assert!(
                    !step.dependencies_satisfied,
                    "Step 2 dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Step 2 not ready yet");
            }
            "linear_step_3" => {
                assert_eq!(step.total_parents, 1, "Step 3 depends on step 2");
                assert!(
                    !step.dependencies_satisfied,
                    "Step 3 dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Step 3 not ready yet");
            }
            "linear_step_4" => {
                assert_eq!(step.total_parents, 1, "Step 4 depends on step 3");
                assert!(
                    !step.dependencies_satisfied,
                    "Step 4 dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Step 4 not ready yet");
            }
            _ => panic!("Unexpected step: {}", step.name),
        }
    }

    tracing::info!("🎯 LINEAR WORKFLOW: Dependency chain test complete");

    Ok(())
}

/// Test complete linear workflow execution with sequential step transitions
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_linear_workflow_complete_execution(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 LINEAR WORKFLOW: Complete execution with state transitions");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "linear_workflow",
        serde_json::json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete linear_step_1
    tracing::info!("📊 PHASE 1: Completing linear_step_1");
    manager
        .complete_step(
            task_uuid,
            "linear_step_1",
            serde_json::json!({"result": 36}),
        )
        .await?;

    // Validate: step_2 should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "linear_step_2", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "linear_step_3", false, 1, 0)
        .await?;

    tracing::info!("✅ Step 1 complete, step 2 now ready");

    // **PHASE 2**: Complete linear_step_2
    tracing::info!("📊 PHASE 2: Completing linear_step_2");
    manager
        .complete_step(
            task_uuid,
            "linear_step_2",
            serde_json::json!({"result": 1296}),
        )
        .await?;

    // Validate: step_3 should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 2, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "linear_step_3", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "linear_step_4", false, 1, 0)
        .await?;

    tracing::info!("✅ Step 2 complete, step 3 now ready");

    // **PHASE 3**: Complete linear_step_3
    tracing::info!("📊 PHASE 3: Completing linear_step_3");
    manager
        .complete_step(
            task_uuid,
            "linear_step_3",
            serde_json::json!({"result": 1679616}),
        )
        .await?;

    // Validate: step_4 should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 3, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "linear_step_4", true, 1, 1)
        .await?;

    tracing::info!("✅ Step 3 complete, step 4 now ready");

    // **PHASE 4**: Complete linear_step_4
    tracing::info!("📊 PHASE 4: Completing linear_step_4 (final step)");
    manager
        .complete_step(
            task_uuid,
            "linear_step_4",
            serde_json::json!({"result": 2821109907456i64}),
        )
        .await?;

    // Validate: All steps complete
    manager
        .validate_task_execution_context(task_uuid, 4, 4, 0)
        .await?;
    assert_eq!(manager.count_ready_steps(task_uuid).await?, 0);

    tracing::info!("✅ All steps complete - linear workflow execution successful");

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

    tracing::info!("🎯 LINEAR WORKFLOW: Complete execution test successful");

    Ok(())
}
