//! Task Coordinator
//!
//! Coordinates task-level finalization when steps complete.

use std::sync::Arc;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::orchestration::lifecycle::task_finalization::TaskFinalizer;
use tasker_shared::models::core::workflow_step::WorkflowStep;
use tasker_shared::state_machine::{TaskEvent, TaskState, TaskStateMachine};
use tasker_shared::system_context::SystemContext;
use tasker_shared::{errors::OrchestrationResult, OrchestrationError};

/// Coordinates task finalization
#[derive(Clone)]
pub struct TaskCoordinator {
    context: Arc<SystemContext>,
    task_finalizer: TaskFinalizer,
}

impl TaskCoordinator {
    pub fn new(context: Arc<SystemContext>, task_finalizer: TaskFinalizer) -> Self {
        Self {
            context,
            task_finalizer,
        }
    }

    /// Coordinate task finalization after step completion
    ///
    /// This method checks if task finalization should be triggered after a step completes,
    /// and delegates to TaskFinalizer if appropriate.
    pub async fn coordinate_task_finalization(
        &self,
        step_uuid: &Uuid,
        status: &String,
        correlation_id: Uuid,
    ) -> OrchestrationResult<()> {
        debug!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            status = %status,
            "Status qualifies for finalization check - looking up WorkflowStep"
        );

        let workflow_step =
            WorkflowStep::find_by_id(self.context.database_pool(), *step_uuid).await?;

        match workflow_step {
            Some(workflow_step) => {
                self.check_and_finalize_task(workflow_step, step_uuid, correlation_id)
                    .await
            }
            None => {
                error!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    "Failed to find WorkflowStep"
                );
                Err(OrchestrationError::DatabaseError {
                    operation: format!("WorkflowStep.find for {step_uuid}"),
                    reason: format!("Failed to find WorkflowStep for step UUID: {step_uuid}"),
                })
            }
        }
    }

    /// Check task state and finalize if appropriate
    async fn check_and_finalize_task(
        &self,
        workflow_step: WorkflowStep,
        step_uuid: &Uuid,
        correlation_id: Uuid,
    ) -> OrchestrationResult<()> {
        // Create state machine for this task
        let mut task_state_machine = TaskStateMachine::for_task(
            workflow_step.task_uuid,
            self.context.database_pool().clone(),
            self.context.processor_uuid(),
        )
        .await?;

        // Check the current state of the task
        let current_state = task_state_machine.current_state().await?;

        debug!(
            correlation_id = %correlation_id,
            task_uuid = %workflow_step.task_uuid,
            current_state = ?current_state,
            step_uuid = %step_uuid,
            "Current task state before attempting transition"
        );

        // Only transition with StepCompleted if we're in StepsInProcess state
        // If we're already in EvaluatingResults, we need to check what the next transition should be
        let should_finalize = if current_state == TaskState::StepsInProcess {
            // We can transition with StepCompleted
            task_state_machine
                .transition(TaskEvent::StepCompleted(*step_uuid))
                .await?
        } else if current_state == TaskState::EvaluatingResults {
            // We're already evaluating results, need to check if we should finalize
            // This happens when multiple steps complete in quick succession
            debug!(
                correlation_id = %correlation_id,
                task_uuid = %workflow_step.task_uuid,
                "Task already in EvaluatingResults state, checking if finalization is needed"
            );
            true // Check if finalization is needed
        } else {
            debug!(
                correlation_id = %correlation_id,
                task_uuid = %workflow_step.task_uuid,
                current_state = ?current_state,
                "Task in state that doesn't require immediate finalization check"
            );
            false
        };

        if should_finalize {
            self.finalize_task(workflow_step.task_uuid, step_uuid, correlation_id)
                .await
        } else {
            error!(
                correlation_id = %correlation_id,
                task_uuid = %workflow_step.task_uuid,
                step_uuid = %step_uuid,
                "Failed to transition state machine"
            );
            Err(OrchestrationError::DatabaseError {
                operation: format!("TaskStateMachine.transition for {step_uuid}"),
                reason: "Failed to transition state machine".to_string(),
            })
        }
    }

    /// Finalize the task
    async fn finalize_task(
        &self,
        task_uuid: Uuid,
        step_uuid: &Uuid,
        correlation_id: Uuid,
    ) -> OrchestrationResult<()> {
        match self.task_finalizer.finalize_task(task_uuid).await {
            Ok(result) => {
                info!(
                    correlation_id = %correlation_id,
                    task_uuid = %task_uuid,
                    step_uuid = %step_uuid,
                    action = ?result.action,
                    reason = ?result.reason,
                    "Task finalization completed successfully"
                );
                Ok(())
            }
            Err(err) => {
                error!(
                    correlation_id = %correlation_id,
                    task_uuid = %task_uuid,
                    step_uuid = %step_uuid,
                    "Failed to finalize task"
                );
                Err(OrchestrationError::DatabaseError {
                    operation: format!("TaskFinalizer.finalize_task for {step_uuid}"),
                    reason: format!("Failed to finalize task: {err}"),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_coordinator_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer =
            Arc::new(crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(context.clone()).await?);
        let task_finalizer = TaskFinalizer::new(context.clone(), step_enqueuer);
        let coordinator = TaskCoordinator::new(context, task_finalizer);

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&coordinator.context) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_coordinator_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer =
            Arc::new(crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(context.clone()).await?);
        let task_finalizer = TaskFinalizer::new(context.clone(), step_enqueuer);
        let coordinator = TaskCoordinator::new(context.clone(), task_finalizer);

        let cloned = coordinator.clone();

        // Verify both share the same Arc
        assert_eq!(
            Arc::as_ptr(&coordinator.context),
            Arc::as_ptr(&cloned.context)
        );
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_coordinate_task_finalization_with_nonexistent_step(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer =
            Arc::new(crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(context.clone()).await?);
        let task_finalizer = TaskFinalizer::new(context.clone(), step_enqueuer);
        let coordinator = TaskCoordinator::new(context, task_finalizer);

        let nonexistent_step = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        let status = "complete".to_string();

        // Should return error for non-existent step
        let result = coordinator
            .coordinate_task_finalization(&nonexistent_step, &status, correlation_id)
            .await;

        assert!(result.is_err());
        Ok(())
    }
}

