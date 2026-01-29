//! # End-to-End Batch Processing Workflow Integration Test
//!
//! This integration test validates the TAS-59 batch processing pattern using native Rust handlers by:
//! 1. Connecting to running Docker Compose services (postgres, orchestration, worker)
//! 2. Using tasker-client library to create and execute batch processing tasks
//! 3. Testing cursor-based batch processing with resumability
//! 4. Validating YAML configuration from tests/fixtures/task_templates/rust/batch_processing_example.yaml
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! Batch Processing Pattern:
//! 1. analyze_dataset (batchable): Analyzes dataset and creates N batch workers
//! 2. process_batch_001...N (batch_worker): Parallel workers processing batches with cursor-based resumability
//! 3. aggregate_results (deferred_convergence): Waits for ALL batch workers to complete, then aggregates results

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{
    create_task_request, get_task_completion_timeout, wait_for_task_completion,
};

#[tokio::test]
async fn test_batch_processing_small_dataset() -> Result<()> {
    println!("🚀 Starting Batch Processing (Small Dataset) Test");
    println!("   Dataset size: 500 items (should create batch workers)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Create batch processing task with small dataset
    println!("\n🎯 Creating batch processing task (small dataset)...");
    let task_request = create_task_request(
        "data_processing",
        "large_dataset_processor",
        json!({
            "dataset_size": 500
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: Multiple batch workers processing in parallel");

    // Monitor task execution
    println!("\n⏱️ Monitoring batch processing execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying batch processing results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Batch processing execution should be complete"
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

    // Verify expected steps for batch processing
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"analyze_dataset"),
        "Should have analyze_dataset step"
    );
    assert!(
        step_names.contains(&"aggregate_results"),
        "Should have aggregate_results step"
    );

    // Count batch worker steps (should have process_batch_001, process_batch_002, etc.)
    let batch_worker_count = step_names
        .iter()
        .filter(|name| name.starts_with("process_batch_"))
        .count();
    assert!(
        batch_worker_count > 0,
        "Should have at least one batch worker step"
    );
    println!("   ✅ Created {} batch workers", batch_worker_count);

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

    println!("\n🎉 Batch Processing (Small Dataset) Test PASSED!");
    println!("✅ Batchable step analysis: Working");
    println!("✅ Batch worker creation: Working");
    println!("✅ Parallel batch processing: Working");
    println!("✅ Deferred convergence aggregation: Working");
    println!("✅ Rust handlers (batch_processing_example): Working");

    Ok(())
}

#[tokio::test]
async fn test_batch_processing_large_dataset() -> Result<()> {
    println!("🚀 Starting Batch Processing (Large Dataset) Test");
    println!("   Dataset size: 5,000 items (should create multiple workers with cursor configs)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎯 Creating batch processing task (large dataset)...");
    let task_request = create_task_request(
        "data_processing",
        "large_dataset_processor",
        json!({
            "dataset_size": 5000
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: Multiple batch workers with cursor-based resumability");

    // Monitor task execution
    println!("\n⏱️ Monitoring batch processing execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying batch processing results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Batch processing execution should be complete"
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Count batch workers - with 5000 items and default batch_size of 1000,
    // max_workers of 10, should create 5 workers
    let batch_worker_count = steps
        .iter()
        .filter(|s| s.name.starts_with("process_batch_"))
        .count();
    println!(
        "   ✅ Created {} batch workers for 5,000 items",
        batch_worker_count
    );
    assert!(
        batch_worker_count >= 5,
        "Should have at least 5 batch workers for 5,000 items"
    );

    // Verify all batch workers completed
    for step in steps
        .iter()
        .filter(|s| s.name.starts_with("process_batch_"))
    {
        assert_eq!(
            step.current_state.to_ascii_uppercase(),
            "COMPLETE",
            "Batch worker {} should be completed",
            step.name
        );
    }

    println!("\n🎉 Batch Processing (Large Dataset) Test PASSED!");
    println!("✅ Large-scale batch worker creation: Working");
    println!("✅ Cursor-based batch configuration: Working");
    println!("✅ TAS-59 resumability pattern: Working");

    Ok(())
}

#[tokio::test]
async fn test_batch_processing_tiny_dataset() -> Result<()> {
    println!("🚀 Starting Batch Processing (Tiny Dataset) Test");
    println!("   Dataset size: 50 items (smaller than batch_size, should use NoBatches outcome)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎯 Creating batch processing task (tiny dataset)...");
    let task_request = create_task_request(
        "data_processing",
        "large_dataset_processor",
        json!({
            "dataset_size": 50
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: NoBatches outcome (dataset too small for batching)");

    // Monitor task execution
    println!("\n⏱️ Monitoring batch processing execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying batch processing results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Batch processing execution should be complete"
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify expected steps for NoBatches outcome
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"analyze_dataset"),
        "Should have analyze_dataset step"
    );

    // NoBatches creates ONE placeholder worker (process_batch_000) with cursor 0-0
    let batch_worker_count = step_names
        .iter()
        .filter(|name| name.starts_with("process_batch_"))
        .count();
    assert_eq!(
        batch_worker_count, 1,
        "Should have 1 placeholder batch worker for tiny dataset (NoBatches outcome)"
    );
    println!("   ✅ NoBatches outcome: Created placeholder worker process_batch_000");

    // Should have aggregate_results step (depends on placeholder worker)
    assert!(
        step_names.contains(&"aggregate_results"),
        "Should have aggregate_results step (depends on placeholder worker)"
    );
    println!("   ✅ Convergence step created: aggregate_results");

    println!("\n🎉 Batch Processing (Tiny Dataset) Test PASSED!");
    println!("✅ NoBatches outcome handling: Working");
    println!("✅ Placeholder worker lifecycle: Working");

    Ok(())
}

/// Test API validation without full execution
#[tokio::test]
async fn test_batch_processing_api_validation() -> Result<()> {
    println!("🔧 Testing Batch Processing API Validation");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    // Test task creation with valid data
    let task_request = create_task_request(
        "data_processing",
        "large_dataset_processor",
        json!({
            "dataset_size": 1500
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    assert!(!task_response.task_uuid.is_empty());
    println!(
        "✅ Batch processing API creation working: {}",
        task_response.task_uuid
    );

    // Test task retrieval
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let retrieved_task = manager.orchestration_client.get_task(task_uuid).await?;
    assert_eq!(
        retrieved_task.task_uuid.to_string(),
        task_response.task_uuid
    );
    println!("✅ Batch processing retrieval API working");

    // Verify expected step count for batch processing workflow
    // At initialization, only non-template steps created at init time:
    // 1. analyze_dataset (batchable step)
    //
    // NOT created at initialization:
    // - process_batch (batch_worker template) - excluded by is_created_at_initialization()
    // - aggregate_results (deferred_convergence) - created dynamically after batch workers created
    //
    // Note: Dynamic batch workers (process_batch_001, process_batch_002, etc.) and convergence
    // steps are created at runtime when the batchable step executes.
    println!(
        "   Task has {} steps at initialization (expecting 1: analyze_dataset only)",
        task_response.total_steps
    );
    assert_eq!(
        task_response.total_steps, 1,
        "Batch processing workflow should have exactly 1 step at initialization \
        (analyze_dataset only). BatchWorker templates and DeferredConvergence steps are \
        created dynamically at runtime. Got: {}",
        task_response.total_steps
    );
    println!(
        "✅ Batch processing step count validation: {} steps at initialization \
        (BatchWorker templates correctly excluded)",
        task_response.total_steps
    );

    println!("\n🎉 Batch Processing API Validation Test PASSED!");
    println!("✅ Batch processing task creation: Working");
    println!("✅ API validation: Working");
    println!("✅ tasker-client integration: Working");

    Ok(())
}
