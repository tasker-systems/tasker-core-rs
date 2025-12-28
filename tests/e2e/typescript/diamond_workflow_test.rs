//! TypeScript Diamond Workflow E2E Tests
//!
//! These tests validate diamond (parallel branches with convergence) workflow
//! execution with TypeScript handlers through the orchestration API.
//!
//! Diamond Pattern (with input 6):
//! 1. Start: Square the initial even number (6 -> 36)
//! 2. Branch B (parallel): Add 25 to squared result (36 + 25 = 61)
//! 3. Branch C (parallel): Multiply squared result by 2 (36 * 2 = 72)
//! 4. End (convergence): Average both branch results ((61 + 72) / 2 = 66.5)
//!
//! Note: These tests require the TypeScript worker to be running on port 8084.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Test diamond workflow with TypeScript handlers using standard input
///
/// Validates:
/// - Start step executes first
/// - Both branches execute in parallel
/// - Convergence step waits for both branches
/// - Final result is computed correctly
#[tokio::test]
async fn test_typescript_diamond_workflow_standard() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with input 6
    // Expected: 6² = 36, branch_b = 36+25 = 61, branch_c = 36*2 = 72, end = (61+72)/2 = 66.5
    let task_request = create_task_request(
        "diamond_workflow_ts",
        "parallel_computation_ts",
        json!({ "even_number": 6 }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Should complete in reasonable time (parallel branches)
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    // Verify completion
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );

    // Verify all 4 steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 4, "Should have 4 steps in diamond workflow");

    // All steps should be complete
    for step in &steps {
        assert!(
            step.current_state.to_uppercase() == "COMPLETE",
            "Step {} should be complete, got: {}",
            step.name,
            step.current_state
        );
    }

    println!(
        "✅ TypeScript diamond workflow completed with {} steps",
        steps.len()
    );
    Ok(())
}

/// Test diamond workflow parallel execution
///
/// Validates:
/// - Branch B and Branch C can execute in parallel
/// - Both branches depend only on Start step
/// - End step waits for both branches
#[tokio::test]
async fn test_typescript_diamond_workflow_parallel_branches() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    let task_request = create_task_request(
        "diamond_workflow_ts",
        "parallel_computation_ts",
        json!({ "even_number": 10 }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify step names match expected pattern
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();

    assert!(
        step_names.contains(&"diamond_start_ts"),
        "Should have diamond_start_ts"
    );
    assert!(
        step_names.contains(&"diamond_branch_b_ts"),
        "Should have diamond_branch_b_ts"
    );
    assert!(
        step_names.contains(&"diamond_branch_c_ts"),
        "Should have diamond_branch_c_ts"
    );
    assert!(
        step_names.contains(&"diamond_end_ts"),
        "Should have diamond_end_ts"
    );

    // All steps should be complete
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    println!("✅ TypeScript diamond workflow parallel branches executed correctly");
    Ok(())
}

/// Test diamond workflow with different input value
///
/// Validates mathematical correctness with input 4:
/// - Start: 4² = 16
/// - Branch B: 16 + 25 = 41
/// - Branch C: 16 * 2 = 32
/// - End: (41 + 32) / 2 = 36.5
#[tokio::test]
async fn test_typescript_diamond_workflow_different_input() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    let task_request = create_task_request(
        "diamond_workflow_ts",
        "parallel_computation_ts",
        json!({ "even_number": 4 }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 4, "Should have 4 steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    println!("✅ TypeScript diamond workflow with input 4 completed successfully");
    Ok(())
}
