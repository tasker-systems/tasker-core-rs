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

use crate::cache::{CacheProvider, CacheUsageContext};
use crate::config::tasker::CacheConfig;
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
use std::time::Duration;
use tracing::{debug, error, info, warn};

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
    /// TAS-156: Optional distributed cache provider
    cache_provider: Option<Arc<CacheProvider>>,
    /// TAS-156: Cache configuration (for TTL values)
    cache_config: Option<CacheConfig>,
}

crate::debug_with_pgpool!(TaskHandlerRegistry {
    db_pool: PgPool,
    event_publisher,
    search_paths,
    cache_config
});

impl TaskHandlerRegistry {
    /// Create a new database-backed task handler registry
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            event_publisher: None,
            search_paths: None,
            cache_provider: None,
            cache_config: None,
        }
    }

    /// Create a new registry with database connection and event publisher
    pub fn with_event_publisher(db_pool: PgPool, event_publisher: Arc<EventPublisher>) -> Self {
        Self {
            db_pool,
            event_publisher: Some(event_publisher),
            search_paths: None,
            cache_provider: None,
            cache_config: None,
        }
    }

    /// Create a new registry with explicit cache provider (TAS-156)
    pub fn with_cache(
        db_pool: PgPool,
        cache_provider: Arc<CacheProvider>,
        cache_config: CacheConfig,
    ) -> Self {
        Self {
            db_pool,
            event_publisher: None,
            search_paths: None,
            cache_provider: Some(cache_provider),
            cache_config: Some(cache_config),
        }
    }

    pub fn with_system_context(context: Arc<SystemContext>) -> Self {
        // TAS-168: Validate cache provider is distributed (safe for template caching)
        // In-memory caches would drift from worker invalidations, causing stale templates
        let cache_provider =
            if CacheUsageContext::Templates.is_valid_provider(&context.cache_provider) {
                Some(context.cache_provider.clone())
            } else {
                warn!(
                    provider = context.cache_provider.provider_name(),
                    "Cache provider '{}' is not safe for template caching (in-memory cache \
                 would drift from worker invalidations). Template caching disabled.",
                    context.cache_provider.provider_name()
                );
                None
            };

        Self {
            db_pool: context.database_pool().clone(),
            event_publisher: Some(context.event_publisher.clone()),
            // TAS-61 Phase 6C/6D: Use V2 config structure (common.task_templates)
            search_paths: Some(
                context
                    .tasker_config
                    .common
                    .task_templates
                    .search_paths
                    .clone(),
            ),
            // TAS-156/TAS-168: Use validated cache provider
            cache_provider,
            cache_config: context.tasker_config.common.cache.clone(),
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
            "Registering TaskTemplate to database"
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

        // Validate configuration is not empty
        if configuration.is_null() || configuration == serde_json::json!({}) {
            error!(
                "Template configuration is empty or invalid for {}/{}/{}",
                template.namespace_name, template.name, template.version
            );
            return Err(TaskerError::ValidationError(format!(
                "Template configuration cannot be empty for {}/{}/{}",
                template.namespace_name, template.name, template.version
            )));
        }

        // Log configuration size for debugging
        let config_str = serde_json::to_string(&configuration).unwrap_or_default();
        debug!(
            "Template configuration size: {} bytes for {}/{}/{}",
            config_str.len(),
            template.namespace_name,
            template.name,
            template.version
        );

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
            use crate::models::core::identity_strategy::IdentityStrategy;
            use crate::models::core::named_task::NewNamedTask;
            let new_task = NewNamedTask {
                name: template.name.clone(),
                version: Some(template.version.clone()),
                description: template.description.clone(),
                task_namespace_uuid: namespace.task_namespace_uuid,
                configuration: Some(configuration),
                identity_strategy: IdentityStrategy::Strict,
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
            "TaskTemplate registered to database successfully"
        );

        // TAS-156: Invalidate cache entry for this template (best-effort)
        self.invalidate_cache(&template.namespace_name, &template.name, &template.version)
            .await;

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
                            "Registered template from {}: {}/{}:{}",
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
                        debug!("{error_msg}");
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
                    debug!("{error_msg}");
                }
            }
        }

        // Deduplicate discovered namespaces
        result.discovered_namespaces.sort();
        result.discovered_namespaces.dedup();

        info!(
            "Template discovery complete: {} files, {} successful, {} failed",
            result.total_files, result.successful_registrations, result.failed_registrations
        );

        Ok(result)
    }

    /// Load a TaskTemplate from a YAML file
    /// Similar to the Ruby TaskTemplateRegistry.load_task_template_from_file method
    pub async fn load_template_from_file(
        &self,
        path: &std::path::Path,
    ) -> TaskerResult<TaskTemplate> {
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
            "DATABASE-FIRST: Resolving handler via database queries"
        );

        let handler_metadata = self
            .get_task_template_from_registry(&request.namespace, &request.name, &request.version)
            .await?;

        Ok(handler_metadata)
    }

    pub async fn get_task_template_from_registry(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<HandlerMetadata> {
        debug!(
            namespace = namespace,
            name = name,
            version = version,
            "Starting lookup for TaskTemplate"
        );

        // TAS-156: Try cache first (cache-aside pattern)
        if let Some(metadata) = self.try_cache_get(namespace, name, version).await {
            return Ok(metadata);
        }

        // Cache miss or cache unavailable - proceed with DB lookup
        let handler_metadata = self
            .get_task_template_from_db(namespace, name, version)
            .await?;

        // TAS-156: Populate cache on successful DB fetch (best-effort)
        self.try_cache_set(namespace, name, version, &handler_metadata)
            .await;

        Ok(handler_metadata)
    }

    /// Database lookup for task template (extracted for cache-aside pattern)
    async fn get_task_template_from_db(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<HandlerMetadata> {
        debug!(
            namespace = namespace,
            name = name,
            version = version,
            "DATABASE: Looking up TaskTemplate"
        );

        // 1. Find the task namespace
        let task_namespace = TaskNamespace::find_by_name(&self.db_pool, namespace)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to query namespace: {e}")))?
            .ok_or_else(|| {
                debug!(namespace = namespace, "Namespace not found in database");
                TaskerError::ValidationError(format!("Namespace not found: {namespace}"))
            })?;

        debug!(
            namespace = namespace,
            namespace_uuid = %task_namespace.task_namespace_uuid,
            "Found namespace in database"
        );

        // 2. Find the named task in that namespace with specific version
        let named_task = NamedTask::find_by_name_version_namespace(
            &self.db_pool,
            name,
            version,
            task_namespace.task_namespace_uuid,
        )
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to query named task: {e}")))?
        .ok_or_else(|| {
            debug!(
                namespace = namespace,
                name = name,
                namespace_uuid = %task_namespace.task_namespace_uuid,
                "Named task not found in database"
            );
            TaskerError::ValidationError(format!("Task not found: {namespace}/{name}"))
        })?;

        debug!(
            namespace = namespace,
            name = name,
            version = named_task.version,
            task_uuid = %named_task.named_task_uuid,
            config_present = named_task.configuration.is_some(),
            config_size = named_task.configuration.as_ref().map(|c| serde_json::to_string(c).unwrap_or_default().len()).unwrap_or(0),
            "Found named task in database"
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
        // Debug configuration before validation
        debug!(
            "🔬 About to validate configuration for {}/{}/{}: is_none={}, is_empty_object={}",
            namespace,
            name,
            version,
            named_task.configuration.is_none(),
            named_task.configuration == Some(serde_json::json!({}))
        );

        // Validate configuration before creating HandlerMetadata
        if named_task.configuration.is_none()
            || named_task.configuration == Some(serde_json::json!({}))
        {
            error!(
                "Empty or invalid configuration detected for {}/{}/{}",
                namespace, name, version
            );
            error!(
                "DEBUG: configuration.is_none()={}, configuration={:?}",
                named_task.configuration.is_none(),
                named_task.configuration
            );
            if let Some(ref config) = named_task.configuration {
                error!(
                    "Configuration JSON: {}",
                    serde_json::to_string_pretty(config).unwrap_or_default()
                );
            }
            return Err(TaskerError::ValidationError(format!(
                "Template configuration is empty or invalid for {}/{}/{}. This usually indicates a database corruption or failed template registration.",
                namespace, name, version
            )));
        }

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

    // =========================================================================
    // TAS-156: Cache helper methods
    // =========================================================================

    /// Build the cache key for a task template
    fn cache_key(&self, namespace: &str, name: &str, version: &str) -> String {
        let prefix = self
            .cache_config
            .as_ref()
            .map(|c| c.key_prefix.as_str())
            .unwrap_or("tasker");
        format!(
            "{}:task_template:{}:{}:{}",
            prefix, namespace, name, version
        )
    }

    /// Get the template TTL from cache config
    fn template_ttl(&self) -> Duration {
        Duration::from_secs(
            self.cache_config
                .as_ref()
                .map(|c| u64::from(c.template_ttl_seconds))
                .unwrap_or(3600),
        )
    }

    /// Try to get a cached HandlerMetadata (returns None on miss or error)
    async fn try_cache_get(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Option<HandlerMetadata> {
        let provider = self.cache_provider.as_ref()?;
        if !provider.is_enabled() {
            return None;
        }

        let key = self.cache_key(namespace, name, version);
        match provider.get(&key).await {
            Ok(Some(cached)) => match serde_json::from_str::<HandlerMetadata>(&cached) {
                Ok(metadata) => {
                    debug!(
                        namespace = namespace,
                        name = name,
                        version = version,
                        "CACHE HIT: Resolved template from distributed cache"
                    );
                    Some(metadata)
                }
                Err(e) => {
                    warn!(
                        key = key.as_str(),
                        error = %e,
                        "Cache deserialization failed, falling through to DB"
                    );
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                warn!(
                    key = key.as_str(),
                    error = %e,
                    "Cache get failed, falling through to DB"
                );
                None
            }
        }
    }

    /// Try to set a HandlerMetadata in cache (best-effort, never fails)
    async fn try_cache_set(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        metadata: &HandlerMetadata,
    ) {
        let provider = match self.cache_provider.as_ref() {
            Some(p) if p.is_enabled() => p,
            _ => return,
        };

        let key = self.cache_key(namespace, name, version);
        let ttl = self.template_ttl();

        match serde_json::to_string(metadata) {
            Ok(serialized) => {
                if let Err(e) = provider.set(&key, &serialized, ttl).await {
                    warn!(
                        key = key.as_str(),
                        error = %e,
                        "Cache set failed (best-effort, DB is source of truth)"
                    );
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to serialize HandlerMetadata for cache"
                );
            }
        }
    }

    /// Invalidate a single template cache entry (best-effort)
    ///
    /// This is the primary invalidation method for workers to use when
    /// refreshing or removing specific templates from the distributed cache.
    pub async fn invalidate_cache(&self, namespace: &str, name: &str, version: &str) {
        let provider = match self.cache_provider.as_ref() {
            Some(p) if p.is_enabled() => p,
            _ => return,
        };

        let key = self.cache_key(namespace, name, version);
        if let Err(e) = provider.delete(&key).await {
            warn!(
                key = key.as_str(),
                error = %e,
                "Cache invalidation failed (best-effort)"
            );
        } else {
            debug!(key = key.as_str(), "Cache entry invalidated");
        }
    }

    /// Invalidate all template cache entries (pattern delete)
    pub async fn invalidate_all_templates(&self) {
        let provider = match self.cache_provider.as_ref() {
            Some(p) if p.is_enabled() => p,
            _ => return,
        };

        let prefix = self
            .cache_config
            .as_ref()
            .map(|c| c.key_prefix.as_str())
            .unwrap_or("tasker");
        let pattern = format!("{}:task_template:*", prefix);

        match provider.delete_pattern(&pattern).await {
            Ok(count) => {
                info!(
                    pattern = pattern.as_str(),
                    deleted = count,
                    "Invalidated all template cache entries"
                );
            }
            Err(e) => {
                warn!(
                    pattern = pattern.as_str(),
                    error = %e,
                    "Failed to invalidate all template cache entries (best-effort)"
                );
            }
        }
    }

    /// Invalidate all templates in a namespace
    pub async fn invalidate_namespace_templates(&self, namespace: &str) {
        let provider = match self.cache_provider.as_ref() {
            Some(p) if p.is_enabled() => p,
            _ => return,
        };

        let prefix = self
            .cache_config
            .as_ref()
            .map(|c| c.key_prefix.as_str())
            .unwrap_or("tasker");
        let pattern = format!("{}:task_template:{}:*", prefix, namespace);

        match provider.delete_pattern(&pattern).await {
            Ok(count) => {
                info!(
                    namespace = namespace,
                    deleted = count,
                    "Invalidated namespace template cache entries"
                );
            }
            Err(e) => {
                warn!(
                    namespace = namespace,
                    error = %e,
                    "Failed to invalidate namespace cache entries (best-effort)"
                );
            }
        }
    }

    /// Check if distributed cache is available and enabled
    pub fn cache_enabled(&self) -> bool {
        self.cache_provider
            .as_ref()
            .map(|p| p.is_enabled())
            .unwrap_or(false)
    }

    /// Get a reference to the cache provider (for health checks, etc.)
    pub fn cache_provider(&self) -> Option<&Arc<CacheProvider>> {
        self.cache_provider.as_ref()
    }
}

// Note: Default implementation removed - TaskHandlerRegistry now requires database pool

impl Clone for TaskHandlerRegistry {
    fn clone(&self) -> Self {
        Self {
            db_pool: self.db_pool.clone(),
            event_publisher: self.event_publisher.clone(),
            search_paths: self.search_paths.clone(),
            cache_provider: self.cache_provider.clone(),
            cache_config: self.cache_config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a registry with a lazy (non-connecting) pool for unit tests
    fn test_registry_no_cache() -> TaskHandlerRegistry {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        TaskHandlerRegistry::new(pool)
    }

    /// Create a registry with NoOp cache for unit tests
    fn test_registry_with_noop_cache(config: Option<CacheConfig>) -> TaskHandlerRegistry {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let provider = Arc::new(CacheProvider::noop());
        let cache_config = config.unwrap_or_default();
        TaskHandlerRegistry::with_cache(pool, provider, cache_config)
    }

    #[tokio::test]
    async fn test_cache_key_default_prefix() {
        let registry = test_registry_no_cache();
        let key = registry.cache_key("payments", "process_order", "1.0.0");
        assert_eq!(key, "tasker:task_template:payments:process_order:1.0.0");
    }

    #[tokio::test]
    async fn test_cache_key_custom_prefix() {
        let config = CacheConfig {
            key_prefix: "myapp".to_string(),
            ..CacheConfig::default()
        };
        let registry = test_registry_with_noop_cache(Some(config));
        let key = registry.cache_key("billing", "invoice", "2.0.0");
        assert_eq!(key, "myapp:task_template:billing:invoice:2.0.0");
    }

    #[tokio::test]
    async fn test_template_ttl_default() {
        let registry = test_registry_no_cache();
        assert_eq!(registry.template_ttl(), Duration::from_secs(3600));
    }

    #[tokio::test]
    async fn test_template_ttl_from_config() {
        let config = CacheConfig {
            template_ttl_seconds: 7200,
            ..CacheConfig::default()
        };
        let registry = test_registry_with_noop_cache(Some(config));
        assert_eq!(registry.template_ttl(), Duration::from_secs(7200));
    }

    #[tokio::test]
    async fn test_cache_enabled_no_provider() {
        let registry = test_registry_no_cache();
        assert!(!registry.cache_enabled());
    }

    #[tokio::test]
    async fn test_cache_enabled_noop_provider() {
        let registry = test_registry_with_noop_cache(None);
        // NoOp provider reports as not enabled
        assert!(!registry.cache_enabled());
    }

    #[tokio::test]
    async fn test_cache_provider_accessor_none() {
        let registry = test_registry_no_cache();
        assert!(registry.cache_provider().is_none());
    }

    #[tokio::test]
    async fn test_cache_provider_accessor_present() {
        let registry = test_registry_with_noop_cache(None);
        assert!(registry.cache_provider().is_some());
    }

    #[tokio::test]
    async fn test_try_cache_get_no_provider_returns_none() {
        let registry = test_registry_no_cache();
        let result = registry.try_cache_get("ns", "name", "1.0").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_cache_get_noop_returns_none() {
        let registry = test_registry_with_noop_cache(None);
        let result = registry.try_cache_get("ns", "name", "1.0").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_cache_set_no_provider_succeeds() {
        let registry = test_registry_no_cache();
        let metadata = HandlerMetadata {
            namespace: "payments".to_string(),
            name: "process_order".to_string(),
            version: "1.0.0".to_string(),
            handler_class: "TestHandler".to_string(),
            config_schema: None,
            default_dependent_system: None,
            registered_at: Utc::now(),
        };
        // Should not panic
        registry.try_cache_set("ns", "name", "1.0", &metadata).await;
    }

    #[tokio::test]
    async fn test_try_cache_set_noop_succeeds() {
        let registry = test_registry_with_noop_cache(None);
        let metadata = HandlerMetadata {
            namespace: "billing".to_string(),
            name: "invoice".to_string(),
            version: "2.0.0".to_string(),
            handler_class: "MyHandler".to_string(),
            config_schema: Some(serde_json::json!({"type": "object"})),
            default_dependent_system: Some("billing_system".to_string()),
            registered_at: Utc::now(),
        };
        // Should not panic (NoOp silently discards)
        registry.try_cache_set("ns", "name", "1.0", &metadata).await;
    }

    #[tokio::test]
    async fn test_invalidate_cache_no_provider() {
        let registry = test_registry_no_cache();
        // Should not panic when no cache provider
        registry.invalidate_cache("ns", "name", "1.0").await;
    }

    #[tokio::test]
    async fn test_invalidate_cache_noop_provider() {
        let registry = test_registry_with_noop_cache(None);
        // NoOp is not enabled, so invalidate should return early
        registry.invalidate_cache("ns", "name", "1.0").await;
    }

    #[tokio::test]
    async fn test_invalidate_all_templates_no_provider() {
        let registry = test_registry_no_cache();
        registry.invalidate_all_templates().await;
    }

    #[tokio::test]
    async fn test_invalidate_all_templates_noop() {
        let registry = test_registry_with_noop_cache(None);
        registry.invalidate_all_templates().await;
    }

    #[tokio::test]
    async fn test_invalidate_namespace_templates_no_provider() {
        let registry = test_registry_no_cache();
        registry.invalidate_namespace_templates("payments").await;
    }

    #[tokio::test]
    async fn test_invalidate_namespace_templates_noop() {
        let registry = test_registry_with_noop_cache(None);
        registry.invalidate_namespace_templates("payments").await;
    }
}
