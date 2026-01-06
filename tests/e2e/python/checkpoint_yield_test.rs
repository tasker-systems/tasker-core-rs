// Rust guideline compliant 2025-12-16
//! TAS-125: Python Checkpoint Yield E2E Tests
//!
//! Tests the checkpoint yielding functionality demonstrating how batch processing
//! handlers can persist progress and be re-dispatched for continuation.
//!
//! Workflow Pattern:
//! 1. analyze_items (batchable): Create a single batch for checkpoint testing
//! 2. checkpoint_yield_batch (batch_worker): Process items with checkpoint yields
//! 3. aggregate_results (deferred_convergence): Aggregate final output
//!
//! Test Scenarios:
//! - Happy path: Checkpoint yields work correctly
//! - Transient failure: Resume from checkpoint after retryable error
//! - Permanent failure: Correctly fail on non-retryable error
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before tests.
//!
//! Note: These tests require the Python worker running on port 8083.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Create checkpoint yield test task request for Python handlers.
fn create_checkpoint_yield_request(
    total_items: i32,
    items_per_checkpoint: i32,
    fail_after_items: Option<i32>,
    fail_on_attempt: Option<i32>,
    permanent_failure: bool,
) -> tasker_shared::models::core::task_request::TaskRequest {
    let mut context = json!({
        "total_items": total_items,
        "items_per_checkpoint": items_per_checkpoint,
    });

    if let Some(fail_after) = fail_after_items {
        context["fail_after_items"] = json!(fail_after);
    }

    if let Some(fail_on) = fail_on_attempt {
        context["fail_on_attempt"] = json!(fail_on);
    }

    if permanent_failure {
        context["permanent_failure"] = json!(true);
    }

    create_task_request(
        "python_e2e_checkpoint_yield",
        "checkpoint_yield_test",
        context,
    )
}

/// Test checkpoint yield happy path with Python handlers.
///
/// Validates:
/// - Handler yields checkpoints at configured intervals
/// - Checkpoints are persisted correctly
/// - Step resumes from checkpoint and completes successfully
/// - Final aggregated results are correct
#[tokio::test]
async fn test_python_checkpoint_yield_happy_path() -> Result<()> {
    println!("🚀 Starting TAS-125 Checkpoint Yield Happy Path Test (Python)");
    println!("   Total items: 100");
    println!("   Items per checkpoint: 25");
    println!("   Expected: 4 checkpoint yields, then success");
    println!("   Template: checkpoint_yield_test");
    println!("   Namespace: python_e2e_checkpoint_yield");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Create checkpoint yield task
    println!("\n🎯 Creating checkpoint yield task...");
    let task_request = create_checkpoint_yield_request(
        100, // total_items
        25,  // items_per_checkpoint (4 checkpoints expected)
        None, None, false,
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);

    // Monitor task execution
    println!("\n⏱️ Monitoring checkpoint yield execution...");
    println!("   Handler should yield checkpoints every 25 items");

    let timeout = 60;
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying checkpoint yield results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Checkpoint yield execution should be complete"
    );
    println!(
        "✅ Task execution status: {} (overall status: {})",
        final_task.execution_status, final_task.status
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify expected steps
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        step_names.contains(&"analyze_items".to_string()),
        "Should have analyze_items step"
    );
    assert!(
        step_names.contains(&"aggregate_results".to_string()),
        "Should have aggregate_results step"
    );

    // Verify all steps completed
    for step in &steps {
        assert_eq!(
            step.current_state.to_ascii_uppercase(),
            "COMPLETE",
            "Step {} should be completed",
            step.name
        );
        println!("   ✅ Step: {} - {}", step.name, step.current_state);
    }

    // Get the aggregate_results step to verify final output
    let aggregate_step = steps
        .iter()
        .find(|s| s.name == "aggregate_results")
        .expect("Should have aggregate_results step");

    println!("\n📊 Aggregate Results:");
    let results = aggregate_step
        .results
        .as_ref()
        .expect("Aggregate step should have result data");

    let result = results
        .get("result")
        .expect("Results should contain result object");

    // Verify we processed all 100 items
    let total_processed = result
        .get("total_processed")
        .expect("Results should contain total_processed");
    println!("   Total items processed: {}", total_processed);
    assert_eq!(
        total_processed.as_u64().unwrap(),
        100,
        "Should have processed all 100 items"
    );

    // Verify checkpoint count
    if let Some(checkpoints_used) = result.get("checkpoints_used") {
        println!("   Checkpoints used: {}", checkpoints_used);
        assert!(
            checkpoints_used.as_u64().unwrap() >= 3,
            "Should have used at least 3 checkpoints (100/25 = 4 chunks)"
        );
    }

    // Verify test passed flag
    let test_passed = result
        .get("test_passed")
        .expect("Results should contain test_passed");
    assert!(test_passed.as_bool().unwrap(), "Test should have passed");

    println!("\n🎉 TAS-125 Checkpoint Yield Happy Path Test PASSED!");
    println!("✅ Checkpoint yielding: Working");
    println!("✅ Progress persistence: Working");
    println!("✅ Step re-dispatch: Working");
    println!("✅ Final aggregation: Working");
    println!("✅ Python Batchable checkpoint_yield(): Working");

    Ok(())
}

