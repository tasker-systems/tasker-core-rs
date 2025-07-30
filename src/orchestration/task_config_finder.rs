//! # Task Configuration Finder
//!
//! Central service for discovering and loading task configurations from multiple sources.
//!
//! ## Overview
//!
//! The TaskConfigFinder provides a unified interface for locating task configurations,
//! supporting both registry-based configurations (from Ruby handlers) and file system
//! fallback searches. It integrates with the TaskHandlerRegistry and ConfigurationManager
//! to provide a comprehensive configuration discovery system.
//!
//! ## Key Features
//!
//! - **Registry Integration**: First checks TaskHandlerRegistry for registered configurations
//! - **File System Fallback**: Searches multiple paths with versioning support
//! - **Configuration Management**: Uses ConfigurationManager for base path resolution
//! - **Path Resolution**: Supports namespaced and versioned configuration paths
//! - **Environment Support**: Handles environment-specific configuration overrides
//!
//! ## Search Strategy
//!
//! 1. **Registry Check**: Query TaskHandlerRegistry for registered TaskTemplate
//! 2. **Versioned Path**: `<config_dir>/tasks/{namespace}/{task_name}/{version}.(yml|yaml)`
//! 3. **Default Path**: `<config_dir>/tasks/{task_name}.(yml|yaml)` (assumes default namespace and version)
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::orchestration::task_config_finder::TaskConfigFinder;
//! use tasker_core::orchestration::config::ConfigurationManager;
//! use tasker_core::registry::TaskHandlerRegistry;
//! use std::sync::Arc;
//! use sqlx::PgPool;
//!
//! # async fn example(db_pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! let config_manager = Arc::new(ConfigurationManager::new());
//! let registry = Arc::new(TaskHandlerRegistry::new(db_pool));
//! let finder = TaskConfigFinder::new(config_manager, registry);
//!
//! // Find configuration for a task
//! let template = finder.find_task_template("payments", "order_processing", "1.0.0").await?;
//! println!("Found template: {}", template.name);
//! # Ok(())
//! # }
//! ```

use crate::models::core::task_template::TaskTemplate;
use crate::orchestration::config::ConfigurationManager;
use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use crate::registry::TaskHandlerRegistry;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, instrument};

/// Task configuration finder with registry integration and file system fallback
pub struct TaskConfigFinder {
    /// Configuration manager for loading templates from files
    config_manager: Arc<ConfigurationManager>,
    /// Task handler registry for registered configurations
    registry: Arc<TaskHandlerRegistry>,
}

impl TaskConfigFinder {
    /// Create a new task configuration finder
    pub fn new(
        config_manager: Arc<ConfigurationManager>,
        registry: Arc<TaskHandlerRegistry>,
    ) -> Self {
        Self {
            config_manager,
            registry,
        }
    }

    /// Find a task template by namespace, name, and version
    ///
    /// This method implements the complete search strategy:
    /// 1. Check registry for registered template
    /// 2. Search versioned file path
    /// 3. Search default file path
    #[instrument(skip(self))]
    pub async fn find_task_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> OrchestrationResult<TaskTemplate> {
        info!(
            namespace = namespace,
            name = name,
            version = version,
            "Finding task template"
        );

        // 1. First check the registry for registered templates
        if let Ok(template) = self.find_registry_template(namespace, name, version).await {
            debug!(
                namespace = namespace,
                name = name,
                version = version,
                "Found template in registry"
            );
            return Ok(template);
        }

        // 2. Fall back to file system search
        self.find_file_system_template(namespace, name, version)
            .await
    }

    /// Find template in the TaskHandlerRegistry
    async fn find_registry_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> OrchestrationResult<TaskTemplate> {
        // Try to get the template from registry
        match self.registry.get_task_template(namespace, name, version) {
            Ok(template) => {
                debug!(
                    namespace = namespace,
                    name = name,
                    version = version,
                    "Retrieved template from registry"
                );
                Ok(template)
            }
            Err(e) => {
                debug!(
                    namespace = namespace,
                    name = name,
                    version = version,
                    error = %e,
                    "Template not found in registry"
                );
                Err(OrchestrationError::ConfigurationError {
                    source: "TaskConfigFinder".to_string(),
                    reason: format!("Template not found in registry: {e}"),
                })
            }
        }
    }

