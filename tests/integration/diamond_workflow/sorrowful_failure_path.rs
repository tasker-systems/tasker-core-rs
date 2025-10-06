//! # Diamond Workflow Sorrowful Failure Path (Permanent Error Tests)
//!
//! Tests the diamond pattern with permanent (non-retryable) errors at various
//! stages of execution. Named "sorrowful" because these failures cannot be
//! recovered through retries - the workflow must acknowledge defeat.
//!
//! ## Test Coverage
//!
//! - Permanent errors exhausting retry limits
//! - Cascading failures blocking dependent steps
//! - SQL function accuracy with error states
//! - Partial workflow completion with terminal failures
//! - Error state propagation through DAG

use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test diamond pattern with early permanent failure (start step fails)
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_early_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Early permanent failure at start");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Exhaust retries on diamond_start (3 failures)
    tracing::info!("📊 PHASE 1: Exhausting retries on diamond_start (permanent failure)");

    for attempt in 1..=3 {
        manager
            .fail_step(
                task_uuid,
                "diamond_start",
                &format!("Critical error - attempt {}", attempt),
            )
            .await?;
    }

    // **PHASE 2**: Validate permanent failure state
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let start_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_start")
        .unwrap();
    assert_eq!(start_step.current_state.as_deref(), Some("error"));
    assert_eq!(start_step.attempts, 3);
    assert_eq!(start_step.max_attempts, 3);
    assert!(!start_step.retry_eligible, "Retries exhausted");
    assert!(!start_step.ready_for_execution, "Cannot execute");

    // **PHASE 3**: Validate all dependent steps are blocked
    let branch_b = step_readiness
        .iter()
        .find(|s| s.name == "diamond_branch_b")
        .unwrap();
    assert_eq!(branch_b.total_parents, 1);
    assert_eq!(
        branch_b.completed_parents, 0,
        "Parent failed, not completed"
    );
    assert!(!branch_b.dependencies_satisfied);
    assert!(!branch_b.ready_for_execution, "Blocked by failed parent");

    let branch_c = step_readiness
        .iter()
        .find(|s| s.name == "diamond_branch_c")
        .unwrap();
    assert_eq!(branch_c.total_parents, 1);
    assert_eq!(branch_c.completed_parents, 0);
    assert!(!branch_c.dependencies_satisfied);
    assert!(!branch_c.ready_for_execution, "Blocked by failed parent");

    let end_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_end")
        .unwrap();
    assert_eq!(end_step.total_parents, 2);
    assert_eq!(end_step.completed_parents, 0);
    assert!(!end_step.dependencies_satisfied);
    assert!(!end_step.ready_for_execution, "Blocked by failed ancestors");

    // **PHASE 4**: Validate task execution context shows blockage
    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            0, // completed_steps (all blocked)
            0, // ready_steps (all blocked)
        )
        .await?;

    tracing::info!("✅ All dependent steps correctly blocked by permanent failure");
    tracing::info!("🎯 SORROWFUL FAILURE: Early permanent failure test complete");

    Ok(())
}

