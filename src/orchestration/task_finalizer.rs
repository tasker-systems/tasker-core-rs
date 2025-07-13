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
//! ## Rails Heritage
//!
//! Migrated from `lib/tasker/orchestration/task_finalizer.rb` with enhanced
//! type safety and performance optimizations.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::events::publisher::EventPublisher;
use crate::models::{Task, WorkflowStep};
use crate::orchestration::task_enqueuer::{EnqueuePriority, EnqueueRequest, TaskEnqueuer};
use crate::state_machine::{TaskEvent, TaskState, TaskStateMachine};

/// Result of task finalization operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizationResult {
    /// Task ID that was finalized
    pub task_id: i64,
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
    pub task_id: i64,
    pub execution_status: String,
    pub health_status: Option<String>,
    pub completion_percentage: Option<f64>,
    pub total_steps: Option<i32>,
    pub ready_steps: Option<i32>,
    pub pending_steps: Option<i32>,
    pub in_progress_steps: Option<i32>,
    pub completed_steps: Option<i32>,
    pub failed_steps: Option<i32>,
    pub recommended_action: Option<String>,
}

/// TaskFinalizer handles task completion and finalization logic
///
/// This component provides implementation for task finalization while firing
/// lifecycle events for observability. Enhanced with TaskExecutionContext
/// integration for intelligent decision making.
pub struct TaskFinalizer {
    pool: PgPool,
    sql_executor: SqlFunctionExecutor,
    event_publisher: EventPublisher,
    task_enqueuer: TaskEnqueuer,
}

impl TaskFinalizer {
    /// Create a new TaskFinalizer
    pub fn new(pool: PgPool) -> Self {
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let event_publisher = EventPublisher::new(1000); // 1000 event capacity
        let task_enqueuer =
            TaskEnqueuer::with_event_publisher(pool.clone(), event_publisher.clone());
        Self {
            pool,
            sql_executor,
            event_publisher,
            task_enqueuer,
        }
    }

    /// Create a new TaskFinalizer with custom event publisher
    pub fn with_event_publisher(pool: PgPool, event_publisher: EventPublisher) -> Self {
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let task_enqueuer =
            TaskEnqueuer::with_event_publisher(pool.clone(), event_publisher.clone());
        Self {
            pool,
            sql_executor,
            event_publisher,
            task_enqueuer,
        }
    }

    /// Create a new TaskFinalizer with custom components
    pub fn with_components(
        pool: PgPool,
        event_publisher: EventPublisher,
        task_enqueuer: TaskEnqueuer,
    ) -> Self {
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        Self {
            pool,
            sql_executor,
            event_publisher,
            task_enqueuer,
        }
    }

    /// Check if the task is blocked by errors
    ///
    /// @param task_id The task ID to check
    /// @return True if task is blocked by errors
    pub async fn blocked_by_errors(&self, task_id: i64) -> Result<bool, FinalizationError> {
        let context = self.get_task_execution_context(task_id).await?;

        // If no context is available, the task has no steps or doesn't exist
        // In either case, it's not blocked by errors
        let Some(context) = context else {
            return Ok(false);
        };

        Ok(context.execution_status == "blocked_by_failures")
    }

    /// Finalize a task with processed steps
    ///
    /// @param task_id The task ID to finalize
    /// @param processed_steps All processed steps
    pub async fn finalize_task_with_steps(
        &self,
        task_id: i64,
        processed_steps: Vec<WorkflowStep>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let context = self.get_task_execution_context(task_id).await?;

        // Fire finalization started event
        self.publish_finalization_started(task_id, &processed_steps, &context)
            .await?;

        // Use context-enhanced finalization logic with synchronous flag
        let result = self.finalize_task(task_id, true).await?;

        // Fire finalization completed event
        let final_context = self.get_task_execution_context(task_id).await?;
        self.publish_finalization_completed(task_id, &processed_steps, &final_context)
            .await?;

        Ok(result)
    }

    /// Finalize a task based on its current state using TaskExecutionContext
    ///
    /// @param task_id The task ID to finalize
    /// @param synchronous Whether this is synchronous processing (default: false)
    pub async fn finalize_task(
        &self,
        task_id: i64,
        synchronous: bool,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task = Task::find_by_id(&self.pool, task_id).await?;
        let Some(task) = task else {
            return Err(FinalizationError::TaskNotFound(task_id));
        };

        let context = self.get_task_execution_context(task_id).await?;

        self.make_finalization_decision(task, context, synchronous)
            .await
    }

