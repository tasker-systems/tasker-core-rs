//! Execution Context Provider
//!
//! Handles fetching task execution context and checking blocked states.

use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

use tasker_shared::models::orchestration::{ExecutionStatus, TaskExecutionContext};
use tasker_shared::models::Task;
use tasker_shared::system_context::SystemContext;

use super::FinalizationError;

/// Provides task execution context for finalization decisions
#[derive(Clone)]
pub struct ExecutionContextProvider {
    context: Arc<SystemContext>,
}

impl ExecutionContextProvider {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Get task execution context using unified TaskExecutionContext
    pub async fn get_task_execution_context(
        &self,
        task_uuid: Uuid,
        correlation_id: Uuid,
    ) -> Result<Option<TaskExecutionContext>, FinalizationError> {
        match TaskExecutionContext::get_for_task(self.context.database_pool(), task_uuid).await {
            Ok(context) => Ok(context),
            Err(e) => {
                debug!(
                    correlation_id = %correlation_id,
                    task_uuid = %task_uuid,
                    error = %e,
                    "Failed to get task execution context"
                );
                Ok(None) // Context not available due to error
            }
        }
    }

    /// Check if the task is blocked by errors
    ///
    /// @param task_uuid The task ID to check
    /// @return True if task is blocked by errors
    pub async fn blocked_by_errors(&self, task_uuid: Uuid) -> Result<bool, FinalizationError> {
        // Fetch correlation_id from task
        let correlation_id = {
            let task = Task::find_by_id(self.context.database_pool(), task_uuid).await?;
            task.map(|t| t.correlation_id).unwrap_or_else(Uuid::nil)
        };
        let context = self
            .get_task_execution_context(task_uuid, correlation_id)
            .await?;

        // If no context is available, the task has no steps or doesn't exist
        // In either case, it's not blocked by errors
        let Some(context) = context else {
            return Ok(false);
        };

        Ok(context.execution_status == ExecutionStatus::BlockedByFailures)
    }
}
