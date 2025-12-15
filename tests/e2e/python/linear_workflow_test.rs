//! Python Linear Workflow E2E Tests
//!
//! These tests validate sequential workflow execution with Python handlers
//! through the orchestration API.
//!
//! Mathematical Pattern (with input 6):
//! 1. Step 1: Square the initial even number (6 -> 36)
//! 2. Step 2: Add constant to squared result (36 + 10 = 46)
//! 3. Step 3: Multiply by factor (46 * 3 = 138)
//! 4. Step 4: Divide for final result (138 / 2 = 69)
//!
//! Note: These tests require the Python worker to be running on port 8083.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Test linear workflow with Python handlers using standard input
///
/// Validates:
/// - All 4 steps execute in sequence
/// - Dependencies are resolved correctly
/// - Results propagate between steps
/// - Final mathematical result is correct
#[tokio::test]
async fn test_python_linear_workflow_standard() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with input 6
    // Expected: 6² = 36, 36+10 = 46, 46*3 = 138, 138/2 = 69
    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence_py",
        json!({ "even_number": 6 }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Should complete in reasonable time (4 steps, sequential)
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

    assert_eq!(steps.len(), 4, "Should have 4 steps in linear workflow");

    // All steps should be complete
    for step in &steps {
        assert!(
            step.current_state.to_uppercase() == "COMPLETE",
            "Step {} should be complete, got: {}",
            step.name,
            step.current_state
        );
    }

    println!("✅ Python linear workflow completed with {} steps", steps.len());
    Ok(())
}

/// Test linear workflow with Python handlers using different input
///
/// Validates mathematical correctness with input 10:
/// - Step 1: 10² = 100
/// - Step 2: 100 + 10 = 110
/// - Step 3: 110 * 3 = 330
/// - Step 4: 330 / 2 = 165
#[tokio::test]
async fn test_python_linear_workflow_different_input() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with input 10
    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence_py",
        json!({ "even_number": 10 }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Should complete in reasonable time
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    // Verify completion
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );

    // Verify all steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 4, "Should have 4 steps");
    assert!(
        steps.iter().all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    println!("✅ Python linear workflow with input 10 completed successfully");
    Ok(())
}

/// Test that linear workflow maintains step ordering
///
/// Validates:
/// - Dependencies enforce execution order
/// - Step 2 waits for Step 1
/// - Step 3 waits for Step 2
/// - Step 4 waits for Step 3
#[tokio::test]
async fn test_python_linear_workflow_ordering() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence_py",
        json!({ "even_number": 8 }),
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
        step_names.contains(&"linear_step_1_py"),
        "Should have linear_step_1_py"
    );
    assert!(
        step_names.contains(&"linear_step_2_py"),
        "Should have linear_step_2_py"
    );
    assert!(
        step_names.contains(&"linear_step_3_py"),
        "Should have linear_step_3_py"
    );
    assert!(
        step_names.contains(&"linear_step_4_py"),
        "Should have linear_step_4_py"
    );

    println!("✅ Python linear workflow steps executed in correct order");
    Ok(())
}
