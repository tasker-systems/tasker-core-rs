//! # Database-First Task Handler Registry
//!
//! Database-backed registry for distributed task handler resolution.
//!
//! ## Architecture
//! ```text
//! TaskRequest -> Database Query -> TaskNamespace + NamedTask -> HandlerMetadata
//! ```
//!
//! ## Key Features
//!
//! - **Database-First**: All handler resolution via database queries (no HashMap)
//! - **Distributed Ready**: Works identically in embedded and distributed deployments
//! - **Persistent State**: Handler registrations survive restarts and deployments
//! - **Worker Awareness**: Real-time knowledge of available workers per namespace
//! - **Event Integration**: Registration notifications via EventPublisher
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::registry::TaskHandlerRegistry;
//! use tasker_shared::models::core::task_request::TaskRequest;
//! use sqlx::PgPool;
//!
//! # async fn example(db_pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! let registry = TaskHandlerRegistry::new(db_pool);
//!
//! // Handler resolution via database queries
//! let task_request = TaskRequest::new("order_processing".to_string(), "fulfillment".to_string());
//! let handler_metadata = registry.resolve_handler(&task_request).await?;
//! # Ok(())
//! # }
//! ```

use crate::errors::{TaskerError, TaskerResult};
use crate::events::EventPublisher;
use crate::models::core::{
    named_task::NamedTask, task_namespace::TaskNamespace, task_request::TaskRequest,
    task_template::TaskTemplate,
};
use crate::system_context::SystemContext;
use crate::types::HandlerMetadata;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{debug, info};

/// Key for handler lookup in the registry
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandlerKey {
    pub namespace: String,
    pub name: String,
    pub version: String,
}

impl HandlerKey {
    /// Create a new handler key with explicit values
    pub fn new(namespace: String, name: String, version: String) -> Self {
        Self {
            namespace,
            name,
            version,
        }
    }

    /// Create a handler key from a TaskRequest
    pub fn from_task_request(request: &TaskRequest) -> Self {
        Self {
            namespace: request.namespace.clone(),
            name: request.name.clone(),
            version: request.version.clone(),
        }
    }

    /// Convert to string key for storage
    pub fn key_string(&self) -> String {
        format!("{}/{}/{}", self.namespace, self.name, self.version)
    }
}

impl std::fmt::Display for HandlerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.namespace, self.name, self.version)
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_handlers: usize,
    pub total_step_handlers: usize,
    pub total_ffi_handlers: usize,
    pub namespaces: Vec<String>,
}

/// Result of TaskTemplate discovery and registration process
#[derive(Debug, Clone)]
pub struct TaskTemplateDiscoveryResult {
    pub total_files: usize,
    pub successful_registrations: usize,
    pub failed_registrations: usize,
    pub errors: Vec<String>,
    pub discovered_templates: Vec<String>,
    pub discovered_namespaces: Vec<String>,
}

/// Database-first task handler registry for distributed orchestration
pub struct TaskHandlerRegistry {
    /// Database connection pool for persistent storage
    db_pool: PgPool,
    /// Event publisher for notifications
    event_publisher: Option<Arc<EventPublisher>>,
    /// Search paths from TaskTemplateConfig
    search_paths: Option<Vec<String>>,
}

