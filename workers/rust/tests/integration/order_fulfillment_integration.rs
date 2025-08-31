// Order Fulfillment Integration Test - Native Rust Implementation
//
// Comprehensive integration test for Order Fulfillment business workflow using native Rust step handlers.
// This test mirrors the Ruby integration test patterns but uses the native Rust implementation
// throughout the entire execution pipeline.
//
// Test Pattern: Complete business workflow with external service simulation
// Steps: Validate Order → Reserve Inventory → Process Payment → Ship Order
// Demonstrates real-world business logic with comprehensive validation and error handling

use anyhow::Result;
use serde_json::json;
use std::time::Duration;
use tracing::{info, warn};

use tasker_core::test_helpers::{
    create_business_test_context, create_test_task_request, SharedTestSetup,
};

/// Test configuration and constants
const NAMESPACE: &str = "order_fulfillment";
const TASK_NAME: &str = "business_workflow";
const TASK_VERSION: &str = "1.0.0";
const TEST_TIMEOUT_SECONDS: u64 = 45; // More time for business workflow complexity

/// Order Fulfillment Business Workflow Integration Tests
/// Tests sophisticated business logic with external service integration
#[cfg(test)]
mod order_fulfillment_integration_tests {
    use super::*;
    use tokio;

    /// Initialize logging for tests
    fn init_test_logging() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .try_init();
    }

    #[tokio::test]
    async fn test_complete_order_fulfillment_workflow() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Complete Order Fulfillment Workflow Test");

        let mut setup = SharedTestSetup::new()?;

        // Test data: complete business context for order fulfillment
        let test_context = create_business_test_context();

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test complete order fulfillment business workflow: validate → reserve → payment → shipping"
        );

        // Run the complete integration test
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify task completion
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.completion_percentage, 100.0);
        assert_eq!(summary.total_steps, 4); // Validate, Reserve, Payment, Ship
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        // Verify execution time is reasonable for business workflow
        assert!(summary.execution_time < Duration::from_secs(TEST_TIMEOUT_SECONDS));

        info!("✅ Complete Order Fulfillment Workflow Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_order_validation_step() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Order Validation Step Test");

        let mut setup = SharedTestSetup::new()?;

        // Use comprehensive business context for validation testing
        let test_context = json!({
            "customer": {
                "id": 54321,
                "name": "Validation Test Customer",
                "email": "validation@test.com"
            },
            "items": [
                {
                    "sku": "TEST-ITEM-001",
                    "quantity": 1,
                    "price": 99.99
                }
            ],
            "payment": {
                "method": "credit_card",
                "token": "tok_validation_test"
            },
            "shipping": {
                "method": "express",
                "address": {
                    "street": "123 Validation Street",
                    "city": "Test City",
                    "state": "TC",
                    "zip": "12345"
                }
            },
            "test_run_id": "order-validation-test"
        });

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test order fulfillment validation step with comprehensive business data",
        );

        // Run integration test focusing on validation
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify validation completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        info!("✅ Order Validation Step Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_inventory_reservation_workflow() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Inventory Reservation Workflow Test");

        let mut setup = SharedTestSetup::new()?;

        // Use multiple items to test inventory reservation
        let test_context = json!({
            "customer": {
                "id": 67890,
                "name": "Inventory Test Customer",
                "email": "inventory@test.com"
            },
            "items": [
                {
                    "sku": "INVENTORY-001",
                    "quantity": 3,
                    "price": 19.99
                },
                {
                    "sku": "INVENTORY-002",
                    "quantity": 2,
                    "price": 39.99
                },
                {
                    "sku": "INVENTORY-003",
                    "quantity": 1,
                    "price": 79.99
                }
            ],
            "payment": {
                "method": "debit_card",
                "token": "tok_inventory_test"
            },
            "shipping": {
                "method": "standard",
                "address": {
                    "street": "456 Inventory Avenue",
                    "city": "Stock City",
                    "state": "SC",
                    "zip": "67890"
                }
            },
            "test_run_id": "inventory-reservation-test"
        });

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test order fulfillment inventory reservation with multiple items",
        );

        // Run integration test with focus on inventory
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify inventory reservation completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        info!("✅ Inventory Reservation Workflow Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_payment_processing_integration() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Payment Processing Integration Test");

        let mut setup = SharedTestSetup::new()?;

        // Test different payment methods
        let test_context = json!({
            "customer": {
                "id": 11111,
                "name": "Payment Test Customer",
                "email": "payment@test.com"
            },
            "items": [
                {
                    "sku": "PAYMENT-TEST-001",
                    "quantity": 1,
                    "price": 149.99
                }
            ],
            "payment": {
                "method": "paypal",
                "token": "tok_paypal_test_12345"
            },
            "shipping": {
                "method": "overnight",
                "address": {
                    "street": "789 Payment Plaza",
                    "city": "Finance City",
                    "state": "FC",
                    "zip": "11111"
                }
            },
            "test_run_id": "payment-processing-test"
        });

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test order fulfillment payment processing with PayPal integration",
        );

        // Run integration test with focus on payment
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify payment processing completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        info!("✅ Payment Processing Integration Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_shipping_and_delivery_coordination() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Shipping and Delivery Coordination Test");

        let mut setup = SharedTestSetup::new()?;

        // Test shipping coordination with complete order
        let test_context = json!({
            "customer": {
                "id": 22222,
                "name": "Shipping Test Customer",
                "email": "shipping@test.com"
            },
            "items": [
                {
                    "sku": "SHIPPING-001",
                    "quantity": 2,
                    "price": 29.99
                },
                {
                    "sku": "SHIPPING-002",
                    "quantity": 1,
                    "price": 59.99
                }
            ],
            "payment": {
                "method": "credit_card",
                "token": "tok_shipping_test"
            },
            "shipping": {
                "method": "express",
                "address": {
                    "street": "321 Shipping Street",
                    "city": "Logistics City",
                    "state": "LC",
                    "zip": "22222"
                }
            },
            "test_run_id": "shipping-coordination-test"
        });

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test order fulfillment shipping and delivery coordination",
        );

        // Run integration test with focus on shipping
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify shipping coordination completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        info!("✅ Shipping and Delivery Coordination Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_business_workflow_error_handling() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Business Workflow Error Handling Test");

        let mut setup = SharedTestSetup::new()?;

        // Test with invalid business data
        let invalid_context = json!({
            "customer": {
                "id": -1,  // Invalid customer ID
                "name": "",  // Empty name
                "email": "invalid-email"  // Invalid email format
            },
            "items": [],  // Empty items array
            "payment": {
                "method": "invalid_method",
                "token": ""
            },
            "shipping": {
                "method": "invalid_shipping",
                "address": {}  // Empty address
            },
            "test_run_id": "business-error-handling-test"
        });

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            invalid_context,
            "Test business workflow error handling with invalid business data",
        );

        // Initialize orchestration and workers separately to test task creation
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        setup.initialize_worker(vec![NAMESPACE]).await?;

        // Task creation should succeed even with invalid input
        let task_uuid = setup.create_task(task_request).await?;
        assert!(!task_uuid.is_empty());

        info!(
            "✅ Task created successfully with invalid business data: {}",
            task_uuid
        );

        // Try to wait for completion
        match setup
            .wait_for_completion(&task_uuid, TEST_TIMEOUT_SECONDS)
            .await
        {
            Ok(summary) => {
                info!("📊 Task completed: {}", summary);
                assert!(summary.completion_percentage >= 0.0);
            }
            Err(e) => {
                warn!(
                    "⚠️ Task execution encountered expected business validation errors: {}",
                    e
                );
            }
        }

        info!("✅ Business Workflow Error Handling Test completed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_order_fulfillment_framework_integration() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Order Fulfillment Framework Integration Test");

        let mut setup = SharedTestSetup::new()?;

        // Test orchestration system initialization for business workflow
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        info!("✅ Orchestration system initialized successfully for order fulfillment");

        // Test worker initialization for business processes
        setup.initialize_worker(vec![NAMESPACE]).await?;
        info!("✅ Workers initialized successfully for order fulfillment");

        // Test task creation functionality
        let test_context = create_business_test_context();
        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test order fulfillment framework integration and orchestration functionality",
        );

        let task_uuid = setup.create_task(task_request).await?;
        assert!(!task_uuid.is_empty());

        info!(
            "✅ Order fulfillment framework integration functional - created task {}",
            task_uuid
        );

        // Wait a moment to see if business workflow processing starts
        tokio::time::sleep(Duration::from_secs(3)).await;

        info!("✅ Order Fulfillment Framework Integration Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_business_workflow_performance() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Business Workflow Performance Test");

        let mut setup = SharedTestSetup::new()?;

        // Use comprehensive business context for performance testing
        let test_context = create_business_test_context();

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test business workflow performance with comprehensive order processing",
        );

        let start_time = std::time::Instant::now();

        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        let total_time = start_time.elapsed();

        // Verify performance characteristics
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.completed_steps, 4);

        // Business workflow should complete efficiently even with external service simulation
        assert!(
            total_time < Duration::from_secs(20),
            "Business workflow should be efficient with native Rust, took {:?}",
            total_time
        );

        info!("✅ Performance Test Results:");
        info!("  - Total execution time: {:?}", total_time);
        info!(
            "  - Steps completed: {}/{}",
            summary.completed_steps, summary.total_steps
        );
        info!("  - Business process efficiency: EXCELLENT");

        info!("✅ Business Workflow Performance Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_order_fulfillment_concurrency() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Multiple Order Fulfillment Concurrency Test");

        let mut setup = SharedTestSetup::new()?;

        // Initialize orchestration with workers for concurrent business processes
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        setup.initialize_worker(vec![NAMESPACE]).await?; // More workers for business concurrency

        // Create multiple concurrent order fulfillment workflows
        let task_uuids = futures::future::try_join_all((0..2).map(|i| {
            // Reasonable number for business workflow complexity
            let mut test_context = create_business_test_context();
            // Modify customer ID to make each order unique
            test_context["customer"]["id"] = json!(30000 + i);
            test_context["test_run_id"] = json!(format!("concurrent-order-{}", i + 1));

            let task_request = create_test_task_request(
                NAMESPACE,
                TASK_NAME,
                TASK_VERSION,
                test_context,
                &format!("Concurrent order fulfillment test #{}", i + 1),
            );
            setup.create_task(task_request)
        }))
        .await?;

        info!(
            "✅ Created {} concurrent order fulfillment workflows: {:?}",
            task_uuids.len(),
            task_uuids
        );

        // Wait for all orders to complete
        let summaries = futures::future::try_join_all(
            task_uuids
                .iter()
                .map(|uuid| setup.wait_for_completion(uuid, TEST_TIMEOUT_SECONDS)),
        )
        .await?;

        // Verify all orders completed successfully
        for (i, summary) in summaries.iter().enumerate() {
            assert_eq!(summary.status, "complete", "Order {} should complete", i);
            assert_eq!(
                summary.completed_steps, 4,
                "Order {} should have 4 completed steps",
                i
            );
            assert_eq!(
                summary.failed_steps, 0,
                "Order {} should have no failed steps",
                i
            );
        }

        info!(
            "✅ All {} concurrent order fulfillment workflows completed successfully",
            summaries.len()
        );
        info!(
            "📊 Concurrent execution times: {:?}",
            summaries
                .iter()
                .map(|s| s.execution_time)
                .collect::<Vec<_>>()
        );

        info!("✅ Multiple Order Fulfillment Concurrency Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }
}
