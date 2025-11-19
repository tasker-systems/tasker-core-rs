//! # End-to-End Ruby CSV Batch Processing Workflow Integration Test
//!
//! This integration test validates the TAS-59 batch processing pattern with Ruby handlers:
//! 1. Connecting to running Docker Compose services (postgres, orchestration, worker)
//! 2. Using tasker-client library to create and execute CSV batch processing tasks
//! 3. Testing cursor-based CSV row selection with actual file I/O
//! 4. Validating YAML configuration from workers/ruby/config/task_templates/batch_processing_products_csv.yaml
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! CSV Batch Processing Pattern:
//! 1. analyze_csv (batchable): Counts CSV rows and creates N batch workers with row cursors
//! 2. process_csv_batch_001...N (batch_worker): Parallel workers reading actual CSV rows and calculating inventory metrics
//! 3. aggregate_csv_results (deferred_convergence): Aggregates inventory metrics from all batches
//!
//! NOTE: This test is language-agnostic and uses the tasker-client API. It does NOT reference
//! Ruby code or handlers directly - ensuring the system works correctly regardless of worker
//! implementation language.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create CSV batch processing task request
///
/// Creates a task request for the Ruby CSV processing workflow template.
/// Uses language-agnostic parameters that work regardless of handler implementation.
fn create_csv_processing_request(
    csv_file_path: &str,
    analysis_mode: &str,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "csv_processing",
        "csv_product_inventory_analyzer_ruby",
        json!({
            "csv_file_path": csv_file_path,
            "analysis_mode": analysis_mode
        }),
    )
}

#[tokio::test]
async fn test_csv_batch_processing_with_ruby_handlers() -> Result<()> {
    println!("🚀 Starting CSV Batch Processing with Ruby Handlers Test");
    println!("   Processing: /app/tests/fixtures/products.csv (1000 data rows)");
    println!("   Expected: 5 batch workers processing 200 rows each");
    println!("   Template: csv_product_inventory_analyzer_ruby");
    println!("   Namespace: csv_processing");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // E2E tests always run against Docker services, so use Docker path
    // The Docker worker container has fixtures mounted at /app/tests/fixtures
    let fixture_base = std::env::var("TASKER_FIXTURE_PATH")
        .unwrap_or_else(|_| "/app/tests/fixtures".to_string());
    let csv_file_path = format!("{}/products.csv", fixture_base);

    println!("\n📄 CSV file path: {}", csv_file_path);
    println!("   Fixture base: {}", fixture_base);
    println!("   NOTE: This test requires the products.csv fixture file to exist");

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
    println!("   This may take longer due to file I/O and Ruby handler execution");
    // CSV processing with file I/O and Ruby handlers needs more time
    let timeout = 30; // 30 seconds for 1000 rows with 5 workers + file I/O + Ruby execution
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
        step_names.contains(&"analyze_csv".to_string()),
        "Should have analyze_csv step"
    );
    assert!(
        step_names.contains(&"aggregate_csv_results".to_string()),
        "Should have aggregate_csv_results step"
    );

    // Count batch worker steps (should have process_csv_batch_001 through process_csv_batch_005)
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

    // Get the aggregate_csv_results step to verify CSV processing worked
    let aggregate_step = steps
        .iter()
        .find(|s| s.name == "aggregate_csv_results")
        .expect("Should have aggregate_csv_results step");

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

    println!("\n🎉 CSV Batch Processing with Ruby Handlers Test PASSED!");
    println!("✅ CSV analysis (row counting): Working");
    println!("✅ Batch worker creation (5 workers): Working");
    println!("✅ Parallel CSV row processing (200 rows each): Working");
    println!("✅ Real file I/O (cursor-based row selection): Working");
    println!("✅ Inventory metrics calculation: Working");
    println!("✅ Deferred convergence aggregation: Working");
    println!("✅ Ruby handlers (batch_processing_products_csv): Working");

    Ok(())
}

#[tokio::test]
async fn test_no_batches_scenario() -> Result<()> {
    println!("🚀 Starting NoBatches Scenario Test");
    println!("   Testing: Empty CSV file handling");
    println!("   Expected: No batch workers created");
    println!("   Template: csv_product_inventory_analyzer_ruby");

    let manager = IntegrationTestManager::setup().await?;

    // Use TASKER_FIXTURE_PATH env var for flexible fixture location
    let fixture_base =
        std::env::var("TASKER_FIXTURE_PATH").unwrap_or_else(|_| "tests/fixtures".to_string());
    let empty_csv_path = format!("{}/products_empty.csv", fixture_base);

    println!("\n📄 Creating task with empty CSV path...");
    println!("   Path: {}", empty_csv_path);
    println!("   Fixture base: {}", fixture_base);
    println!("   NOTE: This tests the NoBatches scenario where no rows are found");

    // Create task with empty CSV path
    // The handler should detect no rows and skip batch worker creation
    let task_request = create_csv_processing_request(&empty_csv_path, "inventory");

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);

    // Wait for completion (should be fast since no batches to process)
    println!("\n⏱️ Waiting for task completion...");
    let timeout = 10; // Shorter timeout since no batches
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

    // The workflow should still have analyze_csv (which discovers 0 batches)
    assert!(
        step_names.contains(&"analyze_csv".to_string()),
        "Should have analyze_csv step"
    );

    println!("\n🎉 NoBatches Scenario Test PASSED!");
    println!("✅ Empty CSV detection: Working");
    println!("✅ No batch workers created: Correct");
    println!("✅ Graceful handling of zero batches: Working");
    println!("✅ Ruby batch processing logic: Working");

    Ok(())
}