impl TaskHandlerRegistry {
    /// Create a new database-backed task handler registry
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            event_publisher: None,
            search_paths: None,
        }
    }

    /// Create a new registry with database connection and event publisher
    pub fn with_event_publisher(db_pool: PgPool, event_publisher: Arc<EventPublisher>) -> Self {
        Self {
            db_pool,
            event_publisher: Some(event_publisher),
            search_paths: None,
        }
    }

    pub fn with_system_context(context: Arc<SystemContext>) -> Self {
        Self {
            db_pool: context.database_pool().clone(),
            event_publisher: Some(context.event_publisher.clone()),
            search_paths: Some(context.tasker_config.task_templates.search_paths.clone()),
        }
    }

    pub async fn get_task_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<TaskTemplate> {
        let handler_metadata = self
            .get_task_template_from_registry(namespace, name, version)
            .await?;

        let task_template =
            serde_json::from_value(handler_metadata.config_schema.unwrap_or_default())?;
        Ok(task_template)
    }

    /// Register a TaskTemplate to the database
    /// Similar to the Ruby TaskTemplateRegistry.register_task_template_in_database method
    pub async fn register_task_template(&self, template: &TaskTemplate) -> TaskerResult<()> {
        debug!(
            namespace = &template.namespace_name,
            name = &template.name,
            version = &template.version,
            "🔄 Registering TaskTemplate to database"
        );

        // Validate template has required fields
        if template.name.is_empty() {
            return Err(TaskerError::ValidationError(
                "Template name cannot be empty".to_string(),
            ));
        }
        if template.namespace_name.is_empty() {
            return Err(TaskerError::ValidationError(
                "Template namespace_name cannot be empty".to_string(),
            ));
        }
        if template.version.is_empty() {
            return Err(TaskerError::ValidationError(
                "Template version cannot be empty".to_string(),
            ));
        }

        // Validate template structure
        template.validate().map_err(|e| {
            TaskerError::ValidationError(format!("Template validation failed: {e}"))
        })?;

        // Convert template to JSON for database storage (similar to Ruby's build_database_configuration)
        let configuration = serde_json::to_value(template).map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to serialize template: {e}"))
        })?;

        use crate::models::core::{named_task::NamedTask, task_namespace::TaskNamespace};

        // 1. Find or create TaskNamespace using the model method
        let namespace = TaskNamespace::find_or_create(&self.db_pool, &template.namespace_name)
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!("Failed to create/find namespace: {e}"))
            })?;

        // 2. Find existing named task or create new one
        let existing_task = NamedTask::find_by_name_version_namespace(
            &self.db_pool,
            &template.name,
            &template.version,
            namespace.task_namespace_uuid,
        )
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to find existing task: {e}")))?;

        if let Some(task) = existing_task {
            // Update existing task with new configuration
            NamedTask::update(
                &self.db_pool,
                task.named_task_uuid,
                template.description.clone(),
                Some(configuration),
            )
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to update named task: {e}")))?;
        } else {
            // Create new named task
            use crate::models::core::named_task::NewNamedTask;
            let new_task = NewNamedTask {
                name: template.name.clone(),
                version: Some(template.version.clone()),
                description: template.description.clone(),
                task_namespace_uuid: namespace.task_namespace_uuid,
                configuration: Some(configuration),
            };

            NamedTask::create(&self.db_pool, new_task)
                .await
                .map_err(|e| {
                    TaskerError::DatabaseError(format!("Failed to create named task: {e}"))
                })?;
        }

        info!(
            namespace = &template.namespace_name,
            name = &template.name,
            version = &template.version,
            "✅ TaskTemplate registered to database successfully"
        );

        Ok(())
    }

    /// Discover and register TaskTemplates from a directory
    /// Similar to the Ruby TaskTemplateRegistry.register_task_templates_from_directory method
    pub async fn discover_and_register_templates(
        &self,
        directory_path: &str,
    ) -> TaskerResult<TaskTemplateDiscoveryResult> {
        info!(
            "📁 Discovering TaskTemplates in directory: {}",
            directory_path
        );

        let path = std::path::Path::new(directory_path);
        if !path.exists() {
            return Err(TaskerError::ConfigurationError(format!(
                "Directory not found: {}",
                directory_path
            )));
        }

        // Find all YAML files recursively
        let yaml_files = self.find_yaml_files(directory_path).await?;

        if yaml_files.is_empty() {
            info!("📁 No YAML files found in directory: {directory_path}");
            return Ok(TaskTemplateDiscoveryResult {
                total_files: 0,
                successful_registrations: 0,
                failed_registrations: 0,
                errors: Vec::new(),
                discovered_templates: Vec::new(),
                discovered_namespaces: Vec::new(),
            });
        }

        let mut result = TaskTemplateDiscoveryResult {
            total_files: yaml_files.len(),
            successful_registrations: 0,
            failed_registrations: 0,
            errors: Vec::new(),
            discovered_templates: Vec::new(),
            discovered_namespaces: Vec::new(),
        };

        // Process each YAML file
        for yaml_path in &yaml_files {
            match self.load_template_from_file(yaml_path).await {
                Ok(template) => match self.register_task_template(&template).await {
                    Ok(_) => {
                        result.successful_registrations += 1;
                        result.discovered_templates.push(format!(
                            "{}/{}/{}",
                            template.namespace_name, template.name, template.version
                        ));
                        result
                            .discovered_namespaces
                            .push(template.namespace_name.clone());
                        debug!(
                            "✅ Registered template from {}: {}/{}:{}",
                            yaml_path.display(),
                            template.namespace_name,
                            template.name,
                            template.version
                        );
                    }
                    Err(e) => {
                        result.failed_registrations += 1;
                        let error_msg = format!(
                            "Failed to register template from {}: {}",
                            yaml_path.display(),
                            e
                        );
                        result.errors.push(error_msg.clone());
                        debug!("❌ {error_msg}");
                    }
                },
                Err(e) => {
                    result.failed_registrations += 1;
                    let error_msg = format!(
                        "Failed to load template from {}: {}",
                        yaml_path.display(),
                        e
                    );
                    result.errors.push(error_msg.clone());
                    debug!("❌ {error_msg}");
                }
            }
        }

        info!(
            "📊 Template discovery complete: {} files, {} successful, {} failed",
            result.total_files, result.successful_registrations, result.failed_registrations
        );

        Ok(result)
    }

    /// Load a TaskTemplate from a YAML file
    /// Similar to the Ruby TaskTemplateRegistry.load_task_template_from_file method
    async fn load_template_from_file(&self, path: &std::path::Path) -> TaskerResult<TaskTemplate> {
        debug!("📖 Loading TaskTemplate from file: {}", path.display());

        let yaml_content = tokio::fs::read_to_string(path).await.map_err(|e| {
            TaskerError::ConfigurationError(format!(
                "Failed to read file {}: {}",
                path.display(),
                e
            ))
        })?;

        let template: TaskTemplate = serde_yaml::from_str(&yaml_content).map_err(|e| {
            TaskerError::ConfigurationError(format!(
                "Failed to parse TaskTemplate YAML from {}: {}",
                path.display(),
                e
            ))
        })?;

        debug!(
            "📖 Loaded TaskTemplate from {}: {}/{}/{}",
            path.display(),
            template.namespace_name,
            template.name,
            template.version
        );

        Ok(template)
    }

    /// Find all YAML files in a directory recursively using async operations
    async fn find_yaml_files(&self, directory_path: &str) -> TaskerResult<Vec<std::path::PathBuf>> {
        use std::collections::VecDeque;
        use std::path::PathBuf;
        use tokio::fs;

        let mut yaml_files = Vec::new();
        let mut dirs_to_scan = VecDeque::new();
        dirs_to_scan.push_back(PathBuf::from(directory_path));

        // Iterative directory walking using async operations
        while let Some(current_dir) = dirs_to_scan.pop_front() {
            let mut entries = match fs::read_dir(&current_dir).await {
                Ok(entries) => entries,
                Err(e) => {
                    debug!("Failed to read directory {:?}: {}", current_dir, e);
                    continue; // Skip directories we can't read
                }
            };

            loop {
                let entry = match entries.next_entry().await {
                    Ok(Some(entry)) => entry,
                    Ok(None) => break, // No more entries
                    Err(e) => {
                        debug!("Failed to read directory entry: {}", e);
                        continue; // Skip this entry and try next
                    }
                };
                let path = entry.path();

                if path.is_dir() {
                    // Add subdirectory to scan queue
                    dirs_to_scan.push_back(path);
                } else if let Some(extension) = path.extension() {
                    if extension == "yml" || extension == "yaml" {
                        yaml_files.push(path);
                    }
                }
            }
        }

        debug!(
            "📁 Found {} YAML files in {}",
            yaml_files.len(),
            directory_path
        );
        Ok(yaml_files)
    }

    /// Check if a TaskTemplate is already registered
    /// Similar to the Ruby TaskTemplateRegistry.task_template_registered? method
    pub async fn is_template_registered(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<bool> {
        use crate::models::core::{named_task::NamedTask, task_namespace::TaskNamespace};

        // First find the namespace
        let namespace_obj = TaskNamespace::find_by_name(&self.db_pool, namespace)
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!("Failed to find namespace '{namespace}': {e}"))
            })?;

        let namespace_uuid = match namespace_obj {
            Some(ns) => ns.task_namespace_uuid,
            None => return Ok(false), // Namespace doesn't exist, so template can't be registered
        };

        // Check if the named task exists
        let named_task =
            NamedTask::find_by_name_version_namespace(&self.db_pool, name, version, namespace_uuid)
                .await
                .map_err(|e| {
                    TaskerError::DatabaseError(format!(
                        "Failed to check template registration: {e}"
                    ))
                })?;

        Ok(named_task.is_some())
    }

    /// Resolve a handler from a TaskRequest using database queries
    pub async fn resolve_handler(&self, request: &TaskRequest) -> TaskerResult<HandlerMetadata> {
        debug!(
            namespace = &request.namespace,
            name = &request.name,
            version = &request.version,
            "🎯 DATABASE-FIRST: Resolving handler via database queries"
        );

        let handler_metadata = self
            .get_task_template_from_registry(&request.namespace, &request.name, &request.version)
            .await?;

        Ok(handler_metadata)
    }

    async fn get_task_template_from_registry(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<HandlerMetadata> {
        debug!(
            namespace = namespace,
            name = name,
            version = version,
            "🔍 Starting database lookup for TaskTemplate"
        );

        // 1. Find the task namespace
        let task_namespace = TaskNamespace::find_by_name(&self.db_pool, namespace)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to query namespace: {e}")))?
            .ok_or_else(|| {
                debug!(namespace = namespace, "❌ Namespace not found in database");
                TaskerError::ValidationError(format!("Namespace not found: {namespace}"))
            })?;

        debug!(
            namespace = namespace,
            namespace_uuid = %task_namespace.task_namespace_uuid,
            "✅ Found namespace in database"
        );

        // 2. Find the named task in that namespace
        let named_task = NamedTask::find_latest_by_name_namespace(
            &self.db_pool,
            name,
            task_namespace.task_namespace_uuid,
        )
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to query named task: {e}")))?
        .ok_or_else(|| {
            debug!(
                namespace = namespace,
                name = name,
                namespace_uuid = %task_namespace.task_namespace_uuid,
                "❌ Named task not found in database"
            );
            TaskerError::ValidationError(format!("Task not found: {namespace}/{name}"))
        })?;

        debug!(
            namespace = namespace,
            name = name,
            version = named_task.version,
            task_uuid = %named_task.named_task_uuid,
            config_present = named_task.configuration.is_some(),
            "✅ Found named task in database"
        );

        info!(
            namespace = &namespace,
            name = &name,
            version = &version,
            "✅ DATABASE-FIRST: Handler resolved for task (pgmq architecture - no worker registration needed)"
        );

        let default_dependent_system = named_task.configuration.as_ref().and_then(|config| {
            config.get("default_dependent_system").and_then(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .or(Some("default".to_string()))
            })
        });
        let handler_class = named_task.configuration.as_ref().and_then(|config| {
            config.get("handler_class").and_then(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .or(Some("TaskerCore::TaskHandler::Base".to_string()))
            })
        });
        let handler_metadata = HandlerMetadata {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            default_dependent_system,
            handler_class: handler_class.unwrap_or("TaskerCore::TaskHandler::Base".to_string()),
            config_schema: named_task.configuration,
            registered_at: Utc::now(),
        };

        Ok(handler_metadata)
    }
}

// Note: Default implementation removed - TaskHandlerRegistry now requires database pool

impl Clone for TaskHandlerRegistry {
    fn clone(&self) -> Self {
        Self {
            db_pool: self.db_pool.clone(),
            event_publisher: self.event_publisher.clone(),
            search_paths: self.search_paths.clone(),
        }
    }
}
