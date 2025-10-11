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
#[derive(Clone)]
pub struct TaskProcessor {
    state_handlers: StateHandlers,
    step_enqueuer: Arc<StepEnqueuer>,
    context: Arc<SystemContext>,
}

impl TaskProcessor {
    pub fn new(
        context: Arc<SystemContext>,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> Self {
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
