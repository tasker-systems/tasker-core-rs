//! Task Finalizer Service
//!
//! Main orchestration service that coordinates task finalization using focused components.

use opentelemetry::KeyValue;
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;
use uuid::Uuid;

use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;

use tasker_shared::metrics::orchestration::*;
use tasker_shared::models::orchestration::{ExecutionStatus, TaskExecutionContext};
use tasker_shared::models::Task;
use tasker_shared::system_context::SystemContext;

use super::completion_handler::CompletionHandler;
use super::event_publisher::EventPublisher;
use super::execution_context_provider::ExecutionContextProvider;
use super::state_handlers::StateHandlers;
use super::{FinalizationAction, FinalizationError, FinalizationResult};

/// TaskFinalizer handles task completion and finalization logic
///
/// This component provides implementation for task finalization while firing
/// lifecycle events for observability. Enhanced with TaskExecutionContext
/// integration for intelligent decision making.
#[derive(Clone)]
pub struct TaskFinalizer {
    context: Arc<SystemContext>,
    context_provider: ExecutionContextProvider,
    completion_handler: CompletionHandler,
    event_publisher: EventPublisher,
    state_handlers: StateHandlers,
}

impl TaskFinalizer {
    /// Create a new TaskFinalizer
    pub fn new(
        context: Arc<SystemContext>,
        step_enqueuer_service: Arc<StepEnqueuerService>,
    ) -> Self {
        Self {
            context_provider: ExecutionContextProvider::new(context.clone()),
            completion_handler: CompletionHandler::new(context.clone()),
            event_publisher: EventPublisher::new(context.clone()),
            state_handlers: StateHandlers::new(context.clone(), step_enqueuer_service),
            context,
        }
    }

    /// Check if the task is blocked by errors
    ///
    /// @param task_uuid The task ID to check
    /// @return True if task is blocked by errors
    pub async fn blocked_by_errors(&self, task_uuid: Uuid) -> Result<bool, FinalizationError> {
        self.context_provider.blocked_by_errors(task_uuid).await
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

        let correlation_id = task.correlation_id;

        // TAS-29 Phase 3.3: Start timing task finalization
        let start_time = Instant::now();

        debug!(
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            "TaskFinalizer: Starting task finalization"
        );

        let context = self
            .context_provider
            .get_task_execution_context(task_uuid, correlation_id)
            .await?;

        let finalization_result = self
            .make_finalization_decision(task, context, correlation_id)
            .await?;

        debug!(
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            action = ?finalization_result.action,
            "TaskFinalizer: Finalization completed"
        );

        // TAS-29 Phase 3.3: Record finalization metrics based on action
        let duration_ms = start_time.elapsed().as_millis() as f64;

        match &finalization_result.action {
            FinalizationAction::Completed => {
                // Record task completion counter
                if let Some(counter) = TASK_COMPLETIONS_TOTAL.get() {
                    counter.add(
                        1,
                        &[KeyValue::new("correlation_id", correlation_id.to_string())],
                    );
                }

                // Record finalization duration for completed tasks
                if let Some(histogram) = TASK_FINALIZATION_DURATION.get() {
                    histogram.record(
                        duration_ms,
                        &[
                            KeyValue::new("correlation_id", correlation_id.to_string()),
                            KeyValue::new("final_state", "complete"),
                        ],
                    );
                }
            }
            FinalizationAction::Failed => {
                // Record task failure counter
                if let Some(counter) = TASK_FAILURES_TOTAL.get() {
                    counter.add(
                        1,
                        &[KeyValue::new("correlation_id", correlation_id.to_string())],
                    );
                }

                // Record finalization duration for failed tasks
                if let Some(histogram) = TASK_FINALIZATION_DURATION.get() {
                    histogram.record(
                        duration_ms,
                        &[
                            KeyValue::new("correlation_id", correlation_id.to_string()),
                            KeyValue::new("final_state", "error"),
                        ],
                    );
                }
            }
            _ => {
                // For other actions (Pending, Reenqueued, NoAction), just record duration
                if let Some(histogram) = TASK_FINALIZATION_DURATION.get() {
                    let state = match finalization_result.action {
                        FinalizationAction::Pending => "pending",
                        FinalizationAction::Reenqueued => "reenqueued",
                        FinalizationAction::NoAction => "no_action",
                        _ => "unknown",
                    };
                    histogram.record(
                        duration_ms,
                        &[
                            KeyValue::new("correlation_id", correlation_id.to_string()),
                            KeyValue::new("final_state", state),
                        ],
                    );
                }
            }
        }

        Ok(finalization_result)
    }

