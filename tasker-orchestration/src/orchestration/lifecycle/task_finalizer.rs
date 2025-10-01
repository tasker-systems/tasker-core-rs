//! # Task Finalizer
//!
//! Handles task completion and finalization logic with state machine integration.
//!
//! ## Overview
//!
//! The TaskFinalizer provides implementation for task finalization while firing
//! lifecycle events for observability. Enhanced with TaskExecutionContext
//! integration for intelligent decision making and state transitions.
//!
//! ## Key Features
//!
//! - **Context-Driven Decisions**: Uses task execution context to determine next actions
//! - **State Machine Integration**: Leverages state transitions for atomic operations
//! - **Event Publishing**: Comprehensive lifecycle events for observability
//! - **Error Handling**: Robust error state management and recovery
//! - **Reenqueue Logic**: Intelligent task reenqueuing with context-aware delays
//!
//! ## Delay Types and Separation of Concerns
//!
//! This module handles **task-level reenqueue delays** which are different from
//! **step-level retry delays**:
//!
//! - **Task Reenqueue Delays** (handled here): Delays between task orchestration attempts
//!   - Used when a task needs to be re-enqueued for continued processing
//!   - Based on task execution context (has_ready_steps, waiting_for_dependencies, etc.)
//!   - Configured via `BackoffConfig.reenqueue_delays` in system configuration
//!   - Typical range: 0-45 seconds
//!
//! - **Step Retry Delays** (handled by BackoffCalculator): Delays between individual step retry attempts
//!   - Used when a specific step fails and needs to be retried
//!   - Based on step execution context and error information
//!   - Configured via `BackoffCalculator` with exponential backoff
//!   - Typical range: 1-300 seconds with exponential growth
//!   - Persisted in database `backoff_request_seconds` field
//!
//! ## Rails Heritage
//!
//! Migrated from `lib/tasker/orchestration/task_finalizer.rb` with enhanced
//! type safety and performance optimizations.

use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::orchestration::lifecycle::step_enqueuer_service::StepEnqueuerService;
use std::sync::Arc;

use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::events::types::{Event, OrchestrationEvent, TaskResult};
use tasker_shared::models::orchestration::{ExecutionStatus, TaskExecutionContext};
use tasker_shared::models::Task;
use tasker_shared::state_machine::{TaskEvent, TaskState, TaskStateMachine};
use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerError;

/// Result of task finalization operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizationResult {
    /// Task ID that was finalized
    pub task_uuid: Uuid,
    /// Final action taken
    pub action: FinalizationAction,
    /// Completion percentage if completed
    pub completion_percentage: Option<f64>,
    /// Total number of steps in task
    pub total_steps: Option<i32>,
    /// Number of steps enqueued in this pass
    pub enqueued_steps: Option<i32>,
    /// Health status of the task
    pub health_status: Option<String>,
    /// Reason for the action (if applicable)
    pub reason: Option<String>,
}

/// Type of finalization action taken
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinalizationAction {
    /// Task was completed successfully
    Completed,
    /// Task was marked as failed due to errors
    Failed,
    /// Task was set to pending state
    Pending,
    /// Task was reenqueued for further processing
    Reenqueued,
    /// No action taken due to unclear state
    NoAction,
}

// TaskExecutionContext is now imported from tasker_shared::models::orchestration

/// TaskFinalizer handles task completion and finalization logic
///
/// This component provides implementation for task finalization while firing
/// lifecycle events for observability. Enhanced with TaskExecutionContext
/// integration for intelligent decision making.
#[derive(Clone)]
pub struct TaskFinalizer {
    context: Arc<SystemContext>,
    sql_executor: SqlFunctionExecutor,
    step_enqueuer_service: Arc<StepEnqueuerService>,
}

impl TaskFinalizer {
    /// Create a new TaskFinalizer without step enqueuer (backward compatible)
    pub fn new(
        context: Arc<SystemContext>,
        step_enqueuer_service: Arc<StepEnqueuerService>,
    ) -> Self {
        let sql_executor = SqlFunctionExecutor::new(context.database_pool().clone());

        Self {
            context,
            sql_executor,
            step_enqueuer_service,
        }
    }

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

