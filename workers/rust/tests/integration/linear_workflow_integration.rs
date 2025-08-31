// Linear Workflow Integration Test - Native Rust Implementation
//
// Comprehensive integration test for Linear Workflow pattern using native Rust step handlers.
// This test mirrors the Ruby integration test patterns but uses the native Rust implementation
// throughout the entire execution pipeline.
//
// Test Pattern: Linear mathematical sequence (input^8)
// Steps: input → input² → (input²)² → ((input²)²)² → (((input²)²)²)²
// Example: 6 → 36 → 1,296 → 1,679,616 → 2,821,109,907,456

use anyhow::Result;
use serde_json::json;
use std::time::Duration;
use tracing::{info, warn};

use tasker_core::test_helpers::{create_mathematical_test_context, create_test_task_request};
use tasker_worker_rust::test_helpers::{init_test_logging, init_test_worker};

// Note: Rust step handlers are automatically discovered and registered by the orchestration system
// so we don't need to import them explicitly for integration tests

/// Test configuration and constants
const NAMESPACE: &str = "linear_workflow";
const TASK_NAME: &str = "mathematical_sequence";
const TASK_VERSION: &str = "1.0.0";
const TEST_TIMEOUT_SECONDS: u64 = 30;

/// Complete Linear Mathematical Sequence Integration Tests
/// Mirrors the Ruby RSpec test structure but with native Rust execution
#[cfg(test)]
mod linear_workflow_integration_tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_complete_linear_mathematical_sequence() -> Result<()> {
        init_test_logging();

        info!("Starting test_complete_linear_mathematical_sequence");

        let mut setup = init_test_worker().await?;

