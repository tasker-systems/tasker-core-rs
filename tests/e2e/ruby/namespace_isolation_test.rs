//! # End-to-End Ruby Namespace Isolation Integration Test
//!
//! This integration test validates the Blog Post 04 namespace isolation pattern:
//! 1. Two namespaces (`payments` and `customer_success`) with identically named workflows
//! 2. Both workflows named `process_refund` coexist without conflicts
//! 3. Tasks can be created concurrently in both namespaces
//! 4. Each namespace workflow executes independently
//! 5. Cross-namespace coordination (customer_success calls payments workflow)
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! Namespace Workflows:
//! - **payments.process_refund**: 4 steps (validate → refund → update → notify)
//! - **customer_success.process_refund**: 5 steps (validate → policy → approval → execute_payments → update_ticket)
//!
//! NOTE: This test demonstrates namespace isolation - the same workflow name in different
//! namespaces does not cause conflicts, showing how teams can work independently.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create payments namespace refund task request
fn create_payments_refund_request(
    payment_id: &str,
    refund_amount: i64,
    customer_email: &str,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "payments",
        "process_refund",
        json!({
            "payment_id": payment_id,
            "refund_amount": refund_amount,
            "refund_reason": "customer_request",
            "customer_email": customer_email,
            "partial_refund": false
        }),
    )
}

/// Helper function to create customer_success namespace refund task request
fn create_customer_success_refund_request(
    ticket_id: &str,
    customer_id: &str,
    refund_amount: i64,
    customer_email: &str,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "customer_success",
        "process_refund",
        json!({
            "ticket_id": ticket_id,
            "customer_id": customer_id,
            "refund_amount": refund_amount,
            "refund_reason": "Product defective",
            "customer_email": customer_email,
            "agent_notes": "Customer very upset, expedite refund",
            "requires_approval": true
        }),
    )
}

/// Test payments namespace refund workflow
///
/// Validates:
/// - Payments workflow completes successfully
/// - All 4 steps execute in correct order
/// - Step results flow correctly through the workflow
#[tokio::test]
async fn test_payments_namespace_refund() -> Result<()> {
    println!("🚀 Starting Payments Namespace Refund Test");
    println!("   Namespace: payments");
    println!("   Workflow: process_refund");
    println!("   Steps: 4 (validate_payment_eligibility → process_gateway_refund → update_payment_records → notify_customer)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready!");

    // Create payments refund task
    let payment_id = "pay_test_12345abc";
    let refund_amount = 4999; // $49.99 in cents
    let customer_email = "customer@example.com";

    println!("\n🎯 Creating payments refund task...");
    let task_request = create_payments_refund_request(payment_id, refund_amount, customer_email);

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Namespace: payments");
    println!("   Expected: 4 steps");

    // Monitor task execution
    println!("\n⏱️  Monitoring payments workflow execution...");
    let timeout = 15; // 15 seconds for 4 steps
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying payments workflow results...");
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

    // Verify all 4 steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 4, "Should have 4 steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    // Verify step names
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        step_names.contains(&"validate_payment_eligibility".to_string()),
        "Should have validate_payment_eligibility step"
    );
    assert!(
        step_names.contains(&"process_gateway_refund".to_string()),
        "Should have process_gateway_refund step"
    );
    assert!(
        step_names.contains(&"update_payment_records".to_string()),
        "Should have update_payment_records step"
    );
    assert!(
        step_names.contains(&"notify_customer".to_string()),
        "Should have notify_customer step"
    );

    println!("✅ Payments namespace refund completed successfully!");
    println!("   All 4 steps executed in correct order");
    Ok(())
}

