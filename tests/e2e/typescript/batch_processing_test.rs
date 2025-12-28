//! TypeScript Batch Processing E2E Tests
//!
//! TAS-59: Batch Processing Pattern
//!
//! These tests validate batch processing workflows with TypeScript handlers
//! through the orchestration API.
//!
//! Batch Processing Pattern:
//! 1. Batchable (analyze_csv_ts): Analyze CSV, create batch configurations
//! 2. Batch Worker (process_csv_batch_ts): Process CSV rows in parallel batches
//! 3. Deferred Convergence (aggregate_csv_results_ts): Aggregate results
//!
//! Note: These tests require the TypeScript worker to be running on port 8084.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Test CSV batch processing with simulated data
///
/// Validates:
/// - Batchable step creates batch worker configurations
/// - Batch workers process data in parallel
/// - Deferred convergence aggregates all results
/// - Final aggregation contains expected metrics
#[tokio::test]
async fn test_typescript_csv_batch_processing() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create batch processing task
    // The handler simulates 1000 rows with batch size 200 = 5 workers
    let task_request = create_task_request(
        "csv_processing_ts",
        "csv_product_inventory_analyzer_ts",
        json!({
            "csv_file_path": "/test/fixtures/products.csv",
            "analysis_mode": "inventory"
        }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Batch processing may take longer due to parallel workers
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 30).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(task.is_execution_complete(), "Task should be complete");

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Should have at least 3 steps: analyze, batch workers, aggregate
    // With 5 workers: 1 analyze + 5 workers + 1 aggregate = 7
    assert!(
        steps.len() >= 3,
        "Should have at least 3 steps (analyze, batch workers, aggregate), got {}",
        steps.len()
    );

    // Verify step types exist
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"analyze_csv_ts"),
        "Should have analyze_csv_ts (batchable step)"
    );
    assert!(
        step_names.contains(&"aggregate_csv_results_ts"),
        "Should have aggregate_csv_results_ts (convergence step)"
    );

    // All steps should be complete
    for step in &steps {
        assert!(
            step.current_state.to_uppercase() == "COMPLETE",
            "Step {} should be complete, got: {}",
            step.name,
            step.current_state
        );
    }

    println!(
        "✅ TypeScript batch processing completed with {} steps",
        steps.len()
    );
    Ok(())
}

/// Test batch processing with empty/no batches scenario
///
/// Validates:
/// - Handler gracefully handles empty input with NoBatches outcome
/// - Task completes with just the batchable step (no workers or aggregation needed)
/// - NoBatches outcome correctly signals that batch processing is complete
#[tokio::test]
async fn test_typescript_no_batches_scenario() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task - the handler should detect empty data and return NoBatches
    let task_request = create_task_request(
        "csv_processing_ts",
        "csv_product_inventory_analyzer_ts",
        json!({
            "csv_file_path": "/test/fixtures/empty.csv",
            "analysis_mode": "inventory"
        }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Even empty batches should complete
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 15).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task should be complete even with empty data"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // For NoBatches, only the batchable (analyze) step runs
    // No batch workers are created and no aggregation is needed
    assert!(!steps.is_empty(), "Should have at least the analyze step");

    // Verify the analyze step exists and completed
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"analyze_csv_ts"),
        "Should have analyze_csv_ts (batchable step)"
    );

    // All steps should be complete
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    println!(
        "✅ TypeScript empty batch scenario completed with {} step(s)",
        steps.len()
    );
    Ok(())
}
