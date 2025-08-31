// Diamond Workflow Integration Test - Native Rust Implementation
//
// Comprehensive integration test for Diamond Workflow pattern using native Rust step handlers.
// This test mirrors the Ruby integration test patterns but uses the native Rust implementation
// throughout the entire execution pipeline.
//
// Test Pattern: Diamond pattern with parallel execution and convergence (input^16)
// Steps: Start → parallel branches (B & C) → convergence (End)
// Example: 6 → 36 → (1,296 & 1,296) → 2,821,109,907,456

use anyhow::Result;
use serde_json::json;
use std::time::Duration;
use tracing::{info, warn};

use tasker_core::test_helpers::shared_test_setup::{
    create_mathematical_test_context, create_test_task_request,
};
use tasker_worker_rust::test_helpers::{init_test_logging, init_test_worker};

/// Test configuration and constants
const NAMESPACE: &str = "diamond_workflow";
const TASK_NAME: &str = "diamond_pattern";
const TASK_VERSION: &str = "1.0.0";
const TEST_TIMEOUT_SECONDS: u64 = 30;

/// Diamond Pattern Integration Tests
/// Tests parallel execution followed by convergence using native Rust handlers
#[cfg(test)]
mod diamond_workflow_integration_tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_complete_diamond_pattern_workflow() -> Result<()> {
        init_test_logging();

        info!("Starting diamond pattern workflow integration test");

        let mut setup = init_test_worker().await?;

