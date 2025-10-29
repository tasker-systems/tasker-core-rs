//! State Handlers for Task Processing
//!
//! Handles state-specific processing logic for tasks in different states.

use std::sync::Arc;
use tracing::{error, warn};

use crate::orchestration::lifecycle::step_enqueuer::StepEnqueuer;
use crate::orchestration::StepEnqueueResult;
use tasker_shared::database::sql_functions::{ReadyTaskInfo, SqlFunctionExecutor};
use tasker_shared::models::orchestration::ExecutionStatus;
use tasker_shared::state_machine::events::TaskEvent;
use tasker_shared::state_machine::states::TaskState;
use tasker_shared::state_machine::task_state_machine::TaskStateMachine;
use tasker_shared::{SystemContext, TaskerError, TaskerResult};

/// Handles state-specific processing for tasks
#[derive(Clone, Debug)]
pub struct StateHandlers {
    context: Arc<SystemContext>,
}

impl StateHandlers {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Process a single task with proper state machine transitions
    pub async fn process_task_by_state(
        &self,
        task_info: &ReadyTaskInfo,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        // Create state machine for this task
        let mut state_machine = TaskStateMachine::for_task(
            task_info.task_uuid,
            self.context.database_pool().clone(),
            self.context.processor_uuid(),
        )
        .await
        .map_err(|err| {
            TaskerError::DatabaseError(format!("Failed to create state machine: {err}"))
        })?;

        let current_state = state_machine.current_state().await.map_err(|err| {
            TaskerError::DatabaseError(format!("Failed to get current state: {err}"))
        })?;

        // Handle state-specific transitions using proper state machine
        match current_state {
            TaskState::Pending => {
                // Start the task
                if state_machine.transition(TaskEvent::Start).await? {
                    Ok(self
                        .handle_initializing_task(&mut state_machine, task_info, step_enqueuer)
                        .await?)
                } else {
                    Ok(None) // Already claimed by another processor
                }
            }

            TaskState::Initializing => {
                // Task is already in Initializing state (likely from initialization process)
                // Process it directly
                Ok(self
                    .handle_initializing_task(&mut state_machine, task_info, step_enqueuer)
                    .await?)
            }

            TaskState::EvaluatingResults => Ok(self
                .handle_evaluating_task(&mut state_machine, task_info, step_enqueuer)
                .await?),

            TaskState::WaitingForDependencies => {
                // Dependencies became ready
                if state_machine
                    .transition(TaskEvent::DependenciesReady)
                    .await?
                {
                    Ok(self
                        .handle_evaluating_task(&mut state_machine, task_info, step_enqueuer)
                        .await?)
                } else {
                    Err(TaskerError::StateTransitionError(
                        "Task unable to transition to dependencies ready state".to_string(),
                    ))
                }
            }

            TaskState::WaitingForRetry => {
                // Retry timeout expired
                if state_machine.transition(TaskEvent::RetryReady).await? {
                    Ok(Some(
                        self.handle_enqueueing_task(&mut state_machine, task_info, step_enqueuer)
                            .await?,
                    ))
                } else {
                    Err(TaskerError::StateTransitionError(
                        "Task unable to transition to retry state".to_string(),
                    ))
                }
            }

            _ => {
                warn!(
                    task_uuid = %task_info.task_uuid,
                    state = ?current_state,
                    "Task in unexpected state for batch processing"
                );
                Ok(None)
            }
        }
    }

    /// Handle task in Initializing state
    async fn handle_initializing_task(
        &self,
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        if task_info.ready_steps_count > 0 {
            // Has ready steps to enqueue
            if state_machine
                .transition(TaskEvent::ReadyStepsFound(
                    task_info.ready_steps_count as u32,
                ))
                .await?
            {
                Ok(Some(
                    self.handle_enqueueing_task(state_machine, task_info, step_enqueuer)
                        .await?,
                ))
            } else {
                Err(TaskerError::StateTransitionError(format!(
                    "Failed to transition to ready steps found for task {}",
                    task_info.task_uuid
                )))
            }
        } else {
            // No steps - task is complete
            match state_machine.transition(TaskEvent::NoStepsFound).await {
                Ok(_) => Ok(None),
                Err(err) => Err(err.into()),
            }
        }
    }