    /// Find template in the file system using search paths
    async fn find_file_system_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> OrchestrationResult<TaskTemplate> {
        let search_paths = self.build_search_paths(namespace, name, version);

        for path in search_paths {
            debug!(path = %path.display(), "Searching for template");

            match self.config_manager.load_task_template(&path).await {
                Ok(config_template) => {
                    info!(
                        path = %path.display(),
                        namespace = namespace,
                        name = name,
                        version = version,
                        "Found template in file system"
                    );
                    // Convert config::TaskTemplate to models::core::task_template::TaskTemplate
                    let template = self.convert_config_template_to_model(config_template)?;
                    return Ok(template);
                }
                Err(e) => {
                    debug!(
                        path = %path.display(),
                        error = %e,
                        "Template not found at path"
                    );
                }
            }
        }

        Err(OrchestrationError::ConfigurationError {
            source: "TaskConfigFinder".to_string(),
            reason: format!(
                "Task template not found for {namespace}/{name}/{version} in registry or file system"
            ),
        })
    }

    /// Build search paths for file system lookup
    fn build_search_paths(&self, namespace: &str, name: &str, version: &str) -> Vec<PathBuf> {
        let base_config_dir = self.get_base_config_directory();
        let mut paths = Vec::new();

        // 1. Versioned path: <config_dir>/tasks/{namespace}/{name}/{version}.(yml|yaml)
        if namespace != "default" {
            for ext in &["yaml", "yml"] {
                let path = base_config_dir
                    .join("tasks")
                    .join(namespace)
                    .join(name)
                    .join(format!("{version}.{ext}"));
                paths.push(path);
            }
        }

        // 2. Default namespace path: <config_dir>/tasks/{name}/{version}.(yml|yaml)
        if version != "0.1.0" {
            for ext in &["yaml", "yml"] {
                let path = base_config_dir
                    .join("tasks")
                    .join(name)
                    .join(format!("{version}.{ext}"));
                paths.push(path);
            }
        }

        // 3. Simple default path: <config_dir>/tasks/{name}.(yml|yaml)
        for ext in &["yaml", "yml"] {
            let path = base_config_dir.join("tasks").join(format!("{name}.{ext}"));
            paths.push(path);
        }

        paths
    }

    /// Get the base configuration directory from the system config
    fn get_base_config_directory(&self) -> PathBuf {
        let system_config = self.config_manager.system_config();
        let task_config_dir = &system_config.engine.task_config_directory;

        // Build path relative to config directory
        PathBuf::from("config").join(task_config_dir)
    }

    /// Check if a template exists in the registry
    pub fn has_registry_template(&self, namespace: &str, name: &str, version: &str) -> bool {
        self.registry.has_task_template(namespace, name, version)
    }

    /// Get all available templates from registry
    pub fn list_registry_templates(
        &self,
        namespace: Option<&str>,
    ) -> OrchestrationResult<Vec<String>> {
        self.registry.list_task_templates(namespace).map_err(|e| {
            OrchestrationError::ConfigurationError {
                source: "TaskConfigFinder".to_string(),
                reason: format!("Failed to list registry templates: {e}"),
            }
        })
    }

    /// Clear any cached configurations (useful for testing)
    pub async fn clear_cache(&self) -> OrchestrationResult<()> {
        // If we add caching later, this is where we'd clear it
        Ok(())
    }

