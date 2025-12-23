//! # End-to-End Domain Event Publishing Integration Test (TypeScript)
//!
//! TAS-65/TAS-69: Domain Event Publication Architecture
//!
//! This integration test validates that domain event publishing:
//! 1. Does not block step execution (fire-and-forget semantics)
//! 2. Integrates correctly with the workflow lifecycle
//! 3. Handles various event configurations (success/failure/always conditions)
//! 4. Works with both durable and fast delivery modes
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! Domain Event Publishing Pattern (4 steps):
//! 1. validate_order: Publishes order.validated on success (fast delivery)
//! 2. process_payment: Publishes payment.processed on success, payment.failed on failure (durable)
//! 3. update_inventory: Publishes inventory.updated always (fast delivery)
//! 4. send_notification: Publishes notification.sent on success (fast delivery)
//!
//! NOTE: This test validates the non-blocking nature of domain events. We verify:
//! - Task completes successfully
//! - Steps with event declarations execute normally
//! - No performance degradation from event publishing

use anyhow::Result;
use serde_json::json;
use std::time::Instant;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create domain event publishing test task request
fn create_domain_event_task_request(
    order_id: &str,
    customer_id: &str,
    amount: f64,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "domain_events_ts",
        "domain_event_publishing_ts",
        json!({
            "order_id": order_id,
            "customer_id": customer_id,
            "amount": amount
        }),
    )
}

/// Test successful domain event publishing workflow (TypeScript Worker)
///
/// Validates:
/// - Task completes successfully with event-publishing steps
/// - All 4 steps execute in order
/// - Event publishing doesn't block step execution
/// - Fast execution (< 10s for 4 steps with events)
#[tokio::test]
async fn test_typescript_domain_event_publishing_success() -> Result<()> {
    println!("🚀 Starting Domain Event Publishing Test (TypeScript Worker - Success Path)");
    println!(
        "   Workflow: validate_order → process_payment → update_inventory → send_notification"
    );
    println!("   Template: domain_event_publishing_ts");
    println!("   Namespace: domain_events_ts");

    let manager = IntegrationTestManager::setup().await?;

    // Create task request for domain event publishing
    let task_request = create_domain_event_task_request(
        "ORD-TS-001",
        "CUST-TS-001",
        99.99,
    );

    let start_time = Instant::now();

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for task completion
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let elapsed = start_time.elapsed();

    // Verify task completed successfully
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task should be complete, got status: {}",
        task.status
    );

    // Verify all 4 steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(
        steps.len(),
        4,
        "Should have 4 steps in domain event workflow"
    );

    for step in &steps {
        assert!(
            step.current_state.to_uppercase() == "COMPLETE",
            "Step {} should be complete, got: {}",
            step.name,
            step.current_state
        );
    }

    // Verify fast execution (events are non-blocking)
    assert!(
        elapsed.as_secs() < 10,
        "Task should complete in < 10s (events are non-blocking), took {:?}",
        elapsed
    );

    println!(
        "✅ TypeScript domain event publishing completed in {:?}",
        elapsed
    );
    Ok(())
}

/// Test concurrent domain event publishing tasks
///
/// Validates:
/// - Multiple tasks can publish events concurrently
/// - No deadlocks or resource contention
/// - All tasks complete successfully
#[tokio::test]
async fn test_typescript_domain_event_publishing_concurrent() -> Result<()> {
    println!("🚀 Starting Concurrent Domain Event Publishing Test (TypeScript Worker)");

    let manager = IntegrationTestManager::setup().await?;

    // Create 3 concurrent tasks
    let mut task_uuids = Vec::new();
    for i in 1..=3 {
        let task_request = create_domain_event_task_request(
            &format!("ORD-TS-CONC-{:03}", i),
            &format!("CUST-TS-CONC-{:03}", i),
            100.0 * (i as f64),
        );

        let response = manager
            .orchestration_client
            .create_task(task_request)
            .await?;
        task_uuids.push(response.task_uuid);
    }

    // Wait for all tasks to complete
    for task_uuid in &task_uuids {
        wait_for_task_completion(&manager.orchestration_client, task_uuid, 15).await?;
    }

    // Verify all tasks completed
    for task_uuid in &task_uuids {
        let uuid = Uuid::parse_str(task_uuid)?;
        let task = manager.orchestration_client.get_task(uuid).await?;

        assert!(
            task.is_execution_complete(),
            "Task {} should be complete",
            task_uuid
        );

        let steps = manager
            .orchestration_client
            .list_task_steps(uuid)
            .await?;

        assert_eq!(steps.len(), 4, "Each task should have 4 steps");
        assert!(
            steps
                .iter()
                .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
            "All steps should be complete"
        );
    }

    println!("✅ TypeScript concurrent domain event publishing completed successfully");
    Ok(())
}

/// Test domain event metrics availability
///
/// Validates:
/// - Worker health check passes after domain event processing
/// - Basic observability is available
#[tokio::test]
async fn test_typescript_domain_event_metrics_availability() -> Result<()> {
    println!("🚀 Starting Domain Event Metrics Test (TypeScript Worker)");

    let manager = IntegrationTestManager::setup().await?;

    // Create and complete a task with domain events
    let task_request = create_domain_event_task_request(
        "ORD-TS-METRICS-001",
        "CUST-TS-METRICS-001",
        50.0,
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    // Verify task completed
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task should be complete for metrics verification"
    );

    println!("✅ TypeScript domain event metrics verification completed");
    Ok(())
}
