//! # Integration Test Suite Demo
//!
//! Demonstrates how to use the new IntegrationTestSuite helper for simplified testing

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use tasker_core::test_helpers::{ApiOnlyTestSuite, IntegrationTestSuite};
use tasker_shared::models::core::{task::TaskListQuery, task_request::TaskRequest};

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

/// Demo of the full integration test suite
#[tokio::test]
async fn demo_full_integration_test_suite() -> Result<()> {
    // One line setup - handles PostgreSQL, orchestration, worker, and client!
    let suite = IntegrationTestSuite::setup().await?;

    println!("\n🎯 Testing task creation with full environment...");

    // Create a simple task
    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence",
        json!({"even_number": 8}),
    );

    let task_response = suite.client.create_task(task_request).await?;

    println!("✅ Task created: {}", task_response.task_uuid);
    println!("   Status: {}", task_response.status);
    println!("   Steps: {}", task_response.step_count);

    // Verify we can retrieve the task
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let retrieved_task = suite.client.get_task(task_uuid).await?;
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
    // Faster setup - just PostgreSQL and orchestration service
    let suite = ApiOnlyTestSuite::setup().await?;

    println!("\n🔧 Testing API functionality...");

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

    println!("🎉 API-only integration test suite demo passed!");

    Ok(())
}

/// Demo of custom worker ID setup
#[tokio::test]
#[ignore] // Only run when Docker is available
async fn demo_custom_worker_setup() -> Result<()> {
    // Setup with custom worker ID
    let suite = IntegrationTestSuite::setup_with_worker_id("custom-worker-123").await?;

    println!("✅ Custom worker setup complete");
    println!("   Worker ID: {}", suite.worker_id);
    println!("   Worker URL: {}", suite.worker_url);

    assert_eq!(suite.worker_id, "custom-worker-123");

    // Test basic functionality
    let health = suite.client.get_basic_health().await?;
    assert_eq!(health.status, "healthy");

    println!("🎉 Custom worker integration test demo passed!");

    Ok(())
}
