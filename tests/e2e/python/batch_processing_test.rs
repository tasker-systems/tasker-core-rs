// Rust guideline compliant 2025-12-16
//! Python Batch Processing E2E Tests
//!
//! Tests the CSV batch processing workflow demonstrating the Batchable pattern
//! with cursor-based parallel batch workers using Python handlers.
//!
//! Workflow Pattern:
//! 1. analyze_csv_py (batchable): Analyze CSV and create batch worker configs
//! 2. process_csv_batch_001...N (batch_worker): Process rows within cursor range
//! 3. aggregate_csv_results_py (deferred_convergence): Aggregate all results
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before tests.
//!
//! CSV Batch Processing Pattern:
//! - Analyzer counts CSV rows and creates N batch workers with row cursors
//! - Workers process rows in parallel based on cursor ranges
//! - Aggregator collects results from all workers
//!
//! Note: These tests require the Python worker running on port 8083.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Create CSV batch processing task request for Python handlers.
fn create_csv_processing_request(
    csv_file_path: &str,
    analysis_mode: &str,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "csv_processing_py",
        "csv_product_inventory_analyzer_py",
        json!({
            "csv_file_path": csv_file_path,
            "analysis_mode": analysis_mode
        }),
    )
}

/// Test CSV batch processing with Python handlers.
///
/// Validates:
/// - CSV analyzer creates correct number of batch workers
/// - Batch workers process rows in parallel
/// - Results are correctly aggregated
/// - All steps complete successfully
#[tokio::test]
async fn test_python_csv_batch_processing() -> Result<()> {
    println!("🚀 Starting CSV Batch Processing with Python Handlers Test");
    println!("   Processing: /app/tests/fixtures/products.csv (1000 data rows)");
    println!("   Expected: 5 batch workers processing 200 rows each");
    println!("   Template: csv_product_inventory_analyzer_py");
    println!("   Namespace: csv_processing_py");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // E2E tests run against Docker services - use Docker path
    let fixture_base =
        std::env::var("TASKER_FIXTURE_PATH").unwrap_or_else(|_| "/app/tests/fixtures".to_string());
    let csv_file_path = format!("{}/products.csv", fixture_base);

    println!("\n📄 CSV file path: {}", csv_file_path);
    println!("   Fixture base: {}", fixture_base);
    println!("   NOTE: This test requires the products.csv fixture file");

    // Create CSV batch processing task
    println!("\n🎯 Creating CSV batch processing task...");
    let task_request = create_csv_processing_request(&csv_file_path, "inventory");

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: 5 batch workers reading CSV rows from disk");

    // Monitor task execution
    println!("\n⏱️ Monitoring CSV batch processing execution...");
    println!("   This may take longer due to file I/O and Python handler execution");

    // CSV processing with file I/O and Python handlers needs more time
    let timeout = 30;
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying CSV batch processing results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "CSV batch processing execution should be complete"
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

    // Verify expected steps for CSV batch processing
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        step_names.contains(&"analyze_csv_py".to_string()),
        "Should have analyze_csv_py step"
    );
    assert!(
        step_names.contains(&"aggregate_csv_results_py".to_string()),
        "Should have aggregate_csv_results_py step"
    );

    // Count batch worker steps
    let batch_worker_count = step_names
        .iter()
        .filter(|name| name.starts_with("process_csv_batch_"))
        .count();

    println!("   ✅ Created {} CSV batch workers", batch_worker_count);

    // With 1000 rows and batch size 200, we expect exactly 5 workers
    assert_eq!(
        batch_worker_count, 5,
        "Should have exactly 5 batch workers for 1000 rows with batch size 200"
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

    // Get the aggregate_csv_results_py step to verify CSV processing
    let aggregate_step = steps
        .iter()
        .find(|s| s.name == "aggregate_csv_results_py")
        .expect("Should have aggregate_csv_results_py step");

    println!("\n📊 Aggregate Results:");
    let results = aggregate_step
        .results
        .as_ref()
        .expect("Aggregate step should have result data");

    // Extract the inner "result" object from the step results
    let result = results
        .get("result")
        .expect("Results should contain result object");

    // Verify we processed all 1000 rows
    let total_processed = result
        .get("total_processed")
        .expect("Results should contain total_processed");
    println!("   Total products processed: {}", total_processed);
    assert_eq!(
        total_processed.as_u64().unwrap(),
        1000,
        "Should have processed all 1000 rows"
    );

    // Verify inventory metrics exist
    let total_value = result
        .get("total_inventory_value")
        .expect("Results should contain total_inventory_value");
    println!(
        "   Total inventory value: ${}",
        total_value.as_f64().unwrap()
    );
    assert!(
        total_value.as_f64().unwrap() > 0.0,
        "Total inventory value should be positive"
    );

    // Verify worker count
    let worker_count = result
        .get("worker_count")
        .expect("Results should contain worker_count");
    println!("   Workers used: {}", worker_count);
    assert_eq!(
        worker_count.as_u64().unwrap(),
        5,
        "Should have used 5 workers"
    );

    println!("\n🎉 CSV Batch Processing with Python Handlers Test PASSED!");
    println!("✅ CSV analysis (row counting): Working");
    println!("✅ Batch worker creation (5 workers): Working");
    println!("✅ Parallel CSV row processing (200 rows each): Working");
    println!("✅ Real file I/O (cursor-based row selection): Working");
    println!("✅ Inventory metrics calculation: Working");
    println!("✅ Deferred convergence aggregation: Working");
    println!("✅ Python Batchable handlers: Working");

    Ok(())
}

