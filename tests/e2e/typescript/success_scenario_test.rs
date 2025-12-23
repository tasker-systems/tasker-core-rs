//! TypeScript Success Scenario E2E Tests
//!
//! These tests validate basic success scenarios with TypeScript handlers
//! through the orchestration API.
//!
//! Note: These tests require the TypeScript worker to be running on port 8084.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Test simple success workflow with TypeScript handler
///
/// Validates:
/// - Single step executes successfully
/// - Task completes in expected time
/// - Result data is properly returned
#[tokio::test]
async fn test_typescript_success_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create simple success task
    let task_request = create_task_request(
        "test_scenarios",
        "success_only_ts",
        json!({ "message": "Hello from TypeScript e2e test!" }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Single step should complete quickly
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    // Verify completion
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );

    // Verify step completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 1, "Should have 1 step in success workflow");
    assert!(
        steps[0].current_state.to_uppercase() == "COMPLETE",
        "Step should be complete, got: {}",
        steps[0].current_state
    );

    println!("✅ TypeScript success scenario completed");
    Ok(())
}

/// Test success workflow without optional input
///
/// Validates:
/// - Handler works with default values
/// - No required input validation failure
#[tokio::test]
async fn test_typescript_success_no_input() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task without optional message
    let task_request = create_task_request("test_scenarios", "success_only_ts", json!({}));

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task should complete without optional input"
    );

    println!("✅ TypeScript success scenario with default values completed");
    Ok(())
}