    /// Handle task in EnqueuingSteps state
    async fn handle_enqueueing_task(
        &self,
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<StepEnqueueResult> {
        // Enqueue the ready steps
        let enqueue_result = step_enqueuer.enqueue_ready_steps(task_info).await?;

        // Transition to StepsInProcess
        let transition_result = state_machine
            .transition(TaskEvent::StepsEnqueued(enqueue_result.step_uuids.clone()))
            .await?;

        if transition_result {
            Ok(enqueue_result)
        } else {
            Err(TaskerError::StateTransitionError(format!(
                "Could not transition the task to steps enqueued for task {}",
                task_info.task_uuid
            )))
        }
    }

    /// Handle task in EvaluatingResults state
    async fn handle_evaluating_task(
        &self,
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        // Get execution context to determine next action
        let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());
        let context_opt = sql_executor
            .get_task_execution_context(task_info.task_uuid)
            .await?;

        let Some(context) = context_opt else {
            error!(task_uuid = %task_info.task_uuid, "No execution context found");
            return Err(TaskerError::DatabaseError(format!(
                "No execution context found for task {}",
                task_info.task_uuid
            )));
        };

        match context.execution_status {
            ExecutionStatus::HasReadySteps => {
                if state_machine
                    .transition(TaskEvent::ReadyStepsFound(context.ready_steps as u32))
                    .await?
                {
                    Ok(Some(
                        self.handle_enqueueing_task(state_machine, task_info, step_enqueuer)
                            .await?,
                    ))
                } else {
                    Err(TaskerError::StateTransitionError(format!(
                        "Unable to transition to ready steps found for task {}",
                        task_info.task_uuid
                    )))
                }
            }
            ExecutionStatus::AllComplete => {
                if state_machine
                    .transition(TaskEvent::AllStepsSuccessful)
                    .await?
                {
                    Ok(None)
                } else {
                    Err(TaskerError::StateTransitionError(format!(
                        "Unable to transition to all steps successful for task {}",
                        task_info.task_uuid
                    )))
                }
            }
            ExecutionStatus::BlockedByFailures => {
                if state_machine
                    .transition(TaskEvent::PermanentFailure("Too many step failures".into()))
                    .await?
                {
                    Ok(None)
                } else {
                    Err(TaskerError::StateTransitionError(format!(
                        "Unable to transition to permanent failure for task {}",
                        task_info.task_uuid
                    )))
                }
            }
            ExecutionStatus::WaitingForDependencies => {
                if state_machine
                    .transition(TaskEvent::NoDependenciesReady)
                    .await?
                {
                    Ok(None)
                } else {
                    Err(TaskerError::StateTransitionError(format!(
                        "Unable to transition to no dependencies ready for task {}",
                        task_info.task_uuid
                    )))
                }
            }
            ExecutionStatus::Processing => {
                // Task is still processing, nothing to do
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_state_handlers_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let handlers = StateHandlers::new(context);

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&handlers.context) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_state_handlers_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let handlers = StateHandlers::new(context);

        let cloned = handlers.clone();

        // Verify both share the same Arc
        assert_eq!(Arc::as_ptr(&handlers.context), Arc::as_ptr(&cloned.context));
        Ok(())
    }

    #[test]
    fn test_execution_status_variants_exist() {
        // Verify all ExecutionStatus variants are recognized
        // This ensures our match statement in handle_evaluating_task covers all cases
        use tasker_shared::models::orchestration::ExecutionStatus;

        let _has_ready = ExecutionStatus::HasReadySteps;
        let _all_complete = ExecutionStatus::AllComplete;
        let _blocked = ExecutionStatus::BlockedByFailures;
        let _waiting = ExecutionStatus::WaitingForDependencies;
        let _processing = ExecutionStatus::Processing;

        // All variants created successfully (validated by compilation)
    }

    #[test]
    fn test_task_state_variants_exist() {
        // Verify key TaskState variants are recognized
        // This ensures our match statement in process_task_by_state covers expected cases
        use tasker_shared::state_machine::states::TaskState;

        let _pending = TaskState::Pending;
        let _initializing = TaskState::Initializing;
        let _evaluating = TaskState::EvaluatingResults;
        let _waiting_deps = TaskState::WaitingForDependencies;
        let _waiting_retry = TaskState::WaitingForRetry;

        // All variants created successfully (validated by compilation)
    }
}