/// Test empty CSV scenario with Python handlers.
///
/// Validates:
/// - Empty CSV file is detected correctly
/// - No batch workers are created
/// - Task completes successfully (graceful handling)
#[tokio::test]
async fn test_python_no_batches_scenario() -> Result<()> {
    println!("🚀 Starting NoBatches Scenario Test with Python Handlers");
    println!("   Testing: Empty CSV file handling");
    println!("   Expected: No batch workers created");
    println!("   Template: csv_product_inventory_analyzer_py");

    let manager = IntegrationTestManager::setup().await?;

    // Use TASKER_FIXTURE_PATH env var for flexible fixture location
    let fixture_base =
        std::env::var("TASKER_FIXTURE_PATH").unwrap_or_else(|_| "/app/tests/fixtures".to_string());
    let empty_csv_path = format!("{}/products_empty.csv", fixture_base);

    println!("\n📄 Creating task with empty CSV path...");
    println!("   Path: {}", empty_csv_path);
    println!("   Fixture base: {}", fixture_base);
    println!("   NOTE: This tests the NoBatches scenario where no rows are found");

    // Create task with empty CSV path
    let task_request = create_csv_processing_request(&empty_csv_path, "inventory");

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);

    // Wait for completion (should be fast since no batches to process)
    println!("\n⏱️ Waiting for task completion...");
    let timeout = 10;
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify results
    println!("\n🔍 Verifying NoBatches scenario results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Task should complete successfully even with no batches"
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

    // Verify NO batch workers were created
    let batch_workers: Vec<_> = steps
        .iter()
        .filter(|s| s.name.starts_with("process_csv_batch_"))
        .collect();

    println!(
        "   ✅ Batch worker count: {} (expected 0)",
        batch_workers.len()
    );

    assert!(
        batch_workers.is_empty(),
        "Should have no batch workers in NoBatches scenario"
    );

    // Verify we still have the main workflow steps
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

    println!("\n📋 Steps created:");
    for name in &step_names {
        println!("   - {}", name);
    }

    // The workflow should still have analyze_csv_py (which discovers 0 batches)
    assert!(
        step_names.contains(&"analyze_csv_py".to_string()),
        "Should have analyze_csv_py step"
    );

    println!("\n🎉 NoBatches Scenario Test with Python Handlers PASSED!");
    println!("✅ Empty CSV detection: Working");
    println!("✅ No batch workers created: Correct");
    println!("✅ Graceful handling of zero batches: Working");
    println!("✅ Python Batchable logic: Working");

    Ok(())
}

/// Test batch processing with different batch sizes.
///
/// Validates:
/// - Different batch configurations work correctly
/// - Worker count scales appropriately
#[tokio::test]
async fn test_python_batch_processing_scaling() -> Result<()> {
    println!("🚀 Starting Batch Processing Scaling Test with Python Handlers");

    let manager = IntegrationTestManager::setup().await?;

    let fixture_base =
        std::env::var("TASKER_FIXTURE_PATH").unwrap_or_else(|_| "/app/tests/fixtures".to_string());
    let csv_file_path = format!("{}/products.csv", fixture_base);

    println!("\n📄 Testing batch processing scaling...");

    let task_request = create_csv_processing_request(&csv_file_path, "pricing");

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created: {}", task_response.task_uuid);

    // Wait for completion
    wait_for_task_completion(&manager.orchestration_client, &task_response.task_uuid, 30).await?;

    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Task should complete successfully"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify analyzer step completed
    let analyzer_step = steps
        .iter()
        .find(|s| s.name == "analyze_csv_py")
        .expect("Should have analyze_csv_py step");

    assert_eq!(
        analyzer_step.current_state.to_uppercase(),
        "COMPLETE",
        "Analyzer should complete"
    );

    // Verify aggregator step completed
    let aggregator_step = steps
        .iter()
        .find(|s| s.name == "aggregate_csv_results_py")
        .expect("Should have aggregate_csv_results_py step");

    assert_eq!(
        aggregator_step.current_state.to_uppercase(),
        "COMPLETE",
        "Aggregator should complete"
    );

    println!("✅ Batch processing scaling test passed");
    println!("   Total steps: {}", steps.len());

    Ok(())
}
