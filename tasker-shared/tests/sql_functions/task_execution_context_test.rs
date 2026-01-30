//! # SQL Function-Based Task Execution Context Tests
//!
//! This module tests the SQL function `get_task_execution_context()` which provides
//! comprehensive workflow execution status used by TaskFinalizer.

use sqlx::PgPool;
use tasker_shared::models::orchestration::execution_status::ExecutionStatus;
use tasker_shared::models::orchestration::TaskExecutionContext;
use uuid::Uuid;

// Import our comprehensive factory system
use tasker_shared::models::core::task_transition::{NewTaskTransition, TaskTransition};
use tasker_shared::models::factories::{
    base::SqlxFactory, complex_workflows::ComplexWorkflowFactory, core::TaskFactory,
    states::WorkflowStepTransitionFactory,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Counter for generating unique task contexts to avoid identity_hash collisions.
    /// The identity_hash is deterministically computed from (named_task_uuid, context),
    /// so each task in a test must have a unique context.
    use std::sync::atomic::{AtomicU64, Ordering};
    static TASK_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Create a ComplexWorkflowFactory with a unique context to avoid identity_hash collisions.
    fn unique_workflow_factory(
        pattern_fn: fn(ComplexWorkflowFactory) -> ComplexWorkflowFactory,
    ) -> ComplexWorkflowFactory {
        let counter = TASK_COUNTER.fetch_add(1, Ordering::Relaxed);
        let unique_task = TaskFactory::new()
            .complex_workflow()
            .with_context(serde_json::json!({
                "workflow_type": "complex",
                "unique_test_id": counter,
                "unique_nonce": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }));
        pattern_fn(ComplexWorkflowFactory::new().with_task_factory(unique_task))
    }

    /// Helper to map factory errors to sqlx errors
    fn map_factory_error(e: tasker_shared::models::factories::base::FactoryError) -> sqlx::Error {
        sqlx::Error::Protocol(format!("Factory error: {e}"))
    }

    /// Helper to create a task with proper initial state transitions for task AND steps
    ///
    /// This mirrors what will eventually be the task initialization flow when receiving
    /// a TaskRequest through the orchestration layer.
    async fn create_task_with_initial_state(
        pool: &PgPool,
        factory: ComplexWorkflowFactory,
    ) -> Result<(Uuid, Vec<Uuid>), sqlx::Error> {
        let (task_uuid, step_uuids) = factory.create(pool).await.map_err(map_factory_error)?;

        // Create initial "pending" state for the task
        TaskTransition::create(
            pool,
            NewTaskTransition {
                task_uuid,
                to_state: "pending".to_string(),
                from_state: None,
                processor_uuid: None,
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
                .map_err(map_factory_error)?;
        }

        Ok((task_uuid, step_uuids))
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_for_pending_task(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, _step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid).await?;
        assert!(context.is_some());

        let ctx = context.unwrap();
        assert_eq!(ctx.task_uuid, task_uuid);
        assert_eq!(ctx.total_steps, 4); // Linear has 4 steps
        assert_eq!(ctx.completed_steps, 0);
        assert_eq!(ctx.failed_steps, 0);
        assert_eq!(ctx.pending_steps, 4);
        assert_eq!(ctx.in_progress_steps, 0);
        assert_eq!(ctx.ready_steps, 1); // Only first step is ready
        assert_eq!(ctx.execution_status, ExecutionStatus::HasReadySteps);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_all_complete(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        // Complete all steps
        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;
        }

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        assert_eq!(context.completed_steps, 4);
        assert_eq!(context.failed_steps, 0);
        assert_eq!(context.pending_steps, 0);
        assert_eq!(context.in_progress_steps, 0);
        assert_eq!(context.ready_steps, 0);
        assert_eq!(context.execution_status, ExecutionStatus::AllComplete);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_with_failures(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        // Set first step to error with retries exhausted
        WorkflowStepTransitionFactory::create_failed_lifecycle(
            step_uuids[0],
            "Critical failure",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;

        // Exhaust retries
        sqlx::query!(
            "UPDATE tasker.workflow_steps
             SET attempts = 3, max_attempts = 3
             WHERE workflow_step_uuid = $1",
            step_uuids[0]
        )
        .execute(&pool)
        .await?;

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        assert_eq!(context.failed_steps, 1);
        assert_eq!(context.pending_steps, 3);
        assert_eq!(context.ready_steps, 0); // No steps ready due to failed dependency
        assert_eq!(context.execution_status, ExecutionStatus::BlockedByFailures);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_with_retry_eligible_failures(
        pool: PgPool,
    ) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        // Set first step to error but with retries remaining
        WorkflowStepTransitionFactory::create_failed_lifecycle(
            step_uuids[0],
            "Temporary failure",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;

        // Set retry info
        sqlx::query!(
            "UPDATE tasker.workflow_steps
             SET attempts = 1, max_attempts = 3, retryable = true,
                 last_attempted_at = NOW() - INTERVAL '30 seconds'
             WHERE workflow_step_uuid = $1",
            step_uuids[0]
        )
        .execute(&pool)
        .await?;

        // Back-date the error transition so that next_retry_time is in the past.
        // The last_failures CTE picks up the most recent error transition's created_at,
        // which feeds calculate_step_next_retry_time. With attempts=1, the fallback
        // computes: failure_time + power(2, 1) seconds = failure_time + 2s.
        // We back-date to 120 seconds ago so the retry window is well past.
        sqlx::query!(
            "UPDATE tasker.workflow_step_transitions
             SET created_at = NOW() - INTERVAL '120 seconds'
             WHERE workflow_step_uuid = $1 AND to_state = 'error'",
            step_uuids[0]
        )
        .execute(&pool)
        .await?;

        // Transition the step from error to waiting_for_retry state.
        // The SQL function evaluate_step_state_readiness only considers steps
        // in 'pending' or 'waiting_for_retry' states as candidates for execution.
        WorkflowStepTransitionFactory::new()
            .for_workflow_step(step_uuids[0])
            .from_state("error")
            .to_state("waiting_for_retry")
            .create(&pool)
            .await
            .map_err(map_factory_error)?;

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        // Step is in waiting_for_retry, which the SQL function counts differently
        // from a terminal error state. The step should be ready to retry.
        assert_eq!(context.ready_steps, 1); // Failed step is ready to retry
        assert_eq!(context.execution_status, ExecutionStatus::HasReadySteps); // Has steps ready for retry

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_with_in_progress(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        // Set one step to in_progress
        if let Some(&step_uuid) = step_uuids.first() {
            WorkflowStepTransitionFactory::new()
                .for_workflow_step(step_uuid)
                .to_state("pending")
                .create(&pool)
                .await
                .map_err(map_factory_error)?;

            WorkflowStepTransitionFactory::new()
                .for_workflow_step(step_uuid)
                .from_state("pending")
                .to_in_progress()
                .create(&pool)
                .await
                .map_err(map_factory_error)?;
        }

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        assert_eq!(context.in_progress_steps, 1);
        assert!(context.in_progress_steps > 0);
        assert_eq!(context.execution_status, ExecutionStatus::Processing);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_diamond_workflow(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().diamond()).await?;

        // Note: Steps already initialized to pending by helper

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        assert_eq!(context.total_steps, 4);
        assert_eq!(context.ready_steps, 1); // Only root is ready

        // Complete root
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        assert_eq!(context.completed_steps, 1);
        assert_eq!(context.ready_steps, 2); // Both branches now ready

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_tree_workflow(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, _step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().tree()).await?;

        // Note: Steps already initialized by helper

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        assert_eq!(context.total_steps, 7); // Tree has 7 steps
        assert_eq!(context.ready_steps, 1); // Only root

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_mixed_dag(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, _step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().mixed_dag())
                .await?;

        // Note: Steps already initialized by helper

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        assert_eq!(context.total_steps, 7);
        assert_eq!(context.execution_status, ExecutionStatus::HasReadySteps);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_execution_context_helper_methods(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, _) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        // Test basic properties
        assert!(context.pending_steps > 0);
        assert_eq!(context.completed_steps, 0);
        assert!(context.ready_steps > 0);
        assert_eq!(context.in_progress_steps, 0);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_execution_status_determination(pool: PgPool) -> sqlx::Result<()> {
        // Test various scenarios for execution status
        // Each task must use a unique context to avoid identity_hash collisions.

        // 1. All complete
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, unique_workflow_factory(|f| f.linear())).await?;

        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;
        }

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.execution_status, ExecutionStatus::AllComplete);

        // 2. Processing (has in_progress steps)
        let (task_uuid2, step_uuids2) =
            create_task_with_initial_state(&pool, unique_workflow_factory(|f| f.linear())).await?;
        if let Some(&step_uuid) = step_uuids2.first() {
            WorkflowStepTransitionFactory::new()
                .for_workflow_step(step_uuid)
                .to_state("pending")
                .create(&pool)
                .await
                .map_err(map_factory_error)?;
            WorkflowStepTransitionFactory::new()
                .for_workflow_step(step_uuid)
                .from_state("pending")
                .to_in_progress()
                .create(&pool)
                .await
                .map_err(map_factory_error)?;
        }
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid2)
            .await?
            .unwrap();
        assert_eq!(context.execution_status, ExecutionStatus::Processing);

        // 3. Has ready steps
        let (task_uuid3, _) =
            create_task_with_initial_state(&pool, unique_workflow_factory(|f| f.diamond())).await?;
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid3)
            .await?
            .unwrap();
        assert_eq!(context.execution_status, ExecutionStatus::HasReadySteps);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_backoff_timing_information(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        // Create a failed step with backoff timing
        WorkflowStepTransitionFactory::create_failed_lifecycle(
            step_uuids[0],
            "API rate limit",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;

        // Set backoff information
        sqlx::query!(
            "UPDATE tasker.workflow_steps
             SET attempts = 1, max_attempts = 3, retryable = true,
                 backoff_request_seconds = 120,
                 last_attempted_at = NOW()
             WHERE workflow_step_uuid = $1",
            step_uuids[0]
        )
        .execute(&pool)
        .await?;

        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();

        // Should have backoff timing reflected in context
        assert_eq!(context.failed_steps, 1);
        // Backoff affects readiness calculations internally

        Ok(())
    }
}
