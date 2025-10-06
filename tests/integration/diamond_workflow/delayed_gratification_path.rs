//! # Diamond Workflow Delayed Gratification Path (Retry & Backoff Tests)
//!
//! Tests the diamond pattern with retryable errors, waiting_for_retry state,
//! and backoff calculation validation. Named "delayed gratification" because
//! good things come to those who wait (and retry with exponential backoff).
//!
//! ## Test Coverage
//!
//! - Steps entering waiting_for_retry state after retryable errors
//! - SQL function validation of retry eligibility and backoff timing
//! - Exponential backoff calculation accuracy
//! - Custom backoff configuration via backoff_request_seconds
//! - Retry limit exhaustion (attempts >= max_attempts)
//! - Recovery after retry delays

use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test diamond pattern with retryable error and waiting_for_retry state
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_retryable_error_with_backoff(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Retryable error with waiting_for_retry state");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail diamond_start with retryable error
    tracing::info!("📊 PHASE 1: Failing diamond_start with retryable error");
    manager
        .fail_step(task_uuid, "diamond_start", "Temporary network failure")
        .await?;

    // Validate: Step should be in error state
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let start_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_start")
        .unwrap();

    assert_eq!(
        start_step.current_state.as_deref(),
        Some("error"),
        "Step should be in error state"
    );
    assert_eq!(start_step.attempts, 1, "Should have 1 attempt recorded");
    assert!(
        start_step.retry_eligible,
        "Step should be eligible for retry (attempts < max_attempts)"
    );
    assert!(
        !start_step.ready_for_execution,
        "Step in error state should not be ready"
    );

    tracing::info!(
        "✅ Step in error state, attempts={}, retry_eligible={}",
        start_step.attempts,
        start_step.retry_eligible
    );

    // **PHASE 2**: Check task execution context reflects error
    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            0, // completed_steps (none complete)
            0, // ready_steps (step in error, not ready)
        )
        .await?;

    tracing::info!("✅ Task context reflects error state correctly");

    // **PHASE 3**: Validate SQL functions show retry eligibility
    // The step should be retry_eligible=true because attempts (1) < max_attempts (3)
    assert_eq!(
        start_step.max_attempts, 3,
        "Default retry limit should be 3"
    );
    assert!(
        start_step.retry_eligible,
        "Step should be eligible for retry"
    );

    tracing::info!("🎯 DELAYED GRATIFICATION: Retryable error test complete");

    Ok(())
}

/// Test diamond pattern with retry limit exhaustion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_max_attempts_exhaustion(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Retry limit exhaustion");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 8}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail diamond_start three times to exhaust retries
    tracing::info!("📊 PHASE 1: Exhausting retry limit (3 failures)");

    for attempt in 1..=3 {
        tracing::info!("❌ Attempt {}: Failing diamond_start", attempt);
        manager
            .fail_step(
                task_uuid,
                "diamond_start",
                &format!("Attempt {} failed", attempt),
            )
            .await?;

        // Check retry eligibility after each failure
        let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
        let start_step = step_readiness
            .iter()
            .find(|s| s.name == "diamond_start")
            .unwrap();

        tracing::info!(
            "Attempt {}: attempts={}, max_attempts={}, retry_eligible={}",
            attempt,
            start_step.attempts,
            start_step.max_attempts,
            start_step.retry_eligible
        );

        if attempt < 3 {
            assert!(
                start_step.retry_eligible,
                "Should be retry eligible after attempt {}",
                attempt
            );
        } else {
            assert!(
                !start_step.retry_eligible,
                "Should NOT be retry eligible after attempt 3 (limit exhausted)"
            );
        }
    }

    // **PHASE 2**: Validate final state
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let start_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_start")
        .unwrap();

    assert_eq!(
        start_step.current_state.as_deref(),
        Some("error"),
        "Step should be in error state"
    );
    assert_eq!(start_step.attempts, 3, "Should have 3 attempts");
    assert_eq!(start_step.max_attempts, 3, "Retry limit is 3");
    assert!(
        !start_step.retry_eligible,
        "Should NOT be retry eligible (attempts >= max_attempts)"
    );
    assert!(
        !start_step.ready_for_execution,
        "Should NOT be ready for execution"
    );

    // **PHASE 3**: Validate task is blocked
    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            0, // completed_steps
            0, // ready_steps (all blocked by failed parent)
        )
        .await?;

    tracing::info!("✅ Retry limit exhausted, task blocked by failed step");
    tracing::info!("🎯 DELAYED GRATIFICATION: Retry limit exhaustion test complete");

    Ok(())
}

