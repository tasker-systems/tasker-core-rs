//! # End-to-End Rust Worker Integration Test
//!
//! This integration test validates the complete Tasker orchestration system by:
//! 1. Connecting to running Docker Compose services (postgres, orchestration, worker)
//! 2. Using tasker-client library to create and execute tasks programmatically
//! 3. Testing real end-to-end workflow execution with step handlers from workers/rust/
//! 4. Validating YAML configurations from workers/rust/config/tasks/
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! This replicates the CLI command:
//! `cargo run --bin tasker-cli task create --namespace linear_workflow --name mathematical_sequence --input '{"even_number": 8}'`

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};
use tasker_shared::models::core::task::TaskListQuery;

#[tokio::test]
async fn test_end_to_end_linear_workflow_with_rust_worker() -> Result<()> {
    println!("🚀 Starting End-to-End Rust Worker Integration Test");

    // Simple setup using DockerIntegrationManager - assumes services are running
    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

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

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Status: {}", task_response.status);
    println!("   Steps: {}", task_response.step_count);
    println!("   Expected pattern: Step handlers from workers/rust/src/step_handlers");

    // Step 7: Monitor task execution
    println!("\n⏱️ Step 7: Monitoring task execution...");

    // Wait for task completion with timeout
    wait_for_task_completion(&manager.orchestration_client, &task_response.task_uuid, 5).await?;

    // Step 8: Verify final results
    println!("\n🔍 Step 8: Verifying execution results...");

    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

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
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify we have workflow steps (exact count depends on configuration)
    assert!(!steps.is_empty(), "Should have workflow steps");

    // Verify all steps completed successfully
    for (i, step) in steps.iter().enumerate() {
        assert_eq!(
            step.current_state.to_ascii_uppercase(),
            "COMPLETE",
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

    Ok(())
}

/// Test orchestration API functionality without full workflow execution
#[tokio::test]
async fn test_orchestration_api_with_docker_compose() -> Result<()> {
    println!("🔧 Testing Orchestration API with Docker Compose");

    // Setup using DockerIntegrationManager (orchestration only)
    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    // Test 1: Health check
    let health = manager.orchestration_client.get_basic_health().await?;
    assert_eq!(health.status, "healthy");
    println!("✅ Health check passed: {}", health.status);

    // Test 2: Task creation API
    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence",
        json!({"even_number": 6}),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;
    assert!(!task_response.task_uuid.is_empty());
    println!("✅ Task creation API working: {}", task_response.task_uuid);

    // Test 3: Task retrieval API
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let retrieved_task = manager.orchestration_client.get_task(task_uuid).await?;
    assert_eq!(
        retrieved_task.task_uuid.to_string(),
        task_response.task_uuid
    );
    println!("✅ Task retrieval API working");

    // Test 4: Task listing API
    let query = TaskListQuery::default();
    let task_list = manager.orchestration_client.list_tasks(&query).await?;
    assert!(!task_list.tasks.is_empty());
    println!(
        "✅ Task listing API working: {} tasks found",
        task_list.tasks.len()
    );

    println!("\n🎉 API Integration Test PASSED!");
    println!("✅ Docker Compose PostgreSQL: Working");
    println!("✅ Docker Compose Orchestration: Working");
    println!("✅ tasker-client API integration: Working");

    Ok(())
}
