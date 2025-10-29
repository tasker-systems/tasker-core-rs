//! Task Processor
//!
//! Handles processing of individual tasks using state handlers.

use std::sync::Arc;
use uuid::Uuid;

use super::state_handlers::StateHandlers;
use crate::orchestration::lifecycle::step_enqueuer::StepEnqueuer;
use crate::orchestration::StepEnqueueResult;
use tasker_shared::database::sql_functions::{ReadyTaskInfo, SqlFunctionExecutor};
use tasker_shared::{SystemContext, TaskerError, TaskerResult};

/// Processes individual tasks
#[derive(Clone, Debug)]
pub struct TaskProcessor {
    state_handlers: StateHandlers,
    step_enqueuer: Arc<StepEnqueuer>,
    context: Arc<SystemContext>,
}

impl TaskProcessor {
    pub fn new(context: Arc<SystemContext>, step_enqueuer: Arc<StepEnqueuer>) -> Self {
        let state_handlers = StateHandlers::new(context.clone());
        Self {
            state_handlers,
            step_enqueuer,
            context,
        }
    }

    /// Process a single task from ready info
    pub async fn process_from_ready_info(
        &self,
        task_info: &ReadyTaskInfo,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        self.state_handlers
            .process_task_by_state(task_info, self.step_enqueuer.clone())
            .await
    }

    /// Process a single task from UUID
    pub async fn process_from_uuid(
        &self,
        task_uuid: Uuid,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());
        let task_info = sql_executor.get_task_ready_info(task_uuid).await?;

        if let Some(task_info) = task_info {
            self.process_from_ready_info(&task_info).await
        } else {
            Err(TaskerError::OrchestrationError(format!(
                "Task not found: {}",
                task_uuid
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_processor_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);
        let processor = TaskProcessor::new(context, step_enqueuer);

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&processor.context) >= 1);
        assert!(Arc::strong_count(&processor.step_enqueuer) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_processor_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);
        let processor = TaskProcessor::new(context, step_enqueuer);

        let cloned = processor.clone();

        // Verify both share the same Arcs
        assert_eq!(
            Arc::as_ptr(&processor.context),
            Arc::as_ptr(&cloned.context)
        );
        assert_eq!(
            Arc::as_ptr(&processor.step_enqueuer),
            Arc::as_ptr(&cloned.step_enqueuer)
        );
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_process_from_uuid_with_nonexistent_task(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);
        let processor = TaskProcessor::new(context, step_enqueuer);

        let nonexistent_uuid = Uuid::new_v4();
        let result = processor.process_from_uuid(nonexistent_uuid).await;

        // Should return error for non-existent task
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Task not found"));
        Ok(())
    }
}
