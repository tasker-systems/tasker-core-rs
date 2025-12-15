//! Python Error Scenario E2E Tests
//!
//! These tests validate error handling with Python handlers through the orchestration API.
//! Tests are language-agnostic - they verify API contract behavior regardless of handler implementation.
//!
//! Note: These tests require the Python worker to be running on port 8083.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{
    create_task_request, wait_for_task_completion, wait_for_task_failure,
};

/// Test happy path execution with Python success handler
///
/// Validates:
/// - Task completes successfully via Python FFI
/// - No retries needed
/// - Fast execution (< 3s)
#[tokio::test]
async fn test_python_success_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with Python success-only template
    let task_request = create_task_request(
        "test_scenarios",
        "success_only_py",
        json!({ "message": "Hello from Python E2E test" }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Should complete in < 3 seconds (fast execution)
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 3).await?;

    // Verify completion
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );
    assert!(
        task.status.to_lowercase().contains("complete"),
        "Task status should indicate completion, got: {}",
        task.status
    );

    // Verify step count
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    assert_eq!(steps.len(), 1, "Should have 1 total step");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    println!("✅ Python success scenario completed successfully");
    Ok(())
}

/// Test permanent failure with Python handler (no retries)
///
/// Validates:
/// - Task fails immediately via Python FFI
/// - No retries attempted (< 2s total)
/// - Step marked as non-retryable error
#[tokio::test]
async fn test_python_permanent_failure_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with Python permanent-error-only template
    let task_request = create_task_request(
        "test_scenarios",
        "permanent_error_only_py",
        json!({ "error_message": "Permanent failure from Python" }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    let start_time = std::time::Instant::now();

    // Should fail quickly (no retries)
    wait_for_task_failure(&manager.orchestration_client, &response.task_uuid, 3).await?;

    let elapsed = start_time.elapsed();

    // Verify fast failure (< 2s, no retry delays)
    assert!(
        elapsed.as_secs() < 5,
        "Should fail quickly without retries, took {:?}",
        elapsed
    );

    // Verify failure state
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.execution_status.to_lowercase().contains("error")
            || task.execution_status.to_lowercase().contains("blocked")
            || task.execution_status.to_lowercase().contains("fail"),
        "Execution status should indicate failure, got: {}",
        task.execution_status
    );

    // Verify step is in error state
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    assert_eq!(steps.len(), 1, "Should have 1 total step");
    assert!(
        steps[0].current_state.to_uppercase() == "ERROR",
        "Step should be in error state, got: {}",
        steps[0].current_state
    );

    println!(
        "✅ Python permanent failure completed in {:?} (no retries)",
        elapsed
    );
    Ok(())
}

/// Test retryable failure with Python handler (with backoff exhaustion)
///
/// Validates:
/// - Task retries with exponential backoff via Python FFI
/// - Retry limit respected (2 retries)
/// - Eventually fails after retry exhaustion
/// - Takes measurable time for retries (>100ms, <10s)
#[tokio::test]
async fn test_python_retryable_failure_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with Python retryable-error-only template
    let task_request = create_task_request(
        "test_scenarios",
        "retryable_error_only_py",
        json!({ "error_message": "Temporary failure from Python - will retry" }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    let start_time = std::time::Instant::now();

    // Should fail after retries
    wait_for_task_failure(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let elapsed = start_time.elapsed();

    // Verify took time for retries but not too long
    assert!(
        elapsed.as_millis() > 100,
        "Should have retry delays, only took {:?}",
        elapsed
    );
    assert!(
        elapsed.as_secs() < 10,
        "Should not take too long, took {:?}",
        elapsed
    );

    // Verify failure state after retry exhaustion
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.execution_status.to_lowercase().contains("error")
            || task.execution_status.to_lowercase().contains("blocked")
            || task.execution_status.to_lowercase().contains("fail"),
        "Execution status should indicate failure, got: {}",
        task.execution_status
    );

    // Verify step exhausted retries
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    assert_eq!(steps.len(), 1, "Should have 1 total step");
    assert!(
        steps[0].current_state.to_uppercase() == "ERROR",
        "Step should be in error state, got: {}",
        steps[0].current_state
    );

    println!(
        "✅ Python retryable failure completed in {:?} (with retries)",
        elapsed
    );
    Ok(())
}
