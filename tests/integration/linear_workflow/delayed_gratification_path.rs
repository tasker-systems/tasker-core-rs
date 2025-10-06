use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test linear workflow with retry limit exhaustion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_max_attempts_exhaustion(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Retry limit exhaustion blocks workflow");

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

    // **PHASE 2**: Fail linear_step_2 three times (exhaust retry limit)
    tracing::info!("📊 PHASE 2: Failing linear_step_2 three times");
    manager
        .fail_step(task_uuid, "linear_step_2", "Attempt 1: Calculation error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_2", "Attempt 2: Calculation error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_2", "Attempt 3: Calculation error")
        .await?;

    // Validate: step_2 in error with 3 attempts, not retry eligible
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step_2 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_2")
        .unwrap();
    assert_eq!(step_2.current_state.as_deref(), Some("error"));
    assert_eq!(step_2.attempts, 3, "Should have 3 attempts");
    assert_eq!(step_2.max_attempts, 3, "Retry limit should be 3");
    assert!(
        !step_2.retry_eligible,
        "Should NOT be retry eligible after 3 attempts"
    );

    // Validate: steps 3 and 4 blocked by step_2 failure
    let step_3 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert!(
        !step_3.dependencies_satisfied,
        "Step 3 dependencies not satisfied"
    );
    assert!(
        !step_3.ready_for_execution,
        "Step 3 not ready for execution"
    );

    let step_4 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert!(
        !step_4.dependencies_satisfied,
        "Step 4 dependencies not satisfied"
    );
    assert!(
        !step_4.ready_for_execution,
        "Step 4 not ready for execution"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            1, // completed_steps (only step_1)
            0, // ready_steps (step_2 in error, exhausted retries)
        )
        .await?;

    tracing::info!("✅ Step 2 retry limit exhausted, workflow blocked");
    tracing::info!("🎯 DELAYED GRATIFICATION: Retry limit exhaustion test complete");

    Ok(())
}

