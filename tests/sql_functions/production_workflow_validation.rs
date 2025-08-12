//! # Production Workflow Validation Tests
//!
//! This module provides comprehensive end-to-end validation of workflow execution
//! patterns that mirror real production scenarios. These tests validate:
//!
//! - Complete task lifecycle management
//! - Step dependency resolution and execution order
//! - Error handling and retry mechanisms
//! - State transition accuracy and consistency
//! - SQL function integration with orchestration layer
//! - Performance characteristics under realistic loads
//!
//! ## Test Architecture
//!
//! Tests use the corrected SQL functions and comprehensive factory system to create
//! realistic workflow scenarios that would be encountered in production environments.
//! Each test validates both the correctness of the workflow execution and the
//! performance characteristics.

use sqlx::PgPool;
use std::time::{Duration, Instant};
use tasker_core::models::orchestration::{StepReadinessStatus, TaskExecutionContext};

// Import our comprehensive factory system
use crate::factories::{
    base::SqlxFactory,
    complex_workflows::{ComplexWorkflowFactory, WorkflowPattern},
    states::WorkflowStepTransitionFactory,
};
use tasker_core::models::core::task_transition::{NewTaskTransition, TaskTransition};
use uuid::Uuid;

#[cfg(test)]
mod production_validation_tests {
    use super::*;

    /// Helper to map factory errors to sqlx errors
    fn map_factory_error(e: crate::factories::base::FactoryError) -> sqlx::Error {
        sqlx::Error::Protocol(format!("Factory error: {e}"))
    }

