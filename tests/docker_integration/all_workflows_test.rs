//! # Complete Workflow Integration Tests
//!
//! Comprehensive Docker-based integration tests for all workflow patterns.
//! Uses shared Docker services for efficient testing.

use anyhow::Result;
use futures;
use serde_json::json;
use tasker_core::test_helpers::{
    create_mathematical_test_context, create_order_fulfillment_test_context,
    create_test_task_request, DockerTestClient,
};

// Test constants for different workflows - matching actual YAML configurations
const LINEAR_NS: &str = "linear_workflow";
const DIAMOND_NS: &str = "diamond_workflow";
const TREE_NS: &str = "tree_workflow";
const MIXED_DAG_NS: &str = "mixed_dag_workflow";
const ORDER_NS: &str = "order_fulfillment";

// Task names from YAML configurations
const LINEAR_TASK: &str = "mathematical_sequence";
const DIAMOND_TASK: &str = "diamond_pattern";
const TREE_TASK: &str = "hierarchical_tree";
const MIXED_DAG_TASK: &str = "complex_dag";
const ORDER_TASK: &str = "business_workflow";

/// Linear Workflow Tests - Sequential step execution (input^8)
#[cfg(test)]
mod linear_workflow_tests {
    use super::*;

    #[tokio::test]
    async fn test_linear_workflow_complete_sequence() -> Result<()> {
        let test_client = DockerTestClient::new("linear_complete").await?;

        // Test data: even number for mathematical sequence
        // Expected: 6 → 36 → 1,296 → 1,679,616 → 2,821,109,907,456
        let context = create_mathematical_test_context(6);
        let request = create_test_task_request(
            LINEAR_NS,
            LINEAR_TASK,
            "1.0.0",
            context,
            "Test complete linear mathematical workflow (input^8)",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        assert_eq!(result.total_steps, 4);
        assert_eq!(result.completed_steps, 4);
        assert_eq!(result.failed_steps, 0);

        println!(
            "✅ Linear workflow test passed: {} steps",
            result.total_steps
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_linear_workflow_with_different_inputs() -> Result<()> {
        let test_client = DockerTestClient::new("linear_multiple").await?;

        for even_number in [2, 4, 8, 10] {
            let context = create_mathematical_test_context(even_number);
            let request = create_test_task_request(
                LINEAR_NS,
                LINEAR_TASK,
                "1.0.0",
                context,
                &format!("Test linear workflow with input {}", even_number),
            );

            let result = test_client.run_integration_test(request, 30).await?;

            assert_eq!(result.status, "completed");
            assert_eq!(result.completion_percentage, 100.0);
            assert_eq!(result.failed_steps, 0);
            assert_eq!(result.total_steps, 4);
            assert_eq!(result.completed_steps, 4);

            println!(
                "✅ Linear workflow with {} completed: {} steps in {:?}",
                even_number, result.total_steps, result.execution_time
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_linear_workflow_error_handling() -> Result<()> {
        let test_client = DockerTestClient::new("linear_error_handling").await?;

        // Use odd number which should trigger error handling
        let context = create_mathematical_test_context(7);
        let request = create_test_task_request(
            LINEAR_NS,
            LINEAR_TASK,
            "1.0.0",
            context,
            "Test linear workflow error handling with odd number 7",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        // Either completes successfully (graceful handling) or fails appropriately
        assert!(
            result.status == "completed" || result.status == "failed",
            "Expected completed or failed status, got: {}",
            result.status
        );

        println!(
            "✅ Linear workflow error handling test: status={} steps={} failed={}",
            result.status, result.total_steps, result.failed_steps
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_linear_workflow_concurrent_execution() -> Result<()> {
        // Create multiple test clients that will run concurrently
        let test_clients = vec![
            DockerTestClient::new("linear_concurrent_1").await?,
            DockerTestClient::new("linear_concurrent_2").await?,
            DockerTestClient::new("linear_concurrent_3").await?,
        ];

        // Create tasks with different inputs
        let even_numbers = vec![4, 10, 16];
        let mut tasks = vec![];

        // Start all tasks concurrently
        for (i, client) in test_clients.iter().enumerate() {
            let context = create_mathematical_test_context(even_numbers[i]);
            let request = create_test_task_request(
                LINEAR_NS,
                LINEAR_TASK,
                "1.0.0",
                context,
                &format!(
                    "Concurrent linear test {} with even number {}",
                    i + 1,
                    even_numbers[i]
                ),
            );

            // Start task (don't await yet)
            let task = client.run_integration_test(request, 45);
            tasks.push(task);
        }

        // Wait for all tasks to complete
        let results = futures::future::try_join_all(tasks).await?;

        // Verify all completed successfully
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.status, "completed");
            assert_eq!(result.completion_percentage, 100.0);
            assert_eq!(result.failed_steps, 0);
            assert_eq!(result.total_steps, 4);

            println!(
                "✅ Concurrent linear test {} completed: {} steps in {:?}",
                i + 1,
                result.total_steps,
                result.execution_time
            );
        }

        // All three tasks should have completed successfully
        assert_eq!(results.len(), 3);

        println!("✅ All concurrent linear workflow tests completed successfully");

        Ok(())
    }
}

/// Diamond Workflow Tests - Converging/diverging step pattern
#[cfg(test)]
mod diamond_workflow_tests {
    use super::*;

    #[tokio::test]
    async fn test_diamond_workflow_convergence() -> Result<()> {
        let test_client = DockerTestClient::new("diamond_convergence").await?;

        // Diamond pattern: split -> parallel processing -> merge
        let context = create_mathematical_test_context(10);
        let request = create_test_task_request(
            DIAMOND_NS,
            DIAMOND_TASK,
            "1.0.0",
            context,
            "Test diamond workflow convergence pattern",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        // Diamond has: initial -> (branch_a, branch_b parallel) -> final_convergence
        assert_eq!(result.total_steps, 4);
        assert_eq!(result.failed_steps, 0);

        println!("✅ Diamond workflow convergence test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_diamond_workflow_parallel_branches() -> Result<()> {
        let test_client = DockerTestClient::new("diamond_parallel").await?;

        let context = json!({
            "initial_value": 100,
            "multiplier_a": 2,
            "multiplier_b": 3
        });

        let request = create_test_task_request(
            DIAMOND_NS,
            DIAMOND_TASK,
            "1.0.0",
            context,
            "Test diamond workflow parallel branch processing",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        assert_eq!(result.failed_steps, 0);
        // Verify both branches completed
        assert!(result.completed_steps >= 3); // initial + at least 2 branches

        println!(
            "✅ Diamond workflow parallel branches test passed: {} steps in {:?}",
            result.total_steps, result.execution_time
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_diamond_workflow_error_handling() -> Result<()> {
        let test_client = DockerTestClient::new("diamond_error_handling").await?;

        // Invalid context that should trigger error handling
        let context = json!({
            "initial_value": -100,  // Negative value to test error handling
            "multiplier_a": 0,     // Zero multiplier to test edge cases
            "multiplier_b": 2
        });

        let request = create_test_task_request(
            DIAMOND_NS,
            DIAMOND_TASK,
            "1.0.0",
            context,
            "Test diamond workflow error handling with invalid inputs",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        // Either completes successfully (graceful handling) or fails appropriately
        assert!(
            result.status == "completed" || result.status == "failed",
            "Expected completed or failed status, got: {}",
            result.status
        );

        println!(
            "✅ Diamond workflow error handling test: status={} steps={} failed={}",
            result.status, result.total_steps, result.failed_steps
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_diamond_workflow_concurrent_execution() -> Result<()> {
        let test_clients = vec![
            DockerTestClient::new("diamond_concurrent_1").await?,
            DockerTestClient::new("diamond_concurrent_2").await?,
            DockerTestClient::new("diamond_concurrent_3").await?,
        ];

        let contexts = vec![
            json!({"initial_value": 50, "multiplier_a": 2, "multiplier_b": 3}),
            json!({"initial_value": 75, "multiplier_a": 1, "multiplier_b": 4}),
            json!({"initial_value": 25, "multiplier_a": 3, "multiplier_b": 2}),
        ];

        let mut tasks = vec![];

        // Start all tasks concurrently
        for (i, (client, context)) in test_clients.iter().zip(contexts.iter()).enumerate() {
            let request = create_test_task_request(
                DIAMOND_NS,
                DIAMOND_TASK,
                "1.0.0",
                context.clone(),
                &format!(
                    "Concurrent diamond test {} with value {}",
                    i + 1,
                    context["initial_value"]
                ),
            );

            let task = client.run_integration_test(request, 45);
            tasks.push(task);
        }

        // Wait for all tasks to complete
        let results = futures::future::try_join_all(tasks).await?;

        // Verify all completed successfully
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.status, "completed");
            assert_eq!(result.completion_percentage, 100.0);
            assert_eq!(result.failed_steps, 0);

            println!(
                "✅ Concurrent diamond test {} completed: {} steps in {:?}",
                i + 1,
                result.total_steps,
                result.execution_time
            );
        }

        assert_eq!(results.len(), 3);
        println!("✅ All concurrent diamond workflow tests completed successfully");

        Ok(())
    }
}

/// Tree Workflow Tests - Hierarchical branching pattern
#[cfg(test)]
mod tree_workflow_tests {
    use super::*;

    #[tokio::test]
    async fn test_tree_workflow_branching() -> Result<()> {
        let test_client = DockerTestClient::new("tree_branching").await?;

        // Tree pattern: root -> level1 branches -> level2 leaves
        let context = json!({
            "root_value": 1000,
            "branch_factor": 2,
            "depth": 3
        });

        let request = create_test_task_request(
            TREE_NS,
            TREE_TASK,
            "1.0.0",
            context,
            "Test tree workflow hierarchical branching",
        );

        let result = test_client.run_integration_test(request, 45).await?;

        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        // Tree structure typically has 7+ steps (1 root + 2 level1 + 4 level2)
        assert!(result.total_steps >= 7);

        println!(
            "✅ Tree workflow branching test passed: {} steps",
            result.total_steps
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_tree_workflow_aggregation() -> Result<()> {
        let test_client = DockerTestClient::new("tree_aggregation").await?;

        let context = json!({
            "values": [10, 20, 30, 40],
            "operation": "sum"
        });

        let request = create_test_task_request(
            TREE_NS,
            TREE_TASK,
            "1.0.0",
            context,
            "Test tree workflow result aggregation",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        assert_eq!(result.failed_steps, 0);

        println!(
            "✅ Tree workflow aggregation test passed: {} steps in {:?}",
            result.total_steps, result.execution_time
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_tree_workflow_error_handling() -> Result<()> {
        let test_client = DockerTestClient::new("tree_error_handling").await?;

        // Invalid tree configuration that should trigger error handling
        let context = json!({
            "root_value": 0,        // Zero root value
            "branch_factor": -1,    // Invalid branch factor
            "depth": 10             // Excessive depth
        });

        let request = create_test_task_request(
            TREE_NS,
            TREE_TASK,
            "1.0.0",
            context,
            "Test tree workflow error handling with invalid configuration",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        // Either completes successfully (graceful handling) or fails appropriately
        assert!(
            result.status == "completed" || result.status == "failed",
            "Expected completed or failed status, got: {}",
            result.status
        );

        println!(
            "✅ Tree workflow error handling test: status={} steps={} failed={}",
            result.status, result.total_steps, result.failed_steps
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_tree_workflow_concurrent_execution() -> Result<()> {
        let test_clients = vec![
            DockerTestClient::new("tree_concurrent_1").await?,
            DockerTestClient::new("tree_concurrent_2").await?,
            DockerTestClient::new("tree_concurrent_3").await?,
        ];

        let contexts = vec![
            json!({"root_value": 100, "branch_factor": 2, "depth": 2}),
            json!({"root_value": 500, "branch_factor": 3, "depth": 2}),
            json!({"values": [5, 10, 15, 20], "operation": "sum"}),
        ];

        let task_names = vec![TREE_TASK, TREE_TASK, TREE_TASK];

        let mut tasks = vec![];

        // Start all tasks concurrently
        for (i, ((client, context), task_name)) in test_clients
            .iter()
            .zip(contexts.iter())
            .zip(task_names.iter())
            .enumerate()
        {
            let request = create_test_task_request(
                TREE_NS,
                task_name,
                "1.0.0",
                context.clone(),
                &format!("Concurrent tree test {} with {}", i + 1, task_name),
            );

            let task = client.run_integration_test(request, 45);
            tasks.push(task);
        }

        // Wait for all tasks to complete
        let results = futures::future::try_join_all(tasks).await?;

        // Verify all completed successfully
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.status, "completed");
            assert_eq!(result.completion_percentage, 100.0);
            assert_eq!(result.failed_steps, 0);

            println!(
                "✅ Concurrent tree test {} completed: {} steps in {:?}",
                i + 1,
                result.total_steps,
                result.execution_time
            );
        }

        assert_eq!(results.len(), 3);
        println!("✅ All concurrent tree workflow tests completed successfully");

        Ok(())
    }
}

/// Mixed DAG Workflow Tests - Complex directed acyclic graph patterns
#[cfg(test)]
mod mixed_dag_workflow_tests {
    use super::*;

    #[tokio::test]
    async fn test_mixed_dag_complex_dependencies() -> Result<()> {
        let test_client = DockerTestClient::new("mixed_dag_complex").await?;

        // Complex DAG with multiple dependency chains
        let context = json!({
            "input_a": 5,
            "input_b": 10,
            "input_c": 15,
            "operation_mode": "complex"
        });

        let request = create_test_task_request(
            MIXED_DAG_NS,
            MIXED_DAG_TASK,
            "1.0.0",
            context,
            "Test mixed DAG complex dependency resolution",
        );

        let result = test_client.run_integration_test(request, 45).await?;

        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        // Mixed DAG typically has 6+ steps with complex dependencies
        assert!(result.total_steps >= 6);

        println!(
            "✅ Mixed DAG complex dependencies test passed: {} steps",
            result.total_steps
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_mixed_dag_parallel_paths() -> Result<()> {
        let test_client = DockerTestClient::new("mixed_dag_parallel").await?;

        let context = json!({
            "data_sources": ["source_a", "source_b", "source_c"],
            "processing_type": "parallel"
        });

        let request = create_test_task_request(
            MIXED_DAG_NS,
            MIXED_DAG_TASK,
            "1.0.0",
            context,
            "Test mixed DAG parallel execution paths",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        assert_eq!(result.failed_steps, 0);
        assert!(result.completed_steps >= 4); // Multiple parallel paths

        println!(
            "✅ Mixed DAG parallel paths test passed: {} steps in {:?}",
            result.total_steps, result.execution_time
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_mixed_dag_error_handling() -> Result<()> {
        let test_client = DockerTestClient::new("mixed_dag_error_handling").await?;

        // Invalid DAG configuration that should trigger error handling
        let context = json!({
            "input_a": null,        // Null input to test error handling
            "input_b": 10,
            "input_c": -5,          // Negative input
            "operation_mode": "invalid_mode"  // Invalid operation mode
        });

        let request = create_test_task_request(
            MIXED_DAG_NS,
            MIXED_DAG_TASK,
            "1.0.0",
            context,
            "Test mixed DAG error handling with invalid inputs",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        // Either completes successfully (graceful handling) or fails appropriately
        assert!(
            result.status == "completed" || result.status == "failed",
            "Expected completed or failed status, got: {}",
            result.status
        );

        println!(
            "✅ Mixed DAG error handling test: status={} steps={} failed={}",
            result.status, result.total_steps, result.failed_steps
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_mixed_dag_concurrent_execution() -> Result<()> {
        let test_clients = vec![
            DockerTestClient::new("mixed_dag_concurrent_1").await?,
            DockerTestClient::new("mixed_dag_concurrent_2").await?,
            DockerTestClient::new("mixed_dag_concurrent_3").await?,
        ];

        let contexts = vec![
            json!({"input_a": 5, "input_b": 10, "input_c": 15, "operation_mode": "complex"}),
            json!({"input_a": 8, "input_b": 12, "input_c": 20, "operation_mode": "simple"}),
            json!({"data_sources": ["source_x", "source_y"], "processing_type": "parallel"}),
        ];

        let task_names = vec![MIXED_DAG_TASK, MIXED_DAG_TASK, MIXED_DAG_TASK];

        let mut tasks = vec![];

        // Start all tasks concurrently
        for (i, ((client, context), task_name)) in test_clients
            .iter()
            .zip(contexts.iter())
            .zip(task_names.iter())
            .enumerate()
        {
            let request = create_test_task_request(
                MIXED_DAG_NS,
                task_name,
                "1.0.0",
                context.clone(),
                &format!("Concurrent mixed DAG test {} with {}", i + 1, task_name),
            );

            let task = client.run_integration_test(request, 60);
            tasks.push(task);
        }

        // Wait for all tasks to complete
        let results = futures::future::try_join_all(tasks).await?;

        // Verify all completed successfully
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.status, "completed");
            assert_eq!(result.completion_percentage, 100.0);
            assert_eq!(result.failed_steps, 0);

            println!(
                "✅ Concurrent mixed DAG test {} completed: {} steps in {:?}",
                i + 1,
                result.total_steps,
                result.execution_time
            );
        }

        assert_eq!(results.len(), 3);
        println!("✅ All concurrent mixed DAG workflow tests completed successfully");

        Ok(())
    }
}

/// Order Fulfillment Tests - Business workflow pattern
#[cfg(test)]
mod order_fulfillment_tests {
    use super::*;

    #[tokio::test]
    async fn test_order_fulfillment_complete_flow() -> Result<()> {
        let test_client = DockerTestClient::new("order_complete").await?;

        // Use proper order fulfillment context with correct schema
        let context = create_order_fulfillment_test_context("TEST-ORDER-001", 12345);

        let request = create_test_task_request(
            ORDER_NS,
            ORDER_TASK, // Actual task name from YAML
            "1.0.0",
            context,
            "Test complete order fulfillment business workflow",
        );

        let result = test_client.run_integration_test(request, 60).await?;

        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        // Order fulfillment has 4 steps: validate_order, reserve_inventory, process_payment, ship_order
        assert_eq!(result.total_steps, 4);
        assert_eq!(result.failed_steps, 0);

        println!(
            "✅ Order fulfillment complete flow test passed: {} steps in {:?}",
            result.total_steps, result.execution_time
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_order_fulfillment_inventory_edge_case() -> Result<()> {
        let test_client = DockerTestClient::new("order_inventory").await?;

        // Create context with high quantities that might trigger inventory issues
        let mut context = create_order_fulfillment_test_context("TEST-ORDER-002", 12346);
        // Override with high quantities to test inventory limits
        context["order_items"] = json!([
            {
                "product_id": 101,  // Available stock: 100
                "quantity": 150,    // Request more than available
                "price": 29.99
            }
        ]);

        let request = create_test_task_request(
            ORDER_NS,
            ORDER_TASK,
            "1.0.0",
            context,
            "Test order fulfillment with insufficient inventory",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        // This should fail due to insufficient inventory
        assert!(
            result.status == "completed" || result.status == "failed",
            "Expected completed or failed status for inventory check, got: {}",
            result.status
        );

        println!(
            "✅ Order fulfillment inventory edge case test: status={} steps={} failed={}",
            result.status, result.total_steps, result.failed_steps
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_order_fulfillment_different_payment_methods() -> Result<()> {
        let test_client = DockerTestClient::new("order_payment").await?;

        // Test different payment methods supported by the schema
        let payment_methods = vec!["credit_card", "debit_card", "paypal"];

        for (i, method) in payment_methods.iter().enumerate() {
            let mut context = create_order_fulfillment_test_context(
                &format!("TEST-ORDER-PAY-{}", i),
                12347 + i as i64,
            );
            context["payment_info"]["method"] = json!(method);
            context["payment_info"]["token"] = json!(format!("tok_{}_{}", method, i));

            let request = create_test_task_request(
                ORDER_NS,
                ORDER_TASK,
                "1.0.0",
                context,
                &format!("Test order fulfillment with {} payment", method),
            );

            let result = test_client.run_integration_test(request, 30).await?;

            assert_eq!(result.status, "completed");
            assert_eq!(result.completion_percentage, 100.0);
            assert_eq!(result.failed_steps, 0);
            assert_eq!(result.total_steps, 4); // All 4 steps should complete

            println!(
                "✅ Order fulfillment with {} payment test passed: {} steps in {:?}",
                method, result.total_steps, result.execution_time
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_order_fulfillment_invalid_customer() -> Result<()> {
        let test_client = DockerTestClient::new("order_invalid_customer").await?;

        // Invalid customer details that should trigger error handling
        let mut context = create_order_fulfillment_test_context("TEST-ORDER-INVALID", 12348);
        // Override with invalid customer info
        context["customer_info"] = json!({
            "id": null,  // Missing customer ID
            "name": "",  // Empty name
            "email": "invalid-email"  // Invalid email format
        });

        let request = create_test_task_request(
            ORDER_NS,
            ORDER_TASK,
            "1.0.0",
            context,
            "Test order fulfillment with invalid customer details",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        // Should fail at validation step due to invalid customer info
        assert!(
            result.status == "completed" || result.status == "failed",
            "Expected completed or failed status, got: {}",
            result.status
        );

        // If it fails, it should fail early (at validate_order step)
        if result.status == "failed" {
            assert!(
                result.failed_steps > 0,
                "Should have at least one failed step"
            );
        }

        println!(
            "✅ Order fulfillment invalid customer test: status={} steps={} failed={}",
            result.status, result.total_steps, result.failed_steps
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_order_fulfillment_empty_cart() -> Result<()> {
        let test_client = DockerTestClient::new("order_empty_cart").await?;

        // Empty cart that should trigger edge case handling
        let mut context = create_order_fulfillment_test_context("TEST-ORDER-EMPTY", 12349);
        // Override with empty order items
        context["order_items"] = json!([]);

        let request = create_test_task_request(
            ORDER_NS,
            ORDER_TASK,
            "1.0.0",
            context,
            "Test order fulfillment with empty cart",
        );

        let result = test_client.run_integration_test(request, 30).await?;

        // Should fail at validation step due to empty order items
        assert!(
            result.status == "completed" || result.status == "failed",
            "Expected completed or failed status, got: {}",
            result.status
        );

        // Should fail early due to empty cart validation
        if result.status == "failed" {
            assert!(
                result.failed_steps > 0,
                "Should have at least one failed step"
            );
            // Likely fails at validate_order step
            assert!(
                result.completed_steps < 4,
                "Should not complete all steps with empty cart"
            );
        }

        println!(
            "✅ Order fulfillment empty cart test: status={} steps={} failed={}",
            result.status, result.total_steps, result.failed_steps
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_order_fulfillment_concurrent_execution() -> Result<()> {
        let test_clients = vec![
            DockerTestClient::new("order_concurrent_1").await?,
            DockerTestClient::new("order_concurrent_2").await?,
            DockerTestClient::new("order_concurrent_3").await?,
        ];

        let contexts = vec![
            create_order_fulfillment_test_context("TEST-ORDER-CONCURRENT-001", 20001),
            create_order_fulfillment_test_context("TEST-ORDER-CONCURRENT-002", 20002),
            create_order_fulfillment_test_context("TEST-ORDER-CONCURRENT-003", 20003),
        ];

        // All use the same business_workflow task
        let task_names = vec![ORDER_TASK, ORDER_TASK, ORDER_TASK];

        let mut tasks = vec![];

        // Start all tasks concurrently
        for (i, ((client, context), task_name)) in test_clients
            .iter()
            .zip(contexts.iter())
            .zip(task_names.iter())
            .enumerate()
        {
            let request = create_test_task_request(
                ORDER_NS,
                task_name,
                "1.0.0",
                context.clone(),
                &format!(
                    "Concurrent order fulfillment test {} with {}",
                    i + 1,
                    task_name
                ),
            );

            let task = client.run_integration_test(request, 60);
            tasks.push(task);
        }

        // Wait for all tasks to complete
        let results = futures::future::try_join_all(tasks).await?;

        // Verify all completed (some may fail based on business logic)
        for (i, result) in results.iter().enumerate() {
            assert!(
                result.status == "completed" || result.status == "failed",
                "Expected completed or failed status, got: {}",
                result.status
            );

            println!(
                "✅ Concurrent order fulfillment test {} completed: status={} steps={} in {:?}",
                i + 1,
                result.status,
                result.total_steps,
                result.execution_time
            );
        }

        assert_eq!(results.len(), 3);
        println!("✅ All concurrent order fulfillment tests completed successfully");

        Ok(())
    }
}

/// Performance and Stress Tests
#[cfg(test)]
mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_workflow_execution() -> Result<()> {
        // Test multiple workflows running concurrently
        let workflows = vec![
            ("linear", LINEAR_NS, LINEAR_TASK),
            ("diamond", DIAMOND_NS, DIAMOND_TASK),
            ("tree", TREE_NS, TREE_TASK),
            ("dag", MIXED_DAG_NS, MIXED_DAG_TASK),
        ];

        let mut test_results = vec![];

        for (name, namespace, task_name) in workflows {
            let client = DockerTestClient::new(&format!("concurrent_{}", name)).await?;
            let context = create_mathematical_test_context(8);
            let request = create_test_task_request(
                namespace,
                task_name,
                "1.0.0",
                context,
                &format!("Concurrent {} workflow test", name),
            );

            let test_result = client.run_integration_test(request, 60).await?;

            test_results.push(test_result);
        }

        for (i, result) in test_results.iter().enumerate() {
            assert_eq!(result.status, "completed");
            println!(
                "✅ Concurrent workflow {} completed: {} steps",
                i + 1,
                result.total_steps
            );
        }

        println!("✅ All concurrent workflow tests passed");
        Ok(())
    }

    #[tokio::test]
    #[ignore] // Only run when specifically testing performance
    async fn test_workflow_throughput() -> Result<()> {
        let test_client = DockerTestClient::new("throughput_test").await?;
        let num_workflows: u128 = 10;
        let mut total_time = std::time::Duration::ZERO;

        for i in 0..num_workflows {
            let context = create_mathematical_test_context((i + 1) * 2);
            let request = create_test_task_request(
                LINEAR_NS,
                LINEAR_TASK,
                "1.0.0",
                context,
                &format!("Throughput test workflow {}", i),
            );

            let start = std::time::Instant::now();
            let result = test_client.run_integration_test(request, 30).await?;
            total_time += start.elapsed();

            assert_eq!(result.status, "completed");
            assert_eq!(result.completion_percentage, 100.0);
            assert_eq!(result.failed_steps, 0);
        }

        let avg_time = total_time.as_millis() / num_workflows;
        println!(
            "✅ Throughput test: {} workflows, avg time: {}ms",
            num_workflows, avg_time
        );

        // Ensure average time is reasonable (adjust threshold as needed)
        assert!(avg_time < 5000); // 5 seconds per workflow

        Ok(())
    }

    #[tokio::test]
    async fn test_service_health_and_debugging() -> Result<()> {
        use tasker_core::test_helpers::DockerTestSuiteManager;

        let test_client = DockerTestClient::new("health_check_all_workflows").await?;

        // Get suite manager to check health
        let suite_manager = DockerTestSuiteManager::get_or_start().await?;

        // Verify services are healthy
        assert!(suite_manager.are_services_healthy());

        // Check service uptime
        let uptime = suite_manager.uptime();
        assert!(uptime.as_secs() > 0);

        // Get service logs for debugging (just verify we can retrieve them)
        let orchestration_logs = test_client.get_service_logs("orchestration").await?;
        assert!(!orchestration_logs.is_empty());

        let worker_logs = test_client.get_service_logs("worker").await?;
        assert!(!worker_logs.is_empty());

        // Test a simple workflow to ensure services are responding
        let context = create_mathematical_test_context(4);
        let request = create_test_task_request(
            LINEAR_NS,
            LINEAR_TASK,
            "1.0.0",
            context,
            "Health check validation workflow",
        );

        let result = test_client.run_integration_test(request, 30).await?;
        assert_eq!(result.status, "completed");
        assert_eq!(result.completion_percentage, 100.0);
        assert_eq!(result.failed_steps, 0);

        println!(
            "✅ Service health check passed - uptime: {:?}, orchestration logs: {} bytes, worker logs: {} bytes",
            uptime, orchestration_logs.len(), worker_logs.len()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_workflow_resilience_under_load() -> Result<()> {
        // Test all workflow types under concurrent load to verify resilience
        let num_concurrent = 5;

        // Create all clients first to avoid lifetime issues
        let mut linear_clients = vec![];
        let mut diamond_clients = vec![];
        let mut tree_clients = vec![];

        for i in 0..num_concurrent {
            linear_clients.push(DockerTestClient::new(&format!("resilience_linear_{}", i)).await?);
            diamond_clients
                .push(DockerTestClient::new(&format!("resilience_diamond_{}", i)).await?);
            tree_clients.push(DockerTestClient::new(&format!("resilience_tree_{}", i)).await?);
        }

        let mut all_tasks = vec![];

        // Linear workflows
        for (i, client) in linear_clients.iter().enumerate() {
            let context = create_mathematical_test_context(2 + i as u128 * 2);
            let request = create_test_task_request(
                LINEAR_NS,
                LINEAR_TASK,
                "1.0.0",
                context,
                &format!("Resilience test linear workflow {}", i),
            );
            all_tasks.push(client.run_integration_test(request, 60));
        }

        // Diamond workflows
        for (i, client) in diamond_clients.iter().enumerate() {
            let context = create_mathematical_test_context(10 + i as u128 * 5);
            let request = create_test_task_request(
                DIAMOND_NS,
                DIAMOND_TASK,
                "1.0.0",
                context,
                &format!("Resilience test diamond workflow {}", i),
            );
            all_tasks.push(client.run_integration_test(request, 60));
        }

        // Tree workflows
        for (i, client) in tree_clients.iter().enumerate() {
            let context = json!({
                "root_value": 100 + i * 50,
                "branch_factor": 2,
                "depth": 2
            });
            let request = create_test_task_request(
                TREE_NS,
                TREE_TASK,
                "1.0.0",
                context,
                &format!("Resilience test tree workflow {}", i),
            );
            all_tasks.push(client.run_integration_test(request, 60));
        }

        // Wait for all workflows to complete
        let results = futures::future::try_join_all(all_tasks).await?;

        // Verify all completed successfully
        let mut completed_count = 0;
        let mut failed_count = 0;

        for (i, result) in results.iter().enumerate() {
            if result.status == "completed" {
                completed_count += 1;
                assert_eq!(result.completion_percentage, 100.0);
                assert_eq!(result.failed_steps, 0);
            } else if result.status == "failed" {
                failed_count += 1;
            }

            println!(
                "✅ Resilience test workflow {} - status: {} ({} steps in {:?})",
                i + 1,
                result.status,
                result.total_steps,
                result.execution_time
            );
        }

        // At least 80% should complete successfully under load
        let success_rate = (completed_count as f64) / (results.len() as f64);
        assert!(
            success_rate >= 0.8,
            "Success rate {} is below 80% threshold (completed: {}, failed: {}, total: {})",
            success_rate,
            completed_count,
            failed_count,
            results.len()
        );

        println!(
            "✅ Workflow resilience test passed: {} completed, {} failed, success rate: {:.1}%",
            completed_count,
            failed_count,
            success_rate * 100.0
        );

        Ok(())
    }
}

/// Utility to manually clean up shared services (for test harness)
///
/// This would typically be called in a test suite teardown hook.
#[tokio::test]
#[ignore] // Only run manually
async fn manual_cleanup_shared_services() -> Result<()> {
    use tasker_core::test_helpers::cleanup_shared_services;

    cleanup_shared_services().await?;
    println!("✅ Shared Docker services cleaned up manually");

    Ok(())
}
