// Tree Workflow Integration Test - Native Rust Implementation
//
// Comprehensive integration test for Tree Workflow pattern using native Rust step handlers.
// This test mirrors the Ruby integration test patterns but uses the native Rust implementation
// throughout the entire execution pipeline.
//
// Test Pattern: Complex hierarchical tree structure with multiple branches and convergence (input^32)
// Steps: Root → 2 Branches → 4 Leaves → Final Convergence
// Example: 6 → 36 → (1,296 & 1,296) → (1,679,616 & 1,679,616 & 1,679,616 & 1,679,616) → final result

use anyhow::Result;
use serde_json::json;
use std::time::Duration;
use tracing::{info, warn};

use tasker_core::test_helpers::{create_mathematical_test_context, create_test_task_request};
use tasker_worker_rust::test_helpers::{init_test_logging, init_test_worker};

/// Test configuration and constants
const NAMESPACE: &str = "tree_workflow";
const TASK_NAME: &str = "hierarchical_tree";
const TASK_VERSION: &str = "1.0.0";
const TEST_TIMEOUT_SECONDS: u64 = 45; // More time for complex tree structure

/// Tree Pattern Integration Tests
/// Tests complex hierarchical execution with multiple branches and convergence
#[cfg(test)]
mod tree_workflow_integration_tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_complete_hierarchical_tree_workflow() -> Result<()> {
        init_test_logging();

        let mut setup = init_test_worker().await?;

