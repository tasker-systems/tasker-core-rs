//! # End-to-End CSV Batch Processing Workflow Integration Test
//!
//! This integration test validates the TAS-59 batch processing pattern with real CSV file processing by:
//! 1. Connecting to running Docker Compose services (postgres, orchestration, worker)
//! 2. Using tasker-client library to create and execute CSV batch processing tasks
//! 3. Testing cursor-based CSV row selection with actual file I/O
//! 4. Validating YAML configuration from tests/fixtures/task_templates/rust/batch_processing_products_csv.yaml
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! CSV Batch Processing Pattern:
//! 1. analyze_csv (batchable): Counts CSV rows and creates N batch workers with row cursors
//! 2. process_csv_batch_001...N (batch_worker): Parallel workers reading actual CSV rows and calculating inventory metrics
//! 3. aggregate_csv_results (deferred_convergence): Aggregates inventory metrics from all batches

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

#[tokio::test]
async fn test_csv_batch_processing_products() -> Result<()> {
    println!("🚀 Starting CSV Batch Processing (Products) Test");
    println!("   Processing: tests/fixtures/products.csv (1000 data rows)");
    println!("   Expected: 5 batch workers processing 200 rows each");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Use Docker container path for CSV file
    // Inside the container, files are mounted at /app/tests/fixtures/
    let csv_file_path = "/app/tests/fixtures/products.csv";

    println!("\n📄 CSV file path: {}", csv_file_path);

    // Create CSV batch processing task
    println!("\n🎯 Creating CSV batch processing task...");
    let task_request = create_task_request(
        "csv_processing",
        "csv_product_inventory_analyzer",
        json!({
            "csv_file_path": csv_file_path,
            "analysis_mode": "inventory"
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: 5 batch workers reading CSV rows from disk");

    // Monitor task execution
    println!("\n⏱️ Monitoring CSV batch processing execution...");
    // CSV processing with file I/O needs more time than synthetic tests
    let timeout = 30; // 30 seconds for 1000 rows with 5 workers + file I/O
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
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"analyze_csv"),
        "Should have analyze_csv step"
    );
    assert!(
        step_names.contains(&"aggregate_csv_results"),
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

    // Verify category counts exist
    let categories = result
        .get("category_counts")
        .expect("Results should contain category_counts");
    println!("   Product categories found: {}", categories);
    assert!(
        categories.is_object(),
        "Category counts should be an object"
    );

    // Verify max price product exists
    let max_price = result
        .get("max_price")
        .expect("Results should contain max_price");
    println!(
        "   Most expensive product price: ${}",
        max_price.as_f64().unwrap()
    );
    assert!(
        max_price.as_f64().unwrap() > 0.0,
        "Max price should be positive"
    );

    let max_product = result
        .get("max_price_product")
        .expect("Results should contain max_price_product");
    println!(
        "   Most expensive product: {}",
        max_product.as_str().unwrap()
    );
    assert!(
        !max_product.as_str().unwrap().is_empty(),
        "Max price product name should not be empty"
    );

    // Verify average rating
    let avg_rating = result
        .get("overall_average_rating")
        .expect("Results should contain overall_average_rating");
    println!("   Average rating: {:.2}", avg_rating.as_f64().unwrap());
    assert!(
        avg_rating.as_f64().unwrap() > 0.0,
        "Average rating should be positive"
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

    println!("\n🎉 CSV Batch Processing Test PASSED!");
    println!("✅ CSV analysis (row counting): Working");
    println!("✅ Batch worker creation (5 workers): Working");
    println!("✅ Parallel CSV row processing (200 rows each): Working");
    println!("✅ Real file I/O (cursor-based row selection): Working");
    println!("✅ Inventory metrics calculation: Working");
    println!("✅ Deferred convergence aggregation: Working");
    println!("✅ Rust handlers (batch_processing_products_csv): Working");

    Ok(())
}
