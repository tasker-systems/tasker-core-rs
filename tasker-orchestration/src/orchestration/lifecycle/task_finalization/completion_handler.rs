//! Completion Handler
//!
//! Handles task completion and error state transitions with proper state machine integration.

use std::sync::Arc;
use tracing::error;
use uuid::Uuid;

use tasker_shared::models::orchestration::TaskExecutionContext;
use tasker_shared::models::Task;
use tasker_shared::state_machine::{TaskEvent, TaskState, TaskStateMachine};
use tasker_shared::system_context::SystemContext;

use super::{FinalizationAction, FinalizationError, FinalizationResult};

/// Handles task completion and error state transitions
#[derive(Clone)]
pub struct CompletionHandler {
    context: Arc<SystemContext>,
}

impl CompletionHandler {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Get state machine for a task
    pub async fn get_state_machine_for_task(
        &self,
        task: &Task,
    ) -> Result<TaskStateMachine, FinalizationError> {
        TaskStateMachine::for_task(
            task.task_uuid,
            self.context.database_pool().clone(),
            self.context.processor_uuid(),
        )
        .await
        .map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to create state machine: {e}"),
            task_uuid: task.task_uuid,
        })
    }

    /// Complete a task successfully
    pub async fn complete_task(
        &self,
        mut task: Task,
        context: Option<TaskExecutionContext>,
        correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;

        // Use state machine for proper state transitions
        let mut state_machine = self.get_state_machine_for_task(&task).await?;

        // Get current state
        let current_state =
            state_machine
                .current_state()
                .await
                .map_err(|e| FinalizationError::StateMachine {
                    error: format!("Failed to get current state: {e}"),
                    task_uuid,
                })?;

        // If task is already complete, just return
        if current_state == TaskState::Complete {
            return Ok(FinalizationResult {
                task_uuid,
                action: FinalizationAction::Completed,
                completion_percentage: context
                    .as_ref()
                    .and_then(|c| c.completion_percentage.to_string().parse().ok()),
                total_steps: context.as_ref().map(|c| c.total_steps as i32),
                health_status: context.as_ref().map(|c| c.health_status.clone()),
                enqueued_steps: None,
                reason: None,
            });
        }

        // Handle proper state transitions based on current state
        match current_state {
            TaskState::EvaluatingResults => {
                // Transition from EvaluatingResults to Complete
                state_machine
                    .transition(TaskEvent::AllStepsSuccessful)
                    .await
                    .map_err(|e| FinalizationError::StateMachine {
                        error: format!(
                            "Failed to transition from EvaluatingResults to Complete: {e}"
                        ),
                        task_uuid,
                    })?;
            }
            TaskState::StepsInProcess => {
                // Need to go through: StepsInProcess -> EvaluatingResults -> Complete
                state_machine
                    .transition(TaskEvent::AllStepsCompleted)
                    .await
                    .map_err(|e| FinalizationError::StateMachine {
                        error: format!(
                            "Failed to transition from StepsInProcess to EvaluatingResults: {e}"
                        ),
                        task_uuid,
                    })?;

                // Now transition from EvaluatingResults to Complete
                state_machine
                    .transition(TaskEvent::AllStepsSuccessful)
                    .await
                    .map_err(|e| FinalizationError::StateMachine {
                        error: format!(
                            "Failed to transition from EvaluatingResults to Complete: {e}"
                        ),
                        task_uuid,
                    })?;
            }
            TaskState::Pending => {
                // This should not happen anymore with the proper initialization fix
                error!(
                    correlation_id = %correlation_id,
                    task_uuid = %task_uuid,
                    current_state = %current_state,
                    "Task is still in Pending state when trying to complete - this indicates an initialization issue"
                );
                return Err(FinalizationError::StateMachine {
                    error: "Task should not be in Pending state when trying to complete. This indicates the task was not properly initialized.".to_string(),
                    task_uuid,
                });
            }
            TaskState::Initializing => {
                // This means task has no steps and can be completed directly
                state_machine
                    .transition(TaskEvent::NoStepsFound)
                    .await
                    .map_err(|e| FinalizationError::StateMachine {
                        error: format!("Failed to transition from Initializing to Complete: {e}"),
                        task_uuid,
                    })?;
            }
            _ => {
                // For other states, try the legacy Complete event as a fallback
                state_machine
                    .transition(TaskEvent::Complete)
                    .await
                    .map_err(|e| FinalizationError::StateMachine {
                        error: format!(
                            "Failed to transition to complete from {}: {e}",
                            current_state
                        ),
                        task_uuid,
                    })?;
            }
        }

        // Update the task complete flag (this might be redundant if the state machine action handles it)
        task.mark_complete(self.context.database_pool()).await?;

        Ok(FinalizationResult {
            task_uuid,
            action: FinalizationAction::Completed,
            completion_percentage: context
                .as_ref()
                .and_then(|c| c.completion_percentage.to_string().parse().ok()),
            total_steps: context.as_ref().map(|c| c.total_steps as i32),
            health_status: context.as_ref().map(|c| c.health_status.clone()),
            enqueued_steps: None,
            reason: None,
        })
    }

    /// Mark a task as failed due to errors
    pub async fn error_task(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        _correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;

        // Use state machine for proper state transitions
        let mut state_machine = self.get_state_machine_for_task(&task).await?;

        // Transition to BlockedByFailures state using PermanentFailure event
        // This is the correct event from EvaluatingResults state
        let error_message = "Steps in error state".to_string();
        state_machine
            .transition(TaskEvent::PermanentFailure(error_message.clone()))
            .await
            .map_err(|e| FinalizationError::StateMachine {
                error: format!("Failed to transition to blocked state: {e}"),
                task_uuid,
            })?;

        Ok(FinalizationResult {
            task_uuid,
            action: FinalizationAction::Failed,
            completion_percentage: context
                .as_ref()
                .and_then(|c| c.completion_percentage.to_string().parse().ok()),
            total_steps: context.as_ref().map(|c| c.total_steps as i32),
            health_status: context.as_ref().map(|c| c.health_status.clone()),
            enqueued_steps: None,
            reason: Some("Steps in error state".to_string()),
        })
    }
}