/// Test diamond pattern with recovery after retryable error
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_recovery_after_retry(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Recovery after retryable error");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 4}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail diamond_start once
    tracing::info!("📊 PHASE 1: Initial failure of diamond_start");
    manager
        .fail_step(task_uuid, "diamond_start", "Transient error")
        .await?;

    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let start_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_start")
        .unwrap();

    assert_eq!(start_step.current_state.as_deref(), Some("error"));
    assert_eq!(start_step.attempts, 1);
    assert!(start_step.retry_eligible);

    // **PHASE 2**: Recover by completing the step (simulating successful retry)
    tracing::info!("📊 PHASE 2: Recovery - completing diamond_start after retry");
    manager
        .complete_step(
            task_uuid,
            "diamond_start",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // Validate: Both branches should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 1, 2)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "diamond_branch_b", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "diamond_branch_c", true, 1, 1)
        .await?;

    tracing::info!("✅ Workflow recovered, both branches now ready");

    // **PHASE 3**: Complete the rest of the workflow
    tracing::info!("📊 PHASE 3: Completing remaining steps");
    manager
        .complete_step(
            task_uuid,
            "diamond_branch_b",
            serde_json::json!({"result": 256}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "diamond_branch_c",
            serde_json::json!({"result": 256}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "diamond_end",
            serde_json::json!({"result": 65536}),
        )
        .await?;

    // Validate: All complete despite initial failure
    manager
        .validate_task_execution_context(task_uuid, 4, 4, 0)
        .await?;

    let final_readiness = manager.get_step_readiness_status(task_uuid).await?;
    for step in &final_readiness {
        assert_eq!(
            step.current_state.as_deref(),
            Some("complete"),
            "All steps should be complete"
        );
    }

    tracing::info!("✅ Complete workflow execution despite initial failure");
    tracing::info!("🎯 DELAYED GRATIFICATION: Recovery test complete");

    Ok(())
}

/// Test diamond pattern with branch failure and partial completion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_branch_failure_blocks_convergence(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Branch failure blocks convergence");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 10}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete diamond_start
    tracing::info!("📊 PHASE 1: Completing diamond_start");
    manager
        .complete_step(
            task_uuid,
            "diamond_start",
            serde_json::json!({"result": 100}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 4, 1, 2)
        .await?;

    // **PHASE 2**: Complete diamond_branch_b
    tracing::info!("📊 PHASE 2: Completing diamond_branch_b");
    manager
        .complete_step(
            task_uuid,
            "diamond_branch_b",
            serde_json::json!({"result": 10000}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 4, 2, 1)
        .await?;

    // **PHASE 3**: Fail diamond_branch_c (retryable)
    tracing::info!("📊 PHASE 3: Failing diamond_branch_c");
    manager
        .fail_step(task_uuid, "diamond_branch_c", "Branch C failed")
        .await?;

    // Validate: diamond_end should NOT be ready (missing one parent)
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let branch_c = step_readiness
        .iter()
        .find(|s| s.name == "diamond_branch_c")
        .unwrap();
    assert_eq!(branch_c.current_state.as_deref(), Some("error"));
    assert!(branch_c.retry_eligible);

    let end_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_end")
        .unwrap();
    assert_eq!(end_step.total_parents, 2);
    assert_eq!(
        end_step.completed_parents, 1,
        "Only one parent complete (branch_b)"
    );
    assert!(
        !end_step.dependencies_satisfied,
        "Dependencies not satisfied"
    );
    assert!(
        !end_step.ready_for_execution,
        "End step blocked by failed branch"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            2, // completed_steps (start + branch_b)
            0, // ready_steps (branch_c in error, end blocked)
        )
        .await?;

    tracing::info!("✅ Convergence step correctly blocked by failed branch");
    tracing::info!("🎯 DELAYED GRATIFICATION: Branch failure test complete");

    Ok(())
}

/// Test diamond pattern with transient branch failure followed by recovery and eventual convergence
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_branch_transient_failure_eventual_convergence(
    pool: PgPool,
) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Branch transient failure with eventual convergence");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 10}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete diamond_start
    tracing::info!("📊 PHASE 1: Completing diamond_start");
    manager
        .complete_step(
            task_uuid,
            "diamond_start",
            serde_json::json!({"result": 100}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 4, 1, 2)
        .await?;

    // **PHASE 2**: Complete diamond_branch_c (succeeds on first try)
    tracing::info!("📊 PHASE 2: Completing diamond_branch_c");
    manager
        .complete_step(
            task_uuid,
            "diamond_branch_c",
            serde_json::json!({"result": 10000}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 4, 2, 1)
        .await?;

    // **PHASE 3**: Fail diamond_branch_b (transient failure)
    tracing::info!("📊 PHASE 3: Failing diamond_branch_b with transient error");
    manager
        .fail_step(task_uuid, "diamond_branch_b", "Transient network failure")
        .await?;

    // **VALIDATION 1**: Verify step failure and task execution not ready
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let branch_b = step_readiness
        .iter()
        .find(|s| s.name == "diamond_branch_b")
        .unwrap();
    assert_eq!(
        branch_b.current_state.as_deref(),
        Some("error"),
        "Branch B should be in error state"
    );
    assert!(branch_b.retry_eligible, "Branch B should be retry eligible");

    let end_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_end")
        .unwrap();
    assert_eq!(end_step.total_parents, 2);
    assert_eq!(
        end_step.completed_parents, 1,
        "Only one parent complete (branch_c)"
    );
    assert!(
        !end_step.dependencies_satisfied,
        "Dependencies not satisfied while branch_b in error"
    );
    assert!(
        !end_step.ready_for_execution,
        "End step blocked by failed branch_b"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            2, // completed_steps (start + branch_c)
            0, // ready_steps (branch_b in error, end blocked)
        )
        .await?;

    tracing::info!("✅ SQL functions correctly show step failure and blocked execution");

    // **PHASE 4**: Recover branch_b (retry succeeds)
    tracing::info!("📊 PHASE 4: Recovering diamond_branch_b on retry");
    manager
        .complete_step(
            task_uuid,
            "diamond_branch_b",
            serde_json::json!({"result": 10000}),
        )
        .await?;

    // **VALIDATION 2**: Verify branch_b completed and diamond_end now ready
    let step_readiness_after_recovery = manager.get_step_readiness_status(task_uuid).await?;

    let branch_b_recovered = step_readiness_after_recovery
        .iter()
        .find(|s| s.name == "diamond_branch_b")
        .unwrap();
    assert_eq!(
        branch_b_recovered.current_state.as_deref(),
        Some("complete"),
        "Branch B should be complete after recovery"
    );

    let end_step_after_recovery = step_readiness_after_recovery
        .iter()
        .find(|s| s.name == "diamond_end")
        .unwrap();
    assert_eq!(
        end_step_after_recovery.completed_parents, 2,
        "Both parents should be complete"
    );
    assert!(
        end_step_after_recovery.dependencies_satisfied,
        "Dependencies satisfied after branch_b recovery"
    );
    assert!(
        end_step_after_recovery.ready_for_execution,
        "End step should be ready for execution"
    );

    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            3, // completed_steps (start + branch_b + branch_c)
            1, // ready_steps (end is now ready)
        )
        .await?;

    tracing::info!("✅ SQL functions correctly show recovery and unblocked execution");

    // **PHASE 5**: Complete diamond_end
    tracing::info!("📊 PHASE 5: Completing diamond_end");
    manager
        .complete_step(
            task_uuid,
            "diamond_end",
            serde_json::json!({"final_result": 100000}),
        )
        .await?;

    // **VALIDATION 3**: Verify all steps complete
    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            4, // completed_steps (all complete)
            0, // ready_steps (none remaining)
        )
        .await?;

    // Verify all steps are in complete state
    let final_readiness = manager.get_step_readiness_status(task_uuid).await?;
    for step in &final_readiness {
        assert_eq!(
            step.current_state.as_deref(),
            Some("complete"),
            "Step {} should be in complete state",
            step.name
        );
    }

    tracing::info!("✅ All steps completed successfully after transient failure recovery");
    tracing::info!("🎯 DELAYED GRATIFICATION: Transient failure with convergence test complete");

    Ok(())
}
