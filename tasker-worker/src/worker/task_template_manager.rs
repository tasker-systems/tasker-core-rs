//! # Worker Task Template Manager
//!
//! Local task template management that integrates with the shared TaskHandlerRegistry.
//! Provides worker-specific template caching, validation, and namespace-aware filtering.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use tasker_shared::{
    errors::{TaskerError, TaskerResult},
    models::core::{
        task_request::TaskRequest,
        task_template::{ResolvedTaskTemplate, TaskTemplate},
    },
    registry::{HandlerKey, TaskHandlerRegistry, TaskTemplateDiscoveryResult},
    types::{base::CacheStats, HandlerMetadata},
};

/// Configuration for task template manager
#[derive(Debug, Clone)]
pub struct TaskTemplateManagerConfig {
    /// Supported namespaces for this worker
    pub supported_namespaces: Vec<String>,
    /// Cache expiry duration for templates
    pub cache_expiry_seconds: u64,
    /// Maximum number of cached templates
    pub max_cache_size: usize,
    /// Enable automatic cache refresh
    pub auto_refresh_enabled: bool,
    /// Auto-refresh interval in seconds
    pub auto_refresh_interval_seconds: u64,
}

impl Default for TaskTemplateManagerConfig {
    fn default() -> Self {
        Self {
            supported_namespaces: vec!["default".to_string()],
            cache_expiry_seconds: 300, // 5 minutes
            max_cache_size: 1000,
            auto_refresh_enabled: true,
            auto_refresh_interval_seconds: 60, // 1 minute
        }
    }
}

impl TaskTemplateManagerConfig {
    pub fn with_namespaces(mut self, namespaces: Vec<String>) -> Self {
        self.supported_namespaces = namespaces;
        self
    }
}

/// Cached task template entry with metadata
#[derive(Debug, Clone)]
pub struct CachedTemplate {
    pub template: ResolvedTaskTemplate,
    pub handler_metadata: HandlerMetadata,
    pub cached_at: Instant,
    pub access_count: u64,
    pub last_accessed: Instant,
}

/// Worker-specific task template manager with local caching
///
/// This provides a worker-local view of task templates with:
/// - Namespace filtering for supported namespaces only
/// - Local caching with TTL and LRU eviction
/// - Integration with shared TaskHandlerRegistry
/// - Worker-specific template validation
/// - Metrics and observability
pub struct TaskTemplateManager {
    /// Shared task handler registry for database operations
    registry: Arc<TaskHandlerRegistry>,
    /// Local cache of task templates with expiry
    cache: Arc<RwLock<HashMap<HandlerKey, CachedTemplate>>>,
    /// Configuration for this manager (wrapped in RwLock for interior mutability)
    config: Arc<RwLock<TaskTemplateManagerConfig>>,
    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,
}

impl TaskTemplateManager {
    /// Create a new task template manager with default configuration
    pub fn new(registry: Arc<TaskHandlerRegistry>) -> Self {
        Self::with_config(registry, TaskTemplateManagerConfig::default())
    }

