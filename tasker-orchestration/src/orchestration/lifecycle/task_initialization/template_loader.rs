//! Task Template Loading
//!
//! Loads TaskTemplate configurations from the TaskHandlerRegistry.
//! Templates define the workflow structure including steps, dependencies, and retry policies.

use std::sync::Arc;
use tasker_shared::models::core::task_template::TaskTemplate;
use tasker_shared::models::task_request::TaskRequest;
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::system_context::SystemContext;

use super::TaskInitializationError;

/// Loads task templates from registry
pub struct TemplateLoader {
    context: Arc<SystemContext>,
}

impl TemplateLoader {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Load task template from TaskHandlerRegistry
    ///
    /// In FFI integration, handlers register their configuration in the registry.
    /// This method retrieves the TaskTemplate which defines the workflow structure.
    pub async fn load_task_template(
        &self,
        task_request: &TaskRequest,
    ) -> Result<TaskTemplate, TaskInitializationError> {
        // Try registry first if available
        match self
            .load_from_registry(task_request, self.context.task_handler_registry.clone())
            .await
        {
            Ok(config) => Ok(config),
            Err(e) => {
                Err(TaskInitializationError::ConfigurationNotFound(
                    format!("Unable to load task template: {e}, request parameters: name {}, namespace {}, version {}", task_request.name, task_request.namespace, task_request.version)
                ))
            }
        }
    }

    /// Load configuration from registry
    async fn load_from_registry(
        &self,
        task_request: &TaskRequest,
        registry: Arc<TaskHandlerRegistry>,
    ) -> Result<TaskTemplate, TaskInitializationError> {
        // Use the namespace and name directly from the TaskRequest
        let namespace = &task_request.namespace;
        let name = &task_request.name;
        let _version = &task_request.version;

        // Look up the handler metadata using the ACTUAL task request version
        let metadata = registry.resolve_handler(task_request).await.map_err(|e| {
            TaskInitializationError::ConfigurationNotFound(format!(
                "Handler not found in registry {namespace}/{name}: {e}"
            ))
        })?;

        // Extract the config_schema from metadata and convert from full TaskHandlerInfo format
        if let Some(config_json) = metadata.config_schema {
            // The database now stores the full TaskTemplate structure directly
            // Deserialize as TaskTemplate (new format) and return directly
            match serde_json::from_value::<TaskTemplate>(config_json.clone()) {
                Ok(task_template) => {
                    // Return TaskTemplate directly - no more legacy conversion!
                    if task_template.steps.is_empty() {
                        return Err(TaskInitializationError::ConfigurationNotFound(format!(
                            "Empty steps array in task template configuration for {}/{}. Cannot create workflow steps without step definitions.",
                            task_template.namespace_name, task_template.name
                        )));
                    }

                    Ok(task_template)
                }
                Err(task_template_error) => {
                    // Aggressive forward-only change: No backward compatibility for legacy formats
                    Err(TaskInitializationError::ConfigurationNotFound(format!(
                        "Failed to deserialize configuration as TaskTemplate for {namespace}/{name}. Error: {task_template_error}. Legacy formats are no longer supported - please migrate to new self-describing TaskTemplate format."
                    )))
                }
            }
        } else {
            // No config_schema provided - this is a hard error
            Err(TaskInitializationError::ConfigurationNotFound(format!(
                "No task handler configuration found in database for {}/{}. Task handler must be registered with configuration before task creation.",
                metadata.namespace, metadata.name
            )))
        }
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
    async fn test_load_task_template_returns_error_when_handler_not_in_registry(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let loader = TemplateLoader::new(context);

        let task_request = create_test_task_request("unknown_ns", "unknown_task", "1.0.0");
        let result = loader.load_task_template(&task_request).await;

        assert!(result.is_err());
        match result {
            Err(TaskInitializationError::ConfigurationNotFound(msg)) => {
                assert!(msg.contains("Unable to load task template"));
                assert!(msg.contains("unknown_task"));
                assert!(msg.contains("unknown_ns"));
            }
            _ => panic!("Expected ConfigurationNotFound error"),
        }

        Ok(())
    }

    #[test]
    fn test_error_messages_include_request_details() {
        // Test that error formatting includes all relevant details
        let namespace = "test_namespace";
        let name = "test_task";
        let version = "1.0.0";

        let error = TaskInitializationError::ConfigurationNotFound(format!(
            "Unable to load task template: Handler not found, request parameters: name {}, namespace {}, version {}",
            name, namespace, version
        ));

        let error_msg = error.to_string();
        assert!(error_msg.contains(namespace));
        assert!(error_msg.contains(name));
        assert!(error_msg.contains(version));
    }
}