    /// Check if the task is blocked by errors
    ///
    /// @param task_uuid The task ID to check
    /// @return True if task is blocked by errors
    pub async fn blocked_by_errors(&self, task_uuid: Uuid) -> Result<bool, FinalizationError> {
        let context = self.get_task_execution_context(task_uuid).await?;

        // If no context is available, the task has no steps or doesn't exist
        // In either case, it's not blocked by errors
        let Some(context) = context else {
            return Ok(false);
        };

        Ok(context.execution_status == ExecutionStatus::BlockedByFailures)
    }

    /// Finalize a task based on its current state using TaskExecutionContext
    ///
    /// @param task_uuid The task ID to finalize
    pub async fn finalize_task(
        &self,
        task_uuid: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task = Task::find_by_id(self.context.database_pool(), task_uuid).await?;
        let Some(task) = task else {
            return Err(FinalizationError::TaskNotFound { task_uuid });
        };

        let context = self.get_task_execution_context(task_uuid).await?;

        let finalization_result = self.make_finalization_decision(task, context).await?;

        Ok(finalization_result)
    }

    /// Complete a task successfully
    async fn complete_task(
        &self,
        mut task: Task,
        context: Option<TaskExecutionContext>,
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

        // Publish completion event
        self.publish_task_completed(task_uuid, &context).await?;

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
    async fn error_task(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;

        // Use state machine for proper state transitions
        let mut state_machine = self.get_state_machine_for_task(&task).await?;

        // Transition to error state using state machine
        let error_message = "Steps in error state".to_string();
        state_machine
            .transition(TaskEvent::Fail(error_message.clone()))
            .await
            .map_err(|e| FinalizationError::StateMachine {
                error: format!("Failed to transition to error: {e}"),
                task_uuid,
            })?;

        // Publish failure event
        self.publish_task_failed(task_uuid, &context).await?;

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

    /// Get task execution context using unified TaskExecutionContext
    async fn get_task_execution_context(
        &self,
        task_uuid: Uuid,
    ) -> Result<Option<TaskExecutionContext>, FinalizationError> {
        match TaskExecutionContext::get_for_task(self.context.database_pool(), task_uuid).await {
            Ok(context) => Ok(context),
            Err(e) => {
                debug!(
                    task_uuid = %task_uuid,
                    error = %e,
                    "Failed to get task execution context"
                );
                Ok(None) // Context not available due to error
            }
        }
    }

    /// Make finalization decision based on task state
    async fn make_finalization_decision(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;

        debug!(
            task_uuid = %task_uuid,
            execution_status = ?context.as_ref().map(|c| &c.execution_status),
            "TaskFinalizer: Making decision for task"
        );

        // Handle nil context case
        let Some(context) = context else {
            debug!(
                task_uuid = %task_uuid,
                "TaskFinalizer: Task - no context available, handling as unclear state"
            );
            return self.handle_unclear_state(task, None).await;
        };

        match context.execution_status {
            ExecutionStatus::AllComplete => {
                debug!(task_uuid = %task_uuid, "TaskFinalizer: Task - calling complete_task");
                self.complete_task(task, Some(context)).await
            }
            ExecutionStatus::BlockedByFailures => {
                debug!(task_uuid = %task_uuid, "TaskFinalizer: Task - calling error_task");
                self.error_task(task, Some(context)).await
            }
            ExecutionStatus::HasReadySteps => {
                debug!(task_uuid = %task_uuid, "TaskFinalizer: Task - has ready steps, should execute them");
                self.handle_ready_steps_state(task, Some(context)).await
            }
            ExecutionStatus::WaitingForDependencies => {
                debug!(task_uuid = %task_uuid, "TaskFinalizer: Task - waiting for dependencies");
                self.handle_waiting_state(task, Some(context)).await
            }
            ExecutionStatus::Processing => {
                debug!(task_uuid = %task_uuid, "TaskFinalizer: Task - handling processing state");
                self.handle_processing_state(task, Some(context)).await
            }
        }
    }

    /// Handle ready steps state - should execute the ready steps
    async fn handle_ready_steps_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;
        let ready_steps = context.as_ref().map(|c| c.ready_steps).unwrap_or(0);

        debug!(
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
            task_uuid = task.task_uuid.to_string(),
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
    async fn handle_waiting_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        // Defensive check: verify we're not blocked by errors before trying to re-enqueue
        if let Some(ref ctx) = context {
            if ctx.execution_status == ExecutionStatus::BlockedByFailures {
                warn!(
                    task_uuid = %task.task_uuid,
                    "Task in waiting state is actually blocked by failures, transitioning to error"
                );
                return self.error_task(task, context).await;
            }

            // Additional verification: check if all failed steps are permanent errors
            // This catches cases where SQL function might not have detected BlockedByFailures
            if ctx.failed_steps > 0 && ctx.ready_steps == 0 {
                // If we have failures but no ready steps, verify these aren't all permanent
                let is_blocked = self.blocked_by_errors(task.task_uuid).await?;
                if is_blocked {
                    warn!(
                        task_uuid = %task.task_uuid,
                        failed_steps = ctx.failed_steps,
                        ready_steps = ctx.ready_steps,
                        "Independent verification detected task is blocked by permanent errors - SQL function may be out of sync"
                    );
                    return self.error_task(task, context).await;
                }
            }
        }

        info!(
            "Handling waiting state for task {} by delegating to ready steps state",
            task.task_uuid
        );
        self.handle_ready_steps_state(task, context).await
    }

    /// Handle processing state
    async fn handle_processing_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        debug!(
            "Handling processing state for task {}, no action taken",
            task.task_uuid
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
    async fn handle_unclear_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;
        error!("TaskFinalizer: Task {task_uuid} has no execution context and unclear state");
        // Without context, attempt to transition to error state
        self.error_task(task, context).await
    }

    async fn publish_task_completed(
        &self,
        task_uuid: Uuid,
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        // Publish structured orchestration event for task completion
        let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
            task_uuid,
            result: TaskResult::Success,
            completed_at: chrono::Utc::now(),
        });

        self.context
            .event_publisher
            .publish_event(event)
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task completed event: {e}"
                ))
            })?;

        // Also publish generic event for broader observability
        self.context
            .event_publisher
            .publish(
                "task.completed",
                json!({
                    "task_uuid": task_uuid,
                    "status": "success",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            )
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task completed generic event: {e}"
                ))
            })?;

        Ok(())
    }

    async fn publish_task_failed(
        &self,
        task_uuid: Uuid,
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        // Publish structured orchestration event for task failure
        let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
            task_uuid,
            result: TaskResult::Failed {
                error: "Task finalization determined task failed".to_string(),
            },
            completed_at: chrono::Utc::now(),
        });

        self.context
            .event_publisher
            .publish_event(event)
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task failed event: {e}"
                ))
            })?;

        // Also publish generic event for broader observability
        self.context
            .event_publisher
            .publish(
                "task.failed",
                json!({
                    "task_uuid": task_uuid,
                    "status": "failed",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            )
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task failed generic event: {e}"
                ))
            })?;

        Ok(())
    }
}