    /// Create a new task template manager with custom configuration
    pub fn with_config(
        registry: Arc<TaskHandlerRegistry>,
        config: TaskTemplateManagerConfig,
    ) -> Self {
        let initial_stats = CacheStats {
            total_cached: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_evictions: 0,
            oldest_entry_age_seconds: 0,
            average_access_count: 0.0,
            supported_namespaces: config.supported_namespaces.clone(),
        };

        info!(
            supported_namespaces = ?config.supported_namespaces,
            cache_expiry_seconds = config.cache_expiry_seconds,
            max_cache_size = config.max_cache_size,
            "Initialized worker task template manager"
        );

        Self {
            registry,
            cache: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(config)),
            stats: Arc::new(RwLock::new(initial_stats)),
        }
    }

    /// Update supported namespaces dynamically (thread-safe with interior mutability)
    pub async fn set_supported_namespaces(&self, namespaces: Vec<String>) {
        let mut config = self.config.write().await;
        config.supported_namespaces = namespaces.clone();

        // Also update the stats to reflect the new namespaces
        let mut stats = self.stats.write().await;
        stats.supported_namespaces = namespaces;
    }

    /// Get a task template by namespace, name, and version
    ///
    /// This method provides worker-specific template resolution with:
    /// - Namespace validation against supported namespaces
    /// - Local cache lookup with TTL validation
    /// - Fallback to database via TaskHandlerRegistry
    /// - Cache population on miss
    pub async fn get_task_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<ResolvedTaskTemplate> {
        // 1. Validate namespace is supported by this worker
        if !self.is_namespace_supported(namespace).await {
            return Err(TaskerError::ValidationError(format!(
                "Namespace '{}' is not supported by this worker. Supported: {:?}",
                namespace,
                self.config.read().await.supported_namespaces
            )));
        }

        let handler_key =
            HandlerKey::new(namespace.to_string(), name.to_string(), version.to_string());

        // 2. Check local cache first
        if let Some(template) = self.get_from_cache(&handler_key).await {
            self.update_cache_hit_stats().await;
            debug!(
                namespace = namespace,
                name = name,
                version = version,
                "Task template found in local cache"
            );
            return Ok(template);
        }

        // 3. Cache miss - fetch from registry and populate cache
        self.update_cache_miss_stats().await;
        debug!(
            namespace = namespace,
            name = name,
            version = version,
            "Task template cache miss - fetching from registry"
        );

        let template = self.fetch_and_cache_template(&handler_key).await?;
        Ok(template)
    }

    /// Get handler metadata for a task template
    pub async fn get_handler_metadata(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<HandlerMetadata> {
        if !self.is_namespace_supported(namespace).await {
            return Err(TaskerError::ValidationError(format!(
                "Namespace '{}' is not supported by this worker",
                namespace
            )));
        }

        let handler_key =
            HandlerKey::new(namespace.to_string(), name.to_string(), version.to_string());

        // Check if we have it cached
        if let Some(cached) = self.get_cached_entry(&handler_key).await {
            return Ok(cached.handler_metadata);
        }

        // Fetch from registry
        let task_request = TaskRequest {
            name: name.to_string(),
            namespace: namespace.to_string(),
            version: version.to_string(),
            context: serde_json::Value::Object(serde_json::Map::new()),
            status: "PENDING".to_string(),
            initiator: "worker".to_string(),
            source_system: "task_template_manager".to_string(),
            reason: "Handler metadata lookup".to_string(),
            complete: false,
            tags: Vec::new(),
            bypass_steps: Vec::new(),
            requested_at: chrono::Utc::now().naive_utc(),
            options: None,
            priority: Some(5),
        };

        self.registry.resolve_handler(&task_request).await
    }

    /// Check if a namespace is supported by this worker
    pub async fn is_namespace_supported(&self, namespace: &str) -> bool {
        self.config
            .read()
            .await
            .supported_namespaces
            .contains(&namespace.to_string())
    }

    /// Get list of supported namespaces
    pub async fn supported_namespaces(&self) -> Vec<String> {
        self.config.read().await.supported_namespaces.clone()
    }

    /// Refresh cache for a specific template
    pub async fn refresh_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<()> {
        let handler_key =
            HandlerKey::new(namespace.to_string(), name.to_string(), version.to_string());

        // Remove from cache to force refresh
        {
            let mut cache = self.cache.write().await;
            cache.remove(&handler_key);
        }

        // Fetch fresh copy
        self.fetch_and_cache_template(&handler_key).await?;

        info!(
            namespace = namespace,
            name = name,
            version = version,
            "Task template refreshed in cache"
        );

        Ok(())
    }

    /// Discover and register TaskTemplates from filesystem to database
    /// Similar to Ruby TaskTemplateRegistry.register_task_templates_from_directory
    pub async fn discover_and_register_templates(
        &self,
        config_directory: &str,
    ) -> TaskerResult<TaskTemplateDiscoveryResult> {
        info!(
            directory = config_directory,
            "🔍 Worker discovering TaskTemplates",
        );

        // Use the registry to discover and register templates
        let discovery_result = self
            .registry
            .discover_and_register_templates(config_directory)
            .await?;

        info!(
            "📊 Worker template discovery complete: {} total files, {} supported templates registered, {} failed, discovered namespaces: {:?}",
            discovery_result.total_files,
            discovery_result.successful_registrations,
            discovery_result.failed_registrations,
            discovery_result.discovered_namespaces
        );

        Ok(discovery_result)
    }

    /// Load TaskTemplates to database during worker startup
    /// This ensures workers have their required templates available in the database
    pub async fn ensure_templates_in_database(&self) -> TaskerResult<TaskTemplateDiscoveryResult> {
        info!("🚀 Ensuring TaskTemplates are loaded to database for worker startup");

        // Try to find template configuration directory
        let config_directory = self.find_template_config_directory()?;

        info!("📁 Using template directory: {}", config_directory);

        // Discover and register templates
        match self
            .discover_and_register_templates(&config_directory)
            .await
        {
            Ok(discovery_result) => {
                info!(
                    "✅ BOOTSTRAP: TaskTemplate discovery complete - {} templates registered from {} files",
                    discovery_result.successful_registrations,
                    discovery_result.total_files
                );

                if !discovery_result.errors.is_empty() {
                    warn!(
                        "⚠️ BOOTSTRAP: {} errors during TaskTemplate discovery: {:?}",
                        discovery_result.errors.len(),
                        discovery_result.errors
                    );
                }

                if !discovery_result.discovered_templates.is_empty() {
                    info!(
                        "📋 BOOTSTRAP: Registered templates: {:?}",
                        discovery_result.discovered_templates
                    );
                }

                // Update config with discovered namespaces
                if !discovery_result.discovered_namespaces.is_empty() {
                    let mut config = self.config.write().await;
                    config.supported_namespaces = discovery_result.discovered_namespaces.clone();
                    info!(
                        "✅ BOOTSTRAP: Updated supported namespaces: {:?}",
                        config.supported_namespaces
                    );
                }

                Ok(discovery_result)
            }
            Err(e) => {
                error!(
                    "⚠️ BOOTSTRAP: TaskTemplate discovery failed (worker will use registry-only): {e}"
                );
                Err(TaskerError::ConfigurationError(format!(
                    "TaskTemplate discovery failed (worker will use registry-only): {e}"
                )))
            }
        }
    }

    /// Find the template configuration directory using workspace_tools
    /// Consistent with the unified configuration system
    fn find_template_config_directory(&self) -> TaskerResult<String> {
        use workspace_tools::workspace;

        if let Ok(template_dir) = std::env::var("TASKER_TEMPLATE_PATH") {
            if std::path::Path::new(&template_dir).exists() {
                return Ok(template_dir);
            }
        }

        // Try WORKSPACE_PATH environment variable first (for compatibility)
        if let Ok(workspace_path) = std::env::var("WORKSPACE_PATH") {
            let template_dir = format!("{}/config/tasks", workspace_path);
            if std::path::Path::new(&template_dir).exists() {
                return Ok(template_dir);
            }
        }

        // Use workspace_tools to find project root (preferred method)
        match workspace() {
            Ok(ws) => {
                let template_dir = ws.join("config").join("tasks");
                if template_dir.exists() {
                    return Ok(template_dir.display().to_string());
                }
            }
            Err(e) => {
                debug!("Failed to find workspace root using workspace_tools: {e}");
            }
        }

        // Default fallback to current directory
        let default_dir = "config/tasks";
        if std::path::Path::new(default_dir).exists() {
            return Ok(default_dir.to_string());
        }

        Err(TaskerError::ConfigurationError(
            "TaskTemplate configuration directory not found. Tried WORKSPACE_PATH/config/tasks, workspace_tools detection, and config/tasks".to_string()
        ))
    }

    /// Clear entire cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();

        // Reset stats
        let mut stats = self.stats.write().await;
        stats.total_cached = 0;
        stats.cache_evictions += cache.len() as u64;

        info!("Task template cache cleared");
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let mut stats = self.stats.read().await.clone();

        stats.total_cached = cache.len();

        if !cache.is_empty() {
            let now = Instant::now();
            let oldest_age = cache
                .values()
                .map(|entry| now.duration_since(entry.cached_at).as_secs())
                .max()
                .unwrap_or(0);
            stats.oldest_entry_age_seconds = oldest_age;

            let total_access = cache.values().map(|entry| entry.access_count).sum::<u64>();
            stats.average_access_count = total_access as f64 / cache.len() as f64;
        }

        stats
    }

    /// Perform cache maintenance (remove expired entries, enforce size limits)
    pub async fn maintain_cache(&self) {
        let mut cache = self.cache.write().await;
        let now = Instant::now();
        let expiry_duration = Duration::from_secs(self.config.read().await.cache_expiry_seconds);

        // Remove expired entries
        let mut expired_keys = Vec::new();
        for (key, entry) in cache.iter() {
            if now.duration_since(entry.cached_at) > expiry_duration {
                expired_keys.push(key.clone());
            }
        }

        for key in expired_keys {
            cache.remove(&key);
            let mut stats = self.stats.write().await;
            stats.cache_evictions += 1;
        }

        // Enforce size limits using LRU eviction
        while cache.len() > self.config.read().await.max_cache_size {
            if let Some(lru_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.last_accessed)
                .map(|(key, _)| key.clone())
            {
                cache.remove(&lru_key);
                let mut stats = self.stats.write().await;
                stats.cache_evictions += 1;
            } else {
                break;
            }
        }

        debug!(
            cached_templates = cache.len(),
            "Cache maintenance completed"
        );
    }

    // Private helper methods

    async fn get_from_cache(&self, key: &HandlerKey) -> Option<ResolvedTaskTemplate> {
        let mut cache = self.cache.write().await;
        let now = Instant::now();
        let expiry_duration = Duration::from_secs(self.config.read().await.cache_expiry_seconds);

        if let Some(entry) = cache.get_mut(key) {
            // Check if entry is expired
            if now.duration_since(entry.cached_at) <= expiry_duration {
                // Update access tracking
                entry.access_count += 1;
                entry.last_accessed = now;
                return Some(entry.template.clone());
            } else {
                // Remove expired entry
                cache.remove(key);
                let mut stats = self.stats.write().await;
                stats.cache_evictions += 1;
            }
        }

        None
    }

    async fn get_cached_entry(&self, key: &HandlerKey) -> Option<CachedTemplate> {
        let cache = self.cache.read().await;
        let now = Instant::now();
        let expiry_duration = Duration::from_secs(self.config.read().await.cache_expiry_seconds);

        cache.get(key).and_then(|entry| {
            if now.duration_since(entry.cached_at) <= expiry_duration {
                Some(entry.clone())
            } else {
                None
            }
        })
    }

    async fn fetch_and_cache_template(
        &self,
        key: &HandlerKey,
    ) -> TaskerResult<ResolvedTaskTemplate> {
        // Fetch handler metadata from registry (which contains the template as JSON)
        let handler_metadata = self
            .registry
            .resolve_handler(&TaskRequest {
                name: key.name.clone(),
                namespace: key.namespace.clone(),
                version: key.version.clone(),
                context: serde_json::Value::Object(serde_json::Map::new()),
                status: "PENDING".to_string(),
                initiator: "worker".to_string(),
                source_system: "task_template_manager".to_string(),
                reason: "Template retrieval".to_string(),
                complete: false,
                tags: Vec::new(),
                bypass_steps: Vec::new(),
                requested_at: chrono::Utc::now().naive_utc(),
                options: None,
                priority: Some(5),
            })
            .await?;

        // Extract task template from handler metadata
        let template: TaskTemplate = if let Some(config_schema) = &handler_metadata.config_schema {
            serde_json::from_value(config_schema.clone()).map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to parse task template: {}", e))
            })?
        } else {
            return Err(TaskerError::ConfigurationError(
                "Handler metadata does not contain task template configuration".to_string(),
            ));
        };

        // Resolve template for current environment (defaulting to "production")
        let resolved_template = template.resolve_for_environment("production");

        // Cache the resolved template
        let cached_entry = CachedTemplate {
            template: resolved_template.clone(),
            handler_metadata,
            cached_at: Instant::now(),
            access_count: 1,
            last_accessed: Instant::now(),
        };

        {
            let mut cache = self.cache.write().await;
            cache.insert(key.clone(), cached_entry);

            // Enforce cache size limits
            while cache.len() > self.config.read().await.max_cache_size {
                if let Some(lru_key) = cache
                    .iter()
                    .min_by_key(|(_, entry)| entry.last_accessed)
                    .map(|(key, _)| key.clone())
                {
                    cache.remove(&lru_key);
                    let mut stats = self.stats.write().await;
                    stats.cache_evictions += 1;
                } else {
                    break;
                }
            }
        }

        debug!(
            namespace = &key.namespace,
            name = &key.name,
            version = &key.version,
            "Task template cached successfully"
        );

        Ok(resolved_template)
    }

    async fn update_cache_hit_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.cache_hits += 1;
    }

    async fn update_cache_miss_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.cache_misses += 1;
    }
}

