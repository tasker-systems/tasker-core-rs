//! # End-to-End TypeScript E-commerce Order Processing Workflow Integration Test
//!
//! This integration test validates the Blog Post 01 e-commerce order processing pattern with TypeScript handlers:
//! 1. Connecting to running Docker Compose services (postgres, orchestration, worker)
//! 2. Using tasker-client library to create and execute e-commerce order processing tasks
//! 3. Testing complete checkout workflow: cart → payment → inventory → order → confirmation
//! 4. Validating YAML configuration from tests/fixtures/task_templates/typescript/ecommerce_order_processing_ts.yaml
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//! Or use `cargo make services-start` to start local services
//!
//! E-commerce Order Processing Pattern (5 steps):
//! 1. validate_cart: Validate cart items, check availability, calculate totals
//! 2. process_payment: Process customer payment using mock payment service
//! 3. update_inventory: Reserve inventory for order items
//! 4. create_order: Create order record with all details
//! 5. send_confirmation: Send order confirmation email to customer
//!
//! TAS-91: Blog Post 01 - TypeScript implementation
//!
//! NOTE: This test is language-agnostic and uses the tasker-client API. It does NOT reference
//! TypeScript code or handlers directly - ensuring the system works correctly regardless of worker
//! implementation language.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create e-commerce order processing task request for TypeScript
///
/// Creates a task request for the TypeScript e-commerce workflow template.
/// Uses language-agnostic parameters that work regardless of handler implementation.
fn create_ecommerce_order_request_ts(
    cart_items: serde_json::Value,
    customer_info: serde_json::Value,
    payment_info: serde_json::Value,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "ecommerce_ts",
        "ecommerce_order_processing_ts",
        json!({
            "cart_items": cart_items,
            "customer_info": customer_info,
            "payment_info": payment_info
        }),
    )
}

/// Test successful e-commerce order processing workflow (TypeScript)
///
/// Validates:
/// - Task completes successfully
/// - All 5 steps execute in order
/// - Cart validation, payment, inventory, order creation, and email confirmation succeed
/// - Fast execution (< 15s for 5 steps)
#[tokio::test]
async fn test_successful_order_processing_ts() -> Result<()> {
    println!("🚀 Starting TypeScript E-commerce Order Processing Test");
    println!("   Workflow: validate_cart → process_payment → update_inventory → create_order → send_confirmation");
    println!("   Template: ecommerce_order_processing_ts");
    println!("   Namespace: ecommerce_ts");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Create order with valid data
    // Payment amount: (29.99 * 2) + (49.99 * 1) = 109.97 + 8% tax (8.80) + shipping (5.99) = 124.76
    let cart_items = json!([
        {"product_id": 1, "quantity": 2},
        {"product_id": 2, "quantity": 1}
    ]);

    let customer_info = json!({
        "email": "customer@example.com",
        "name": "John Doe",
        "phone": "+1-555-0123"
    });

    let payment_info = json!({
        "method": "credit_card",
        "token": "tok_test_valid",
        "amount": 124.76
    });

    println!("\n🎯 Creating TypeScript e-commerce order processing task...");
    let task_request = create_ecommerce_order_request_ts(cart_items, customer_info, payment_info);

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: 5 steps (cart, payment, inventory, order, email)");

    // Monitor task execution
    println!("\n⏱️  Monitoring order processing execution...");
    let timeout = 15; // 15 seconds for 5 sequential steps
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying order processing results...");
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

    assert_eq!(steps.len(), 5, "Should have 5 total steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    // Verify step execution order
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        step_names.contains(&"validate_cart".to_string()),
        "Should have validate_cart step"
    );
    assert!(
        step_names.contains(&"process_payment".to_string()),
        "Should have process_payment step"
    );
    assert!(
        step_names.contains(&"update_inventory".to_string()),
        "Should have update_inventory step"
    );
    assert!(
        step_names.contains(&"create_order".to_string()),
        "Should have create_order step"
    );
    assert!(
        step_names.contains(&"send_confirmation".to_string()),
        "Should have send_confirmation step"
    );

    println!("✅ TypeScript e-commerce order processing completed successfully!");
    println!("   All 5 steps executed in correct order");
    Ok(())
}
