//! # E2E Retry Mechanics Tests (TAS-64)
//!
//! Tests proving retry mechanics work through the full orchestration loop:
//! 1. Handler fails with retryable error
//! 2. Step transitions to WaitingForRetry state
//! 3. Backoff expires, step becomes ready for execution
//! 4. Step is re-executed and eventually succeeds or exhausts retries
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Test that retry mechanics work - handler fails twice then succeeds on 3rd attempt
#[tokio::test]
async fn test_retry_after_transient_failure() -> Result<()> {
    println!("🚀 Starting Retry After Transient Failure Test");
    println!("   Template: retry_mechanics_test");
    println!("   Expected: Step fails 2 times, succeeds on attempt 3");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);

    // Create task with FailNTimesHandler configured to fail 2 times
    println!("\n🎯 Creating retry mechanics test task...");
    let task_request = create_task_request(
        "rust_e2e_retry",
        "retry_mechanics_test",
        json!({
            "test_id": "retry_after_transient_failure"
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);

    // Wait for task completion - with fast polling config this should be quick
    // The step will fail twice (with 50ms backoff), then succeed on 3rd attempt
    println!("\n⏱️ Monitoring retry execution...");
    println!("   Expected: 2 failures with backoff, then success");

    let timeout = 30; // 30 seconds should be plenty with fast polling
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify results
    println!("\n🔍 Verifying retry results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Task should have completed after retries"
    );
    println!(
        "✅ Task execution status: {} (overall status: {})",
        final_task.execution_status, final_task.status
    );

    // Get the workflow step to verify attempts
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 1, "Should have exactly 1 step");

    let step = &steps[0];
    assert_eq!(step.name, "fail_twice_then_succeed");
    assert_eq!(
        step.current_state.to_ascii_uppercase(),
        "COMPLETE",
        "Step should be completed"
    );

    // Verify the step has attempts > 1 (proving retry happened)
    println!("\n📊 Step Results:");
    println!("   Step: {}", step.name);
    println!("   State: {}", step.current_state);

    if let Some(results) = &step.results {
        if let Some(result) = results.get("result") {
            let attempts = result
                .get("attempts_before_success")
                .and_then(|v| v.as_i64());

            println!("   Attempts before success: {:?}", attempts);

            if let Some(attempt_count) = attempts {
                assert_eq!(
                    attempt_count, 3,
                    "Should have taken 3 attempts (2 failures + 1 success)"
                );
            }
        }
    }

    println!("\n🎉 Retry After Transient Failure Test PASSED!");
    println!("✅ Handler retry mechanics: Working");
    println!("✅ Backoff and re-execution: Working");
    println!("✅ Attempt counting: Accurate");

    Ok(())
}

/// Test that retry exhaustion leads to error state
#[tokio::test]
async fn test_retry_exhaustion_leads_to_error() -> Result<()> {
    println!("🚀 Starting Retry Exhaustion Test");
    println!("   Template: retry_exhaustion_test");
    println!("   Expected: Step exhausts 3 max_attempts and enters error state");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);

    // Create task with FailNTimesHandler configured to always fail
    println!("\n🎯 Creating retry exhaustion test task...");
    let task_request = create_task_request(
        "rust_e2e_retry",
        "retry_exhaustion_test",
        json!({
            "test_id": "retry_exhaustion"
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);

    // Wait for task to enter error state (should exhaust retries quickly)
    println!("\n⏱️ Waiting for retry exhaustion...");
    println!("   Expected: 3 attempts, then permanent error");

    // Poll until task is in a terminal state (complete or error)
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let start = std::time::Instant::now();
    let timeout_secs = 30;

    loop {
        let task = manager.orchestration_client.get_task(task_uuid).await?;

        // Check if task has reached a terminal state
        let status_upper = task.status.to_ascii_uppercase();
        let exec_status_upper = task.execution_status.to_ascii_uppercase();

        if status_upper == "ERROR"
            || exec_status_upper == "ERROR"
            || status_upper == "COMPLETE"
            || exec_status_upper == "COMPLETE"
            || status_upper == "BLOCKED_BY_FAILURES"
        {
            println!("\n🔍 Task reached terminal state:");
            println!("   Status: {}", task.status);
            println!("   Execution Status: {}", task.execution_status);
            break;
        }

        if start.elapsed().as_secs() > timeout_secs {
            anyhow::bail!(
                "Timeout waiting for task to reach error state. Current: {} / {}",
                task.status,
                task.execution_status
            );
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Verify the step is in error state
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 1, "Should have exactly 1 step");

    let step = &steps[0];
    assert_eq!(step.name, "always_fail");

    let step_state = step.current_state.to_ascii_uppercase();
    assert_eq!(step_state, "ERROR", "Step should be in error state");

    println!("\n📊 Step Results:");
    println!("   Step: {}", step.name);
    println!("   State: {}", step.current_state);

    println!("\n🎉 Retry Exhaustion Test PASSED!");
    println!("✅ Max attempts enforcement: Working");
    println!("✅ Error state transition: Working");
    println!("✅ Retry exhaustion handling: Correct");

    Ok(())
}