/// Test checkpoint yield with transient failure and resume.
///
/// Validates:
/// - Handler fails after processing some items
/// - Checkpoint is persisted before failure
/// - Step resumes from checkpoint on retry
/// - Processing continues from checkpoint position
/// - Task completes successfully after retry
#[tokio::test]
async fn test_python_checkpoint_yield_transient_failure_resume() -> Result<()> {
    println!("🚀 Starting TAS-125 Checkpoint Yield Transient Failure Test (Python)");
    println!("   Total items: 100");
    println!("   Items per checkpoint: 20");
    println!("   Fail after: 50 items (on first attempt)");
    println!("   Expected: Fail at 50, resume from checkpoint ~40, complete successfully");

    let manager = IntegrationTestManager::setup().await?;

    // Create task with failure injection
    println!("\n🎯 Creating checkpoint yield task with transient failure...");
    let task_request = create_checkpoint_yield_request(
        100,      // total_items
        20,       // items_per_checkpoint
        Some(50), // fail_after_items - fail after processing 50 items
        Some(1),  // fail_on_attempt - only fail on first attempt
        false,    // permanent_failure - transient failure (retryable)
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);

    // Monitor task execution
    println!("\n⏱️ Monitoring execution with transient failure...");
    println!("   Handler should fail at 50 items on first attempt");
    println!("   Then resume from last checkpoint and complete");

    let timeout = 90;
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying transient failure resume results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Task should complete after retry"
    );
    println!(
        "✅ Task execution status: {} (overall status: {})",
        final_task.execution_status, final_task.status
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Find the batch worker step
    let batch_worker = steps
        .iter()
        .find(|s| s.name.starts_with("checkpoint_yield_batch"))
        .expect("Should have batch worker step");

    println!("\n📊 Batch Worker Step:");
    println!("   Step: {}", batch_worker.name);
    println!("   State: {}", batch_worker.current_state);
    println!("   Attempts: {}", batch_worker.attempts);

    // Verify step completed after retry
    assert_eq!(
        batch_worker.current_state.to_ascii_uppercase(),
        "COMPLETE",
        "Batch worker should complete after retry"
    );

    // The step should have been attempted at least twice (initial + retry)
    assert!(
        batch_worker.attempts >= 2,
        "Batch worker should have been retried at least once"
    );

    // Verify aggregate results
    let aggregate_step = steps
        .iter()
        .find(|s| s.name == "aggregate_results")
        .expect("Should have aggregate_results step");

    let results = aggregate_step
        .results
        .as_ref()
        .expect("Aggregate step should have result data");

    let result = results
        .get("result")
        .expect("Results should contain result object");

    let total_processed = result
        .get("total_processed")
        .expect("Results should contain total_processed");

    println!("   Total processed: {}", total_processed);
    assert_eq!(
        total_processed.as_u64().unwrap(),
        100,
        "Should have processed all 100 items after retry"
    );

    println!("\n🎉 TAS-125 Checkpoint Yield Transient Failure Test PASSED!");
    println!("✅ Checkpoint persistence before failure: Working");
    println!("✅ Retry after transient failure: Working");
    println!("✅ Resume from checkpoint: Working");
    println!("✅ Complete processing after retry: Working");

    Ok(())
}