    /// Helper to create a production-ready task with proper initialization
    async fn create_production_task(
        pool: &PgPool,
        pattern: WorkflowPattern,
    ) -> Result<(Uuid, Vec<Uuid>), sqlx::Error> {
        let (task_uuid, step_uuids) = ComplexWorkflowFactory::new()
            .with_pattern(pattern)
            .create(pool)
            .await
            .map_err(map_factory_error)?;

        // Initialize with proper task state
        TaskTransition::create(
            pool,
            NewTaskTransition {
                task_uuid,
                to_state: "pending".to_string(),
                from_state: None,
                metadata: Some(serde_json::json!({
                    "source": "production_validation",
                    "pattern": format!("{:?}", pattern),
                    "initialized_at": chrono::Utc::now().to_rfc3339(),
                })),
            },
        )
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("TaskTransition error: {e}")))?;

        // Initialize all workflow steps with pending state
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

    /// Test complete linear workflow execution from start to finish
    #[sqlx::test]
    async fn test_complete_linear_workflow_execution(pool: PgPool) -> sqlx::Result<()> {
        let start_time = Instant::now();
        let (task_uuid, step_uuids) =
            create_production_task(&pool, WorkflowPattern::Linear).await?;

        // Validate initial state
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.total_steps, 4);
        assert_eq!(context.ready_steps, 1);
        assert_eq!(context.execution_status, "has_ready_steps");

        // Execute steps in dependency order
        for (i, &step_uuid) in step_uuids.iter().enumerate() {
            // Verify step is ready
            let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;
            let ready_step = readiness.iter().find(|s| s.workflow_step_uuid == step_uuid);

            if i == 0 {
                // First step should be ready
                assert!(ready_step.unwrap().ready_for_execution);
            } else {
                // Other steps should only be ready after previous steps complete
                let ready_count = readiness.iter().filter(|s| s.ready_for_execution).count();
                assert!(
                    ready_count <= 1,
                    "Only one step should be ready at a time in linear workflow"
                );
            }

            // Execute the step
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;

            // Validate intermediate state
            let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
                .await?
                .unwrap();
            assert_eq!(context.completed_steps, (i + 1) as i64);

            if i < step_uuids.len() - 1 {
                assert_eq!(context.ready_steps, 1, "Next step should be ready");
                assert_eq!(context.execution_status, "has_ready_steps");
            } else {
                assert_eq!(
                    context.ready_steps, 0,
                    "No steps should be ready when complete"
                );
                assert_eq!(context.execution_status, "all_complete");
            }
        }

        // Final validation
        let final_context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(final_context.completed_steps, 4);
        assert_eq!(final_context.failed_steps, 0);
        assert_eq!(final_context.execution_status, "all_complete");

        let execution_time = start_time.elapsed();
        assert!(
            execution_time < Duration::from_secs(5),
            "Workflow should complete quickly"
        );

        Ok(())
    }

    /// Test diamond workflow with parallel branch execution
    #[sqlx::test]
    async fn test_diamond_workflow_parallel_execution(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_production_task(&pool, WorkflowPattern::Diamond).await?;

        // Initial state: only root should be ready
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.total_steps, 4);
        assert_eq!(context.ready_steps, 1);

        // Execute root step (A)
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        // Now both branches should be ready (B and C)
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.completed_steps, 1);
        assert_eq!(context.ready_steps, 2, "Both branches should be ready");

        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;
        let ready_steps: Vec<_> = readiness.iter().filter(|s| s.ready_for_execution).collect();
        assert_eq!(ready_steps.len(), 2);
        assert!(ready_steps
            .iter()
            .any(|s| s.workflow_step_uuid == step_uuids[1]));
        assert!(ready_steps
            .iter()
            .any(|s| s.workflow_step_uuid == step_uuids[2]));

        // Execute both branches in parallel (simulate)
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[1], &pool)
            .await
            .map_err(map_factory_error)?;
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[2], &pool)
            .await
            .map_err(map_factory_error)?;

        // Now merge step should be ready (D)
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.completed_steps, 3);
        assert_eq!(context.ready_steps, 1, "Merge step should be ready");

        // Complete merge step
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[3], &pool)
            .await
            .map_err(map_factory_error)?;

        // Final validation
        let final_context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(final_context.completed_steps, 4);
        assert_eq!(final_context.ready_steps, 0);
        assert_eq!(final_context.execution_status, "all_complete");

        Ok(())
    }

    /// Test error handling and retry mechanisms in production scenario
    #[sqlx::test]
    async fn test_production_error_handling_and_retry(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_production_task(&pool, WorkflowPattern::Linear).await?;

        // Execute first step successfully
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        // Simulate error in second step
        WorkflowStepTransitionFactory::create_failed_lifecycle(
            step_uuids[1],
            "Simulated production error - external API timeout",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;

        // Set retry information with proper backoff timing
        sqlx::query!(
            "UPDATE tasker_workflow_steps
             SET attempts = 1, retry_limit = 3, retryable = true,
                 last_attempted_at = NOW() - INTERVAL '30 seconds'
             WHERE workflow_step_uuid = $1",
            step_uuids[1]
        )
        .execute(&pool)
        .await?;

        // Back-date error to be past backoff period
        sqlx::query!(
            "UPDATE tasker_workflow_step_transitions
             SET created_at = NOW() - INTERVAL '5 seconds'
             WHERE workflow_step_uuid = $1 AND to_state = 'error' AND most_recent = true",
            step_uuids[1]
        )
        .execute(&pool)
        .await?;

        // Validate error state
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.completed_steps, 1);
        assert_eq!(context.failed_steps, 1);
        assert_eq!(
            context.ready_steps, 1,
            "Failed step should be ready for retry"
        );
        assert_eq!(context.execution_status, "has_ready_steps");

        // Verify step is marked as ready for retry
        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;
        let failed_step = readiness
            .iter()
            .find(|s| s.workflow_step_uuid == step_uuids[1])
            .unwrap();
        assert_eq!(failed_step.current_state, "error");
        assert!(failed_step.retry_eligible);
        assert!(failed_step.ready_for_execution);

        // Simulate successful retry
        WorkflowStepTransitionFactory::create_retry_lifecycle(step_uuids[1], 2, &pool)
            .await
            .map_err(map_factory_error)?;

        // Validate recovery
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.completed_steps, 2);
        assert_eq!(
            context.failed_steps, 0,
            "Failed steps should be reset after successful retry"
        );
        assert_eq!(context.ready_steps, 1, "Next step should be ready");

        Ok(())
    }

    /// Test complex tree workflow with multiple dependency levels
    #[sqlx::test]
    async fn test_complex_tree_workflow_execution(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) = create_production_task(&pool, WorkflowPattern::Tree).await?;

        // Tree pattern: A -> (B -> (D, E), C -> (F, G))
        // Validate initial state
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.total_steps, 7);
        assert_eq!(context.ready_steps, 1, "Only root should be ready");

        // Execute root (A)
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        // Branches B and C should be ready
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.completed_steps, 1);
        assert_eq!(context.ready_steps, 2);

        // Execute branch B
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[1], &pool)
            .await
            .map_err(map_factory_error)?;

        // D and E should be ready (children of B), C still ready
        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;
        let ready_count = readiness.iter().filter(|s| s.ready_for_execution).count();
        assert_eq!(ready_count, 3, "C + D + E should be ready");

        // Execute C
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[2], &pool)
            .await
            .map_err(map_factory_error)?;

        // Now all leaf nodes should be ready: D, E, F, G
        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;
        let ready_count = readiness.iter().filter(|s| s.ready_for_execution).count();
        assert_eq!(ready_count, 4, "All leaf nodes should be ready");

        // Complete all leaf nodes
        for &step_uuid in &step_uuids[3..] {
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;
        }

        // Final validation
        let final_context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(final_context.completed_steps, 7);
        assert_eq!(final_context.ready_steps, 0);
        assert_eq!(final_context.execution_status, "all_complete");

        Ok(())
    }

    /// Test mixed DAG workflow with complex dependencies
    #[sqlx::test]
    async fn test_mixed_dag_workflow_complex_dependencies(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_production_task(&pool, WorkflowPattern::MixedDAG).await?;

        // Mixed DAG: A -> B, A -> C, B -> D, C -> D, B -> E, C -> F, (D,E,F) -> G
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.total_steps, 7);
        assert_eq!(context.ready_steps, 1, "Only init should be ready");

        // Execute A (init)
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        // B and C should be ready
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.ready_steps, 2);

        // Execute B
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[1], &pool)
            .await
            .map_err(map_factory_error)?;

        // D and E should be ready (D needs B, E needs B), C still ready
        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;
        let ready_count = readiness.iter().filter(|s| s.ready_for_execution).count();
        assert!(ready_count >= 2, "Multiple steps should be ready");

        // Execute C
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[2], &pool)
            .await
            .map_err(map_factory_error)?;

        // Now D, E, F should all be ready (D needs B+C, E needs B, F needs C)
        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;
        let ready_steps: Vec<_> = readiness.iter().filter(|s| s.ready_for_execution).collect();
        assert!(ready_steps.len() >= 3, "D, E, F should be ready");

        // Execute D, E, F
        for &step_uuid in &step_uuids[3..6] {
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;
        }

        // G should be ready (needs D + E + F)
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.ready_steps, 1, "Final step should be ready");

        // Complete G
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[6], &pool)
            .await
            .map_err(map_factory_error)?;

        // Final validation
        let final_context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(final_context.completed_steps, 7);
        assert_eq!(final_context.execution_status, "all_complete");

        Ok(())
    }

    /// Test performance characteristics with multiple concurrent workflows
    #[sqlx::test]
    async fn test_concurrent_workflow_performance(pool: PgPool) -> sqlx::Result<()> {
        let start_time = Instant::now();
        let mut task_uuids = Vec::new();

        // Create 10 concurrent workflows of different patterns
        for i in 0..10 {
            let pattern = match i % 4 {
                0 => WorkflowPattern::Linear,
                1 => WorkflowPattern::Diamond,
                2 => WorkflowPattern::Tree,
                _ => WorkflowPattern::MixedDAG,
            };

            let (task_uuid, _) = create_production_task(&pool, pattern).await?;
            task_uuids.push(task_uuid);
        }

        let creation_time = start_time.elapsed();
        assert!(
            creation_time < Duration::from_secs(2),
            "Task creation should be fast"
        );

        // Validate all tasks are properly initialized
        for task_uuid in &task_uuids {
            let context = TaskExecutionContext::get_for_task(&pool, *task_uuid).await?;
            assert!(context.is_some());
            let ctx = context.unwrap();
            assert!(ctx.ready_steps > 0, "Each task should have ready steps");
            assert_eq!(ctx.execution_status, "has_ready_steps");
        }

        let validation_time = start_time.elapsed();
        assert!(
            validation_time < Duration::from_secs(3),
            "Validation should be fast"
        );

        // Test concurrent readiness queries
        let readiness_start = Instant::now();
        for task_uuid in &task_uuids {
            let _readiness = StepReadinessStatus::get_for_task(&pool, *task_uuid).await?;
        }
        let readiness_time = readiness_start.elapsed();
        assert!(
            readiness_time < Duration::from_secs(1),
            "Readiness queries should be fast"
        );

        Ok(())
    }

    /// Test workflow with exhausted retries and failure scenarios
    #[sqlx::test]
    async fn test_production_failure_scenarios(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_production_task(&pool, WorkflowPattern::Linear).await?;

        // Execute first step successfully
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        // Simulate step with exhausted retries
        WorkflowStepTransitionFactory::create_failed_lifecycle(
            step_uuids[1],
            "Critical system failure - database unavailable",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;

        // Exhaust retries
        sqlx::query!(
            "UPDATE tasker_workflow_steps
             SET attempts = 3, retry_limit = 3, retryable = false
             WHERE workflow_step_uuid = $1",
            step_uuids[1]
        )
        .execute(&pool)
        .await?;

        // Validate blocked state
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.completed_steps, 1);
        assert_eq!(context.failed_steps, 1);
        assert_eq!(
            context.ready_steps, 0,
            "No steps should be ready when blocked"
        );
        assert_eq!(context.execution_status, "blocked_by_failures");

        // Verify no steps are ready
        let readiness = StepReadinessStatus::get_for_task(&pool, task_uuid).await?;
        let ready_count = readiness.iter().filter(|s| s.ready_for_execution).count();
        assert_eq!(
            ready_count, 0,
            "No steps should be ready when permanently failed"
        );

        // Test manual resolution
        WorkflowStepTransitionFactory::create_manual_resolution_lifecycle(
            step_uuids[1],
            "production_admin",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;

        // After manual resolution, next step should be ready
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(
            context.completed_steps, 2,
            "Should count resolved as completed"
        );
        assert_eq!(context.failed_steps, 0, "Failed count should reset");
        assert_eq!(context.ready_steps, 1, "Next step should be ready");

        Ok(())
    }

    /// Test data consistency across multiple state transitions
    #[sqlx::test]
    async fn test_state_transition_data_consistency(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_production_task(&pool, WorkflowPattern::Diamond).await?;

        // Track state transitions for consistency validation
        let mut execution_log = Vec::new();

        // Execute root and track
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;
        execution_log.push(("root_complete", Instant::now()));

        // Validate consistency after each major transition
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid).await?;
        assert!(context.is_some());

        // Execute branches with error in one
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[1], &pool)
            .await
            .map_err(map_factory_error)?;
        execution_log.push(("branch_1_complete", Instant::now()));

        // Error in branch 2
        WorkflowStepTransitionFactory::create_failed_lifecycle(
            step_uuids[2],
            "Network partition error",
            &pool,
        )
        .await
        .map_err(map_factory_error)?;
        execution_log.push(("branch_2_error", Instant::now()));

        // Set retry with backoff
        sqlx::query!(
            "UPDATE tasker_workflow_steps
             SET attempts = 1, retry_limit = 3, retryable = true,
                 last_attempted_at = NOW() - INTERVAL '30 seconds'
             WHERE workflow_step_uuid = $1",
            step_uuids[2]
        )
        .execute(&pool)
        .await?;

        // Back-date error
        sqlx::query!(
            "UPDATE tasker_workflow_step_transitions
             SET created_at = NOW() - INTERVAL '5 seconds'
             WHERE workflow_step_uuid = $1 AND to_state = 'error' AND most_recent = true",
            step_uuids[2]
        )
        .execute(&pool)
        .await?;

        // Validate intermediate state
        let context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(context.completed_steps, 2);
        assert_eq!(context.failed_steps, 1);
        assert_eq!(context.ready_steps, 1); // Failed step ready for retry

        // Retry and succeed
        WorkflowStepTransitionFactory::create_retry_lifecycle(step_uuids[2], 2, &pool)
            .await
            .map_err(map_factory_error)?;
        execution_log.push(("branch_2_retry_success", Instant::now()));

        // Complete final step
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[3], &pool)
            .await
            .map_err(map_factory_error)?;
        execution_log.push(("merge_complete", Instant::now()));

        // Final consistency validation
        let final_context = TaskExecutionContext::get_for_task(&pool, task_uuid)
            .await?
            .unwrap();
        assert_eq!(final_context.completed_steps, 4);
        assert_eq!(final_context.failed_steps, 0);
        assert_eq!(final_context.execution_status, "all_complete");

        // Verify execution log makes sense
        assert_eq!(execution_log.len(), 5);

        // Validate timing sequence
        for i in 1..execution_log.len() {
            assert!(
                execution_log[i].1 > execution_log[i - 1].1,
                "Events should be in chronological order"
            );
        }

        Ok(())
    }
}
