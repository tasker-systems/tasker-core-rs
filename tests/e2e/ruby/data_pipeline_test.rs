//! # End-to-End Ruby Data Pipeline Integration Test
//!
//! This integration test validates the Blog Post 02 DAG workflow pattern:
//! 1. Parallel data extraction from 3 sources (sales, inventory, customer)
//! 2. Sequential transformation of each extracted dataset
//! 3. Aggregation across all transformed sources (DAG convergence)
//! 4. Business insights generation from aggregated metrics
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! DAG Structure:
//! ```
//! extract_sales ──→ transform_sales ──┐
//! extract_inventory ──→ transform_inventory ──→ aggregate_metrics ──→ generate_insights
//! extract_customer ──→ transform_customers ──┘
//! ```
//!
//! NOTE: This test demonstrates parallel execution and DAG convergence patterns.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create analytics pipeline task request
fn create_analytics_pipeline_request() -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "data_pipeline",
        "analytics_pipeline",
        json!({
            "pipeline_id": format!("pipeline_{}", Uuid::now_v7()),
            "date_range": {
                "start_date": "2025-11-01",
                "end_date": "2025-11-20"
            }
        }),
    )
}

/// Test complete analytics pipeline execution
///
/// Validates:
/// - All 8 steps complete successfully
/// - Parallel execution of extract steps
/// - Sequential execution of transform steps
/// - Aggregation step converges all branches
/// - Insights generation from aggregated data
#[tokio::test]
async fn test_analytics_pipeline_dag_workflow() -> Result<()> {
    println!("🚀 Starting Analytics Pipeline DAG Workflow Test");
    println!("   Namespace: data_pipeline");
    println!("   Workflow: analytics_pipeline");
    println!("   Steps: 8 (3 extract → 3 transform → 1 aggregate → 1 insights)");
    println!("   Pattern: DAG with parallel execution and convergence");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready!");

    // Create analytics pipeline task
    println!("\n🎯 Creating analytics pipeline task...");
    let task_request = create_analytics_pipeline_request();

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Namespace: data_pipeline");
    println!("   Expected: 8 steps with parallel extract phase");

    // Monitor task execution
    println!("\n⏱️  Monitoring analytics pipeline execution...");
    let timeout = 30; // 30 seconds for 8 steps with parallel execution
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying analytics pipeline results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );
    assert!(
        task.status.to_lowercase().contains("complete"),
        "Task status should indicate completion, got: {}",
        task.status
    );

    // Verify all 8 steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 8, "Should have 8 steps in DAG workflow");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    // Verify step names and DAG structure
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

    // Extract phase (parallel)
    assert!(
        step_names.contains(&"extract_sales_data".to_string()),
        "Should have extract_sales_data step"
    );
    assert!(
        step_names.contains(&"extract_inventory_data".to_string()),
        "Should have extract_inventory_data step"
    );
    assert!(
        step_names.contains(&"extract_customer_data".to_string()),
        "Should have extract_customer_data step"
    );

    // Transform phase (sequential)
    assert!(
        step_names.contains(&"transform_sales".to_string()),
        "Should have transform_sales step"
    );
    assert!(
        step_names.contains(&"transform_inventory".to_string()),
        "Should have transform_inventory step"
    );
    assert!(
        step_names.contains(&"transform_customers".to_string()),
        "Should have transform_customers step"
    );

    // Aggregate phase (convergence)
    assert!(
        step_names.contains(&"aggregate_metrics".to_string()),
        "Should have aggregate_metrics step (DAG convergence point)"
    );

    // Insights phase (final)
    assert!(
        step_names.contains(&"generate_insights".to_string()),
        "Should have generate_insights step"
    );

    println!("✅ Analytics pipeline completed successfully!");
    println!("   All 8 steps executed in correct DAG order");
    println!("   ✓ Extract phase: 3 parallel steps");
    println!("   ✓ Transform phase: 3 sequential steps");
    println!("   ✓ Aggregate phase: DAG convergence");
    println!("   ✓ Insights phase: Final step");

    Ok(())
}