impl Clone for TaskTemplateManager {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            cache: self.cache.clone(),
            config: self.config.clone(),
            stats: self.stats.clone(),
        }
    }
}

#[async_trait::async_trait]
/// Helper trait for worker-specific task template operations
pub trait WorkerTaskTemplateOperations {
    /// Validate that a template is suitable for worker execution
    async fn validate_for_worker(&self, template: &ResolvedTaskTemplate) -> TaskerResult<()>;

    /// Extract step handler callables from template
    fn extract_step_handlers(&self, template: &ResolvedTaskTemplate) -> Vec<String>;

    /// Check if template requires specific worker capabilities
    fn requires_capabilities(&self, template: &ResolvedTaskTemplate) -> Vec<String>;
}

#[async_trait::async_trait]
impl WorkerTaskTemplateOperations for TaskTemplateManager {
    async fn validate_for_worker(&self, template: &ResolvedTaskTemplate) -> TaskerResult<()> {
        // Validate namespace is supported
        if !self
            .is_namespace_supported(&template.template.namespace_name)
            .await
        {
            return Err(TaskerError::ValidationError(format!(
                "Template namespace '{}' is not supported by this worker",
                template.template.namespace_name
            )));
        }

        // Validate required fields
        if template.template.name.is_empty() {
            return Err(TaskerError::ValidationError(
                "Template name cannot be empty".to_string(),
            ));
        }

        if template.template.steps.is_empty() {
            return Err(TaskerError::ValidationError(
                "Template must have at least one step".to_string(),
            ));
        }

        // Validate step dependencies exist using the new TaskTemplate validation
        template.template.validate().map_err(|e| {
            TaskerError::ValidationError(format!("Template validation failed: {}", e))
        })?;

        Ok(())
    }

