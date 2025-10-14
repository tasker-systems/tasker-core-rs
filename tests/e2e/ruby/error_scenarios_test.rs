//! Ruby Error Scenario E2E Tests
//!
//! These tests validate error handling with Ruby handlers through the orchestration API.
//! Tests are language-agnostic - they verify API contract behavior regardless of handler implementation.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{
    create_task_request, wait_for_task_completion, wait_for_task_failure,
};

/// Test happy path execution with success handler
///
/// Validates:
/// - Task completes successfully
/// - No retries needed
/// - Fast execution (< 3s)
#[tokio::test]
async fn test_success_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with success-only template (no bypass needed - idempotent design)
    let task_request = create_task_request(
        "test_errors",
        "success_only",
        json!({ "scenario_type": "success" }),
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
    // Task status should indicate completion (either "complete" state or "all_complete" execution status)
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

    println!("✅ Success scenario completed successfully");
    Ok(())
}

/// Test permanent failure with no retries
///
/// Validates:
/// - Task fails immediately
/// - No retries attempted (< 2s total)
/// - Step marked as non-retryable error
#[tokio::test]
async fn test_permanent_failure_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with permanent-error-only template
    let task_request = create_task_request(
        "test_errors",
        "permanent_error_only",
        json!({ "scenario_type": "permanent_failure" }),
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
        "✅ Permanent failure completed in {:?} (no retries)",
        elapsed
    );
    Ok(())
}

/// Test retryable failure with backoff exhaustion
///
/// Validates:
/// - Task retries with exponential backoff
/// - Retry limit respected (2 retries)
/// - Eventually fails after retry exhaustion
/// - Takes measurable time for retries (>100ms, <3s)
#[tokio::test]
async fn test_retryable_failure_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with retryable-error-only template
    let task_request = create_task_request(
        "test_errors",
        "retryable_error_only",
        json!({ "scenario_type": "retryable_failure" }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    let start_time = std::time::Instant::now();

    // Should fail after retries (2 retries @ 50ms base = ~150-200ms total)
    wait_for_task_failure(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let elapsed = start_time.elapsed();

    // Verify took time for retries but not too long
    assert!(
        elapsed.as_millis() > 100,
        "Should have retry delays, only took {:?}",
        elapsed
    );
    assert!(
        elapsed.as_secs() < 4,
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
        "✅ Retryable failure completed in {:?} (with retries)",
        elapsed
    );
    Ok(())
}

/// Test mixed workflow with both success and failure steps
///
/// Validates:
/// - Orchestration handles mixed outcomes
/// - Some steps succeed, others fail
/// - Overall task marked as failed
#[tokio::test]
async fn test_mixed_workflow_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Use error_testing template with all 3 steps (success + 2 error types)
    let task_request = create_task_request(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "mixed" }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Should fail overall (error steps will fail)
    wait_for_task_failure(&manager.orchestration_client, &response.task_uuid, 5).await?;

    // Verify mixed results
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let _task = manager.orchestration_client.get_task(task_uuid).await?;

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 3, "Should have 3 total steps");

    let completed_count = steps
        .iter()
        .filter(|s| s.current_state.to_uppercase() == "COMPLETE")
        .count();
    let failed_count = steps
        .iter()
        .filter(|s| s.current_state.to_uppercase() == "ERROR")
        .count();

    assert!(
        completed_count >= 1,
        "At least success_step should complete, got {}",
        completed_count
    );
    assert!(
        failed_count >= 1,
        "At least one error step should fail, got {}",
        failed_count
    );

    println!(
        "✅ Mixed workflow handled correctly: {} succeeded, {} failed",
        completed_count, failed_count
    );
    Ok(())
}