/// Test diamond pattern with mid-workflow permanent failure (branch fails)
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_mid_workflow_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Mid-workflow permanent failure at branch");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 8}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Successfully complete diamond_start
    tracing::info!("📊 PHASE 1: Completing diamond_start successfully");
    manager
        .complete_step(
            task_uuid,
            "diamond_start",
            serde_json::json!({"result": 64}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 4, 1, 2)
        .await?;

    // **PHASE 2**: Successfully complete diamond_branch_b
    tracing::info!("📊 PHASE 2: Completing diamond_branch_b successfully");
    manager
        .complete_step(
            task_uuid,
            "diamond_branch_b",
            serde_json::json!({"result": 4096}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 4, 2, 1)
        .await?;

    // **PHASE 3**: Permanently fail diamond_branch_c (3 failures)
    tracing::info!("📊 PHASE 3: Permanently failing diamond_branch_c");

    for attempt in 1..=3 {
        manager
            .fail_step(
                task_uuid,
                "diamond_branch_c",
                &format!("Permanent branch failure - attempt {}", attempt),
            )
            .await?;
    }

    // **PHASE 4**: Validate mixed state (partial success + permanent failure)
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let start_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_start")
        .unwrap();
    assert_eq!(
        start_step.current_state.as_deref(),
        Some("complete"),
        "Start completed successfully"
    );

    let branch_b = step_readiness
        .iter()
        .find(|s| s.name == "diamond_branch_b")
        .unwrap();
    assert_eq!(
        branch_b.current_state.as_deref(),
        Some("complete"),
        "Branch B completed successfully"
    );

    let branch_c = step_readiness
        .iter()
        .find(|s| s.name == "diamond_branch_c")
        .unwrap();
    assert_eq!(
        branch_c.current_state.as_deref(),
        Some("error"),
        "Branch C permanently failed"
    );
    assert_eq!(branch_c.attempts, 3);
    assert!(!branch_c.retry_eligible);

    let end_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_end")
        .unwrap();
    assert_eq!(end_step.total_parents, 2);
    assert_eq!(
        end_step.completed_parents, 1,
        "Only one parent completed (branch_b)"
    );
    assert!(!end_step.dependencies_satisfied, "Missing branch_c");
    assert!(!end_step.ready_for_execution, "Blocked by failed branch_c");

    // **PHASE 5**: Validate task context shows partial completion
    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            2, // completed_steps (start + branch_b)
            0, // ready_steps (branch_c failed, end blocked)
        )
        .await?;

    tracing::info!("✅ Partial completion with permanent failure validated");
    tracing::info!("🎯 SORROWFUL FAILURE: Mid-workflow permanent failure test complete");

    Ok(())
}

/// Test diamond pattern with both branches failing permanently
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_dual_branch_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Both branches fail permanently");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 12}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete diamond_start
    tracing::info!("📊 PHASE 1: Completing diamond_start");
    manager
        .complete_step(
            task_uuid,
            "diamond_start",
            serde_json::json!({"result": 144}),
        )
        .await?;

    manager
        .validate_task_execution_context(task_uuid, 4, 1, 2)
        .await?;

    // **PHASE 2**: Permanently fail diamond_branch_b
    tracing::info!("📊 PHASE 2: Permanently failing diamond_branch_b");
    for attempt in 1..=3 {
        manager
            .fail_step(
                task_uuid,
                "diamond_branch_b",
                &format!("Branch B critical error - attempt {}", attempt),
            )
            .await?;
    }

    // **PHASE 3**: Permanently fail diamond_branch_c
    tracing::info!("📊 PHASE 3: Permanently failing diamond_branch_c");
    for attempt in 1..=3 {
        manager
            .fail_step(
                task_uuid,
                "diamond_branch_c",
                &format!("Branch C critical error - attempt {}", attempt),
            )
            .await?;
    }

    // **PHASE 4**: Validate catastrophic failure state
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let start_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_start")
        .unwrap();
    assert_eq!(
        start_step.current_state.as_deref(),
        Some("complete"),
        "Start completed"
    );

    let branch_b = step_readiness
        .iter()
        .find(|s| s.name == "diamond_branch_b")
        .unwrap();
    assert_eq!(
        branch_b.current_state.as_deref(),
        Some("error"),
        "Branch B failed"
    );
    assert!(!branch_b.retry_eligible);

    let branch_c = step_readiness
        .iter()
        .find(|s| s.name == "diamond_branch_c")
        .unwrap();
    assert_eq!(
        branch_c.current_state.as_deref(),
        Some("error"),
        "Branch C failed"
    );
    assert!(!branch_c.retry_eligible);

    let end_step = step_readiness
        .iter()
        .find(|s| s.name == "diamond_end")
        .unwrap();
    assert_eq!(end_step.total_parents, 2);
    assert_eq!(
        end_step.completed_parents, 0,
        "Both parents failed, not completed"
    );
    assert!(!end_step.dependencies_satisfied, "Both parents failed");
    assert!(
        !end_step.ready_for_execution,
        "Cannot execute without parents"
    );

    // **PHASE 5**: Validate task completely blocked
    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            1, // completed_steps (only start)
            0, // ready_steps (both branches failed, end blocked)
        )
        .await?;

    tracing::info!("✅ Convergence step correctly blocked by both failed parents");
    tracing::info!("🎯 SORROWFUL FAILURE: Dual branch failure test complete");

    Ok(())
}

