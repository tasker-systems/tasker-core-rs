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
use sqlx::PgPool;
use uuid::Uuid;

use crate::orchestration::lifecycle::task_claim_step_enqueuer::TaskClaimStepEnqueuer;
use std::sync::Arc;

use tasker_shared::database::sql_functions::SqlFunctionExecutor;

use tasker_shared::events::types::{Event, OrchestrationEvent, TaskResult};
use tasker_shared::models::{Task, WorkflowStep};
use tasker_shared::state_machine::{TaskEvent, TaskState, TaskStateMachine};
use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerError;

/// Execution status returned by the SQL get_task_execution_context function
/// Matches the execution_status values from the SQL function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Task has ready steps that can be executed
    HasReadySteps,
    /// Task has steps currently being processed
    Processing,
    /// Task is blocked by failures that cannot be retried
    BlockedByFailures,
    /// All steps are complete
    AllComplete,
    /// Task is waiting for dependencies to complete
    WaitingForDependencies,
}

impl ExecutionStatus {
    /// Parse execution status from SQL string value (kept for backward compatibility)
    pub fn from_str(value: &str) -> Self {
        value.into()
    }

    /// Convert to SQL string value
    pub fn as_str(&self) -> &str {
        match self {
            Self::HasReadySteps => "has_ready_steps",
            Self::Processing => "processing",
            Self::BlockedByFailures => "blocked_by_failures",
            Self::AllComplete => "all_complete",
            Self::WaitingForDependencies => "waiting_for_dependencies",
        }
    }
}

impl From<&str> for ExecutionStatus {
    fn from(value: &str) -> Self {
        match value {
            "has_ready_steps" => Self::HasReadySteps,
            "processing" => Self::Processing,
            "blocked_by_failures" => Self::BlockedByFailures,
            "all_complete" => Self::AllComplete,
            "waiting_for_dependencies" => Self::WaitingForDependencies,
            _ => Self::WaitingForDependencies, // Default fallback
        }
    }
}

impl From<String> for ExecutionStatus {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

/// Recommended action returned by the SQL get_task_execution_context function
/// Matches the recommended_action values from the SQL function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecommendedAction {
    /// Execute the ready steps
    ExecuteReadySteps,
    /// Wait for in-progress steps to complete
    WaitForCompletion,
    /// Handle permanent failures
    HandleFailures,
    /// Finalize the completed task
    FinalizeTask,
    /// Wait for dependencies to become ready
    WaitForDependencies,
}

impl RecommendedAction {
    /// Parse recommended action from SQL string value (kept for backward compatibility)
    pub fn from_str(value: &str) -> Self {
        value.into()
    }

    /// Convert to SQL string value
    pub fn as_str(&self) -> &str {
        match self {
            Self::ExecuteReadySteps => "execute_ready_steps",
            Self::WaitForCompletion => "wait_for_completion",
            Self::HandleFailures => "handle_failures",
            Self::FinalizeTask => "finalize_task",
            Self::WaitForDependencies => "wait_for_dependencies",
        }
    }
}

impl From<&str> for RecommendedAction {
    fn from(value: &str) -> Self {
        match value {
            "execute_ready_steps" => Self::ExecuteReadySteps,
            "wait_for_completion" => Self::WaitForCompletion,
            "handle_failures" => Self::HandleFailures,
            "finalize_task" => Self::FinalizeTask,
            "wait_for_dependencies" => Self::WaitForDependencies,
            _ => Self::WaitForDependencies, // Default fallback
        }
    }
}

impl From<String> for RecommendedAction {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

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

/// Context information for task execution status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionContext {
    pub task_uuid: Uuid,
    pub execution_status: ExecutionStatus,
    pub health_status: Option<String>,
    pub completion_percentage: Option<f64>,
    pub total_steps: Option<i32>,
    pub ready_steps: Option<i32>,
    pub pending_steps: Option<i32>,
    pub in_progress_steps: Option<i32>,
    pub completed_steps: Option<i32>,
    pub failed_steps: Option<i32>,
    pub recommended_action: Option<RecommendedAction>,
}

/// TaskFinalizer handles task completion and finalization logic
///
/// This component provides implementation for task finalization while firing
/// lifecycle events for observability. Enhanced with TaskExecutionContext
/// integration for intelligent decision making.
#[derive(Clone)]
pub struct TaskFinalizer {
    context: Arc<SystemContext>,
    sql_executor: SqlFunctionExecutor,
    task_claim_step_enqueuer: Option<Arc<TaskClaimStepEnqueuer>>,
}