/// Test linear workflow recovery after retry
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_recovery_after_retry(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Recovery after retry");

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

    // **PHASE 2**: Fail linear_step_2 twice
    tracing::info!("📊 PHASE 2: Failing linear_step_2 twice");
    manager
        .fail_step(task_uuid, "linear_step_2", "Attempt 1: Transient error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_2", "Attempt 2: Transient error")
        .await?;

    // Validate: step_2 in error with 2 attempts, still retry eligible
    let step_readiness_mid = manager.get_step_readiness_status(task_uuid).await?;
    let step_2_mid = step_readiness_mid
        .iter()
        .find(|s| s.name == "linear_step_2")
        .unwrap();
    assert_eq!(step_2_mid.attempts, 2, "Should have 2 attempts");
    assert!(step_2_mid.retry_eligible, "Should still be retry eligible");

    // **PHASE 3**: Succeed on third attempt
    tracing::info!("📊 PHASE 3: Succeeding on third attempt");
    manager
        .complete_step(
            task_uuid,
            "linear_step_2",
            serde_json::json!({"result": 1296}),
        )
        .await?;

    // Validate: step_2 complete with 3 attempts, step_3 now ready
    let step_readiness_after = manager.get_step_readiness_status(task_uuid).await?;

    let step_2_after = step_readiness_after
        .iter()
        .find(|s| s.name == "linear_step_2")
        .unwrap();
    assert_eq!(
        step_2_after.current_state.as_deref(),
        Some("complete"),
        "Step 2 should be complete"
    );
    assert_eq!(step_2_after.attempts, 3, "Should have 3 total attempts");

    let step_3_after = step_readiness_after
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert!(
        step_3_after.dependencies_satisfied,
        "Step 3 dependencies satisfied after step 2 recovery"
    );
    assert!(
        step_3_after.ready_for_execution,
        "Step 3 ready for execution after step 2 recovery"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            2, // completed_steps (steps 1 and 2)
            1, // ready_steps (step_3 now ready)
        )
        .await?;

    // **PHASE 4**: Complete remaining steps to verify full workflow recovery
    tracing::info!("📊 PHASE 4: Completing remaining steps");
    manager
        .complete_step(
            task_uuid,
            "linear_step_3",
            serde_json::json!({"result": 1679616}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "linear_step_4",
            serde_json::json!({"result": 2821109907456i64}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 4, 4, 0)
        .await?;

    tracing::info!("✅ Workflow recovered after step 2 retry and completed successfully");
    tracing::info!("🎯 DELAYED GRATIFICATION: Recovery after retry test complete");

    Ok(())
}

/// Test linear workflow with retryable error and backoff calculation
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_retryable_error_with_backoff(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Retryable error with backoff");

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

    // **PHASE 2**: Fail linear_step_3 with retryable error
    tracing::info!("📊 PHASE 2: Failing linear_step_3 with retryable error");
    manager
        .fail_step(task_uuid, "linear_step_3", "Network timeout - retryable")
        .await?;

    // Validate: step_3 in error, retry eligible, backoff calculated
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step_3 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert_eq!(step_3.current_state.as_deref(), Some("error"));
    assert_eq!(step_3.attempts, 1, "Should have 1 attempt");
    assert!(step_3.retry_eligible, "Should be retry eligible");
    assert!(
        step_3.next_retry_at.is_some(),
        "Should have next_retry_at calculated"
    );

    // Validate: step_4 blocked by step_3 failure
    let step_4 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert!(
        !step_4.dependencies_satisfied,
        "Step 4 dependencies not satisfied"
    );
    assert!(!step_4.ready_for_execution, "Step 4 not ready");

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            2, // completed_steps (steps 1 and 2)
            0, // ready_steps (step_3 in error with backoff, step_4 blocked)
        )
        .await?;

    tracing::info!("✅ Step 3 failed with backoff calculated, step 4 blocked");

    // **PHASE 3**: Recover step_3 (simulate retry after backoff)
    tracing::info!("📊 PHASE 3: Recovering linear_step_3 after backoff");
    manager
        .complete_step(
            task_uuid,
            "linear_step_3",
            serde_json::json!({"result": 1679616}),
        )
        .await?;

    // Validate: step_3 complete, step_4 now ready
    let step_readiness_after = manager.get_step_readiness_status(task_uuid).await?;

    let step_3_after = step_readiness_after
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert_eq!(
        step_3_after.current_state.as_deref(),
        Some("complete"),
        "Step 3 should be complete"
    );
    assert_eq!(step_3_after.attempts, 2, "Should have 2 total attempts");

    let step_4_after = step_readiness_after
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert!(
        step_4_after.dependencies_satisfied,
        "Step 4 dependencies satisfied after step 3 recovery"
    );
    assert!(
        step_4_after.ready_for_execution,
        "Step 4 ready after step 3 recovery"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            3, // completed_steps (steps 1, 2, and 3)
            1, // ready_steps (step_4 now ready)
        )
        .await?;

    tracing::info!("✅ Step 3 recovered, step 4 unblocked");
    tracing::info!("🎯 DELAYED GRATIFICATION: Retryable error with backoff test complete");

    Ok(())
}

/// Test linear workflow where mid-chain failure permanently blocks downstream steps
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_mid_chain_failure_blocks_downstream(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Mid-chain failure blocks downstream");

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

    // **PHASE 2**: Fail linear_step_2 permanently (exhaust retries)
    tracing::info!("📊 PHASE 2: Failing linear_step_2 permanently");
    manager
        .fail_step(task_uuid, "linear_step_2", "Permanent calculation error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_2", "Permanent calculation error")
        .await?;
    manager
        .fail_step(task_uuid, "linear_step_2", "Permanent calculation error")
        .await?;

    // Validate: step_2 in terminal error state
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step_2 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_2")
        .unwrap();
    assert_eq!(
        step_2.current_state.as_deref(),
        Some("error"),
        "Step 2 should be in error"
    );
    assert_eq!(step_2.attempts, 3, "Should have 3 attempts");
    assert!(!step_2.retry_eligible, "Should not be retry eligible");

    // Validate: steps 3 and 4 permanently blocked
    let step_3 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert_eq!(
        step_3.current_state.as_deref(),
        Some("pending"),
        "Step 3 still pending (never executed)"
    );
    assert!(
        !step_3.dependencies_satisfied,
        "Step 3 dependencies not satisfied"
    );
    assert!(!step_3.ready_for_execution, "Step 3 never becomes ready");

    let step_4 = step_readiness
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert_eq!(
        step_4.current_state.as_deref(),
        Some("pending"),
        "Step 4 still pending (never executed)"
    );
    assert!(
        !step_4.dependencies_satisfied,
        "Step 4 dependencies not satisfied"
    );
    assert!(!step_4.ready_for_execution, "Step 4 never becomes ready");

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            1, // completed_steps (only step_1)
            0, // ready_steps (step_2 permanently failed, 3-4 blocked)
        )
        .await?;

    tracing::info!("✅ Step 2 permanent failure blocks steps 3 and 4");
    tracing::info!("🎯 DELAYED GRATIFICATION: Mid-chain failure blocks downstream test complete");

    Ok(())
}

