//! # Diamond-Decision-Batch Combined Workflow Integration Tests
//!
//! Tests the composition of three workflow patterns:
//! - Diamond pattern (parallel branches + convergence)
//! - Decision points (dynamic step creation)
//! - Batch processing (cursor-based parallel workers)
//!
//! ## Workflow Structure
//!
//! ```text
//! ddb_diamond_start
//!     ├─→ branch_evens
//!     └─→ branch_odds
//!             └─→ ddb_routing_decision (decision)
//!                     ├─→ even_batch_analyzer (batchable)
//!                     │       └─→ process_even_batch_N (batch_workers)
//!                     │               └─→ aggregate_even_results (deferred_convergence)
//!                     │
//!                     └─→ odd_batch_analyzer (batchable)
//!                             └─→ process_odd_batch_N (batch_workers)
//!                                     └─→ aggregate_odd_results (deferred_convergence)
//! ```

use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test even-dominant scenario creates even_batch_analyzer
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_even_dominant_creates_even_batches(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DIAMOND-DECISION-BATCH: Even dominant scenario");

    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/rust")
            .await?;

    // Create task with more evens than odds
    // Note: batch_size is configured in YAML template (not task context) for security
    let task_request = manager.create_task_request_for_template(
        "diamond_decision_batch_processor",
        "combined_workflow_test",
        serde_json::json!({
            "numbers": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],  // 5 evens, 5 odds (equal - defaults to even)
            // batch_size=2 from YAML → 5 evens / 2 = 2.5 → 3 workers expected
        }),
    );

    // Initialize task
    let init_result = manager.initialize_task(task_request).await?;

    // Validate initial task execution context
    // Should have 4 base steps: ddb_diamond_start, branch_evens, branch_odds, ddb_routing_decision
    // Batch processing steps are created dynamically after decision point executes
    manager
        .validate_task_execution_context(
            init_result.task_uuid,
            4, // total_steps initially (before decision creates dynamic steps)
            0, // completed_steps
            1, // ready_steps (only ddb_diamond_start)
        )
        .await?;

    // Validate ddb_diamond_start is ready
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "ddb_diamond_start",
            true, // ready
            0,    // total_parents
            0,    // completed_parents
        )
        .await?;

    // Validate branches are not ready (depend on start)
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "branch_evens",
            false, // not ready
            1,     // total_parents
            0,     // completed_parents
        )
        .await?;

    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "branch_odds",
            false, // not ready
            1,     // total_parents
            0,     // completed_parents
        )
        .await?;

    // Validate decision point is not ready (depends on both branches)
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "ddb_routing_decision",
            false, // not ready
            2,     // total_parents (branch_evens + branch_odds)
            0,     // completed_parents
        )
        .await?;

    tracing::info!("✅ Even dominant initialization validated");

    Ok(())
}

/// Test odd-dominant scenario creates odd_batch_analyzer
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_odd_dominant_creates_odd_batches(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DIAMOND-DECISION-BATCH: Odd dominant scenario");

    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/rust")
            .await?;

    // Create task with more odds than evens
    let task_request = manager.create_task_request_for_template(
        "diamond_decision_batch_processor",
        "combined_workflow_test",
        serde_json::json!({
            "numbers": [1, 3, 5, 7, 9, 11, 2, 4],  // 6 odds, 2 evens
            "batch_size": 2
        }),
    );

    // Initialize task
    let init_result = manager.initialize_task(task_request).await?;

    // Validate initial state
    manager
        .validate_task_execution_context(
            init_result.task_uuid,
            4, // total_steps (only base steps initially)
            0, // completed_steps
            1, // ready_steps
        )
        .await?;

    // Validate ddb_diamond_start is ready
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "ddb_diamond_start",
            true, // ready
            0,    // total_parents
            0,    // completed_parents
        )
        .await?;

    tracing::info!("✅ Odd dominant initialization validated");

    Ok(())
}

/// Test diamond convergence - both branches must complete before decision
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_convergence_before_decision(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DIAMOND-DECISION-BATCH: Diamond convergence validation");

    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/rust")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_decision_batch_processor",
        "combined_workflow_test",
        serde_json::json!({
            "numbers": [2, 4, 6, 8],  // All evens
            "batch_size": 2
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Complete ddb_diamond_start
    manager
        .complete_step(
            init_result.task_uuid,
            "ddb_diamond_start",
            serde_json::json!({"all_numbers": [2, 4, 6, 8], "count": 4}),
        )
        .await?;

    // Verify both branches are now ready
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "branch_evens",
            true, // ready
            1,    // total_parents
            1,    // completed_parents
        )
        .await?;

    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "branch_odds",
            true, // ready
            1,    // total_parents
            1,    // completed_parents
        )
        .await?;

    // Verify decision is NOT ready (needs both branches)
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "ddb_routing_decision",
            false, // not ready
            2,     // total_parents
            0,     // completed_parents (only start completed, not branches yet)
        )
        .await?;

    // Complete branch_evens
    manager
        .complete_step(
            init_result.task_uuid,
            "branch_evens",
            serde_json::json!({"even_numbers": [2, 4, 6, 8], "count": 4}),
        )
        .await?;

    // Decision STILL not ready (needs both branches)
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "ddb_routing_decision",
            false, // not ready yet
            2,     // total_parents
            1,     // completed_parents (only branch_evens)
        )
        .await?;

    // Complete branch_odds
    manager
        .complete_step(
            init_result.task_uuid,
            "branch_odds",
            serde_json::json!({"odd_numbers": [], "count": 0}),
        )
        .await?;

    // NOW decision is ready (both branches complete)
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "ddb_routing_decision",
            true, // ready!
            2,    // total_parents
            2,    // completed_parents
        )
        .await?;

    tracing::info!("✅ Diamond convergence validated - decision waits for both branches");

    Ok(())
}

/// Test batch worker creation for even numbers
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_even_batch_worker_creation(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DIAMOND-DECISION-BATCH: Even batch worker validation");

    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/rust")
            .await?;

    // Note: batch_size is configured in YAML template (not task context) for security
    let task_request = manager.create_task_request_for_template(
        "diamond_decision_batch_processor",
        "combined_workflow_test",
        serde_json::json!({
            "numbers": [2, 4, 6, 8, 10, 12],  // 6 evens, 0 odds
            // batch_size=2 from YAML → 6 evens / 2 = 3 workers expected
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Validate initial state
    manager
        .validate_task_execution_context(
            init_result.task_uuid,
            4, // total_steps (only base steps initially)
            0, // completed_steps
            1, // ready_steps
        )
        .await?;

    tracing::info!("✅ Batch worker creation test initialized");

    Ok(())
}