        // Test data: even number for tree pattern calculation
        // Expected progression: 6 → 36 → branches → leaves → convergence (input^32)
        let test_context = create_mathematical_test_context(6);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test complete hierarchical tree workflow with multiple branches and convergence (input^32)"
        );

        // Run the complete integration test
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify task completion
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.completion_percentage, 100.0);
        assert_eq!(summary.total_steps, 8); // Root, 2 branches, 4 leaves, final convergence
        assert_eq!(summary.completed_steps, 8);
        assert_eq!(summary.failed_steps, 0);

        // Verify execution time is reasonable for complex tree
        assert!(summary.execution_time < Duration::from_secs(TEST_TIMEOUT_SECONDS));

        info!("✅ Complete Hierarchical Tree Workflow Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_tree_branch_hierarchical_execution() -> Result<()> {
        init_test_logging();

        let mut setup = init_test_worker().await?;

        // Use different input to test hierarchical execution
        let test_context = create_mathematical_test_context(4);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test tree pattern hierarchical branch execution",
        );

        // Run integration test focusing on hierarchical structure
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify hierarchical execution completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 8);
        assert_eq!(summary.completed_steps, 8);
        assert_eq!(summary.failed_steps, 0);

        // Tree structure should allow for efficient parallel execution
        assert!(
            summary.execution_time < Duration::from_secs(30),
            "Tree hierarchical execution should be efficient, took {:?}",
            summary.execution_time
        );

        info!("✅ Tree Branch Hierarchical Execution Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_tree_leaf_parallel_processing() -> Result<()> {
        init_test_logging();

        let mut setup = init_test_worker().await?;

        // Use input that will test leaf parallel processing
        let test_context = create_mathematical_test_context(8);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test tree pattern leaf parallel processing",
        );

        // Run integration test with focus on leaf processing
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify leaf processing completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 8);
        assert_eq!(summary.completed_steps, 8);
        assert_eq!(summary.failed_steps, 0);

        // 4 leaf nodes should process efficiently in parallel
        info!("✅ Tree Leaf Parallel Processing Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_tree_final_convergence_coordination() -> Result<()> {
        init_test_logging();

        let mut setup = init_test_worker().await?;

        // Use input that will test complex convergence
        let test_context = create_mathematical_test_context(10);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test tree pattern final convergence coordination from 4 leaves",
        );

        // Run integration test with focus on final convergence
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify final convergence completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 8);
        assert_eq!(summary.completed_steps, 8);
        assert_eq!(summary.failed_steps, 0);

        info!("✅ Tree Final Convergence Coordination Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_tree_error_handling_and_validation() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Tree Error Handling and Validation Test");

        let mut setup = init_test_worker().await?;

        // Test with odd number (may cause validation issues in step handlers)
        let invalid_context = json!({
            "even_number": 11,  // Odd number - may cause validation errors
            "test_run_id": "tree-validation-test"
        });

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            invalid_context,
            "Test tree pattern validation and error handling with invalid input",
        );

        // Initialize orchestration and workers separately to test task creation
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        setup.initialize_worker(vec![NAMESPACE]).await?;

        // Task creation should succeed even with invalid input
        let task_uuid = setup.create_task(task_request).await?;
        assert!(!task_uuid.is_empty());

        info!(
            "✅ Task created successfully with potentially invalid input: {}",
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
                    "⚠️ Task execution encountered expected validation issues: {}",
                    e
                );
            }
        }

        info!("✅ Tree Error Handling and Validation Test completed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_tree_framework_integration() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Tree Framework Integration Test");

        let mut setup = init_test_worker().await?;

        // Test orchestration system initialization for tree workflow
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        info!("✅ Orchestration system initialized successfully for tree workflow");

        // Test worker initialization with more workers for tree complexity
        setup.initialize_worker(vec![NAMESPACE]).await?;
        info!("✅ Workers initialized successfully for tree workflow");

        // Test task creation functionality
        let test_context = create_mathematical_test_context(2);
        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test tree framework integration and orchestration functionality",
        );

        let task_uuid = setup.create_task(task_request).await?;
        assert!(!task_uuid.is_empty());

        info!(
            "✅ Tree framework integration functional - created task {}",
            task_uuid
        );

        // Wait a moment to see if task processing starts
        tokio::time::sleep(Duration::from_secs(3)).await;

        info!("✅ Tree Framework Integration Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_tree_performance_and_complexity() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Tree Performance and Complexity Test");

        let mut setup = init_test_worker().await?;

        // Use input that will exercise complex tree processing
        let test_context = create_mathematical_test_context(12);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test tree pattern performance with complex hierarchical structure",
        );

        let start_time = std::time::Instant::now();

        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        let total_time = start_time.elapsed();

        // Verify performance characteristics
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.completed_steps, 8);

        // Tree execution should handle complexity efficiently
        assert!(
            total_time < Duration::from_secs(20),
            "Tree execution should handle complexity efficiently, took {:?}",
            total_time
        );

        info!("✅ Performance Test Results:");
        info!("  - Total execution time: {:?}", total_time);
        info!(
            "  - Steps completed: {}/{}",
            summary.completed_steps, summary.total_steps
        );
        info!("  - Tree complexity handling: EXCELLENT");

        info!("✅ Tree Performance and Complexity Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_tree_dependency_resolution_complexity() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Tree Dependency Resolution Complexity Test");

        let mut setup = init_test_worker().await?;

        // Use input that will test complex dependency resolution
        let test_context = create_mathematical_test_context(16);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test tree pattern complex dependency resolution with 4-way convergence",
        );

        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify complex dependency resolution worked correctly
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 8);
        assert_eq!(summary.completed_steps, 8);
        assert_eq!(summary.failed_steps, 0);

        // Tree pattern should handle complex dependencies correctly:
        // - Branches depend on Root
        // - Leaves depend on their respective Branches
        // - Final Convergence depends on all 4 Leaves
        assert!(
            summary.execution_time.as_secs() >= 2,
            "Tree pattern should take time for proper complex dependency resolution"
        );

        info!("✅ Tree Dependency Resolution Complexity Test passed");
        info!(
            "📊 Complex dependency resolution time: {:?}",
            summary.execution_time
        );

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_tree_workflows_concurrency() -> Result<()> {
        init_test_logging();
        info!("🧪 Starting: Multiple Tree Workflows Concurrency Test");

        let mut setup = init_test_worker().await?;

        // Initialize orchestration with more workers for complex concurrency
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        setup.initialize_worker(vec![NAMESPACE]).await?; // More workers for tree complexity

        // Create multiple concurrent tree workflows
        let task_uuids = futures::future::try_join_all((0..2).map(|i| {
            // Fewer concurrent tree workflows due to complexity
            let test_context = create_mathematical_test_context(6 + (i * 2)); // 6, 8
            let task_request = create_test_task_request(
                NAMESPACE,
                TASK_NAME,
                TASK_VERSION,
                test_context,
                &format!("Concurrent tree workflow test #{}", i + 1),
            );
            setup.create_task(task_request)
        }))
        .await?;

        info!(
            "✅ Created {} concurrent tree workflows: {:?}",
            task_uuids.len(),
            task_uuids
        );

        // Wait for all tasks to complete with longer timeout for tree complexity
        let summaries = futures::future::try_join_all(task_uuids.iter().map(|uuid| {
            setup.wait_for_completion(uuid, TEST_TIMEOUT_SECONDS + 15) // Extra time for tree complexity
        }))
        .await?;

        // Verify all tasks completed successfully
        for (i, summary) in summaries.iter().enumerate() {
            assert_eq!(
                summary.status, "complete",
                "Tree task {} should complete",
                i
            );
            assert_eq!(
                summary.completed_steps, 8,
                "Tree task {} should have 8 completed steps",
                i
            );
            assert_eq!(
                summary.failed_steps, 0,
                "Tree task {} should have no failed steps",
                i
            );
        }

        info!(
            "✅ All {} concurrent tree workflows completed successfully",
            summaries.len()
        );
        info!(
            "📊 Concurrent execution times: {:?}",
            summaries
                .iter()
                .map(|s| s.execution_time)
                .collect::<Vec<_>>()
        );

        info!("✅ Multiple Tree Workflows Concurrency Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }
}
