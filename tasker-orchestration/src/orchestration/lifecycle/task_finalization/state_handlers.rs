//! State Handlers
//!
//! Handles different task execution states during finalization.

use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;

use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::models::orchestration::{ExecutionStatus, TaskExecutionContext};
use tasker_shared::models::Task;
use tasker_shared::state_machine::{TaskEvent, TaskState, TaskStateMachine};
use tasker_shared::system_context::SystemContext;

use super::completion_handler::CompletionHandler;
use super::execution_context_provider::ExecutionContextProvider;
use super::{FinalizationAction, FinalizationError, FinalizationResult};

/// Handles different task execution states
#[derive(Clone, Debug)]
pub struct StateHandlers {
    sql_executor: SqlFunctionExecutor,
    step_enqueuer_service: Arc<StepEnqueuerService>,
    context_provider: ExecutionContextProvider,
    completion_handler: CompletionHandler,
}

impl StateHandlers {
    pub fn new(
        context: Arc<SystemContext>,
        step_enqueuer_service: Arc<StepEnqueuerService>,
    ) -> Self {
        let sql_executor = SqlFunctionExecutor::new(context.database_pool().clone());
        let context_provider = ExecutionContextProvider::new(context.clone());
        let completion_handler = CompletionHandler::new(context.clone());

        Self {
            sql_executor,
            step_enqueuer_service,
            context_provider,
            completion_handler,
        }
    }

    /// Get state machine for a task
    async fn get_state_machine_for_task(
        &self,
        task: &Task,
    ) -> Result<TaskStateMachine, FinalizationError> {
        self.completion_handler
            .get_state_machine_for_task(task)
            .await
    }

    /// Handle ready steps state - should execute the ready steps
    pub async fn handle_ready_steps_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;
        let ready_steps = context.as_ref().map(|c| c.ready_steps).unwrap_or(0);

        debug!(
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            ready_steps = ready_steps,
            "TaskFinalizer: Task has ready steps - transitioning to in_progress"
        );

        // Use state machine to transition to in_progress if needed
        let mut state_machine = self.get_state_machine_for_task(&task).await?;

        let current_state =
            state_machine
                .current_state()
                .await
                .map_err(|e| FinalizationError::StateMachine {
                    error: format!("Failed to get current state: {e}"),
                    task_uuid,
                })?;

        // TAS-67: Defensive check - if task is already in BlockedByFailures state,
        // don't try to enqueue more steps. This can happen when SQL returns
        // has_ready_steps because there are still retryable steps waiting,
        // but another step has already permanently failed.
        if current_state == TaskState::BlockedByFailures {
            warn!(
                correlation_id = %correlation_id,
                task_uuid = %task_uuid,
                ready_steps = ready_steps,
                "Task state is already BlockedByFailures - calling error_task instead of enqueueing"
            );
            return self
                .completion_handler
                .error_task(task, context, correlation_id)
                .await;
        }

        // Transition to active processing state if not already active or complete
        let is_active = matches!(
            current_state,
            TaskState::EnqueuingSteps | TaskState::StepsInProcess | TaskState::EvaluatingResults
        );

        if !is_active && current_state != TaskState::Complete {
            state_machine
                .transition(TaskEvent::Start)
                .await
                .map_err(|e| FinalizationError::StateMachine {
                    error: format!("Failed to transition to active state: {e}"),
                    task_uuid,
                })?;
        }

