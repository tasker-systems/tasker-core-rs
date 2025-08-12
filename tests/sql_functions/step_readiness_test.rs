//! # SQL Function-Based Step Readiness Status Tests
//!
//! This module tests the SQL function `get_step_readiness_status()` which is critical
//! for workflow orchestration decisions.

use sqlx::PgPool;
use tasker_core::models::orchestration::StepReadinessStatus;
use uuid::Uuid;

// Import our comprehensive factory system
use crate::factories::{
    base::SqlxFactory, complex_workflows::ComplexWorkflowFactory,
    states::WorkflowStepTransitionFactory,
};
use tasker_core::models::core::task_transition::{NewTaskTransition, TaskTransition};

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to map factory errors to sqlx errors
    fn map_factory_error(e: crate::factories::base::FactoryError) -> sqlx::Error {
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

    /// Create a diamond workflow for readiness testing
    async fn create_diamond_workflow_for_readiness(
        pool: &PgPool,
    ) -> Result<(Uuid, Vec<Uuid>), sqlx::Error> {
        // Helper already initializes both task and step states
        create_task_with_initial_state(pool, ComplexWorkflowFactory::new().diamond()).await
    }

    #[sqlx::test]
    async fn test_initial_readiness_state(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;

        // Diamond pattern: A -> (B, C) -> D
        // Only A should be ready initially
        let ready_steps: Vec<_> = readiness.iter().filter(|s| s.ready_for_execution).collect();
        assert_eq!(
            ready_steps.len(),
            1,
            "Only root step should be ready initially"
        );

        // The ready step should be the first one (root)
        let root_step = ready_steps.first().unwrap();
        assert_eq!(root_step.workflow_step_uuid, step_uuids[0]);
        assert!(root_step.dependencies_satisfied);
        assert_eq!(root_step.current_state, "pending");
        assert_eq!(root_step.attempts, 0);
        assert!(root_step.retry_eligible);

        // Other steps should be blocked by dependencies
        let blocked_steps: Vec<_> = readiness
            .iter()
            .filter(|s| !s.ready_for_execution)
            .collect();
        assert_eq!(
            blocked_steps.len(),
            3,
            "Three steps should be blocked by dependencies"
        );

        for step in blocked_steps {
            if step.workflow_step_uuid != step_uuids[0] {
                // Non-root steps should have unsatisfied dependencies
                assert!(!step.dependencies_satisfied || step.current_state != "pending");
            }
        }

        Ok(())
    }

    #[sqlx::test]
    async fn test_readiness_after_root_completion(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        // Complete the root step (A)
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;

        // Now B and C should be ready (indices 1 and 2)
        let ready_steps: Vec<_> = readiness.iter().filter(|s| s.ready_for_execution).collect();
        assert_eq!(
            ready_steps.len(),
            2,
            "Both branch steps should be ready after root completes"
        );

        // Check that the ready steps are B and C
        let ready_ids: Vec<Uuid> = ready_steps.iter().map(|s| s.workflow_step_uuid).collect();
        assert!(ready_ids.contains(&step_uuids[1]));
        assert!(ready_ids.contains(&step_uuids[2]));

        // Root should be complete
        let root_status = readiness
            .iter()
            .find(|s| s.workflow_step_uuid == step_uuids[0])
            .unwrap();
        assert_eq!(root_status.current_state, "complete");
        assert!(!root_status.ready_for_execution);

        // Final merge step should still be blocked
        let merge_status = readiness
            .iter()
            .find(|s| s.workflow_step_uuid == step_uuids[3])
            .unwrap();
        assert!(!merge_status.ready_for_execution);
        assert!(!merge_status.dependencies_satisfied);

        Ok(())
    }

    #[sqlx::test]
    async fn test_readiness_with_partial_branch_completion(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        // Complete root and one branch
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[1], &pool)
            .await
            .map_err(map_factory_error)?;

        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;

        // Only the other branch (C) should be ready
        let ready_steps: Vec<_> = readiness.iter().filter(|s| s.ready_for_execution).collect();
        assert_eq!(
            ready_steps.len(),
            1,
            "Only remaining branch should be ready"
        );
        assert_eq!(ready_steps[0].workflow_step_uuid, step_uuids[2]);

        // Check merge step dependencies
        let merge_status = readiness
            .iter()
            .find(|s| s.workflow_step_uuid == step_uuids[3])
            .unwrap();
        assert_eq!(merge_status.total_parents, 2);
        assert_eq!(merge_status.completed_parents, 1); // Only B is complete
        assert!(!merge_status.dependencies_satisfied);

        Ok(())
    }

    #[sqlx::test]
    async fn test_readiness_with_error_and_retry(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        // Complete root
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        // Set one branch to error with retry eligibility
        WorkflowStepTransitionFactory::create_failed_lifecycle(
            step_uuids[1],
            "Network timeout",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;

        // Update retry information
        sqlx::query!(
            "UPDATE tasker_workflow_steps
             SET attempts = 1, retry_limit = 3, retryable = true,
                 last_attempted_at = NOW() - INTERVAL '30 seconds'
             WHERE workflow_step_uuid = $1",
            step_uuids[1]
        )
        .execute(&pool)
        .await?;

        // Back-date the error transition to be past the exponential backoff period
        sqlx::query!(
            "UPDATE tasker_workflow_step_transitions
             SET created_at = NOW() - INTERVAL '5 seconds'
             WHERE workflow_step_uuid = $1 AND to_state = 'error' AND most_recent = true",
            step_uuids[1]
        )
        .execute(&pool)
        .await?;

        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;

        // Find the error step
        let error_step = readiness
            .iter()
            .find(|s| s.workflow_step_uuid == step_uuids[1])
            .unwrap();

        assert_eq!(error_step.current_state, "error");
        assert!(error_step.retry_eligible);
        assert!(error_step.ready_for_execution); // Ready to retry
        assert_eq!(error_step.attempts, 1);
        assert_eq!(error_step.retry_limit, 3);

        Ok(())
    }

    #[sqlx::test]
    async fn test_readiness_with_exhausted_retries(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        // Complete root
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        // Set one branch to error with exhausted retries
        WorkflowStepTransitionFactory::create_failed_lifecycle(
            step_uuids[1],
            "Persistent failure",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;

        // Update to exhaust retries
        sqlx::query!(
            "UPDATE tasker_workflow_steps
             SET attempts = 3, retry_limit = 3, retryable = true
             WHERE workflow_step_uuid = $1",
            step_uuids[1]
        )
        .execute(&pool)
        .await?;

        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;

        // Find the error step
        let error_step = readiness
            .iter()
            .find(|s| s.workflow_step_uuid == step_uuids[1])
            .unwrap();

        assert_eq!(error_step.current_state, "error");
        assert!(!error_step.retry_eligible); // No more retries
        assert!(!error_step.ready_for_execution); // Cannot execute
        assert_eq!(error_step.attempts, 3);
        assert_eq!(error_step.retry_limit, 3);

        Ok(())
    }

    #[sqlx::test]
    async fn test_readiness_summary(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, _step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        let summary = StepReadinessStatus::get_readiness_summary(&pool, task_uuid).await?;

        assert_eq!(summary.total_steps, 4); // Diamond has 4 steps
        assert_eq!(summary.ready_steps, 1); // Only root is ready initially
        assert_eq!(summary.blocked_steps, 3); // Three steps blocked by dependencies
        assert_eq!(summary.failed_steps, 0);
        assert_eq!(summary.processing_steps, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn test_get_ready_for_task(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        // Complete root to make branches ready
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        let ready_steps = StepReadinessStatus::get_ready_for_task(&pool, task_uuid).await?;

        assert_eq!(ready_steps.len(), 2); // Both branches should be ready
        assert!(ready_steps.iter().all(|s| s.ready_for_execution));
        assert!(ready_steps.iter().all(|s| s.dependencies_satisfied));

        Ok(())
    }

    #[sqlx::test]
    async fn test_get_blocked_by_dependencies(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, _step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        let blocked_steps =
            StepReadinessStatus::get_blocked_by_dependencies(&pool, task_uuid).await?;

        // Initially, B, C, and D are blocked (only A is ready)
        assert_eq!(blocked_steps.len(), 3);
        assert!(blocked_steps.iter().all(|s| !s.dependencies_satisfied));

        Ok(())
    }

    #[sqlx::test]
    async fn test_all_steps_complete(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) = create_diamond_workflow_for_readiness(&pool).await?;

        // Initially not all complete
        assert!(!StepReadinessStatus::all_steps_complete(&pool, task_uuid).await?);

        // Complete all steps
        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;
        }

        // Now all should be complete
        assert!(StepReadinessStatus::all_steps_complete(&pool, task_uuid).await?);

        Ok(())
    }

    #[sqlx::test]
    async fn test_complex_workflow_readiness(pool: PgPool) -> sqlx::Result<()> {
        // Use Tree pattern for more complex dependency testing
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().tree()).await?;

        // Initialize all with pending state
        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::new()
                .for_workflow_step(step_uuid)
                .to_state("pending")
                .create(&pool)
                .await
                .map_err(map_factory_error)?;
        }

        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;

        // Tree pattern: A -> (B -> (D, E), C -> (F, G))
        // Only root should be ready
        assert_eq!(
            readiness.iter().filter(|s| s.ready_for_execution).count(),
            1
        );

        // Complete root
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;

        // Now B and C should be ready (branches)
        assert_eq!(
            readiness.iter().filter(|s| s.ready_for_execution).count(),
            2
        );

        Ok(())
    }
}
