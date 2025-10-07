use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test 1: max_attempts=0 with attempts=0 should allow first execution
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_max_attempts_zero_allows_first_execution(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST 1: max_attempts=0 allows first execution (attempts=0)");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "boundary_max_attempts_zero",
        "test_sql",
        serde_json::json!({"test_scenario": "zero_first"}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Get step readiness immediately after initialization
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .expect("boundary_test_step should exist");

    // Validate: attempts=0, max_attempts=0, should be retry_eligible=true
    assert_eq!(step.attempts, 0, "Should have 0 attempts");
    assert_eq!(step.max_attempts, 0, "Max attempts should be 0");
    assert!(
        step.retry_eligible,
        "CRITICAL: First execution (attempts=0) must be eligible regardless of max_attempts"
    );
    assert!(
        step.ready_for_execution,
        "Step should be ready for first execution"
    );

    tracing::info!("✅ TEST 1 PASSED: max_attempts=0 allows first execution");
    Ok(())
}

/// Test 2: max_attempts=0 with attempts=1 should block further execution
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_max_attempts_zero_blocks_after_first(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST 2: max_attempts=0 blocks after first attempt (attempts=1)");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "boundary_max_attempts_zero",
        "test_sql",
        serde_json::json!({"test_scenario": "zero_exhausted"}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Fail the step once to increment attempts
    manager
        .fail_step(task_uuid, "boundary_test_step", "First failure")
        .await?;

    // Get step readiness after first failure
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .expect("boundary_test_step should exist");

    // Validate: attempts=1, max_attempts=0, should NOT be retry_eligible
    assert_eq!(step.attempts, 1, "Should have 1 attempt");
    assert_eq!(step.max_attempts, 0, "Max attempts should be 0");
    assert!(
        !step.retry_eligible,
        "Should NOT be retry eligible: attempts (1) >= max_attempts (0)"
    );

    tracing::info!("✅ TEST 2 PASSED: max_attempts=0 blocks after first attempt");
    Ok(())
}

/// Test 3: max_attempts=1 means exactly one total attempt
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_max_attempts_one_semantics(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST 3: max_attempts=1 allows exactly one attempt");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "boundary_max_attempts_one",
        "test_sql",
        serde_json::json!({"test_scenario": "one_attempt"}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Check initial state: attempts=0, should be eligible
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .unwrap();

    assert_eq!(step.attempts, 0, "Should have 0 attempts");
    assert_eq!(step.max_attempts, 1, "Max attempts should be 1");
    assert!(
        step.retry_eligible,
        "First attempt should be eligible with max_attempts=1"
    );

    // Fail the step to increment attempts
    manager
        .fail_step(task_uuid, "boundary_test_step", "First failure")
        .await?;

    // Check after first failure: attempts=1, should NOT be eligible for retry
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .unwrap();

    assert_eq!(step.attempts, 1, "Should have 1 attempt");
    assert_eq!(step.max_attempts, 1, "Max attempts should be 1");
    assert!(
        !step.retry_eligible,
        "Should NOT be retry eligible: attempts (1) >= max_attempts (1)"
    );

    tracing::info!("✅ TEST 3 PASSED: max_attempts=1 semantics correct");
    Ok(())
}

/// Test 4: max_attempts=3 allows attempts 0, 1, 2 then blocks at 3
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_max_attempts_three_progression(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST 4: max_attempts=3 progression (0→1→2→blocked)");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "boundary_max_attempts_three",
        "test_sql",
        serde_json::json!({"test_scenario": "three_attempts"}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Check attempts=0: should be eligible
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .unwrap();
    assert_eq!(step.attempts, 0);
    assert!(step.retry_eligible, "Attempt 0 should be eligible");

    // Fail to get attempts=1: should be eligible
    manager
        .fail_step(task_uuid, "boundary_test_step", "Failure 1")
        .await?;
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .unwrap();
    assert_eq!(step.attempts, 1);
    assert!(step.retry_eligible, "Attempt 1 should be eligible (1 < 3)");

    // Fail to get attempts=2: should be eligible
    manager
        .fail_step(task_uuid, "boundary_test_step", "Failure 2")
        .await?;
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .unwrap();
    assert_eq!(step.attempts, 2);
    assert!(step.retry_eligible, "Attempt 2 should be eligible (2 < 3)");

    // Fail to get attempts=3: should NOT be eligible
    manager
        .fail_step(task_uuid, "boundary_test_step", "Failure 3")
        .await?;
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .unwrap();
    assert_eq!(step.attempts, 3);
    assert!(
        !step.retry_eligible,
        "Attempt 3 should NOT be eligible (3 >= 3)"
    );

    tracing::info!("✅ TEST 4 PASSED: max_attempts=3 progression correct");
    Ok(())
}

/// Test 5: First execution (attempts=0) should ignore retryable flag
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_first_attempt_ignores_retryable_flag(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST 5: First execution ignores retryable=false");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "boundary_non_retryable_first",
        "test_sql",
        serde_json::json!({"test_scenario": "non_retryable_first"}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Get step readiness immediately after initialization
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .expect("boundary_test_step should exist");

    // Validate: attempts=0, retryable=false, should STILL be retry_eligible=true
    assert_eq!(step.attempts, 0, "Should have 0 attempts");
    assert!(
        step.retry_eligible,
        "CRITICAL: First execution (attempts=0) must be eligible even with retryable=false"
    );
    assert!(
        step.ready_for_execution,
        "Step should be ready for first execution despite retryable=false"
    );

    tracing::info!("✅ TEST 5 PASSED: First execution ignores retryable flag");
    Ok(())
}

/// Test 6: Retry attempts (attempts > 0) require retryable=true
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_retries_require_retryable_true(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST 6: Retries require retryable=true");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "boundary_non_retryable_retries",
        "test_sql",
        serde_json::json!({"test_scenario": "non_retryable_retries"}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // First execution should succeed (attempts=0 ignores retryable)
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .unwrap();
    assert_eq!(step.attempts, 0);
    assert!(step.retry_eligible, "First execution should be eligible");

    // Fail the step to increment attempts
    manager
        .fail_step(task_uuid, "boundary_test_step", "First failure")
        .await?;

    // After first failure: attempts=1, retryable=false, should NOT be eligible
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let step = step_readiness
        .iter()
        .find(|s| s.name == "boundary_test_step")
        .unwrap();

    assert_eq!(step.attempts, 1, "Should have 1 attempt");
    assert_eq!(step.max_attempts, 3, "Max attempts should be 3");
    assert!(
        !step.retry_eligible,
        "CRITICAL: Retry attempts (attempts > 0) with retryable=false should NOT be eligible"
    );

    tracing::info!("✅ TEST 6 PASSED: Retries correctly require retryable=true");
    Ok(())
}