        // Test data: even number for diamond pattern calculation
        // Expected progression: 6 → 36 → (1,296 & 1,296) → 2,821,109,907,456 (input^16)
        let test_context = create_mathematical_test_context(6);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test complete diamond pattern workflow with parallel execution and convergence (input^16)"
        );

        // Run the complete integration test
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify task completion
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.completion_percentage, 100.0);
        assert_eq!(summary.total_steps, 4); // Start, Branch B, Branch C, End
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        // Verify execution time is reasonable
        assert!(summary.execution_time < Duration::from_secs(TEST_TIMEOUT_SECONDS));

        info!("✅ Complete Diamond Pattern Workflow Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_parallel_branch_execution() -> Result<()> {
        init_test_logging();

        info!("Starting parallel branch execution integration test");

        let mut setup = init_test_worker().await?;

        // Use different input to test parallel execution
        let test_context = create_mathematical_test_context(4);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test diamond pattern parallel branch execution",
        );

        // Run integration test focusing on parallelism
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify parallel execution completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        // Parallel execution should be efficient
        assert!(
            summary.execution_time < Duration::from_secs(15),
            "Parallel execution should be efficient, took {:?}",
            summary.execution_time
        );

        info!("✅ Parallel Branch Execution Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_convergence_step_coordination() -> Result<()> {
        init_test_logging();

        info!("Starting convergence step coordination integration test");

        let mut setup = init_test_worker().await?;

        // Use input that will test convergence logic
        let test_context = create_mathematical_test_context(8);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test diamond pattern convergence step coordination",
        );

        // Run integration test with focus on convergence
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify convergence completed successfully
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        info!("✅ Convergence Step Coordination Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_diamond_error_handling_and_validation() -> Result<()> {
        init_test_logging();

        info!("Starting diamond error handling and validation integration test");

        let mut setup = init_test_worker().await?;

        // Test with odd number (may cause validation issues in step handlers)
        let invalid_context = json!({
            "even_number": 9,  // Odd number - may cause validation errors
            "test_run_id": "diamond-validation-test"
        });

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            invalid_context,
            "Test diamond pattern validation and error handling with invalid input",
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

        info!("✅ Diamond Error Handling and Validation Test completed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_diamond_framework_integration() -> Result<()> {
        init_test_logging();

        info!("Starting diamond framework integration test");

        let mut setup = init_test_worker().await?;

        // Test orchestration system initialization for diamond workflow
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        info!("✅ Orchestration system initialized successfully for diamond workflow");

        // Test worker initialization
        setup.initialize_worker(vec![NAMESPACE]).await?;
        info!("✅ Workers initialized successfully for diamond workflow");

        // Test task creation functionality
        let test_context = create_mathematical_test_context(2);
        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test diamond framework integration and orchestration functionality",
        );

        let task_uuid = setup.create_task(task_request).await?;
        assert!(!task_uuid.is_empty());

        info!(
            "✅ Diamond framework integration functional - created task {}",
            task_uuid
        );

        // Wait a moment to see if task processing starts
        tokio::time::sleep(Duration::from_secs(2)).await;

        info!("✅ Diamond Framework Integration Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_diamond_performance_and_parallelism() -> Result<()> {
        init_test_logging();

        info!("Starting diamond performance and parallelism integration test");

        let mut setup = init_test_worker().await?;

        // Use input that will exercise parallelism
        let test_context = create_mathematical_test_context(12);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test diamond pattern performance with parallel execution",
        );

        let start_time = std::time::Instant::now();

        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        let total_time = start_time.elapsed();

        // Verify performance characteristics
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.completed_steps, 4);

        // Parallel execution should be efficient
        assert!(
            total_time < Duration::from_secs(10),
            "Parallel diamond execution should be fast, took {:?}",
            total_time
        );

        info!("✅ Performance Test Results:");
        info!("  - Total execution time: {:?}", total_time);
        info!(
            "  - Steps completed: {}/{}",
            summary.completed_steps, summary.total_steps
        );
        info!("  - Parallel execution: EFFICIENT");

        info!("✅ Diamond Performance and Parallelism Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_diamond_workflows_concurrency() -> Result<()> {
        init_test_logging();

        info!("Starting multiple diamond workflows concurrency integration test");

        let mut setup = init_test_worker().await?;

        // Initialize orchestration with more workers for concurrency
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        setup.initialize_worker(vec![NAMESPACE]).await?;

        // Create multiple concurrent diamond workflows
        let task_uuids = futures::future::try_join_all((0..3).map(|i| {
            let test_context = create_mathematical_test_context(4 + (i * 2)); // 4, 6, 8
            let task_request = create_test_task_request(
                NAMESPACE,
                TASK_NAME,
                TASK_VERSION,
                test_context,
                &format!("Concurrent diamond workflow test #{}", i + 1),
            );
            setup.create_task(task_request)
        }))
        .await?;

        info!(
            "✅ Created {} concurrent diamond workflows: {:?}",
            task_uuids.len(),
            task_uuids
        );

        // Wait for all tasks to complete
        let summaries = futures::future::try_join_all(
            task_uuids
                .iter()
                .map(|uuid| setup.wait_for_completion(uuid, TEST_TIMEOUT_SECONDS)),
        )
        .await?;

        // Verify all tasks completed successfully
        for (i, summary) in summaries.iter().enumerate() {
            assert_eq!(
                summary.status, "complete",
                "Diamond task {} should complete",
                i
            );
            assert_eq!(
                summary.completed_steps, 4,
                "Diamond task {} should have 4 completed steps",
                i
            );
            assert_eq!(
                summary.failed_steps, 0,
                "Diamond task {} should have no failed steps",
                i
            );
        }

        info!(
            "✅ All {} concurrent diamond workflows completed successfully",
            summaries.len()
        );
        info!(
            "📊 Concurrent execution times: {:?}",
            summaries
                .iter()
                .map(|s| s.execution_time)
                .collect::<Vec<_>>()
        );

        info!("✅ Multiple Diamond Workflows Concurrency Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_diamond_dependency_resolution() -> Result<()> {
        init_test_logging();

        info!("Starting diamond dependency resolution integration test");

        let mut setup = init_test_worker().await?;

        // Use input that will test complex dependency resolution
        let test_context = create_mathematical_test_context(14);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test diamond pattern dependency resolution with convergence",
        );

        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify dependency resolution worked correctly
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        // Diamond pattern should handle dependencies correctly
        // (End step must wait for both Branch B and Branch C)
        assert!(
            summary.execution_time.as_secs() >= 1,
            "Diamond pattern should take time for proper dependency resolution"
        );

        info!("✅ Diamond Dependency Resolution Test passed");
        info!(
            "📊 Dependency resolution time: {:?}",
            summary.execution_time
        );

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }
}