        // Use TaskClaimStepEnqueuer for step processing
        debug!(
            correlation_id = %correlation_id,
            task_uuid = %task.task_uuid,
            "Processing ready steps with TaskClaimStepEnqueuer"
        );
        let maybe_task_info = self.sql_executor.get_task_ready_info(task_uuid).await?;
        match maybe_task_info {
            Some(task_info) => {
                if let Some(enqueue_result) = self
                    .step_enqueuer_service
                    .process_single_task_from_ready_info(&task_info)
                    .await?
                {
                    Ok(FinalizationResult {
                        task_uuid: task.task_uuid,
                        action: FinalizationAction::Reenqueued,
                        completion_percentage: context
                            .as_ref()
                            .and_then(|c| c.completion_percentage.to_string().parse().ok()),
                        total_steps: context.as_ref().map(|c| c.total_steps as i32),
                        health_status: context.as_ref().map(|c| c.health_status.clone()),
                        enqueued_steps: Some(enqueue_result.steps_enqueued as i32),
                        reason: Some("Ready steps enqueued".to_string()),
                    })
                } else {
                    // No steps were enqueued - task may be blocked or have no ready steps
                    let failed_steps = context.as_ref().map(|c| c.failed_steps).unwrap_or(0);
                    let total_steps = context.as_ref().map(|c| c.total_steps).unwrap_or(0);

                    Err(FinalizationError::General(format!(
                        "No ready steps to enqueue for task {} (failed: {}/{} steps) - task may be blocked by errors or have no executable steps remaining",
                        task.task_uuid,
                        failed_steps,
                        total_steps
                    )))
                }
            }
            None => {
                // Task info not found - may indicate task is in an invalid state
                let failed_steps = context.as_ref().map(|c| c.failed_steps).unwrap_or(0);
                let total_steps = context.as_ref().map(|c| c.total_steps).unwrap_or(0);

                Err(FinalizationError::General(format!(
                    "No task ready info found for task {} (failed: {}/{} steps) - task may have no steps or be in invalid state",
                    task.task_uuid,
                    failed_steps,
                    total_steps
                )))
            }
        }
    }

    /// Handle waiting for dependencies state
    pub async fn handle_waiting_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        // Defensive check: verify we're not blocked by errors before trying to re-enqueue
        if let Some(ref ctx) = context {
            if ctx.execution_status == ExecutionStatus::BlockedByFailures {
                warn!(
                    correlation_id = %correlation_id,
                    task_uuid = %task.task_uuid,
                    "Task in waiting state is actually blocked by failures, transitioning to error"
                );
                return self
                    .completion_handler
                    .error_task(task, context, correlation_id)
                    .await;
            }

            // Additional verification: check if all failed steps are permanent errors
            // This catches cases where SQL function might not have detected BlockedByFailures
            if ctx.failed_steps > 0 && ctx.ready_steps == 0 {
                // If we have failures but no ready steps, verify these aren't all permanent
                let is_blocked = self
                    .context_provider
                    .blocked_by_errors(task.task_uuid)
                    .await?;
                if is_blocked {
                    warn!(
                        correlation_id = %correlation_id,
                        task_uuid = %task.task_uuid,
                        failed_steps = ctx.failed_steps,
                        ready_steps = ctx.ready_steps,
                        "Independent verification detected task is blocked by permanent errors - SQL function may be out of sync"
                    );
                    return self
                        .completion_handler
                        .error_task(task, context, correlation_id)
                        .await;
                }
            }
        }

        info!(
            correlation_id = %correlation_id,
            task_uuid = %task.task_uuid,
            "Handling waiting state by delegating to ready steps state"
        );
        self.handle_ready_steps_state(task, context, correlation_id)
            .await
    }

    /// Handle processing state
    pub async fn handle_processing_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        debug!(
            correlation_id = %correlation_id,
            task_uuid = %task.task_uuid,
            "Handling processing state, no action taken"
        );

        Ok(FinalizationResult {
            task_uuid: task.task_uuid,
            action: FinalizationAction::NoAction,
            completion_percentage: context
                .as_ref()
                .and_then(|c| c.completion_percentage.to_string().parse().ok()),
            total_steps: context.as_ref().map(|c| c.total_steps as i32),
            health_status: context.as_ref().map(|c| c.health_status.clone()),
            enqueued_steps: None,
            reason: Some("Steps in progress".to_string()),
        })
    }

    /// Handle unclear task state
    pub async fn handle_unclear_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;
        error!(
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            "TaskFinalizer: Task has no execution context and unclear state"
        );
        // Without context, attempt to transition to error state
        self.completion_handler
            .error_task(task, context, correlation_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_state_handlers_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test that StateHandlers implements Clone
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuerService::new(context.clone()).await?);
        let handlers = StateHandlers::new(context.clone(), step_enqueuer.clone());

        let cloned = handlers.clone();

        // Verify both share the same Arc
        assert_eq!(
            Arc::as_ptr(&handlers.step_enqueuer_service),
            Arc::as_ptr(&cloned.step_enqueuer_service)
        );
        Ok(())
    }

    #[test]
    fn test_processing_state_result_structure() {
        // Test the result structure for processing state (NoAction)
        let task_uuid = Uuid::new_v4();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::NoAction,
            completion_percentage: Some(50.0),
            total_steps: Some(10),
            enqueued_steps: None,
            health_status: Some("processing".to_string()),
            reason: Some("Steps in progress".to_string()),
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::NoAction));
        assert_eq!(result.reason, Some("Steps in progress".to_string()));
    }

    #[test]
    fn test_finalization_result_for_reenqueued_state() {
        // Test the result structure for reenqueued state
        let task_uuid = Uuid::new_v4();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::Reenqueued,
            completion_percentage: Some(40.0),
            total_steps: Some(10),
            enqueued_steps: Some(3),
            health_status: Some("healthy".to_string()),
            reason: Some("Ready steps enqueued".to_string()),
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::Reenqueued));
        assert_eq!(result.enqueued_steps, Some(3));
        assert_eq!(result.reason, Some("Ready steps enqueued".to_string()));
    }

    #[test]
    fn test_finalization_action_variants() {
        // Test that all FinalizationAction variants can be created
        let _completed = FinalizationAction::Completed;
        let _failed = FinalizationAction::Failed;
        let _pending = FinalizationAction::Pending;
        let _reenqueued = FinalizationAction::Reenqueued;
        let _no_action = FinalizationAction::NoAction;

        // Verify they're all different
        assert!(!matches!(
            FinalizationAction::Completed,
            FinalizationAction::Failed
        ));
        assert!(!matches!(
            FinalizationAction::NoAction,
            FinalizationAction::Reenqueued
        ));
    }
}
