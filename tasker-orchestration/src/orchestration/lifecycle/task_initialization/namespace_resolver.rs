//! Namespace and NamedTask Resolution
//!
//! Handles finding or creating task namespaces and named tasks during task initialization.
//! This component ensures that all tasks have proper namespace context and are linked to
//! their NamedTask definitions in the database.

use sqlx::types::Uuid;
use std::sync::Arc;
use tasker_shared::models::core::task_namespace::NewTaskNamespace;
use tasker_shared::models::task_request::TaskRequest;
use tasker_shared::models::{NamedTask, TaskNamespace};
use tasker_shared::system_context::SystemContext;

use super::TaskInitializationError;

/// Resolves or creates task namespaces and named tasks
pub struct NamespaceResolver {
    context: Arc<SystemContext>,
}

impl NamespaceResolver {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Resolve NamedTask ID from TaskRequest (create if not exists)
    pub async fn resolve_named_task_uuid(
        &self,
        task_request: &TaskRequest,
    ) -> Result<Uuid, TaskInitializationError> {
        // First, find or create the task namespace
        let namespace = self
            .find_or_create_namespace(&task_request.namespace)
            .await?;

        // Find or create the named task
        let named_task = self
            .find_or_create_named_task(task_request, namespace.task_namespace_uuid as Uuid)
            .await?;

        Ok(named_task.named_task_uuid)
    }

    /// Find or create a task namespace
    async fn find_or_create_namespace(
        &self,
        namespace_name: &str,
    ) -> Result<TaskNamespace, TaskInitializationError> {
        // Try to find existing namespace first
        if let Some(existing) =
            TaskNamespace::find_by_name(self.context.database_pool(), namespace_name)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!("Failed to query namespace: {e}"))
                })?
        {
            return Ok(existing);
        }

        // Create new namespace if not found
        let new_namespace = NewTaskNamespace {
            name: namespace_name.to_string(),
            description: Some(format!("Auto-created namespace for {namespace_name}")),
        };

        let namespace = TaskNamespace::create(self.context.database_pool(), new_namespace)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!("Failed to create namespace: {e}"))
            })?;

        Ok(namespace)
    }

    /// Find or create a named task
    async fn find_or_create_named_task(
        &self,
        task_request: &TaskRequest,
        task_namespace_uuid: Uuid,
    ) -> Result<NamedTask, TaskInitializationError> {
        // Try to find existing named task first
        let existing_task = NamedTask::find_by_name_version_namespace(
            self.context.database_pool(),
            &task_request.name,
            &task_request.version,
            task_namespace_uuid,
        )
        .await
        .map_err(|e| {
            TaskInitializationError::Database(format!("Failed to query named task: {e}"))
        })?;

        if let Some(existing) = existing_task {
            return Ok(existing);
        }

        // Template not found - this should never happen in production
        // Templates must be registered by workers before orchestration can use them
        Err(TaskInitializationError::ConfigurationNotFound(format!(
            "Task template not found: {}/{}/{}. Templates must be registered by workers before orchestration can use them.",
            task_request.namespace, task_request.name, task_request.version
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_task_request(namespace: &str, name: &str, version: &str) -> TaskRequest {
        TaskRequest::new(name.to_string(), namespace.to_string())
            .with_version(version.to_string())
            .with_context(serde_json::json!({"test": true}))
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_find_or_create_namespace_creates_new(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let resolver = NamespaceResolver::new(context.clone());

        let namespace_name = "test_namespace_new";
        let namespace = resolver.find_or_create_namespace(namespace_name).await?;

        assert_eq!(namespace.name, namespace_name);
        assert!(namespace.description.is_some());
        assert!(namespace.description.unwrap().contains(namespace_name));

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_find_or_create_namespace_finds_existing(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let resolver = NamespaceResolver::new(context.clone());

        let namespace_name = "test_namespace_existing";

        // Create first namespace
        let first_namespace = resolver.find_or_create_namespace(namespace_name).await?;
        let first_uuid = first_namespace.task_namespace_uuid;

        // Try to create again - should find existing
        let second_namespace = resolver.find_or_create_namespace(namespace_name).await?;
        let second_uuid = second_namespace.task_namespace_uuid;

        assert_eq!(first_uuid, second_uuid);
        assert_eq!(first_namespace.name, second_namespace.name);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_find_or_create_named_task_returns_error_when_not_found(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let resolver = NamespaceResolver::new(context.clone());

        // Create a namespace first
        let namespace = resolver.find_or_create_namespace("test_ns").await?;

        // Try to find named task that doesn't exist
        let task_request = create_test_task_request("test_ns", "nonexistent_task", "1.0.0");
        let result = resolver
            .find_or_create_named_task(&task_request, namespace.task_namespace_uuid)
            .await;

        assert!(result.is_err());
        match result {
            Err(TaskInitializationError::ConfigurationNotFound(msg)) => {
                assert!(msg.contains("Task template not found"));
                assert!(msg.contains("nonexistent_task"));
            }
            _ => panic!("Expected ConfigurationNotFound error"),
        }

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_resolve_named_task_uuid_error_when_task_not_registered(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let resolver = NamespaceResolver::new(context);

        let task_request = create_test_task_request("some_namespace", "unregistered_task", "1.0.0");
        let result = resolver.resolve_named_task_uuid(&task_request).await;

        assert!(result.is_err());
        match result {
            Err(TaskInitializationError::ConfigurationNotFound(msg)) => {
                assert!(msg.contains("Task template not found"));
            }
            _ => panic!("Expected ConfigurationNotFound error"),
        }

        Ok(())
    }
}
