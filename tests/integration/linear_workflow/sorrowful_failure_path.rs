use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test linear workflow with early failure at step 1
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_early_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Early failure blocks entire workflow");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "linear_workflow",
        serde_json::json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail linear_step_1 permanently (exhaust retries)
    tracing::info!("📊 PHASE 1: Failing linear_step_1 permanently");
    manager
        .fail_step(task_uuid, "linear_step_1", "Permanent validation error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_1", "Permanent validation error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_1", "Permanent validation error")
        .await?;

    // Validate: step_1 in terminal error, all downstream steps blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step_1 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_1")
        .unwrap();
    assert_eq!(
        step_1.current_state.as_deref(),
        Some("error"),
        "Step 1 should be in error"
    );
    assert_eq!(step_1.attempts, 3, "Should have 3 attempts");
    assert!(!step_1.retry_eligible, "Should not be retry eligible");

    // All subsequent steps remain pending, never executable
    for step_name in ["linear_step_2", "linear_step_3", "linear_step_4"] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert_eq!(
            step.current_state.as_deref(),
            Some("pending"),
            "{} should remain pending",
            step_name
        );
        assert!(
            !step.dependencies_satisfied,
            "{} dependencies not satisfied",
            step_name
        );
        assert!(
            !step.ready_for_execution,
            "{} never becomes ready",
            step_name
        );
    }

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            0, // completed_steps (none)
            0, // ready_steps (step_1 failed, all blocked)
        )
        .await?;

    tracing::info!("✅ Step 1 early failure permanently blocks entire workflow");
    tracing::info!("🎯 SORROWFUL FAILURE: Early failure test complete");

    Ok(())
}

/// Test linear workflow with mid-workflow permanent failure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_mid_workflow_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Mid-workflow permanent failure");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "linear_workflow",
        serde_json::json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete steps 1 and 2
    tracing::info!("📊 PHASE 1: Completing linear_step_1 and linear_step_2");
    manager
        .complete_step(
            task_uuid,
            "linear_step_1",
            serde_json::json!({"result": 36}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "linear_step_2",
            serde_json::json!({"result": 1296}),
        )
        .await?;

    // **PHASE 2**: Fail linear_step_3 permanently
    tracing::info!("📊 PHASE 2: Failing linear_step_3 permanently");
    manager
        .fail_step(task_uuid, "linear_step_3", "Permanent calculation error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_3", "Permanent calculation error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_3", "Permanent calculation error")
        .await?;

    // Validate: steps 1-2 complete, step 3 in terminal error, step 4 blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step_1 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_1")
        .unwrap();
    assert_eq!(
        step_1.current_state.as_deref(),
        Some("complete"),
        "Step 1 should be complete"
    );

    let step_2 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_2")
        .unwrap();
    assert_eq!(
        step_2.current_state.as_deref(),
        Some("complete"),
        "Step 2 should be complete"
    );

    let step_3 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert_eq!(
        step_3.current_state.as_deref(),
        Some("error"),
        "Step 3 should be in error"
    );
    assert_eq!(step_3.attempts, 3, "Should have 3 attempts");
    assert!(!step_3.retry_eligible, "Should not be retry eligible");

    let step_4 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert_eq!(
        step_4.current_state.as_deref(),
        Some("pending"),
        "Step 4 should remain pending"
    );
    assert!(
        !step_4.dependencies_satisfied,
        "Step 4 dependencies not satisfied"
    );
    assert!(!step_4.ready_for_execution, "Step 4 never becomes ready");

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            2, // completed_steps (steps 1 and 2)
            0, // ready_steps (step_3 failed, step_4 blocked)
        )
        .await?;

    tracing::info!("✅ Step 3 permanent failure blocks step 4");
    tracing::info!("🎯 SORROWFUL FAILURE: Mid-workflow permanent failure test complete");

    Ok(())
}

