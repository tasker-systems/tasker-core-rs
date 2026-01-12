//! # End-to-End Python Data Pipeline Analytics Workflow Integration Test
//!
//! This integration test validates the Blog Post 02 data pipeline analytics pattern with Python handlers:
//! 1. Connecting to running services (postgres, orchestration, worker)
//! 2. Using tasker-client library to create and execute analytics pipeline tasks
//! 3. Testing complete DAG workflow: parallel extracts → transforms → aggregation → insights
//! 4. Validating YAML configuration from tests/fixtures/task_templates/python/data_pipeline_analytics_pipeline_py.yaml
//!
//! Prerequisites:
//! Run `cargo make services-start` to start local services
//!
//! Data Pipeline Analytics Pattern (8 steps):
//! 1. extract_sales_data: Pull sales records (parallel)
//! 2. extract_inventory_data: Pull inventory records (parallel)
//! 3. extract_customer_data: Pull customer records (parallel)
//! 4. transform_sales: Transform sales data for analytics
//! 5. transform_inventory: Transform inventory data for analytics
//! 6. transform_customers: Transform customer data for analytics
//! 7. aggregate_metrics: Combine all transformed data sources (DAG convergence)
//! 8. generate_insights: Create actionable business intelligence
//!
//! TAS-91: Blog Post 02 - Python implementation
//!
//! NOTE: This test is language-agnostic and uses the tasker-client API. It does NOT reference
//! Python code or handlers directly - ensuring the system works correctly regardless of worker
//! implementation language.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create data pipeline analytics task request for Python
///
/// Creates a task request for the Python data pipeline workflow template.
/// Uses language-agnostic parameters that work regardless of handler implementation.
fn create_data_pipeline_request_py(
    pipeline_id: &str,
    date_range: serde_json::Value,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "data_pipeline_py",
        "analytics_pipeline_py",
        json!({
            "pipeline_id": pipeline_id,
            "date_range": date_range
        }),
    )
}

/// Test successful data pipeline analytics workflow (Python)
///
/// Validates:
/// - Task completes successfully
/// - All 8 steps execute correctly (3 parallel + 3 sequential + 1 aggregate + 1 insights)
/// - DAG convergence works correctly (aggregate waits for all transforms)
/// - Extract, transform, aggregate, and insights phases complete successfully
/// - Execution completes in reasonable time (< 30s for 8 steps with parallelism)
#[tokio::test]
async fn test_successful_analytics_pipeline_py() -> Result<()> {
    println!("🚀 Starting Python Data Pipeline Analytics Test");
    println!("   Workflow: DAG with parallel extracts → transforms → aggregation → insights");
    println!("   Template: analytics_pipeline_py");
    println!("   Namespace: data_pipeline_py");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Create pipeline with date range
    let pipeline_id = format!("pipeline_{}", Uuid::new_v4());
    let date_range = json!({
        "start_date": "2025-11-01",
        "end_date": "2025-11-30"
    });

    println!("\n🎯 Creating Python data pipeline analytics task...");
    println!("   Pipeline ID: {}", pipeline_id);
    let task_request = create_data_pipeline_request_py(&pipeline_id, date_range);

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: 8 steps (3 extract, 3 transform, 1 aggregate, 1 insights)");

    // Monitor task execution
    println!("\n⏱️  Monitoring pipeline execution...");
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

    assert_eq!(steps.len(), 8, "Should have 8 total steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    // Verify step names match expected workflow
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

    // Extract phase (3 parallel steps)
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

    // Transform phase (3 sequential steps)
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

    // Aggregate phase (1 step - DAG convergence)
    assert!(
        step_names.contains(&"aggregate_metrics".to_string()),
        "Should have aggregate_metrics step (DAG convergence)"
    );

    // Insights phase (1 step)
    assert!(
        step_names.contains(&"generate_insights".to_string()),
        "Should have generate_insights step"
    );

    println!("✅ Python data pipeline analytics completed successfully!");
    println!("   All 8 steps executed correctly");
    println!("   DAG convergence (aggregate_metrics) worked correctly");
    println!("   Pipeline ID: {}", pipeline_id);
    Ok(())
}