impl TaskFinalizer {
    /// Create a new TaskFinalizer without step enqueuer (backward compatible)
    pub fn new(context: Arc<SystemContext>) -> Self {
        let sql_executor = SqlFunctionExecutor::new(context.database_pool().clone());

        Self {
            context,
            sql_executor,
            task_claim_step_enqueuer: None,
        }
    }

    /// Create a TaskFinalizer with TaskClaimStepEnqueuer for immediate step enqueuing
    pub fn with_step_enqueuer(
        context: Arc<SystemContext>,
        task_claim_step_enqueuer: Arc<TaskClaimStepEnqueuer>,
    ) -> Self {
        let sql_executor = SqlFunctionExecutor::new(context.database_pool().clone());

        Self {
            context,
            sql_executor,
            task_claim_step_enqueuer: Some(task_claim_step_enqueuer),
        }
    }

    pub fn get_state_machine_for_task(&self, task: &Task) -> TaskStateMachine {
        TaskStateMachine::new(
            task.clone(),
            self.context.database_pool().clone(),
            Some(self.context.event_publisher.clone()),
        )
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

        self.make_finalization_decision(task, context).await
    }

    /// Complete a task successfully
    async fn complete_task(
        &self,
        mut task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;

        // Use state machine for proper state transitions
        let mut state_machine = self.get_state_machine_for_task(&task);

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
                completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
                total_steps: context.as_ref().and_then(|c| c.total_steps),
                health_status: context.as_ref().and_then(|c| c.health_status.clone()),
                reason: None,
            });
        }

        // Transition to complete state using state machine
        state_machine
            .transition(TaskEvent::Complete)
            .await
            .map_err(|e| FinalizationError::StateMachine {
                error: format!("Failed to transition to complete: {e}"),
                task_uuid,
            })?;

        // Update the task complete flag (this might be redundant if the state machine action handles it)
        task.mark_complete(self.context.database_pool()).await?;

        // Publish completion event
        self.publish_task_completed(task_uuid, &context).await?;

        Ok(FinalizationResult {
            task_uuid,
            action: FinalizationAction::Completed,
            completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
            total_steps: context.as_ref().and_then(|c| c.total_steps),
            health_status: context.as_ref().and_then(|c| c.health_status.clone()),
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
        let mut state_machine = self.get_state_machine_for_task(&task);

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
            completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
            total_steps: context.as_ref().and_then(|c| c.total_steps),
            health_status: context.as_ref().and_then(|c| c.health_status.clone()),
            reason: Some("Steps in error state".to_string()),
        })
    }

    /// Set task to pending state
    async fn pending_task(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        reason: Option<String>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;

        // Use state machine for proper state transitions
        let mut state_machine = self.get_state_machine_for_task(&task);

        // Get current state to determine appropriate transition
        let current_state =
            state_machine
                .current_state()
                .await
                .map_err(|e| FinalizationError::StateMachine {
                    error: format!("Failed to get current state: {e}"),
                    task_uuid,
                })?;

        // If already pending, no need to transition
        if current_state != TaskState::Pending {
            // For now, we'll use Start event to get to pending state
            // In a more complete implementation, there might be specific events for different pending reasons
            state_machine
                .transition(TaskEvent::Start)
                .await
                .map_err(|e| FinalizationError::StateMachine {
                    error: format!("Failed to transition to pending: {e}"),
                    task_uuid,
                })?;
        }

        let final_reason = reason.unwrap_or_else(|| {
            context
                .as_ref()
                .and_then(|c| self.determine_pending_reason(c))
                .unwrap_or_else(|| "Unknown reason".to_string())
        });

        // Publish pending transition event
        self.publish_task_pending_transition(task_uuid, &context, &final_reason)
            .await?;

        Ok(FinalizationResult {
            task_uuid,
            action: FinalizationAction::Pending,
            completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
            total_steps: context.as_ref().and_then(|c| c.total_steps),
            health_status: context.as_ref().and_then(|c| c.health_status.clone()),
            reason: Some(final_reason),
        })
    }

    /// Reenqueue task with context intelligence
    async fn get_task_execution_context(
        &self,
        task_uuid: Uuid,
    ) -> Result<Option<TaskExecutionContext>, FinalizationError> {
        let context_result = self
            .sql_executor
            .get_task_execution_context(task_uuid)
            .await;

        match context_result {
            Ok(Some(sql_context)) => Ok(Some(TaskExecutionContext {
                task_uuid,
                execution_status: sql_context.execution_status.as_str().into(),
                health_status: Some(sql_context.health_status),
                completion_percentage: Some(
                    sql_context
                        .completion_percentage
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.0),
                ),
                total_steps: Some(sql_context.total_steps as i32),
                ready_steps: Some(sql_context.ready_steps as i32),
                pending_steps: Some(sql_context.pending_steps as i32),
                in_progress_steps: Some(sql_context.in_progress_steps as i32),
                completed_steps: Some(sql_context.completed_steps as i32),
                failed_steps: Some(sql_context.failed_steps as i32),
                recommended_action: Some(sql_context.recommended_action.as_str().into()),
            })),
            Ok(None) => Ok(None), // No context available
            Err(_) => Ok(None),   // Context not available due to error
        }
    }

    /// Make finalization decision based on task state
    async fn make_finalization_decision(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;

        println!(
            "TaskFinalizer: Making decision for task {} with execution_status: {:?}",
            task_uuid,
            context.as_ref().map(|c| &c.execution_status)
        );

        // Handle nil context case
        let Some(context) = context else {
            println!(
                "TaskFinalizer: Task {task_uuid} - no context available, handling as unclear state"
            );
            return self.handle_unclear_state(task, None).await;
        };

        match context.execution_status {
            ExecutionStatus::AllComplete => {
                println!("TaskFinalizer: Task {task_uuid} - calling complete_task");
                self.complete_task(task, Some(context)).await
            }
            ExecutionStatus::BlockedByFailures => {
                println!("TaskFinalizer: Task {task_uuid} - calling error_task");
                self.error_task(task, Some(context)).await
            }
            ExecutionStatus::HasReadySteps => {
                println!("TaskFinalizer: Task {task_uuid} - has ready steps, should execute them");
                self.handle_ready_steps_state(task, Some(context)).await
            }
            ExecutionStatus::WaitingForDependencies => {
                println!("TaskFinalizer: Task {task_uuid} - waiting for dependencies");
                self.handle_waiting_state(task, Some(context)).await
            }
            ExecutionStatus::Processing => {
                println!("TaskFinalizer: Task {task_uuid} - handling processing state");
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
        let ready_steps = context.as_ref().and_then(|c| c.ready_steps).unwrap_or(0);

        println!(
            "TaskFinalizer: Task {task_uuid} has {ready_steps} ready steps - transitioning to in_progress"
        );

        // Use state machine to transition to in_progress if needed
        let mut state_machine = self.get_state_machine_for_task(&task);

        let current_state =
            state_machine
                .current_state()
                .await
                .map_err(|e| FinalizationError::StateMachine {
                    error: format!("Failed to get current state: {e}"),
                    task_uuid,
                })?;

        // Transition to in_progress if not already there or complete
        if current_state != TaskState::InProgress && current_state != TaskState::Complete {
            state_machine
                .transition(TaskEvent::Start)
                .await
                .map_err(|e| FinalizationError::StateMachine {
                    error: format!("Failed to transition to in_progress: {e}"),
                    task_uuid,
                })?;
        }

        // In asynchronous mode, process ready steps immediately
        if let Some(enqueuer) = &self.task_claim_step_enqueuer {
            enqueuer.process_single_task(task.task_uuid).await?;
        }

        Ok(FinalizationResult {
            task_uuid: task.task_uuid,
            action: FinalizationAction::Reenqueued,
            completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
            total_steps: context.as_ref().and_then(|c| c.total_steps),
            health_status: context.as_ref().and_then(|c| c.health_status.clone()),
            reason: Some("Ready steps enqueued".to_string()),
        })
    }

    /// Handle waiting for dependencies state
    async fn handle_waiting_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        // Process any ready steps (there might be some even in waiting state)
        if let Some(enqueuer) = &self.task_claim_step_enqueuer {
            enqueuer.process_single_task(task.task_uuid).await?;
        }

        Ok(FinalizationResult {
            task_uuid: task.task_uuid,
            action: FinalizationAction::Reenqueued,
            completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
            total_steps: context.as_ref().and_then(|c| c.total_steps),
            health_status: context.as_ref().and_then(|c| c.health_status.clone()),
            reason: Some("Awaiting dependencies".to_string()),
        })
    }

    /// Handle processing state
    async fn handle_processing_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        // Process any newly ready steps
        if let Some(enqueuer) = &self.task_claim_step_enqueuer {
            enqueuer.process_single_task(task.task_uuid).await?;
        }

        Ok(FinalizationResult {
            task_uuid: task.task_uuid,
            action: FinalizationAction::Reenqueued,
            completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
            total_steps: context.as_ref().and_then(|c| c.total_steps),
            health_status: context.as_ref().and_then(|c| c.health_status.clone()),
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

        if let Some(ref ctx) = context {
            eprintln!(
                "TaskFinalizer: Task {} in unclear state: execution_status={:?}, health_status={:?}, ready_steps={:?}, failed_steps={:?}, in_progress_steps={:?}",
                task_uuid,
                ctx.execution_status,
                ctx.health_status,
                ctx.ready_steps,
                ctx.failed_steps,
                ctx.in_progress_steps
            );

            // Default to processing any ready steps for unclear states
            if let Some(enqueuer) = &self.task_claim_step_enqueuer {
                enqueuer.process_single_task(task.task_uuid).await?;
            }

            Ok(FinalizationResult {
                task_uuid: task.task_uuid,
                action: FinalizationAction::Reenqueued,
                completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
                total_steps: context.as_ref().and_then(|c| c.total_steps),
                health_status: context.as_ref().and_then(|c| c.health_status.clone()),
                reason: Some("Continuing workflow".to_string()),
            })
        } else {
            eprintln!("TaskFinalizer: Task {task_uuid} has no execution context and unclear state");

            // Without context, attempt to transition to error state
            self.error_task(task, None).await
        }
    }

    /// Determine reason for pending state
    fn determine_pending_reason(&self, context: &TaskExecutionContext) -> Option<String> {
        match context.execution_status {
            ExecutionStatus::HasReadySteps => Some("Ready for processing".to_string()),
            ExecutionStatus::WaitingForDependencies => Some("Waiting for dependencies".to_string()),
            ExecutionStatus::Processing => Some("Waiting for step completion".to_string()),
            ExecutionStatus::BlockedByFailures => Some("Blocked by failures".to_string()),
            ExecutionStatus::AllComplete => Some("Task complete".to_string()),
        }
    }

    // Event publishing methods - Real implementations using EventPublisher
    async fn publish_finalization_started(
        &self,
        task_uuid: Uuid,
        processed_steps: &[WorkflowStep],
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        use serde_json::json;

        // Publish generic event for task finalization started
        self.context.event_publisher
            .publish(
                "task.finalization.started",
                json!({
                    "task_uuid": task_uuid,
                    "processed_steps_count": processed_steps.len(),
                    "step_uuids": processed_steps.iter().map(|s| s.workflow_step_uuid).collect::<Vec<_>>(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            )
            .await
            .map_err(|e| FinalizationError::EventPublishing(format!("Failed to publish finalization started event: {e}")))?;

        Ok(())
    }

    async fn publish_finalization_completed(
        &self,
        task_uuid: Uuid,
        processed_steps: &[WorkflowStep],
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        use serde_json::json;

        // Publish generic event for task finalization completed
        self.context.event_publisher
            .publish(
                "task.finalization.completed",
                json!({
                    "task_uuid": task_uuid,
                    "processed_steps_count": processed_steps.len(),
                    "step_uuids": processed_steps.iter().map(|s| s.workflow_step_uuid).collect::<Vec<_>>(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            )
            .await
            .map_err(|e| FinalizationError::EventPublishing(format!("Failed to publish finalization completed event: {e}")))?;

        Ok(())
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

    async fn publish_task_pending_transition(
        &self,
        task_uuid: Uuid,
        _context: &Option<TaskExecutionContext>,
        reason: &str,
    ) -> Result<(), FinalizationError> {
        use serde_json::json;

        // Publish generic event for task pending transition
        self.context
            .event_publisher
            .publish(
                "task.pending_transition",
                json!({
                    "task_uuid": task_uuid,
                    "status": "pending",
                    "reason": reason,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            )
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task pending transition event: {e}"
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
            reason: None,
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::Completed));
        assert_eq!(result.completion_percentage, Some(100.0));
    }

    #[test]
    fn test_task_execution_context_creation() {
        let task_uuid = Uuid::now_v7();
        let context = TaskExecutionContext {
            task_uuid,
            execution_status: ExecutionStatus::AllComplete,
            health_status: Some("healthy".to_string()),
            completion_percentage: Some(100.0),
            total_steps: Some(3),
            ready_steps: Some(0),
            pending_steps: Some(0),
            in_progress_steps: Some(0),
            completed_steps: Some(3),
            failed_steps: Some(0),
            recommended_action: Some(RecommendedAction::FinalizeTask),
        };

        assert_eq!(context.task_uuid, task_uuid);
        assert_eq!(context.execution_status, ExecutionStatus::AllComplete);
        assert_eq!(context.completed_steps, Some(3));
    }

    #[test]
    fn test_execution_status_conversion() {
        // Test from_str method (backward compatibility)
        assert_eq!(
            ExecutionStatus::from_str("has_ready_steps"),
            ExecutionStatus::HasReadySteps
        );
        assert_eq!(
            ExecutionStatus::from_str("processing"),
            ExecutionStatus::Processing
        );
        assert_eq!(
            ExecutionStatus::from_str("blocked_by_failures"),
            ExecutionStatus::BlockedByFailures
        );
        assert_eq!(
            ExecutionStatus::from_str("all_complete"),
            ExecutionStatus::AllComplete
        );
        assert_eq!(
            ExecutionStatus::from_str("waiting_for_dependencies"),
            ExecutionStatus::WaitingForDependencies
        );
        assert_eq!(
            ExecutionStatus::from_str("unknown"),
            ExecutionStatus::WaitingForDependencies
        ); // Default

        // Test From<&str> implementation
        let status: ExecutionStatus = "has_ready_steps".into();
        assert_eq!(status, ExecutionStatus::HasReadySteps);
        let status: ExecutionStatus = "processing".into();
        assert_eq!(status, ExecutionStatus::Processing);
        let status: ExecutionStatus = "unknown".into();
        assert_eq!(status, ExecutionStatus::WaitingForDependencies); // Default

        // Test From<String> implementation
        let status: ExecutionStatus = String::from("all_complete").into();
        assert_eq!(status, ExecutionStatus::AllComplete);
        let status: ExecutionStatus = String::from("blocked_by_failures").into();
        assert_eq!(status, ExecutionStatus::BlockedByFailures);

        // Test as_str method
        assert_eq!(ExecutionStatus::HasReadySteps.as_str(), "has_ready_steps");
        assert_eq!(ExecutionStatus::Processing.as_str(), "processing");
        assert_eq!(
            ExecutionStatus::BlockedByFailures.as_str(),
            "blocked_by_failures"
        );
        assert_eq!(ExecutionStatus::AllComplete.as_str(), "all_complete");
        assert_eq!(
            ExecutionStatus::WaitingForDependencies.as_str(),
            "waiting_for_dependencies"
        );
    }

    #[test]
    fn test_recommended_action_conversion() {
        // Test from_str method (backward compatibility)
        assert_eq!(
            RecommendedAction::from_str("execute_ready_steps"),
            RecommendedAction::ExecuteReadySteps
        );
        assert_eq!(
            RecommendedAction::from_str("wait_for_completion"),
            RecommendedAction::WaitForCompletion
        );
        assert_eq!(
            RecommendedAction::from_str("handle_failures"),
            RecommendedAction::HandleFailures
        );
        assert_eq!(
            RecommendedAction::from_str("finalize_task"),
            RecommendedAction::FinalizeTask
        );
        assert_eq!(
            RecommendedAction::from_str("wait_for_dependencies"),
            RecommendedAction::WaitForDependencies
        );
        assert_eq!(
            RecommendedAction::from_str("unknown"),
            RecommendedAction::WaitForDependencies
        ); // Default

        // Test From<&str> implementation
        let action: RecommendedAction = "execute_ready_steps".into();
        assert_eq!(action, RecommendedAction::ExecuteReadySteps);
        let action: RecommendedAction = "wait_for_completion".into();
        assert_eq!(action, RecommendedAction::WaitForCompletion);
        let action: RecommendedAction = "unknown".into();
        assert_eq!(action, RecommendedAction::WaitForDependencies); // Default

        // Test From<String> implementation
        let action: RecommendedAction = String::from("finalize_task").into();
        assert_eq!(action, RecommendedAction::FinalizeTask);
        let action: RecommendedAction = String::from("handle_failures").into();
        assert_eq!(action, RecommendedAction::HandleFailures);

        // Test as_str method
        assert_eq!(
            RecommendedAction::ExecuteReadySteps.as_str(),
            "execute_ready_steps"
        );
        assert_eq!(
            RecommendedAction::WaitForCompletion.as_str(),
            "wait_for_completion"
        );
        assert_eq!(
            RecommendedAction::HandleFailures.as_str(),
            "handle_failures"
        );
        assert_eq!(RecommendedAction::FinalizeTask.as_str(), "finalize_task");
        assert_eq!(
            RecommendedAction::WaitForDependencies.as_str(),
            "wait_for_dependencies"
        );
    }
}