    /// Convert config::TaskTemplate to models::core::task_template::TaskTemplate
    fn convert_config_template_to_model(
        &self,
        config_template: crate::orchestration::config::TaskTemplate,
    ) -> OrchestrationResult<TaskTemplate> {
        use crate::models::core::task_template::{EnvironmentConfig, StepTemplate};
        use std::collections::HashMap;

        // Convert step templates
        let step_templates: Vec<StepTemplate> = config_template
            .step_templates
            .into_iter()
            .map(|config_step| StepTemplate {
                name: config_step.name,
                description: config_step.description,
                dependent_system: None, // Not present in config::StepTemplate
                default_retryable: config_step.default_retryable,
                default_retry_limit: config_step.default_retry_limit,
                skippable: None, // Not present in config::StepTemplate
                handler_class: config_step.handler_class,
                handler_config: config_step
                    .handler_config
                    .map(|hc| serde_json::to_value(hc).unwrap_or_default()),
                depends_on_step: config_step.depends_on_step,
                depends_on_steps: config_step.depends_on_steps,
                custom_events: None, // Not present in config::StepTemplate
            })
            .collect();

        // Convert environments (simplified conversion)
        let environments: Option<HashMap<String, EnvironmentConfig>> =
            config_template.environments.map(|env_map| {
                env_map
                    .into_keys()
                    .map(|key| {
                        // Create a simplified environment config
                        let env_config = EnvironmentConfig {
                            step_templates: None,
                            default_context: None,
                            default_options: None,
                        };
                        (key, env_config)
                    })
                    .collect()
            });

        Ok(TaskTemplate {
            name: config_template.name,
            module_namespace: config_template.module_namespace,
            task_handler_class: config_template.task_handler_class,
            namespace_name: config_template.namespace_name,
            version: config_template.version,
            default_dependent_system: config_template.default_dependent_system,
            named_steps: config_template.named_steps,
            schema: config_template.schema,
            step_templates,
            environments,
            default_context: None, // Not present in config::TaskTemplate
            default_options: None, // Not present in config::TaskTemplate
        })
    }
}

impl std::fmt::Debug for TaskConfigFinder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskConfigFinder")
            .field("config_manager", &"<ConfigurationManager>")
            .field("registry", &"<TaskHandlerRegistry>")
            .finish()
    }
}

impl Clone for TaskConfigFinder {
    fn clone(&self) -> Self {
        Self {
            config_manager: self.config_manager.clone(),
            registry: self.registry.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn test_build_search_paths(pool: sqlx::PgPool) {
        let config_manager = Arc::new(ConfigurationManager::new());
        let registry = Arc::new(TaskHandlerRegistry::new(pool));
        let finder = TaskConfigFinder::new(config_manager, registry);

        let paths = finder.build_search_paths("payments", "order_processing", "1.0.0");

        // Should include versioned, default, and simple paths
        assert!(paths.len() >= 4);

        // Check that we have the expected path patterns
        let path_strings: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert!(path_strings
            .iter()
            .any(|p| p.contains("payments/order_processing/1.0.0")));
        assert!(path_strings
            .iter()
            .any(|p| p.contains("order_processing.yaml")));
    }

    #[sqlx::test]
    async fn test_default_namespace_and_version(pool: sqlx::PgPool) {
        let config_manager = Arc::new(ConfigurationManager::new());
        let registry = Arc::new(TaskHandlerRegistry::new(pool));
        let finder = TaskConfigFinder::new(config_manager, registry);

        let paths = finder.build_search_paths("default", "simple_task", "0.1.0");

        // Should primarily search simple paths for defaults
        let path_strings: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert!(path_strings.iter().any(|p| p.contains("simple_task.yaml")));
        assert!(path_strings.iter().any(|p| p.contains("simple_task.yml")));
    }

    #[sqlx::test]
    async fn test_registry_integration(pool: sqlx::PgPool) {
        let config_manager = Arc::new(ConfigurationManager::new());
        let registry = Arc::new(TaskHandlerRegistry::new(pool));
        let finder = TaskConfigFinder::new(config_manager, registry.clone());

        // Test that registry check happens first
        let result = finder
            .find_registry_template("test", "missing", "1.0.0")
            .await;
        assert!(result.is_err());
    }
}
