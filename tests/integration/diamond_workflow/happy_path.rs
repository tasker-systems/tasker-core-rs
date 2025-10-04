//! # Diamond Workflow Happy Path Integration Tests
//!
//! Tests the diamond pattern workflow through its complete happy path lifecycle,
//! validating SQL function integration at each stage.
//!
//! ## Diamond Pattern Structure
//!
//! ```text
//!       diamond_start (input²)
//!           /      \
//!          /        \
//!   branch_b²    branch_c²
//!          \        /
//!           \      /
//!        diamond_end (multiply & square)
//! ```
//!
//! ## Test Coverage
//!
//! - Task initialization with proper dependency structure
//! - SQL function validation of initial state
//! - Step readiness based on dependency completion
//! - Task execution context accuracy throughout lifecycle

use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test diamond pattern initialization and initial SQL function validation
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_initialization(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DIAMOND PATTERN: Initialization and SQL validation");

    // Create test manager
    let manager = LifecycleTestManager::new(pool).await?;

    // Create task request for diamond pattern
    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 6}),
    );

    // Initialize task
    let init_result = manager.initialize_task(task_request).await?;

    // Validate task execution context
    manager
        .validate_task_execution_context(
            init_result.task_uuid,
            4, // total_steps
            0, // completed_steps
            1, // ready_steps (only diamond_start)
        )
        .await?;

    // Validate individual step readiness
    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "diamond_start",
            true, // ready
            0,    // total_parents
            0,    // completed_parents
        )
        .await?;

    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "diamond_branch_b",
            false, // not ready (depends on start)
            1,     // total_parents
            0,     // completed_parents
        )
        .await?;

    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "diamond_branch_c",
            false, // not ready (depends on start)
            1,     // total_parents
            0,     // completed_parents
        )
        .await?;

    manager
        .validate_step_readiness(
            init_result.task_uuid,
            "diamond_end",
            false, // not ready (depends on both branches)
            2,     // total_parents
            0,     // completed_parents
        )
        .await?;

    // Verify ready step count
    let ready_count = manager.count_ready_steps(init_result.task_uuid).await?;
    assert_eq!(ready_count, 1, "Only diamond_start should be ready");

    tracing::info!("🎯 DIAMOND PATTERN: Initialization test complete");

    Ok(())
}

/// Test diamond pattern with detailed dependency analysis
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_dependency_structure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DIAMOND PATTERN: Dependency structure validation");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 8}),
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
            "diamond_start" => {
                assert_eq!(step.total_parents, 0, "Start has no dependencies");
                assert!(step.dependencies_satisfied, "Start dependencies satisfied");
                assert!(step.ready_for_execution, "Start should be ready");
            }
            "diamond_branch_b" | "diamond_branch_c" => {
                assert_eq!(step.total_parents, 1, "Branches have 1 dependency");
                assert!(
                    !step.dependencies_satisfied,
                    "Branch dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Branches not ready yet");
            }
            "diamond_end" => {
                assert_eq!(step.total_parents, 2, "End has 2 dependencies");
                assert!(
                    !step.dependencies_satisfied,
                    "End dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "End not ready yet");
            }
            _ => panic!("Unexpected step: {}", step.name),
        }
    }

    tracing::info!("🎯 DIAMOND PATTERN: Dependency structure test complete");

    Ok(())
}

/// Test complete diamond workflow execution with step transitions
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_diamond_pattern_complete_execution(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DIAMOND PATTERN: Complete execution with state transitions");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "diamond_pattern",
        "diamond_workflow",
        serde_json::json!({"even_number": 4}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Initial state validation
    tracing::info!("📊 PHASE 1: Initial state validation");
    manager
        .validate_task_execution_context(task_uuid, 4, 0, 1)
        .await?;
    assert_eq!(manager.count_ready_steps(task_uuid).await?, 1);

    // **PHASE 2**: Complete diamond_start
    tracing::info!("📊 PHASE 2: Completing diamond_start");
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
    manager
        .validate_step_readiness(task_uuid, "diamond_end", false, 2, 0)
        .await?;

    tracing::info!("✅ Both branches ready after start completion");

    // **PHASE 3**: Complete diamond_branch_b
    tracing::info!("📊 PHASE 3: Completing diamond_branch_b");
    manager
        .complete_step(
            task_uuid,
            "diamond_branch_b",
            serde_json::json!({"result": 256}),
        )
        .await?;

    // Validate: diamond_end still not ready (needs both branches)
    manager
        .validate_task_execution_context(task_uuid, 4, 2, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "diamond_branch_c", true, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "diamond_end", false, 2, 1)
        .await?;

    tracing::info!("✅ Branch B complete, branch C still ready, end waiting for both");

    // **PHASE 4**: Complete diamond_branch_c
    tracing::info!("📊 PHASE 4: Completing diamond_branch_c");
    manager
        .complete_step(
            task_uuid,
            "diamond_branch_c",
            serde_json::json!({"result": 256}),
        )
        .await?;

    // Validate: diamond_end should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 3, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "diamond_end", true, 2, 2)
        .await?;

    tracing::info!("✅ Both branches complete, convergence step now ready");

    // **PHASE 5**: Complete diamond_end
    tracing::info!("📊 PHASE 5: Completing diamond_end (convergence)");
    manager
        .complete_step(
            task_uuid,
            "diamond_end",
            serde_json::json!({"result": 65536}),
        )
        .await?;

    // Validate: All steps complete
    manager
        .validate_task_execution_context(task_uuid, 4, 4, 0)
        .await?;
    assert_eq!(manager.count_ready_steps(task_uuid).await?, 0);

    tracing::info!("✅ All steps complete - diamond workflow execution successful");

    // **PHASE 6**: Detailed step readiness analysis
    tracing::info!("📊 PHASE 6: Final state validation");
    let final_readiness = manager.get_step_readiness_status(task_uuid).await?;

    for step in &final_readiness {
        assert_eq!(
            step.current_state.as_deref(),
            Some("complete"),
            "Step {} should be in complete state",
            step.name
        );
        assert!(
            !step.ready_for_execution,
            "Step {} should not be ready (already complete)",
            step.name
        );

        tracing::info!(
            step = %step.name,
            state = ?step.current_state,
            ready = step.ready_for_execution,
            "📋 Final step state"
        );
    }

    tracing::info!("🎯 DIAMOND PATTERN: Complete execution test successful");

    Ok(())
}