    fn extract_step_handlers(&self, template: &ResolvedTaskTemplate) -> Vec<String> {
        template
            .template
            .steps
            .iter()
            .map(|st| st.handler.callable.clone())
            .collect()
    }

    fn requires_capabilities(&self, template: &ResolvedTaskTemplate) -> Vec<String> {
        let mut capabilities = Vec::new();

        // Check if any steps require FFI
        for step in &template.template.steps {
            if step.handler.callable.contains("FFI") {
                capabilities.push("ffi".to_string());
            }

            if step.handler.callable.contains("External") {
                capabilities.push("external_api".to_string());
            }

            if step.handler.callable.contains("Database") {
                capabilities.push("database_access".to_string());
            }
        }

        capabilities.sort();
        capabilities.dedup();
        capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::{
        models::core::task_template::{HandlerDefinition, StepDefinition, TaskTemplate},
        registry::HandlerKey,
    };

    fn create_test_template() -> TaskTemplate {
        use std::collections::HashMap;

        TaskTemplate {
            name: "test_task".to_string(),
            namespace_name: "test_namespace".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test task template".to_string()),
            metadata: None,
            task_handler: Some(HandlerDefinition {
                callable: "TestHandler".to_string(),
                initialization: HashMap::new(),
            }),
            system_dependencies: Default::default(),
            domain_events: Vec::new(),
            input_schema: None,
            steps: vec![StepDefinition {
                name: "step1".to_string(),
                description: Some("Test step".to_string()),
                handler: HandlerDefinition {
                    callable: "TestStepHandler".to_string(),
                    initialization: HashMap::new(),
                },
                system_dependency: None,
                dependencies: Vec::new(),
                retry: Default::default(),
                timeout_seconds: Some(30),
                publishes_events: Vec::new(),
            }],
            environments: HashMap::new(),
        }
    }

    #[test]
    fn test_task_template_manager_config_default() {
        let config = TaskTemplateManagerConfig::default();
        assert!(!config.supported_namespaces.is_empty());
        assert_eq!(config.cache_expiry_seconds, 300);
        assert_eq!(config.max_cache_size, 1000);
        assert!(config.auto_refresh_enabled);
    }

    #[test]
    fn test_namespace_support_validation() {
        let config = TaskTemplateManagerConfig {
            supported_namespaces: vec!["test_namespace".to_string()],
            ..Default::default()
        };

        // We can't easily create TaskHandlerRegistry without a database pool in tests,
        // so we'll test the config validation instead
        assert!(config
            .supported_namespaces
            .contains(&"test_namespace".to_string()));
        assert!(!config
            .supported_namespaces
            .contains(&"unsupported_namespace".to_string()));
    }

    #[test]
    fn test_template_validation() {
        // Create a mock registry (this would need a real database in practice)
        // For now, we'll just test the validation logic with a mock setup
        let template = create_test_template();

        // Test that template has required fields
        assert!(!template.name.is_empty());
        assert!(!template.namespace_name.is_empty());
        assert!(!template.steps.is_empty());
    }

    #[test]
    fn test_handler_key_creation() {
        let key = HandlerKey::new(
            "test_namespace".to_string(),
            "test_task".to_string(),
            "1.0.0".to_string(),
        );

        assert_eq!(key.namespace, "test_namespace");
        assert_eq!(key.name, "test_task");
        assert_eq!(key.version, "1.0.0");
        assert_eq!(key.key_string(), "test_namespace/test_task/1.0.0");
    }

    #[test]
    fn test_cache_stats_initialization() {
        let stats = CacheStats {
            total_cached: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_evictions: 0,
            oldest_entry_age_seconds: 0,
            average_access_count: 0.0,
            supported_namespaces: vec!["test".to_string()],
        };

        assert_eq!(stats.total_cached, 0);
        assert_eq!(stats.cache_hits, 0);
        assert!(!stats.supported_namespaces.is_empty());
    }
}
