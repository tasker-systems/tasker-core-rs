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

/// Action to take when processing a step completion based on current task state
enum CoordinatorAction {
    /// Transition task state and attempt finalization
    TransitionAndFinalize,
    /// Task already evaluating, just check if finalization needed
    CheckFinalization,
    /// Idempotent no-op with reason (late/duplicate message)
    IdempotentNoOp(&'static str),
    /// Unexpected state that shouldn't receive step completions
    UnexpectedState,
}

/// Coordinates task finalization
#[derive(Clone, Debug)]
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
            "Current task state before determining action"
        );

        // Exhaustively match on current_state to determine action
        // This ensures we explicitly handle all TaskState variants
        let action = match current_state {
            // Active processing states - attempt transition and finalization
            TaskState::StepsInProcess => CoordinatorAction::TransitionAndFinalize,

            // Already evaluating - just check finalization (concurrent step completions)
            TaskState::EvaluatingResults => CoordinatorAction::CheckFinalization,

            // Terminal states - idempotent no-op (duplicate/retried messages)
            TaskState::Complete
            | TaskState::Error
            | TaskState::Cancelled
            | TaskState::ResolvedManually => {
                CoordinatorAction::IdempotentNoOp("Task already in terminal state")
            }

            // Waiting/transitional states - idempotent no-op (late-arriving messages)
            TaskState::WaitingForDependencies => CoordinatorAction::IdempotentNoOp(
                "Task waiting for dependencies (e.g., after batch edge creation)",
            ),
            TaskState::WaitingForRetry => {
                CoordinatorAction::IdempotentNoOp("Task waiting for retry timeout")
            }
            TaskState::EnqueuingSteps => {
                CoordinatorAction::IdempotentNoOp("Task currently enqueuing newly-ready steps")
            }

            // Blocked state - idempotent no-op (other steps failed, this one succeeded late)
            TaskState::BlockedByFailures => CoordinatorAction::IdempotentNoOp(
                "Task blocked by failures from other parallel steps",
            ),

            // Initial states - unexpected (step shouldn't complete before task starts)
            TaskState::Pending | TaskState::Initializing => CoordinatorAction::UnexpectedState,
        };

        // Execute the determined action
        match action {
            CoordinatorAction::TransitionAndFinalize => {
                // Transition state machine and finalize if transition succeeds
                let transition_result = task_state_machine
                    .transition(TaskEvent::StepCompleted(*step_uuid))
                    .await?;

                if transition_result {
                    self.finalize_task(workflow_step.task_uuid, step_uuid, correlation_id)
                        .await
                } else {
                    error!(
                        correlation_id = %correlation_id,
                        task_uuid = %workflow_step.task_uuid,
                        step_uuid = %step_uuid,
                        current_state = ?current_state,
                        "Transition failed for StepsInProcess state"
                    );
                    Err(OrchestrationError::DatabaseError {
                        operation: format!("TaskStateMachine.transition for {step_uuid}"),
                        reason: format!("Failed to transition from {current_state:?}"),
                    })
                }
            }

            CoordinatorAction::CheckFinalization => {
                // Already in EvaluatingResults, just check finalization
                debug!(
                    correlation_id = %correlation_id,
                    task_uuid = %workflow_step.task_uuid,
                    "Task already in EvaluatingResults, checking finalization"
                );
                self.finalize_task(workflow_step.task_uuid, step_uuid, correlation_id)
                    .await
            }

            CoordinatorAction::IdempotentNoOp(reason) => {
                // Idempotent handling - late or duplicate message, safe to ignore
                debug!(
                    correlation_id = %correlation_id,
                    task_uuid = %workflow_step.task_uuid,
                    step_uuid = %step_uuid,
                    current_state = ?current_state,
                    reason = %reason,
                    "Treating step result as idempotent no-op"
                );
                Ok(())
            }

            CoordinatorAction::UnexpectedState => {
                // Unexpected state - log error and fail
                error!(
                    correlation_id = %correlation_id,
                    task_uuid = %workflow_step.task_uuid,
                    step_uuid = %step_uuid,
                    current_state = ?current_state,
                    "Step completion received while task in unexpected state"
                );
                Err(OrchestrationError::DatabaseError {
                    operation: format!("TaskCoordinator.check_and_finalize for {step_uuid}"),
                    reason: format!(
                        "Task in unexpected state {:?} for step completion",
                        current_state
                    ),
                })
            }
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
        let step_enqueuer = Arc::new(
            crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(
                context.clone(),
            )
            .await?,
        );
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
        let step_enqueuer = Arc::new(
            crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(
                context.clone(),
            )
            .await?,
        );
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
        let step_enqueuer = Arc::new(
            crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(
                context.clone(),
            )
            .await?,
        );
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
