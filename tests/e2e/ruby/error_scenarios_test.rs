//! Ruby Error Scenario E2E Tests
//!
//! These tests validate error handling with Ruby handlers through the orchestration API.
//! Tests are language-agnostic - they verify API contract behavior regardless of handler implementation.

use anyhow::Result;
use serde_json::json;

use crate::common::{
    create_task_request_with_bypass, wait_for_task_completion, wait_for_task_failure,
    IntegrationTestManager,
};

/// Test happy path execution with success handler
///
/// Validates:
/// - Task completes successfully
/// - No retries needed
/// - Fast execution (< 3s)
#[tokio::test]
#[ignore] // Run with --ignored when Docker services are running
async fn test_success_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with only success_step (bypass error steps)
    let task_request = create_task_request_with_bypass(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "success" }),
        vec!["permanent_error_step".into(), "retryable_error_step".into()],
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Should complete in < 3 seconds (fast execution)
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 3).await?;

    // Verify completion
    let task = manager
        .orchestration_client
        .get_task(response.task_uuid)
        .await?;

    assert_eq!(task.status, "complete", "Task should be complete");
    assert_eq!(
        task.execution_status, "all_complete",
        "Execution status should be all_complete"
    );

    // Verify step counts
    let stats = manager
        .orchestration_client
        .get_task_stats(response.task_uuid)
        .await?;

    assert_eq!(stats.total_steps, 1, "Should have 1 total step");
    assert_eq!(stats.completed_steps, 1, "Should have 1 completed step");
    assert_eq!(stats.failed_steps, 0, "Should have 0 failed steps");

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
#[ignore] // Run with --ignored when Docker services are running
async fn test_permanent_failure_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with only permanent_error_step
    let task_request = create_task_request_with_bypass(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "permanent_failure" }),
        vec!["success_step".into(), "retryable_error_step".into()],
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
        elapsed.as_secs() < 2,
        "Should fail quickly without retries, took {:?}",
        elapsed
    );

    // Verify failure state
    let task = manager
        .orchestration_client
        .get_task(response.task_uuid)
        .await?;

    assert_eq!(
        task.execution_status, "blocked_by_failures",
        "Execution status should indicate blocking failures"
    );

    // Verify step counts
    let stats = manager
        .orchestration_client
        .get_task_stats(response.task_uuid)
        .await?;

    assert_eq!(stats.total_steps, 1, "Should have 1 total step");
    assert_eq!(stats.completed_steps, 0, "Should have 0 completed steps");
    assert_eq!(stats.failed_steps, 1, "Should have 1 failed step");

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
#[ignore] // Run with --ignored when Docker services are running
async fn test_retryable_failure_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with only retryable_error_step
    let task_request = create_task_request_with_bypass(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "retryable_failure" }),
        vec!["success_step".into(), "permanent_error_step".into()],
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
        elapsed.as_secs() < 3,
        "Should not take too long, took {:?}",
        elapsed
    );

    // Verify failure state after retry exhaustion
    let task = manager
        .orchestration_client
        .get_task(response.task_uuid)
        .await?;

    assert_eq!(
        task.execution_status, "blocked_by_failures",
        "Execution status should indicate blocking failures"
    );

    // Verify step counts
    let stats = manager
        .orchestration_client
        .get_task_stats(response.task_uuid)
        .await?;

    assert_eq!(stats.total_steps, 1, "Should have 1 total step");
    assert_eq!(stats.completed_steps, 0, "Should have 0 completed steps");
    assert_eq!(stats.failed_steps, 1, "Should have 1 failed step");

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
#[ignore] // Run with --ignored when Docker services are running
async fn test_mixed_workflow_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Run all steps (no bypass) - parallel execution
    let task_request = create_task_request_with_bypass(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "mixed" }),
        vec![], // No bypass - run all steps
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Should fail overall (error steps will fail)
    wait_for_task_failure(&manager.orchestration_client, &response.task_uuid, 5).await?;

    // Verify mixed results
    let task = manager
        .orchestration_client
        .get_task(response.task_uuid)
        .await?;

    let stats = manager
        .orchestration_client
        .get_task_stats(response.task_uuid)
        .await?;

    assert_eq!(stats.total_steps, 3, "Should have 3 total steps");
    assert!(
        stats.completed_steps >= 1,
        "At least success_step should complete, got {}",
        stats.completed_steps
    );
    assert!(
        stats.failed_steps >= 1,
        "At least one error step should fail, got {}",
        stats.failed_steps
    );

    println!(
        "✅ Mixed workflow handled correctly: {} succeeded, {} failed",
        stats.completed_steps, stats.failed_steps
    );
    Ok(())
}
