//! # Integration Test Suite Demo
//!
//! Demonstrates how to use the new DockerIntegrationManager for simplified testing.
//! Assumes Docker Compose services are already running.
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::create_task_request;
use tasker_shared::models::core::task::TaskListQuery;

/// Demo of the full integration test suite with worker
#[tokio::test]
async fn demo_full_integration_test_suite() -> Result<()> {
    // Simple setup - connects to running Docker Compose services
    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎯 Testing task creation with full environment...");

    // Create a simple task
    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence",
        json!({"even_number": 8}),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created: {}", task_response.task_uuid);
    println!("   Status: {}", task_response.status);
    println!("   Steps: {}", task_response.step_count);

    // Verify we can retrieve the task
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let retrieved_task = manager.orchestration_client.get_task(task_uuid).await?;
    assert_eq!(
        retrieved_task.task_uuid.to_string(),
        task_response.task_uuid
    );

    println!("✅ Task retrieval working");
    println!("🎉 Full integration test suite demo passed!");

    Ok(())
}

/// Demo of the API-only test suite (faster, no worker needed)
#[tokio::test]
async fn demo_api_only_test_suite() -> Result<()> {
    // Faster setup - just connects to orchestration service
    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    println!("\n🔧 Testing API functionality...");

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

    println!("🎉 API-only integration test suite demo passed!");

    Ok(())
}

/// Demo of Docker Compose environment configuration
#[tokio::test]
async fn demo_custom_environment_setup() -> Result<()> {
    // This test demonstrates how to use custom environment variables
    // to connect to different service endpoints
    println!("🔧 Testing custom environment configuration...");

    // The DockerIntegrationManager will read from environment variables:
    // TASKER_TEST_ORCHESTRATION_URL (default: http://localhost:8080)
    // TASKER_TEST_WORKER_URL (default: http://localhost:8081)
    let manager = IntegrationTestManager::setup().await?;

    println!("✅ Connected to services:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Test basic functionality
    let health = manager.orchestration_client.get_basic_health().await?;
    assert_eq!(health.status, "healthy");

    // Display configuration info
    manager.display_info();

    println!("🎉 Custom environment integration test demo passed!");

    Ok(())
}