    /// Handle no viable steps event
    ///
    /// Convenience method for event-driven workflows when no viable steps are found.
    /// This triggers task finalization to determine next action.
    ///
    /// @param task_id The task ID to handle
    pub async fn handle_no_viable_steps(
        &self,
        task_id: i64,
    ) -> Result<FinalizationResult, FinalizationError> {
        self.finalize_task(task_id, false).await
    }

    /// Complete a task successfully
    async fn complete_task(
        &self,
        mut task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_id = task.task_id;

        // Use state machine for proper state transitions
        let mut state_machine = TaskStateMachine::new(
            task.clone(),
            self.pool.clone(),
            self.event_publisher.clone(),
        );

        // Get current state
        let current_state = state_machine.current_state().await.map_err(|e| {
            FinalizationError::StateMachine(format!("Failed to get current state: {e}"))
        })?;

        // If task is already complete, just return
        if current_state == TaskState::Complete {
            return Ok(FinalizationResult {
                task_id,
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
            .map_err(|e| {
                FinalizationError::StateMachine(format!("Failed to transition to complete: {e}"))
            })?;

        // Update the task complete flag (this might be redundant if the state machine action handles it)
        task.mark_complete(&self.pool).await?;

        // Publish completion event
        self.publish_task_completed(task_id, &context).await?;

        Ok(FinalizationResult {
            task_id,
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
        let task_id = task.task_id;

        // Use state machine for proper state transitions
        let mut state_machine = TaskStateMachine::new(
            task.clone(),
            self.pool.clone(),
            self.event_publisher.clone(),
        );

        // Transition to error state using state machine
        let error_message = "Steps in error state".to_string();
        state_machine
            .transition(TaskEvent::Fail(error_message.clone()))
            .await
            .map_err(|e| {
                FinalizationError::StateMachine(format!("Failed to transition to error: {e}"))
            })?;

        // Publish failure event
        self.publish_task_failed(task_id, &context).await?;

        Ok(FinalizationResult {
            task_id,
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
        let task_id = task.task_id;

        // Use state machine for proper state transitions
        let mut state_machine = TaskStateMachine::new(
            task.clone(),
            self.pool.clone(),
            self.event_publisher.clone(),
        );

        // Get current state to determine appropriate transition
        let current_state = state_machine.current_state().await.map_err(|e| {
            FinalizationError::StateMachine(format!("Failed to get current state: {e}"))
        })?;

        // If already pending, no need to transition
        if current_state != TaskState::Pending {
            // For now, we'll use Start event to get to pending state
            // In a more complete implementation, there might be specific events for different pending reasons
            state_machine
                .transition(TaskEvent::Start)
                .await
                .map_err(|e| {
                    FinalizationError::StateMachine(format!("Failed to transition to pending: {e}"))
                })?;
        }

        let final_reason = reason.unwrap_or_else(|| {
            context
                .as_ref()
                .and_then(|c| self.determine_pending_reason(c))
                .unwrap_or_else(|| "Unknown reason".to_string())
        });

        // Publish pending transition event
        self.publish_task_pending_transition(task_id, &context, &final_reason)
            .await?;

        Ok(FinalizationResult {
            task_id,
            action: FinalizationAction::Pending,
            completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
            total_steps: context.as_ref().and_then(|c| c.total_steps),
            health_status: context.as_ref().and_then(|c| c.health_status.clone()),
            reason: Some(final_reason),
        })
    }

    /// Reenqueue task with context intelligence
    async fn reenqueue_task_with_context(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        reason: Option<String>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_id = task.task_id;
        let delay_seconds = self.calculate_reenqueue_delay(&context);
        let final_reason = reason.unwrap_or_else(|| {
            context
                .as_ref()
                .and_then(|c| self.determine_reenqueue_reason(c))
                .unwrap_or_else(|| "Continuing workflow".to_string())
        });

        // Use the TaskEnqueuer to handle the actual reenqueue operation
        let priority = self.determine_enqueue_priority(&context, &final_reason);

        let enqueue_request = EnqueueRequest::reenqueue(task.clone())
            .with_delay(delay_seconds)
            .with_priority(priority)
            .with_reason(&final_reason)
            .with_metadata(
                "ready_steps".to_string(),
                serde_json::json!(context.as_ref().and_then(|c| c.ready_steps)),
            );

        match self.task_enqueuer.enqueue(enqueue_request).await {
            Ok(enqueue_result) => {
                println!(
                    "TaskFinalizer: Task {} successfully reenqueued - {} (delay: {}s, job_id: {:?})",
                    task_id,
                    final_reason,
                    delay_seconds,
                    enqueue_result.job_id
                );
            }
            Err(enqueue_error) => {
                eprintln!(
                    "TaskFinalizer: Failed to reenqueue task {task_id} - {final_reason}: {enqueue_error}"
                );
                // Don't return the error - log it and continue with finalization
                // The task state has already been updated appropriately
            }
        }

        Ok(FinalizationResult {
            task_id,
            action: FinalizationAction::Reenqueued,
            completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
            total_steps: context.as_ref().and_then(|c| c.total_steps),
            health_status: context.as_ref().and_then(|c| c.health_status.clone()),
            reason: Some(final_reason),
        })
    }

    /// Get TaskExecutionContext using function-based implementation
    async fn get_task_execution_context(
        &self,
        task_id: i64,
    ) -> Result<Option<TaskExecutionContext>, FinalizationError> {
        let context_result = self.sql_executor.get_task_execution_context(task_id).await;

        match context_result {
            Ok(Some(sql_context)) => Ok(Some(TaskExecutionContext {
                task_id,
                execution_status: sql_context.recommended_action.clone(), // Use recommended_action as a proxy for status
                health_status: Some("healthy".to_string()),               // Default health status
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
                in_progress_steps: Some(0), // Not available in SQL context
                completed_steps: Some(sql_context.completed_steps as i32),
                failed_steps: Some(sql_context.failed_steps as i32),
                recommended_action: Some(sql_context.recommended_action),
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
        synchronous: bool,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_id = task.task_id;

        println!(
            "TaskFinalizer: Making decision for task {} with execution_status: {:?}",
            task_id,
            context.as_ref().map(|c| &c.execution_status)
        );

        // Handle nil context case
        let Some(context) = context else {
            println!(
                "TaskFinalizer: Task {task_id} - no context available, handling as unclear state"
            );
            return self.handle_unclear_state(task, None).await;
        };

        match context.execution_status.as_str() {
            "all_complete" => {
                println!("TaskFinalizer: Task {task_id} - calling complete_task");
                self.complete_task(task, Some(context)).await
            }
            "blocked_by_failures" => {
                println!("TaskFinalizer: Task {task_id} - calling error_task");
                self.error_task(task, Some(context)).await
            }
            "has_ready_steps" => {
                println!("TaskFinalizer: Task {task_id} - has ready steps, should execute them");
                self.handle_ready_steps_state(task, Some(context), synchronous)
                    .await
            }
            "waiting_for_dependencies" => {
                println!("TaskFinalizer: Task {task_id} - waiting for dependencies");
                self.handle_waiting_state(task, Some(context), synchronous)
                    .await
            }
            "processing" => {
                println!("TaskFinalizer: Task {task_id} - handling processing state");
                self.handle_processing_state(task, Some(context), synchronous)
                    .await
            }
            _ => {
                println!("TaskFinalizer: Task {task_id} - handling unclear state");
                self.handle_unclear_state(task, Some(context)).await
            }
        }
    }

    /// Handle ready steps state - should execute the ready steps
    async fn handle_ready_steps_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        synchronous: bool,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_id = task.task_id;
        let ready_steps = context.as_ref().and_then(|c| c.ready_steps).unwrap_or(0);

        println!(
            "TaskFinalizer: Task {task_id} has {ready_steps} ready steps - transitioning to in_progress"
        );

        // Use state machine to transition to in_progress if needed
        let mut state_machine = TaskStateMachine::new(
            task.clone(),
            self.pool.clone(),
            self.event_publisher.clone(),
        );

        let current_state = state_machine.current_state().await.map_err(|e| {
            FinalizationError::StateMachine(format!("Failed to get current state: {e}"))
        })?;

        // Transition to in_progress if not already there or complete
        if current_state != TaskState::InProgress && current_state != TaskState::Complete {
            state_machine
                .transition(TaskEvent::Start)
                .await
                .map_err(|e| {
                    FinalizationError::StateMachine(format!(
                        "Failed to transition to in_progress: {e}"
                    ))
                })?;
        }

        if synchronous {
            // In synchronous mode, we can't actually execute steps here
            // The calling code should handle step execution
            println!("TaskFinalizer: Task {task_id} ready for synchronous step execution");
            Ok(FinalizationResult {
                task_id,
                action: FinalizationAction::Pending,
                completion_percentage: context.as_ref().and_then(|c| c.completion_percentage),
                total_steps: context.as_ref().and_then(|c| c.total_steps),
                health_status: context.as_ref().and_then(|c| c.health_status.clone()),
                reason: Some("Ready for synchronous step execution".to_string()),
            })
        } else {
            // In asynchronous mode, reenqueue immediately for step execution
            self.reenqueue_task_with_context(
                task,
                context,
                Some("Ready steps available".to_string()),
            )
            .await
        }
    }

    /// Handle waiting for dependencies state
    async fn handle_waiting_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        synchronous: bool,
    ) -> Result<FinalizationResult, FinalizationError> {
        if synchronous {
            self.pending_task(task, context, Some("Waiting for dependencies".to_string()))
                .await
        } else {
            self.reenqueue_task_with_context(
                task,
                context,
                Some("Awaiting dependencies".to_string()),
            )
            .await
        }
    }

    /// Handle processing state
    async fn handle_processing_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        synchronous: bool,
    ) -> Result<FinalizationResult, FinalizationError> {
        if synchronous {
            self.pending_task(
                task,
                context,
                Some("Waiting for step completion".to_string()),
            )
            .await
        } else {
            self.reenqueue_task_with_context(task, context, Some("Steps in progress".to_string()))
                .await
        }
    }

    /// Handle unclear task state
    async fn handle_unclear_state(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_id = task.task_id;

        if let Some(ref ctx) = context {
            eprintln!(
                "TaskFinalizer: Task {} in unclear state: execution_status={}, health_status={:?}, ready_steps={:?}, failed_steps={:?}, in_progress_steps={:?}",
                task_id,
                ctx.execution_status,
                ctx.health_status,
                ctx.ready_steps,
                ctx.failed_steps,
                ctx.in_progress_steps
            );

            // Default to re-enqueuing with a longer delay for unclear states
            self.reenqueue_task_with_context(task, context, Some("Continuing workflow".to_string()))
                .await
        } else {
            eprintln!("TaskFinalizer: Task {task_id} has no execution context and unclear state");

            // Without context, attempt to transition to error state
            self.error_task(task, None).await
        }
    }

    /// Determine reason for pending state
    fn determine_pending_reason(&self, context: &TaskExecutionContext) -> Option<String> {
        match context.execution_status.as_str() {
            "has_ready_steps" => Some("Ready for processing".to_string()),
            "waiting_for_dependencies" => Some("Waiting for dependencies".to_string()),
            "processing" => Some("Waiting for step completion".to_string()),
            _ => Some("Workflow paused".to_string()),
        }
    }

    /// Determine reason for reenqueue
    fn determine_reenqueue_reason(&self, context: &TaskExecutionContext) -> Option<String> {
        match context.execution_status.as_str() {
            "has_ready_steps" => Some("Ready steps available".to_string()),
            "waiting_for_dependencies" => Some("Awaiting dependencies".to_string()),
            "processing" => Some("Steps in progress".to_string()),
            _ => Some("Continuing workflow".to_string()),
        }
    }

    /// Calculate intelligent re-enqueue delay based on execution context
    fn calculate_reenqueue_delay(&self, context: &Option<TaskExecutionContext>) -> u32 {
        // Default delays for different states (in seconds)
        const DEFAULT_DELAY: u32 = 30;
        const READY_STEPS_DELAY: u32 = 5;
        const WAITING_DELAY: u32 = 60;
        const PROCESSING_DELAY: u32 = 15;

        let Some(context) = context else {
            return DEFAULT_DELAY;
        };

        match context.execution_status.as_str() {
            "has_ready_steps" => READY_STEPS_DELAY,
            "waiting_for_dependencies" => WAITING_DELAY,
            "processing" => PROCESSING_DELAY,
            _ => DEFAULT_DELAY,
        }
    }

    /// Determine enqueue priority based on context and reason
    fn determine_enqueue_priority(
        &self,
        context: &Option<TaskExecutionContext>,
        reason: &str,
    ) -> EnqueuePriority {
        // Check for critical conditions first
        if reason.contains("critical") || reason.contains("urgent") {
            return EnqueuePriority::Critical;
        }

        // Check context-based priorities
        if let Some(ctx) = context {
            match ctx.execution_status.as_str() {
                "has_ready_steps" => {
                    // Ready steps should be processed with higher priority
                    EnqueuePriority::High
                }
                "blocked_by_failures" => {
                    // Failed tasks need attention but not urgent
                    EnqueuePriority::Normal
                }
                "waiting_for_dependencies" => {
                    // Waiting tasks can be lower priority
                    EnqueuePriority::Low
                }
                "processing" => {
                    // Currently processing tasks get normal priority
                    EnqueuePriority::Normal
                }
                _ => EnqueuePriority::Normal,
            }
        } else {
            // No context available, use normal priority
            EnqueuePriority::Normal
        }
    }

    // Event publishing methods (placeholder implementations)
    async fn publish_finalization_started(
        &self,
        task_id: i64,
        processed_steps: &[WorkflowStep],
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        println!(
            "TaskFinalizer: Finalization started for task {} with {} processed steps",
            task_id,
            processed_steps.len()
        );
        // TODO: Implement actual event publishing
        Ok(())
    }

    async fn publish_finalization_completed(
        &self,
        task_id: i64,
        processed_steps: &[WorkflowStep],
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        println!(
            "TaskFinalizer: Finalization completed for task {} with {} processed steps",
            task_id,
            processed_steps.len()
        );
        // TODO: Implement actual event publishing
        Ok(())
    }

    async fn publish_task_completed(
        &self,
        task_id: i64,
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        println!("TaskFinalizer: Task {task_id} completed successfully");
        // TODO: Implement actual event publishing
        Ok(())
    }

    async fn publish_task_failed(
        &self,
        task_id: i64,
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        println!("TaskFinalizer: Task {task_id} failed");
        // TODO: Implement actual event publishing
        Ok(())
    }

    async fn publish_task_pending_transition(
        &self,
        task_id: i64,
        _context: &Option<TaskExecutionContext>,
        reason: &str,
    ) -> Result<(), FinalizationError> {
        println!("TaskFinalizer: Task {task_id} set to pending - {reason}");
        // TODO: Implement actual event publishing
        Ok(())
    }
}

/// Errors that can occur during task finalization
#[derive(Debug, thiserror::Error)]
pub enum FinalizationError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Task not found: {0}")]
    TaskNotFound(i64),

    #[error("State machine error: {0}")]
    StateMachine(String),

    #[error("Invalid state transition: {0}")]
    InvalidTransition(String),

    #[error("Context unavailable for task: {0}")]
    ContextUnavailable(i64),
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
        let result = FinalizationResult {
            task_id: 123,
            action: FinalizationAction::Completed,
            completion_percentage: Some(100.0),
            total_steps: Some(5),
            health_status: Some("healthy".to_string()),
            reason: None,
        };

        assert_eq!(result.task_id, 123);
        assert!(matches!(result.action, FinalizationAction::Completed));
        assert_eq!(result.completion_percentage, Some(100.0));
    }

    #[test]
    fn test_task_execution_context_creation() {
        let context = TaskExecutionContext {
            task_id: 456,
            execution_status: "all_complete".to_string(),
            health_status: Some("healthy".to_string()),
            completion_percentage: Some(100.0),
            total_steps: Some(3),
            ready_steps: Some(0),
            pending_steps: Some(0),
            in_progress_steps: Some(0),
            completed_steps: Some(3),
            failed_steps: Some(0),
            recommended_action: Some("complete".to_string()),
        };

        assert_eq!(context.task_id, 456);
        assert_eq!(context.execution_status, "all_complete");
        assert_eq!(context.completed_steps, Some(3));
    }
}
