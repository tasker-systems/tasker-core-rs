//! # End-to-End Rust Worker Integration Test
//!
//! This integration test validates the complete Tasker orchestration system by:
//! 1. Using ImageFromDockerfile to build and start PostgreSQL, orchestration, and rust worker containers
//! 2. Using tasker-client library to create and execute tasks programmatically
//! 3. Testing real end-to-end workflow execution with step handlers from workers/rust/
//! 4. Validating YAML configurations from workers/rust/config/tasks/
//!
//! This replicates the CLI command:
//! `cargo run --bin tasker-cli task create --namespace linear_workflow --name mathematical_sequence --input '{"even_number": 8}'`

use anyhow::Result;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

use tasker_client::OrchestrationApiClient;
use tasker_core::test_helpers::{ApiOnlyTestSuite, IntegrationTestSuite};
use tasker_shared::models::core::{task::TaskListQuery, task_request::TaskRequest};
use tasker_shared::models::orchestration::execution_status::ExecutionStatus;

/// Helper to create a TaskRequest matching CLI usage
fn create_task_request(
    namespace: &str,
    name: &str,
    input_context: serde_json::Value,
) -> TaskRequest {
    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        context: input_context,
        status: "PENDING".to_string(),
        initiator: "integration-test".to_string(),
        source_system: "test".to_string(),
        reason: "Integration test execution".to_string(),
        complete: false,
        tags: vec!["integration-test".to_string()],
        bypass_steps: vec![],
        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
    }
}

/// Wait for task completion by polling task status
async fn wait_for_task_completion(
    client: &OrchestrationApiClient,
    task_uuid: &str,
    max_wait_seconds: u64,
) -> Result<()> {
    let start_time = std::time::Instant::now();
    let max_duration = Duration::from_secs(max_wait_seconds);

    println!(
        "⏳ Waiting for task {} to complete (max {}s)...",
        task_uuid, max_wait_seconds
    );

    while start_time.elapsed() < max_duration {
        let uuid = Uuid::parse_str(task_uuid)?;
        match client.get_task(uuid).await {
            Ok(task_response) => {
                let execution_status = task_response.execution_status_typed();
                println!(
                    "   Task execution status: {} ({})",
                    task_response.execution_status, task_response.status
                );

                match execution_status {
                    ExecutionStatus::AllComplete => {
                        println!("✅ Task completed successfully!");
                        return Ok(());
                    }
                    ExecutionStatus::BlockedByFailures => {
                        return Err(anyhow::anyhow!(
                            "Task blocked by failures that cannot be retried: {}",
                            task_response.execution_status
                        ));
                    }
                    ExecutionStatus::HasReadySteps
                    | ExecutionStatus::Processing
                    | ExecutionStatus::WaitingForDependencies => {
                        // Still processing, continue polling
                        sleep(Duration::from_secs(2)).await;
                    }
                }
            }
            Err(e) => {
                println!("   Error polling task status: {}", e);
                sleep(Duration::from_secs(2)).await;
            }
        }
    }

    Err(anyhow::anyhow!(
        "Task did not complete within {}s",
        max_wait_seconds
    ))
}