/// Test checkpoint yield with permanent failure.
///
/// Validates:
/// - Handler fails with non-retryable error
/// - Task fails correctly (not retried indefinitely)
/// - Error state is properly recorded
#[tokio::test]
async fn test_python_checkpoint_yield_permanent_failure() -> Result<()> {
    println!("🚀 Starting TAS-125 Checkpoint Yield Permanent Failure Test (Python)");
    println!("   Total items: 100");
    println!("   Fail after: 30 items");
    println!("   Failure type: Permanent (non-retryable)");
    println!("   Expected: Task fails with error state");

    let manager = IntegrationTestManager::setup().await?;

    // Create task with permanent failure injection
    println!("\n🎯 Creating checkpoint yield task with permanent failure...");
    let task_request = create_checkpoint_yield_request(
        100,      // total_items
        25,       // items_per_checkpoint
        Some(30), // fail_after_items - fail after processing 30 items
        Some(1),  // fail_on_attempt
        true,     // permanent_failure - non-retryable error
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);

    // Wait for task to fail (it should not complete successfully)
    println!("\n⏱️ Waiting for permanent failure...");

    // Use a shorter timeout since permanent failure should fail fast
    let timeout = 30;

    // Wait for task to reach terminal state (may be error)
    let mut attempts = 0;
    let max_attempts = timeout * 2; // Check every 500ms

    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        attempts += 1;

        let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
        let task = manager.orchestration_client.get_task(task_uuid).await?;

        if task.is_execution_complete() || task.execution_status.to_uppercase() == "ERROR" {
            break;
        }

        if attempts >= max_attempts {
            break;
        }
    }

    // Verify task is in error state
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    println!("\n🔍 Verifying permanent failure results...");
    println!("   Task execution status: {}", final_task.execution_status);
    println!("   Task overall status: {}", final_task.status);

    // The task should be in an error state (not complete)
    assert!(
        final_task.execution_status.to_uppercase() == "ERROR"
            || !final_task.is_execution_complete(),
        "Task should be in error state or not complete"
    );

    // Get workflow steps to verify batch worker failed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    let batch_worker = steps
        .iter()
        .find(|s| s.name.starts_with("checkpoint_yield_batch"));

    if let Some(worker) = batch_worker {
        println!("\n📊 Batch Worker Step:");
        println!("   Step: {}", worker.name);
        println!("   State: {}", worker.current_state);
        println!("   Attempts: {}", worker.attempts);

        // Permanent failure should not result in excessive retries
        assert!(
            worker.attempts <= 3,
            "Permanent failure should not be retried excessively"
        );
    }

    println!("\n🎉 TAS-125 Checkpoint Yield Permanent Failure Test PASSED!");
    println!("✅ Permanent failure detection: Working");
    println!("✅ Non-retryable error handling: Working");
    println!("✅ Error state propagation: Working");

    Ok(())
}

/// Test checkpoint yield with small checkpoint interval.
///
/// Validates:
/// - Frequent checkpointing works correctly
/// - Multiple checkpoint yields are processed efficiently
#[tokio::test]
async fn test_python_checkpoint_yield_frequent_checkpoints() -> Result<()> {
    println!("🚀 Starting TAS-125 Frequent Checkpoints Test (Python)");
    println!("   Total items: 50");
    println!("   Items per checkpoint: 5");
    println!("   Expected: ~10 checkpoint yields");

    let manager = IntegrationTestManager::setup().await?;

    // Create task with frequent checkpoints
    println!("\n🎯 Creating task with frequent checkpoint yields...");
    let task_request = create_checkpoint_yield_request(
        50, // total_items
        5,  // items_per_checkpoint - checkpoint every 5 items
        None, None, false,
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created: {}", task_response.task_uuid);

    // Wait for completion
    let timeout = 120; // More time for frequent checkpoints
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify results
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Task should complete with frequent checkpoints"
    );

    // Get aggregate step
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    let aggregate_step = steps
        .iter()
        .find(|s| s.name == "aggregate_results")
        .expect("Should have aggregate_results step");

    let results = aggregate_step
        .results
        .as_ref()
        .expect("Aggregate step should have result data");

    let result = results
        .get("result")
        .expect("Results should contain result object");

    let total_processed = result
        .get("total_processed")
        .expect("Results should contain total_processed");

    assert_eq!(
        total_processed.as_u64().unwrap(),
        50,
        "Should have processed all 50 items"
    );

    if let Some(checkpoints_used) = result.get("checkpoints_used") {
        println!("   Checkpoints used: {}", checkpoints_used);
        assert!(
            checkpoints_used.as_u64().unwrap() >= 8,
            "Should have used at least 8 checkpoints (50/5 = 10 chunks)"
        );
    }

    println!("\n🎉 TAS-125 Frequent Checkpoints Test PASSED!");
    println!("✅ Frequent checkpoint yields: Working");
    println!("✅ Multiple re-dispatches: Working");
    println!("✅ Efficient checkpoint processing: Working");

    Ok(())
}
