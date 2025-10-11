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