#[tokio::test]
async fn test_end_to_end_linear_workflow_with_rust_worker() -> Result<()> {
    println!("🚀 Starting End-to-End Rust Worker Integration Test with ImageFromDockerfile");

    // One-line setup using IntegrationTestSuite helper
    let suite = IntegrationTestSuite::setup_with_worker_id("test-worker-001").await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Database: {}", suite.database_url);
    println!("   Orchestration: {}", suite.orchestration_url);
    println!("   Worker: {} (ID: {})", suite.worker_url, suite.worker_id);

    // Step 6: Create linear workflow task (equivalent to CLI command)
    println!("\n🎯 Step 6: Creating linear workflow task...");
    println!("   Equivalent CLI: cargo run --bin tasker-cli task create \\");
    println!("     --namespace linear_workflow \\");
    println!("     --name mathematical_sequence \\");
    println!("     --input '{{\"even_number\": 8}}'");

    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence",
        json!({
            "even_number": 8  // This will be processed through the linear workflow steps
        }),
    );

    let task_response = suite.client.create_task(task_request).await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Status: {}", task_response.status);
    println!("   Steps: {}", task_response.step_count);
    println!("   Expected pattern: Step handlers from workers/rust/src/step_handlers");

    // Step 7: Monitor task execution
    println!("\n⏱️ Step 7: Monitoring task execution...");

    // Wait for task completion with timeout
    wait_for_task_completion(&suite.client, &task_response.task_uuid, 120).await?;

    // Step 8: Verify final results
    println!("\n🔍 Step 8: Verifying execution results...");

    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = suite.client.get_task(task_uuid).await?;

    // Verify the task completed successfully via execution status
    assert!(
        final_task.is_execution_complete(),
        "Task execution should be complete"
    );
    println!(
        "✅ Task execution status: {} (overall status: {})",
        final_task.execution_status, final_task.status
    );

    // Get the workflow steps to verify step-by-step execution
    let steps = suite.client.list_task_steps(task_uuid).await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify we have workflow steps (exact count depends on configuration)
    assert!(!steps.is_empty(), "Should have workflow steps");

    // Verify all steps completed successfully
    for (i, step) in steps.iter().enumerate() {
        assert_eq!(
            step.current_state,
            "COMPLETED",
            "Step {} ({}) should be completed",
            i + 1,
            step.name
        );
        println!(
            "   ✅ Step {}: {} - {}",
            i + 1,
            step.name,
            step.current_state
        );
    }

    // Verify the final result if available
    if let Some(final_step) = steps.last() {
        if let Some(result_data) = &final_step.results {
            println!(
                "✅ Final step results: {}",
                serde_json::to_string_pretty(result_data)?
            );
        }
    }

    println!("\n🎉 End-to-End Integration Test PASSED!");
    println!("✅ PostgreSQL container with PGMQ: Working");
    println!("✅ Orchestration service (ImageFromDockerfile): Working");
    println!("✅ Rust worker (ImageFromDockerfile): Working");
    println!("✅ tasker-client API integration: Working");
    println!("✅ Linear workflow execution: Working");
    println!("✅ Step handlers from workers/rust/src/step_handlers: Working");
    println!("✅ YAML config from workers/rust/config/tasks: Working");
    println!("✅ End-to-end task lifecycle: Working");
    println!("✅ Centralized Docker structure: Working");

    Ok(())
}

/// Test orchestration API functionality without full workflow execution
#[tokio::test]
async fn test_orchestration_api_with_image_from_dockerfile() -> Result<()> {
    println!("🔧 Testing Orchestration API with ImageFromDockerfile");

    // One-line setup using ApiOnlyTestSuite helper (no worker needed)
    let suite = ApiOnlyTestSuite::setup().await?;

    // Test 1: Health check
    let health = suite.client.get_basic_health().await?;
    assert_eq!(health.status, "healthy");
    println!("✅ Health check passed: {}", health.status);

    // Test 2: Task creation API
    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence",
        json!({"even_number": 6}),
    );

    let task_response = suite.client.create_task(task_request).await?;
    assert!(!task_response.task_uuid.is_empty());
    println!("✅ Task creation API working: {}", task_response.task_uuid);

    // Test 3: Task retrieval API
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let retrieved_task = suite.client.get_task(task_uuid).await?;
    assert_eq!(
        retrieved_task.task_uuid.to_string(),
        task_response.task_uuid
    );
    println!("✅ Task retrieval API working");

    // Test 4: Task listing API
    let query = TaskListQuery::default();
    let task_list = suite.client.list_tasks(&query).await?;
    assert!(task_list.tasks.len() >= 1);
    println!(
        "✅ Task listing API working: {} tasks found",
        task_list.tasks.len()
    );

    println!("\n🎉 API Integration Test PASSED!");
    println!("✅ ImageFromDockerfile PostgreSQL: Working");
    println!("✅ ImageFromDockerfile Orchestration: Working");
    println!("✅ tasker-client API integration: Working");

    Ok(())
}
