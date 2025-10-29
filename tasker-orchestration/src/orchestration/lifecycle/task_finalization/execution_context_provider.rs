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
#[derive(Clone, Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_execution_context_provider_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test that we can create ExecutionContextProvider
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let provider = ExecutionContextProvider::new(context);

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&provider.context) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_execution_context_provider_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test that ExecutionContextProvider implements Clone
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let provider = ExecutionContextProvider::new(context.clone());

        let cloned = provider.clone();

        // Verify both share the same Arc
        assert_eq!(Arc::as_ptr(&provider.context), Arc::as_ptr(&cloned.context));
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_task_execution_context_returns_none_for_nonexistent_task(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let provider = ExecutionContextProvider::new(context);

        let nonexistent_uuid = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        // Should return Ok(None) for non-existent task
        let result = provider
            .get_task_execution_context(nonexistent_uuid, correlation_id)
            .await?;

        assert!(result.is_none());
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_blocked_by_errors_returns_false_for_nonexistent_task(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let provider = ExecutionContextProvider::new(context);

        let nonexistent_uuid = Uuid::new_v4();

        // Should return Ok(false) for non-existent task
        let result = provider.blocked_by_errors(nonexistent_uuid).await?;

        assert!(!result);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_blocked_by_errors_returns_false_when_context_unavailable(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let provider = ExecutionContextProvider::new(context);

        // Create namespace and named task (required for foreign keys)
        let namespace = tasker_shared::models::TaskNamespace::create(
            &pool,
            tasker_shared::models::core::task_namespace::NewTaskNamespace {
                name: "test_ns".to_string(),
                description: Some("Test namespace".to_string()),
            },
        )
        .await?;

        let named_task =
            tasker_shared::models::NamedTask::find_or_create_by_name_version_namespace(
                &pool,
                "test_task",
                "1.0.0",
                namespace.task_namespace_uuid,
            )
            .await?;

        // Create a task without execution context
        let task_request = tasker_shared::models::task_request::TaskRequest::new(
            "test_task".to_string(),
            "test_ns".to_string(),
        );
        let mut task = Task::from_task_request(task_request);
        task.named_task_uuid = named_task.named_task_uuid;
        let task = Task::create(&pool, task).await?;

        // Should return Ok(false) when context is unavailable
        let result = provider.blocked_by_errors(task.task_uuid).await?;

        assert!(!result);
        Ok(())
    }
}