    /// Make finalization decision based on task state
    async fn make_finalization_decision(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;

        debug!(
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            execution_status = ?context.as_ref().map(|c| &c.execution_status),
            "TaskFinalizer: Making decision for task"
        );

        // Handle nil context case
        let Some(context) = context else {
            debug!(
                correlation_id = %correlation_id,
                task_uuid = %task_uuid,
                "TaskFinalizer: Task - no context available, handling as unclear state"
            );
            return self
                .state_handlers
                .handle_unclear_state(task, None, correlation_id)
                .await;
        };

        match context.execution_status {
            ExecutionStatus::AllComplete => {
                debug!(correlation_id = %correlation_id, task_uuid = %task_uuid, "TaskFinalizer: Task - calling complete_task");
                let result = self
                    .completion_handler
                    .complete_task(task, Some(context.clone()), correlation_id)
                    .await?;

                // Publish completion event
                self.event_publisher
                    .publish_task_completed(task_uuid, &Some(context))
                    .await?;

                Ok(result)
            }
            ExecutionStatus::BlockedByFailures => {
                debug!(correlation_id = %correlation_id, task_uuid = %task_uuid, "TaskFinalizer: Task - calling error_task");
                let result = self
                    .completion_handler
                    .error_task(task, Some(context.clone()), correlation_id)
                    .await?;

                // Publish failure event
                self.event_publisher
                    .publish_task_failed(task_uuid, &Some(context))
                    .await?;

                Ok(result)
            }
            ExecutionStatus::HasReadySteps => {
                debug!(correlation_id = %correlation_id, task_uuid = %task_uuid, "TaskFinalizer: Task - has ready steps, should execute them");
                self.state_handlers
                    .handle_ready_steps_state(task, Some(context), correlation_id)
                    .await
            }
            ExecutionStatus::WaitingForDependencies => {
                debug!(correlation_id = %correlation_id, task_uuid = %task_uuid, "TaskFinalizer: Task - waiting for dependencies");
                self.state_handlers
                    .handle_waiting_state(task, Some(context), correlation_id)
                    .await
            }
            ExecutionStatus::Processing => {
                debug!(correlation_id = %correlation_id, task_uuid = %task_uuid, "TaskFinalizer: Task - handling processing state");
                self.state_handlers
                    .handle_processing_state(task, Some(context), correlation_id)
                    .await
            }
        }
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

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_finalizer_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test that TaskFinalizer implements Clone
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuerService::new(context.clone()).await?);
        let finalizer = TaskFinalizer::new(context.clone(), step_enqueuer);

        let cloned = finalizer.clone();

        // Verify both share the same Arc
        assert_eq!(
            Arc::as_ptr(&finalizer.context),
            Arc::as_ptr(&cloned.context)
        );
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_finalize_task_returns_error_for_nonexistent_task(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuerService::new(context.clone()).await?);
        let finalizer = TaskFinalizer::new(context, step_enqueuer);

        let nonexistent_uuid = Uuid::new_v4();

        // Should return TaskNotFound error
        let result = finalizer.finalize_task(nonexistent_uuid).await;

        assert!(result.is_err());
        match result {
            Err(FinalizationError::TaskNotFound { task_uuid }) => {
                assert_eq!(task_uuid, nonexistent_uuid);
            }
            _ => panic!("Expected TaskNotFound error"),
        }

        Ok(())
    }

    #[test]
    fn test_finalization_result_structure_for_all_complete() {
        // Test result structure for AllComplete status (Completed action)
        let task_uuid = Uuid::new_v4();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::Completed,
            completion_percentage: Some(100.0),
            total_steps: Some(8),
            enqueued_steps: None,
            health_status: Some("healthy".to_string()),
            reason: None,
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::Completed));
        assert_eq!(result.completion_percentage, Some(100.0));
        assert_eq!(result.total_steps, Some(8));
        assert!(result.reason.is_none());
    }

    #[test]
    fn test_finalization_result_structure_for_blocked_by_failures() {
        // Test result structure for BlockedByFailures status (Failed action)
        let task_uuid = Uuid::new_v4();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::Failed,
            completion_percentage: Some(60.0),
            total_steps: Some(10),
            enqueued_steps: None,
            health_status: Some("degraded".to_string()),
            reason: Some("Steps in error state".to_string()),
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::Failed));
        assert_eq!(result.completion_percentage, Some(60.0));
        assert_eq!(result.total_steps, Some(10));
        assert_eq!(result.reason, Some("Steps in error state".to_string()));
    }

    #[test]
    fn test_finalization_result_structure_for_processing() {
        // Test result structure for Processing status (NoAction action)
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
    fn test_all_finalization_actions_are_distinct() {
        // Verify all FinalizationAction variants are distinct
        let actions = vec![
            FinalizationAction::Completed,
            FinalizationAction::Failed,
            FinalizationAction::Pending,
            FinalizationAction::Reenqueued,
            FinalizationAction::NoAction,
        ];

        // Verify count
        assert_eq!(actions.len(), 5);

        // Verify they serialize differently
        let serialized: Vec<String> = actions
            .iter()
            .map(|a| serde_json::to_string(a).unwrap())
            .collect();

        assert_eq!(serialized[0], "\"Completed\"");
        assert_eq!(serialized[1], "\"Failed\"");
        assert_eq!(serialized[2], "\"Pending\"");
        assert_eq!(serialized[3], "\"Reenqueued\"");
        assert_eq!(serialized[4], "\"NoAction\"");
    }
}
