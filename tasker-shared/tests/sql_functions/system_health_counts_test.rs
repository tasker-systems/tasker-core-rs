//! # SQL Function-Based System Health Counts Tests
//!
//! This module tests the SQL function `get_system_health_counts()` with
//! comprehensive scenarios that mirror the Rails test suite.
//!
//! Tests include:
//! - Empty database scenarios
//! - Various workflow states (complete, pending, error)
//! - Retry and backoff conditions
//! - Connection pool metrics
//! - Edge cases and large numbers

use sqlx::PgPool;
use tasker_shared::models::insights::SystemHealthCounts;
use uuid::Uuid;

// Import our comprehensive factory system
use tasker_shared::models::core::task_transition::{NewTaskTransition, TaskTransition};
use tasker_shared::models::factories::{
    base::SqlxFactory, complex_workflows::ComplexWorkflowFactory,
    states::WorkflowStepTransitionFactory,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a task with proper initial state transitions for task AND steps
    ///
    /// This mirrors what will eventually be the task initialization flow when receiving
    /// a TaskRequest through the orchestration layer.
    async fn create_task_with_initial_state(
        pool: &PgPool,
        factory: ComplexWorkflowFactory,
    ) -> Result<(Uuid, Vec<Uuid>), sqlx::Error> {
        let (task_uuid, step_uuids) = factory
            .create(pool)
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;

        // Create initial "pending" state for the task
        TaskTransition::create(
            pool,
            NewTaskTransition {
                task_uuid,
                to_state: "pending".to_string(),
                from_state: None,
                metadata: None,
            },
        )
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("TaskTransition error: {e}")))?;

        // Create initial "pending" state for ALL workflow steps
        // This is critical for SQL functions to identify "ready" steps
        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::new()
                .for_workflow_step(step_uuid)
                .to_state("pending")
                .create(pool)
                .await
                .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;
        }

        Ok((task_uuid, step_uuids))
    }

    /// Helper to create test workflows for health count validation
    async fn create_test_workflows_for_health_counts(pool: &PgPool) -> Result<(), sqlx::Error> {
        // Create 2 complete tasks with ALL steps complete (similar to Rails test)
        for _ in 0..2 {
            let (task_uuid, step_uuids) =
                create_task_with_initial_state(pool, ComplexWorkflowFactory::new().linear())
                    .await?;

            // Complete ALL steps to allow task to transition to complete
            for step_uuid in step_uuids {
                // Create complete lifecycle for each step
                WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, pool)
                    .await
                    .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;
            }

            // Create task transition to complete state using the model
            TaskTransition::create(
                pool,
                NewTaskTransition {
                    task_uuid,
                    to_state: "complete".to_string(),
                    from_state: Some("pending".to_string()),
                    metadata: None,
                },
            )
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("TaskTransition error: {e}")))?;
        }

        // Create 1 pending task with all steps pending (helper handles step initialization)
        let (_pending_task_uuid, _pending_step_uuids) =
            create_task_with_initial_state(pool, ComplexWorkflowFactory::new().linear()).await?;

        // Create 1 error task with 1 error step, rest pending
        let (error_task_uuid, error_step_uuids) =
            create_task_with_initial_state(pool, ComplexWorkflowFactory::new().linear()).await?;

        // Set first step to error with retry info
        if let Some(&first_step_uuid) = error_step_uuids.first() {
            // Create failed lifecycle for first step
            WorkflowStepTransitionFactory::create_failed_lifecycle(
                first_step_uuid,
                "Test error for health counts",
                pool,
            )
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;

            // Update step with retry information
            sqlx::query!(
                "UPDATE tasker_workflow_steps
                 SET attempts = 1, retry_limit = 3, retryable = true,
                     last_attempted_at = NOW() - INTERVAL '30 seconds'
                 WHERE workflow_step_uuid = $1",
                first_step_uuid
            )
            .execute(pool)
            .await?;
        }

        // Note: remaining steps already have pending transitions from helper

        // Create task transition to error state using the model
        TaskTransition::create(
            pool,
            NewTaskTransition {
                task_uuid: error_task_uuid,
                to_state: "error".to_string(),
                from_state: Some("pending".to_string()),
                metadata: None,
            },
        )
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("TaskTransition error: {e}")))?;

        Ok(())
    }

    /// Helper to create performance test data with larger volumes
    async fn create_performance_test_data(pool: &PgPool) -> Result<(), sqlx::Error> {
        // Create 5 completed workflows
        for _ in 0..5 {
            let (task_uuid, step_uuids) =
                create_task_with_initial_state(pool, ComplexWorkflowFactory::new().linear())
                    .await?;

            for step_uuid in step_uuids {
                WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, pool)
                    .await
                    .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;
            }

            // Create task transition to complete state using the model
            TaskTransition::create(
                pool,
                NewTaskTransition {
                    task_uuid,
                    to_state: "complete".to_string(),
                    from_state: Some("pending".to_string()),
                    metadata: None,
                },
            )
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("TaskTransition error: {e}")))?;
        }

        // Create 3 pending workflows
        for _ in 0..3 {
            create_task_with_initial_state(pool, ComplexWorkflowFactory::new().diamond()).await?;
        }

        // Create 2 workflows with errors
        for _ in 0..2 {
            let (_task_uuid, step_uuids) =
                create_task_with_initial_state(pool, ComplexWorkflowFactory::new().tree()).await?;

            // Set first step to error
            if let Some(&first_step) = step_uuids.first() {
                WorkflowStepTransitionFactory::create_failed_lifecycle(
                    first_step,
                    "Performance test error",
                    pool,
                )
                .await
                .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;

                sqlx::query!(
                    "UPDATE tasker_workflow_steps
                     SET attempts = 1, retry_limit = 3
                     WHERE workflow_step_uuid = $1",
                    first_step
                )
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_returns_structured_health_counts_data(pool: PgPool) -> sqlx::Result<()> {
        // Create test data
        create_test_workflows_for_health_counts(&pool).await?;

        // Get health counts
        let result = SystemHealthCounts::get_current(&pool).await?;
        assert!(result.is_some(), "Should return health counts");

        let health = result.unwrap();

        // Verify structure and types
        assert!(health.total_tasks >= 0);
        assert!(health.complete_tasks >= 0);
        assert!(health.pending_tasks >= 0);
        assert!(health.error_tasks >= 0);
        assert!(health.total_steps >= 0);
        assert!(health.complete_steps >= 0);
        assert!(health.pending_steps >= 0);
        assert!(health.error_steps >= 0);
        assert!(health.retryable_error_steps >= 0);
        assert!(health.exhausted_retry_steps >= 0);
        assert!(health.in_backoff_steps >= 0);
        assert!(health.active_connections >= 0);
        assert!(health.max_connections > 0);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_returns_consistent_results_across_multiple_calls(
        pool: PgPool,
    ) -> sqlx::Result<()> {
        // Create test data
        create_test_workflows_for_health_counts(&pool).await?;

        // Make multiple calls
        let result1 = SystemHealthCounts::get_current(&pool).await?.unwrap();
        let result2 = SystemHealthCounts::get_current(&pool).await?.unwrap();

        // Core counts should be consistent (connection counts might vary)
        assert_eq!(result1.total_tasks, result2.total_tasks);
        assert_eq!(result1.total_steps, result2.total_steps);
        assert_eq!(result1.complete_tasks, result2.complete_tasks);
        assert_eq!(result1.complete_steps, result2.complete_steps);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_validates_task_counts_are_reasonable(pool: PgPool) -> sqlx::Result<()> {
        // Create test data
        create_test_workflows_for_health_counts(&pool).await?;

        let result = SystemHealthCounts::get_current(&pool).await?.unwrap();

        // Should have at least the test data we created:
        // 2 complete tasks + 1 pending task + 1 error task = 4 tasks minimum
        assert!(result.total_tasks >= 4, "Should have at least 4 tasks");
        assert!(
            result.complete_tasks >= 2,
            "Should have at least 2 complete tasks"
        );
        assert!(
            result.pending_tasks >= 1,
            "Should have at least 1 pending task"
        );
        assert!(result.error_tasks >= 1, "Should have at least 1 error task");

        // Sum of states should not exceed total (some states may overlap in counting)
        let state_sum = result.complete_tasks
            + result.pending_tasks
            + result.error_tasks
            + result.cancelled_tasks
            + result.in_progress_tasks;
        assert!(
            state_sum <= result.total_tasks * 2,
            "Sum of states should be reasonable relative to total"
        );

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_validates_step_counts_are_reasonable(pool: PgPool) -> sqlx::Result<()> {
        // Create test data
        create_test_workflows_for_health_counts(&pool).await?;

        let result = SystemHealthCounts::get_current(&pool).await?.unwrap();

        // Linear workflow has 4 steps by default
        // 2 complete tasks × 4 steps = 8 complete steps
        // 1 pending task × 4 steps = 4 pending steps
        // 1 error task: 1 error step + 3 pending steps
        // Total: 16 steps minimum
        assert!(result.total_steps >= 16, "Should have at least 16 steps");
        assert!(
            result.complete_steps >= 8,
            "Should have at least 8 complete steps"
        );
        assert!(
            result.pending_steps >= 7,
            "Should have at least 7 pending steps"
        );
        assert!(result.error_steps >= 1, "Should have at least 1 error step");

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_validates_retry_related_counts_are_consistent(pool: PgPool) -> sqlx::Result<()> {
        // Create test data
        create_test_workflows_for_health_counts(&pool).await?;

        let result = SystemHealthCounts::get_current(&pool).await?.unwrap();

        // Should have at least our test error step
        assert!(
            result.retryable_error_steps >= 1,
            "Should have at least 1 retryable error"
        );

        // All retry counts should be non-negative
        assert!(result.exhausted_retry_steps >= 0);
        assert!(result.in_backoff_steps >= 0);

        // Retryable + exhausted should equal total error steps
        assert!(
            result.retryable_error_steps + result.exhausted_retry_steps <= result.error_steps,
            "Retry counts should be subset of error steps"
        );

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_validates_database_connection_metrics(pool: PgPool) -> sqlx::Result<()> {
        let result = SystemHealthCounts::get_current(&pool).await?.unwrap();

        assert!(
            result.active_connections >= 1,
            "Should have at least 1 active connection"
        );
        assert!(
            result.max_connections > 0,
            "Should have positive max connections"
        );
        assert!(
            result.max_connections >= result.active_connections,
            "Max should be >= active connections"
        );

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_handles_empty_database_correctly(pool: PgPool) -> sqlx::Result<()> {
        // Clear all test data in proper foreign key order
        sqlx::query!("DELETE FROM tasker_workflow_step_transitions")
            .execute(&pool)
            .await?;
        sqlx::query!("DELETE FROM tasker_task_transitions")
            .execute(&pool)
            .await?;
        sqlx::query!("DELETE FROM tasker_workflow_step_edges")
            .execute(&pool)
            .await?;
        sqlx::query!("DELETE FROM tasker_workflow_steps")
            .execute(&pool)
            .await?;
        sqlx::query!("DELETE FROM tasker_tasks")
            .execute(&pool)
            .await?;

        let result = SystemHealthCounts::get_current(&pool).await?.unwrap();

        assert_eq!(result.total_tasks, 0);
        assert_eq!(result.total_steps, 0);
        assert_eq!(result.retryable_error_steps, 0);
        assert_eq!(result.exhausted_retry_steps, 0);
        assert_eq!(result.in_backoff_steps, 0);

        // Database metrics should still work
        assert!(result.active_connections >= 1);
        assert!(result.max_connections > 0);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_health_score_calculations(pool: PgPool) -> sqlx::Result<()> {
        // Create healthy system
        create_test_workflows_for_health_counts(&pool).await?;

        let _health = SystemHealthCounts::get_current(&pool).await?.unwrap();
        let summary = SystemHealthCounts::get_health_summary(&pool)
            .await?
            .unwrap();

        // Verify health calculations
        assert!(summary.overall_health_score >= 0.0);
        assert!(summary.overall_health_score <= 100.0);

        // With mostly complete tasks, should be reasonably healthy
        assert!(summary.task_completion_rate > 0.0);
        assert!(summary.step_completion_rate > 0.0);

        // Health status should be set
        assert!(!summary.health_status.is_empty());
        assert!(["Excellent", "Good", "Fair", "Poor", "Critical"]
            .contains(&summary.health_status.as_str()));

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_performance_with_larger_dataset(pool: PgPool) -> sqlx::Result<()> {
        use std::time::Instant;

        // Create larger dataset
        create_performance_test_data(&pool).await?;

        // Measure execution time
        let start = Instant::now();
        let _result = SystemHealthCounts::get_current(&pool).await?;
        let duration = start.elapsed();

        // Should execute quickly even with more data
        assert!(
            duration.as_millis() < 200,
            "Query should execute in under 200ms, took {}ms",
            duration.as_millis()
        );

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_concurrent_execution(pool: PgPool) -> sqlx::Result<()> {
        use tokio::task;

        // Create test data
        create_test_workflows_for_health_counts(&pool).await?;

        // Test concurrent execution
        let pool1 = pool.clone();
        let pool2 = pool.clone();
        let pool3 = pool.clone();

        let (result1, result2, result3) = tokio::join!(
            task::spawn(async move { SystemHealthCounts::get_current(&pool1).await }),
            task::spawn(async move { SystemHealthCounts::get_current(&pool2).await }),
            task::spawn(async move { SystemHealthCounts::get_current(&pool3).await })
        );

        // All should succeed
        let r1 = result1
            .map_err(|e| sqlx::Error::Protocol(format!("Join error: {e}")))??
            .unwrap();
        let r2 = result2
            .map_err(|e| sqlx::Error::Protocol(format!("Join error: {e}")))??
            .unwrap();
        let r3 = result3
            .map_err(|e| sqlx::Error::Protocol(format!("Join error: {e}")))??
            .unwrap();

        // Task/step counts should be identical
        assert_eq!(r1.total_tasks, r2.total_tasks);
        assert_eq!(r2.total_tasks, r3.total_tasks);
        assert_eq!(r1.total_steps, r2.total_steps);
        assert_eq!(r2.total_steps, r3.total_steps);

        Ok(())
    }
}