/// Test SQL function accuracy with mixed states (complete, error, pending)
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_mixed_states_sql_accuracy(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: SQL accuracy with mixed states");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 14}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Create a complex mixed state scenario
    // - diamond_start: complete
    // - diamond_branch_b: complete
    // - diamond_branch_c: error (exhausted retries)
    // - diamond_end: pending (blocked)

    tracing::info!("📊 Creating mixed state scenario");
    manager
        .complete_step(
            task_uuid,
            "diamond_start",
            serde_json::json!({"result": 196}),
        )
        .await?;

    manager
        .complete_step(
            task_uuid,
            "diamond_branch_b",
            serde_json::json!({"result": 38416}),
        )
        .await?;

    for attempt in 1..=3 {
        manager
            .fail_step(
                task_uuid,
                "diamond_branch_c",
                &format!("Mixed state error - attempt {}", attempt),
            )
            .await?;
    }

    // **VALIDATE**: SQL functions correctly analyze mixed state
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    // Count states
    let complete_count = step_readiness
        .iter()
        .filter(|s| s.current_state.as_deref() == Some("complete"))
        .count();
    let error_count = step_readiness
        .iter()
        .filter(|s| s.current_state.as_deref() == Some("error"))
        .count();
    let pending_count = step_readiness
        .iter()
        .filter(|s| s.current_state.as_deref() == Some("pending"))
        .count();

    assert_eq!(complete_count, 2, "2 steps complete (start + branch_b)");
    assert_eq!(error_count, 1, "1 step in error (branch_c)");
    assert_eq!(pending_count, 1, "1 step pending (end - blocked)");

    tracing::info!(
        "✅ Mixed states: {} complete, {} error, {} pending",
        complete_count,
        error_count,
        pending_count
    );

    // Validate task context aggregates correctly
    manager
        .validate_task_execution_context(
            task_uuid, 4, // total_steps
            2, // completed_steps
            0, // ready_steps (none ready due to blockage)
        )
        .await?;

    // Validate specific step states
    for step in &step_readiness {
        match step.name.as_str() {
            "diamond_start" | "diamond_branch_b" => {
                assert_eq!(step.current_state.as_deref(), Some("complete"));
                assert!(!step.ready_for_execution, "Complete steps not ready");
            }
            "diamond_branch_c" => {
                assert_eq!(step.current_state.as_deref(), Some("error"));
                assert!(!step.retry_eligible, "Retries exhausted");
                assert!(!step.ready_for_execution, "Error step not ready");
            }
            "diamond_end" => {
                assert_eq!(step.current_state.as_deref(), Some("pending"));
                assert!(!step.dependencies_satisfied, "Missing branch_c");
                assert!(!step.ready_for_execution, "Blocked by failed parent");
            }
            _ => panic!("Unexpected step: {}", step.name),
        }

        tracing::info!(
            "Step {}: state={:?}, ready={}, deps_satisfied={}, retry_eligible={}",
            step.name,
            step.current_state,
            step.ready_for_execution,
            step.dependencies_satisfied,
            step.retry_eligible
        );
    }

    tracing::info!("✅ SQL functions accurately analyze mixed state scenario");
    tracing::info!("🎯 SORROWFUL FAILURE: Mixed states SQL accuracy test complete");

    Ok(())
}