/// Errors that can occur during task finalization
#[derive(Debug, thiserror::Error)]
pub enum FinalizationError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Task not found: {task_uuid}")]
    TaskNotFound { task_uuid: Uuid },

    #[error("State machine error: {error}, for task {task_uuid}")]
    StateMachine { error: String, task_uuid: Uuid },

    #[error("Invalid state transition: {transition}, for task {task_uuid}")]
    InvalidTransition { transition: String, task_uuid: Uuid },

    #[error("Context unavailable for task: {task_uuid}")]
    ContextUnavailable { task_uuid: Uuid },

    #[error("Event publishing error: {0}")]
    EventPublishing(String),

    #[error("General error: {0}")]
    General(String),
}

impl From<TaskerError> for FinalizationError {
    fn from(error: TaskerError) -> Self {
        FinalizationError::General(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finalization_action_serialization() {
        let action = FinalizationAction::Completed;
        let serialized = serde_json::to_string(&action).unwrap();
        assert_eq!(serialized, "\"Completed\"");
    }

    #[test]
    fn test_finalization_result_creation() {
        let task_uuid = Uuid::now_v7();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::Completed,
            completion_percentage: Some(100.0),
            total_steps: Some(5),
            health_status: Some("healthy".to_string()),
            enqueued_steps: None,
            reason: None,
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::Completed));
        assert_eq!(result.completion_percentage, Some(100.0));
    }

    // Tests for ExecutionStatus and RecommendedAction enums are now in the unified module
}