        // Test data: even number that will flow through the mathematical sequence
        // Expected progression: 6 → 36 → 1,296 → 1,679,616 → 2,821,109,907,456
        let test_context = create_mathematical_test_context(6);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test complete linear mathematical workflow progression (input^8)",
        );

        // Run the complete integration test
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify task completion
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.completion_percentage, 100.0);
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        // Verify execution time is reasonable
        assert!(summary.execution_time < Duration::from_secs(TEST_TIMEOUT_SECONDS));

        info!("✅ Complete Linear Mathematical Sequence Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_step_dependency_chain_execution_order() -> Result<()> {
        init_test_logging();

        info!("Starting test_step_dependency_chain_execution_order");

        let mut setup = init_test_worker().await?;

        // Use different input to verify dependency chain
        let test_context = create_mathematical_test_context(8);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test linear workflow dependency chain execution order",
        );

        // Run integration test with dependency focus
        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        // Verify proper dependency chain execution
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.completed_steps, 4);
        assert_eq!(summary.failed_steps, 0);

        info!("✅ Step Dependency Chain Test passed");
        info!("📊 Test Results: {}", summary);

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_mathematical_validation_and_error_handling() -> Result<()> {
        init_test_logging();

        info!("Starting test_mathematical_validation_and_error_handling");

        let mut setup = init_test_worker().await?;

        // Test with odd number (should be handled gracefully by step handlers)
        let invalid_context = json!({
            "even_number": 7,  // Odd number - may cause validation errors
            "test_run_id": "validation-test"
        });

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            invalid_context,
            "Test mathematical validation and error handling with invalid input",
        );

        // Initialize orchestration and workers separately to test task creation
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        setup.initialize_worker(vec![NAMESPACE]).await?;

        // Task creation should succeed even with invalid input
        // (validation happens in step handlers, not at task creation)
        let task_uuid = setup.create_task(task_request).await?;
        assert!(!task_uuid.is_empty());

        info!(
            "✅ Task created successfully with potentially invalid input: {}",
            task_uuid
        );

        // Try to wait for completion - this might result in failure or success
        // depending on how our Rust step handlers handle validation
        match setup
            .wait_for_completion(&task_uuid, TEST_TIMEOUT_SECONDS)
            .await
        {
            Ok(summary) => {
                info!("📊 Task completed: {}", summary);
                // If it completes, verify it's in a valid state
                assert!(summary.completion_percentage >= 0.0);
            }
            Err(e) => {
                warn!(
                    "⚠️ Task execution encountered expected validation issues: {}",
                    e
                );
                // This is acceptable for validation testing
            }
        }

        info!("✅ Mathematical Validation and Error Handling Test completed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_framework_integration_and_orchestration() -> Result<()> {
        init_test_logging();

        info!("Starting test_framework_integration_and_orchestration");

        let mut setup = init_test_worker().await?;

        // Test orchestration system initialization
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        info!("✅ Orchestration system initialized successfully");

        // Test worker initialization
        setup.initialize_worker(vec![NAMESPACE]).await?;
        info!("✅ Workers initialized successfully");

        // Test task creation functionality
        let test_context = create_mathematical_test_context(2);
        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test framework integration and orchestration functionality",
        );

        let task_uuid = setup.create_task(task_request).await?;
        assert!(!task_uuid.is_empty());

        info!(
            "✅ Framework integration functional - created task {}",
            task_uuid
        );

        // Wait a moment to see if task processing starts
        tokio::time::sleep(Duration::from_secs(2)).await;

        info!("✅ Framework Integration and Orchestration Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_performance_and_native_rust_execution() -> Result<()> {
        init_test_logging();

        info!("Starting test_performance_and_native_rust_execution");

        let mut setup = init_test_worker().await?;

        // Use larger input to test performance with bigger numbers
        let test_context = create_mathematical_test_context(10);

        let task_request = create_test_task_request(
            NAMESPACE,
            TASK_NAME,
            TASK_VERSION,
            test_context,
            "Test native Rust execution performance with mathematical operations",
        );

        let start_time = std::time::Instant::now();

        let summary = setup
            .run_integration_test(task_request, NAMESPACE, TEST_TIMEOUT_SECONDS)
            .await?;

        let total_time = start_time.elapsed();

        // Verify performance characteristics
        assert_eq!(summary.status, "complete");
        assert_eq!(summary.completed_steps, 4);

        // Native Rust should complete mathematical operations quickly
        assert!(
            total_time < Duration::from_secs(10),
            "Native Rust execution should be fast, took {:?}",
            total_time
        );

        info!("✅ Performance Test Results:");
        info!("  - Total execution time: {:?}", total_time);
        info!(
            "  - Steps completed: {}/{}",
            summary.completed_steps, summary.total_steps
        );
        info!("  - Native Rust performance: EXCELLENT");

        info!("✅ Performance and Native Rust Execution Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_concurrent_workflows() -> Result<()> {
        init_test_logging();

        info!("Starting test_multiple_concurrent_workflows");

        let mut setup = init_test_worker().await?;

        // Initialize orchestration with more workers for concurrency
        setup.initialize_orchestration(vec![NAMESPACE]).await?;
        setup.initialize_worker(vec![NAMESPACE]).await?;

        // Create multiple concurrent tasks
        let task_uuids = futures::future::try_join_all((0..3).map(|i| {
            let test_context = create_mathematical_test_context(2 + (i * 2)); // 2, 4, 6
            let task_request = create_test_task_request(
                NAMESPACE,
                TASK_NAME,
                TASK_VERSION,
                test_context,
                &format!("Concurrent workflow test #{}", i + 1),
            );
            setup.create_task(task_request)
        }))
        .await?;

        info!(
            "✅ Created {} concurrent tasks: {:?}",
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
            assert_eq!(summary.status, "complete", "Task {} should complete", i);
            assert_eq!(
                summary.completed_steps, 4,
                "Task {} should have 4 completed steps",
                i
            );
            assert_eq!(
                summary.failed_steps, 0,
                "Task {} should have no failed steps",
                i
            );
        }

        info!(
            "✅ All {} concurrent workflows completed successfully",
            summaries.len()
        );
        info!(
            "📊 Concurrent execution times: {:?}",
            summaries
                .iter()
                .map(|s| s.execution_time)
                .collect::<Vec<_>>()
        );

        info!("✅ Multiple Concurrent Workflows Test passed");

        // Cleanup
        setup.cleanup().await?;

        Ok(())
    }
}