/// Test linear workflow with late failure at final step
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_late_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Late failure at final step");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "linear_workflow",
        serde_json::json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete steps 1, 2, and 3
    tracing::info!("📊 PHASE 1: Completing linear_step_1, 2, and 3");
    manager
        .complete_step(
            task_uuid,
            "linear_step_1",
            serde_json::json!({"result": 36}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "linear_step_2",
            serde_json::json!({"result": 1296}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "linear_step_3",
            serde_json::json!({"result": 1679616}),
        )
        .await?;

    // **PHASE 2**: Fail linear_step_4 permanently
    tracing::info!("📊 PHASE 2: Failing linear_step_4 permanently");
    manager
        .fail_step(task_uuid, "linear_step_4", "Permanent finalization error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_4", "Permanent finalization error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_4", "Permanent finalization error")
        .await?;

    // Validate: steps 1-3 complete, step 4 in terminal error
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    for (step_name, expected_state) in [
        ("linear_step_1", "complete"),
        ("linear_step_2", "complete"),
        ("linear_step_3", "complete"),
    ] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert_eq!(
            step.current_state.as_deref(),
            Some(expected_state),
            "{} should be {}",
            step_name,
            expected_state
        );
    }

    let step_4 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert_eq!(
        step_4.current_state.as_deref(),
        Some("error"),
        "Step 4 should be in error"
    );
    assert_eq!(step_4.attempts, 3, "Should have 3 attempts");
    assert!(!step_4.retry_eligible, "Should not be retry eligible");

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            3, // completed_steps (steps 1, 2, and 3)
            0, // ready_steps (step_4 failed)
        )
        .await?;

    tracing::info!("✅ Step 4 late failure after steps 1-3 completed");
    tracing::info!("🎯 SORROWFUL FAILURE: Late failure test complete");

    Ok(())
}

/// Test linear workflow with mixed states and SQL accuracy
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_mixed_states_sql_accuracy(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Mixed states SQL accuracy");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

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

    // **PHASE 2**: Fail linear_step_2 (leave in error)
    tracing::info!("📊 PHASE 2: Failing linear_step_2 once");
    manager
        .fail_step(task_uuid, "linear_step_2", "Transient error")
        .await?;

    // **VALIDATION**: Mixed state verification
    // - Step 1: complete
    // - Step 2: error (with retry eligibility)
    // - Step 3: pending (blocked by step 2)
    // - Step 4: pending (blocked by step 2)

    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step_1 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_1")
        .unwrap();
    assert_eq!(
        step_1.current_state.as_deref(),
        Some("complete"),
        "Step 1 should be complete"
    );
    assert_eq!(step_1.completed_parents, 0, "Step 1 has no parents");

    let step_2 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_2")
        .unwrap();
    assert_eq!(
        step_2.current_state.as_deref(),
        Some("error"),
        "Step 2 should be in error"
    );
    assert_eq!(step_2.attempts, 1, "Should have 1 attempt");
    assert!(step_2.retry_eligible, "Should be retry eligible");
    assert_eq!(step_2.total_parents, 1, "Step 2 has 1 parent");
    assert_eq!(step_2.completed_parents, 1, "Step 2's parent is complete");

    let step_3 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert_eq!(
        step_3.current_state.as_deref(),
        Some("pending"),
        "Step 3 should be pending"
    );
    assert_eq!(step_3.total_parents, 1, "Step 3 has 1 parent");
    assert_eq!(
        step_3.completed_parents, 0,
        "Step 3's parent (step_2) not complete"
    );
    assert!(
        !step_3.dependencies_satisfied,
        "Step 3 dependencies not satisfied"
    );
    assert!(!step_3.ready_for_execution, "Step 3 not ready");

    let step_4 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert_eq!(
        step_4.current_state.as_deref(),
        Some("pending"),
        "Step 4 should be pending"
    );
    assert_eq!(step_4.total_parents, 1, "Step 4 has 1 parent");
    assert_eq!(
        step_4.completed_parents, 0,
        "Step 4's parent (step_3) not complete"
    );
    assert!(
        !step_4.dependencies_satisfied,
        "Step 4 dependencies not satisfied"
    );
    assert!(!step_4.ready_for_execution, "Step 4 not ready");

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            1, // completed_steps (only step_1)
            0, // ready_steps (step_2 in error, 3-4 blocked)
        )
        .await?;

    tracing::info!("✅ SQL functions accurately reflect mixed state workflow");
    tracing::info!("🎯 SORROWFUL FAILURE: Mixed states SQL accuracy test complete");

    Ok(())
}