/// Test linear workflow with transient failure followed by eventual completion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_linear_workflow_transient_failure_eventual_completion(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Transient failure with eventual completion");

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

    // **PHASE 2**: Fail linear_step_3 transiently
    tracing::info!("📊 PHASE 2: Failing linear_step_3 with transient error");
    manager
        .fail_step(task_uuid, "linear_step_3", "Transient network error")
        .await?;

    // **VALIDATION 1**: Verify step failure and workflow blocked
    let step_readiness_blocked = manager.get_step_readiness_status(task_uuid).await?;

    let step_3_blocked = step_readiness_blocked
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert_eq!(
        step_3_blocked.current_state.as_deref(),
        Some("error"),
        "Step 3 should be in error"
    );
    assert!(
        step_3_blocked.retry_eligible,
        "Step 3 should be retry eligible"
    );

    let step_4_blocked = step_readiness_blocked
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert!(
        !step_4_blocked.dependencies_satisfied,
        "Step 4 dependencies not satisfied"
    );
    assert!(!step_4_blocked.ready_for_execution, "Step 4 not ready");

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            2, // completed_steps (steps 1 and 2)
            0, // ready_steps (step_3 in error, step_4 blocked)
        )
        .await?;

    tracing::info!("✅ SQL functions correctly show step failure and blocked execution");

    // **PHASE 3**: Recover linear_step_3
    tracing::info!("📊 PHASE 3: Recovering linear_step_3");
    manager
        .complete_step(
            task_uuid,
            "linear_step_3",
            serde_json::json!({"result": 1679616}),
        )
        .await?;

    // **VALIDATION 2**: Verify step recovery and workflow unblocked
    let step_readiness_recovered = manager.get_step_readiness_status(task_uuid).await?;

    let step_3_recovered = step_readiness_recovered
        .iter()
        .find(|s| s.name == "linear_step_3")
        .unwrap();
    assert_eq!(
        step_3_recovered.current_state.as_deref(),
        Some("complete"),
        "Step 3 should be complete"
    );

    let step_4_recovered = step_readiness_recovered
        .iter()
        .find(|s| s.name == "linear_step_4")
        .unwrap();
    assert!(
        step_4_recovered.dependencies_satisfied,
        "Step 4 dependencies satisfied"
    );
    assert!(step_4_recovered.ready_for_execution, "Step 4 ready");

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            3, // completed_steps (steps 1, 2, and 3)
            1, // ready_steps (step_4 now ready)
        )
        .await?;

    tracing::info!("✅ SQL functions correctly show recovery and unblocked execution");

    // **PHASE 4**: Complete linear_step_4
    tracing::info!("📊 PHASE 4: Completing linear_step_4");
    manager
        .complete_step(
            task_uuid,
            "linear_step_4",
            serde_json::json!({"result": 2821109907456i64}),
        )
        .await?;

    // **VALIDATION 3**: Verify workflow completion
    manager
        .validate_task_execution_context(task_uuid, 4, 4, 0)
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

    tracing::info!("✅ Workflow completed successfully after transient failure recovery");
    tracing::info!(
        "🎯 DELAYED GRATIFICATION: Transient failure with eventual completion test complete"
    );

    Ok(())
}
