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
//! use tasker_shared::orchestration::task_config_finder::TaskConfigFinder;
//! use tasker_shared::orchestration::config::ConfigurationManager;
//! use tasker_shared::registry::TaskHandlerRegistry;
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

use crate::orchestration::config::ConfigurationManager;
use std::path::PathBuf;
use std::sync::Arc;
use tasker_shared::errors::{OrchestrationError, OrchestrationResult};
use tasker_shared::models::core::task_template::TaskTemplate;
use tasker_shared::registry::TaskHandlerRegistry;
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
        match self
            .registry
            .get_task_template(namespace, name, version)
            .await
        {
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
                    config_source: "TaskConfigFinder".to_string(),
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

            // Try loading as new TaskTemplate format first
            match self.load_new_format_template(&path).await {
                Ok(template) => {
                    info!(
                        path = %path.display(),
                        namespace = namespace,
                        name = name,
                        version = version,
                        "Found new-format template in file system"
                    );
                    return Ok(template);
                }
                Err(_) => {
                    debug!(
                        path = %path.display(),
                        "Not a new-format template, trying old format"
                    );
                }
            }
        }

        Err(OrchestrationError::ConfigurationError {
            config_source: "TaskConfigFinder".to_string(),
            reason: format!(
                "Task template not found for {namespace}/{name}/{version} in registry or file system"
            ),
        })
    }

    /// Build search paths for file system lookup
    fn build_search_paths(&self, namespace: &str, name: &str, version: &str) -> Vec<PathBuf> {
        let base_config_dir = self.get_base_config_directory();
        let mut paths = Vec::new();

        debug!(
            "Building search paths with base_config_dir: {}",
            base_config_dir.display()
        );

        // 1. Versioned path: <config_dir>/{namespace}/{name}/{version}.(yml|yaml)
        if namespace != "default" {
            for ext in &["yaml", "yml"] {
                let path = base_config_dir
                    .join(namespace)
                    .join(name)
                    .join(format!("{version}.{ext}"));
                paths.push(path);
            }
        }

        // 2. Default namespace path: <config_dir>/{name}/{version}.(yml|yaml)
        if version != "0.1.0" {
            for ext in &["yaml", "yml"] {
                let path = base_config_dir.join(name).join(format!("{version}.{ext}"));
                paths.push(path);
            }
        }

        // 3. Simple default path: <config_dir>/{name}.(yml|yaml)
        for ext in &["yaml", "yml"] {
            let path = base_config_dir.join(format!("{name}.{ext}"));
            paths.push(path);
        }

        paths
    }

    /// Get the base configuration directory from the system config
    fn get_base_config_directory(&self) -> PathBuf {
        // For tests and development, use WORKSPACE_PATH to find the project root
        // This is automatically set by Cargo when running tests
        if let Ok(workspace_path) = std::env::var("WORKSPACE_PATH") {
            return PathBuf::from(workspace_path).join("config").join("tasks");
        }

        // Allow override via TASKER_CONFIG_PATH environment variable
        if let Ok(config_path) = std::env::var("TASKER_CONFIG_PATH") {
            return PathBuf::from(config_path).join("tasks");
        }

        // For production, use the configured path
        let system_config = self.config_manager.system_config();
        let task_config_dir = &system_config.engine.task_config_directory;
        PathBuf::from("config").join(task_config_dir)
    }

    /// Load TaskTemplate directly from new format YAML file
    async fn load_new_format_template(
        &self,
        path: &std::path::Path,
    ) -> OrchestrationResult<TaskTemplate> {
        use tokio::fs;

        // Check if file exists with either yaml or yml extension
        let mut actual_path = path.to_path_buf();
        if !actual_path.exists() {
            // Try with .yaml extension
            if actual_path.extension().is_none() {
                actual_path.set_extension("yaml");
                if !actual_path.exists() {
                    actual_path.set_extension("yml");
                }
            }
        }

        if !actual_path.exists() {
            return Err(OrchestrationError::ConfigurationError {
                config_source: "TaskConfigFinder".to_string(),
                reason: format!("TaskTemplate file not found: {}", path.display()),
            });
        }

        // Read and parse the YAML file
        let yaml_content = fs::read_to_string(&actual_path).await.map_err(|e| {
            OrchestrationError::ConfigurationError {
                config_source: "TaskConfigFinder".to_string(),
                reason: format!(
                    "Failed to read TaskTemplate file {}: {}",
                    actual_path.display(),
                    e
                ),
            }
        })?;

        // Parse YAML content directly into TaskTemplate
        let template: TaskTemplate = serde_yaml::from_str(&yaml_content).map_err(|e| {
            OrchestrationError::ConfigurationError {
                config_source: "TaskConfigFinder".to_string(),
                reason: format!(
                    "Failed to parse TaskTemplate YAML {}: {}",
                    actual_path.display(),
                    e
                ),
            }
        })?;

        debug!(
            path = %actual_path.display(),
            template_name = %template.name,
            template_namespace = %template.namespace_name,
            template_version = %template.version,
            "Successfully loaded new-format TaskTemplate"
        );

        Ok(template)
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

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
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

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
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

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
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