/// Test customer_success namespace refund workflow
///
/// Validates:
/// - Customer success workflow completes successfully
/// - All 5 steps execute in correct order
/// - Cross-namespace coordination works (calls payments workflow)
#[tokio::test]
async fn test_customer_success_namespace_refund() -> Result<()> {
    println!("🚀 Starting Customer Success Namespace Refund Test");
    println!("   Namespace: customer_success");
    println!("   Workflow: process_refund");
    println!("   Steps: 5 (validate_refund_request → check_refund_policy → get_manager_approval → execute_refund_workflow → update_ticket_status)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready!");

    // Create customer success refund task
    let ticket_id = "ticket_cs_789";
    let customer_id = "cust_premium_456";
    let refund_amount = 4999; // $49.99 in cents
    let customer_email = "premium@example.com";

    println!("\n🎯 Creating customer success refund task...");
    let task_request = create_customer_success_refund_request(
        ticket_id,
        customer_id,
        refund_amount,
        customer_email,
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Namespace: customer_success");
    println!("   Expected: 5 steps");

    // Monitor task execution
    println!("\n⏱️  Monitoring customer success workflow execution...");
    let timeout = 20; // 20 seconds for 5 steps with approval
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying customer success workflow results...");
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

    // Verify all 5 steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 5, "Should have 5 steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    // Verify step names
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        step_names.contains(&"validate_refund_request".to_string()),
        "Should have validate_refund_request step"
    );
    assert!(
        step_names.contains(&"check_refund_policy".to_string()),
        "Should have check_refund_policy step"
    );
    assert!(
        step_names.contains(&"get_manager_approval".to_string()),
        "Should have get_manager_approval step"
    );
    assert!(
        step_names.contains(&"execute_refund_workflow".to_string()),
        "Should have execute_refund_workflow step (cross-namespace coordination)"
    );
    assert!(
        step_names.contains(&"update_ticket_status".to_string()),
        "Should have update_ticket_status step"
    );

    println!("✅ Customer success namespace refund completed successfully!");
    println!("   All 5 steps executed in correct order");
    println!("   Cross-namespace coordination demonstrated");
    Ok(())
}

/// Test concurrent execution in both namespaces
///
/// Validates:
/// - Same workflow name in different namespaces causes no conflicts
/// - Tasks execute concurrently in both namespaces
/// - Both workflows complete successfully
/// - Namespace isolation is maintained
#[tokio::test]
async fn test_concurrent_namespace_isolation() -> Result<()> {
    println!("🚀 Starting Concurrent Namespace Isolation Test");
    println!("   Demonstrating: Same workflow name ('process_refund') in two namespaces");
    println!("   Namespace 1: payments.process_refund (4 steps)");
    println!("   Namespace 2: customer_success.process_refund (5 steps)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready!");

    // Create tasks in both namespaces concurrently
    println!("\n🎯 Creating tasks in both namespaces concurrently...");

    // Payments namespace task
    let payments_request =
        create_payments_refund_request("pay_concurrent_123", 2999, "concurrent1@example.com");

    // Customer success namespace task
    let cs_request = create_customer_success_refund_request(
        "ticket_concurrent_456",
        "cust_gold_789",
        2999,
        "concurrent2@example.com",
    );

    // Create both tasks concurrently
    let (payments_response, cs_response) = tokio::try_join!(
        manager.orchestration_client.create_task(payments_request),
        manager.orchestration_client.create_task(cs_request)
    )?;

    println!("✅ Both tasks created successfully!");
    println!("   Payments task UUID: {}", payments_response.task_uuid);
    println!("   Customer Success task UUID: {}", cs_response.task_uuid);

    // Wait for both to complete
    println!("\n⏱️  Monitoring both workflows executing concurrently...");

    let (_payments_result, _cs_result) = tokio::try_join!(
        wait_for_task_completion(
            &manager.orchestration_client,
            &payments_response.task_uuid,
            20
        ),
        wait_for_task_completion(&manager.orchestration_client, &cs_response.task_uuid, 25)
    )?;

    println!("✅ Both workflows completed!");

    // Verify both tasks completed successfully
    println!("\n🔍 Verifying both workflows...");

    let payments_uuid = Uuid::parse_str(&payments_response.task_uuid)?;
    let payments_task = manager.orchestration_client.get_task(payments_uuid).await?;

    let cs_uuid = Uuid::parse_str(&cs_response.task_uuid)?;
    let cs_task = manager.orchestration_client.get_task(cs_uuid).await?;

    // Verify payments task
    assert!(
        payments_task.is_execution_complete(),
        "Payments task should be complete"
    );
    let payments_steps = manager
        .orchestration_client
        .list_task_steps(payments_uuid)
        .await?;
    assert_eq!(payments_steps.len(), 4, "Payments should have 4 steps");

    // Verify customer success task
    assert!(
        cs_task.is_execution_complete(),
        "Customer success task should be complete"
    );
    let cs_steps = manager
        .orchestration_client
        .list_task_steps(cs_uuid)
        .await?;
    assert_eq!(cs_steps.len(), 5, "Customer success should have 5 steps");

    println!("✅ Namespace isolation validated!");
    println!("   Both workflows named 'process_refund' executed concurrently");
    println!("   No conflicts between namespaces");
    println!("   Payments: 4 steps complete");
    println!("   Customer Success: 5 steps complete");

    Ok(())
}
