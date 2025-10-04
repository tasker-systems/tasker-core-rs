//! Worker Web Application State
//!
//! Contains shared state for the worker web API including database connections,
//! configuration, and metrics tracking.

use crate::worker::task_template_manager::TaskTemplateManager;
use serde::Serialize;
use sqlx::PgPool;
use std::{sync::Arc, time::Instant};
use tasker_shared::{
    config::TaskerConfig, errors::TaskerResult, messaging::clients::UnifiedMessageClient,
    registry::TaskHandlerRegistry, types::base::CacheStats,
};
use tracing::info;

/// Configuration for the worker web API
#[derive(Debug, Clone, Serialize)]
pub struct WorkerWebConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub request_timeout_ms: u64,
    pub authentication_enabled: bool,
    pub cors_enabled: bool,
    pub metrics_enabled: bool,
    pub health_check_interval_seconds: u64,
}

impl Default for WorkerWebConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: "0.0.0.0:8081".to_string(),
            request_timeout_ms: 30000,
            authentication_enabled: false,
            cors_enabled: true,
            metrics_enabled: true,
            health_check_interval_seconds: 30,
        }
    }
}

impl WorkerWebConfig {
    /// Create WorkerWebConfig from TaskerConfig with proper configuration loading
    ///
    /// TAS-43: This method replaces ::default() usage with configuration-driven setup
    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        // Handle optional web configuration with sensible defaults
        let worker_config = config.worker.clone();
        match worker_config {
            Some(worker_config) => {
                let web_config = worker_config.web;
                Self {
                    enabled: web_config.enabled,
                    bind_address: web_config.bind_address.clone(),
                    request_timeout_ms: web_config.request_timeout_ms,
                    authentication_enabled: web_config.auth.enabled,
                    cors_enabled: web_config.cors.enabled,
                    metrics_enabled: true, // TODO: Load from web configuration when available
                    health_check_interval_seconds: 30, // TODO: Load from web configuration when available
                }
            }
            None => {
                panic!("Worker web configuration is missing")
            }
        }
    }
}

/// Shared state for the worker web application
#[derive(Clone)]
pub struct WorkerWebState {
    /// Worker core reference for health checks and status
    /// Wrapped in Arc<Mutex<>> for thread-safe shared access
    pub worker_core: Arc<tokio::sync::Mutex<crate::worker::core::WorkerCore>>,

    /// Database connection pool
    pub database_pool: Arc<PgPool>,

    /// Unified message client for queue metrics and monitoring
    pub message_client: Arc<UnifiedMessageClient>,

    /// Task template manager for template caching and validation
    pub task_template_manager: Arc<TaskTemplateManager>,

    /// Web API configuration
    pub config: WorkerWebConfig,

    /// Application start time for uptime calculations
    pub start_time: Instant,

    /// Worker system configuration
    pub system_config: TaskerConfig,

    /// Cached worker ID (to avoid locking mutex on every status check)
    worker_id: String,
}

impl WorkerWebState {
    /// Create new worker web state with all required components
    pub async fn new(
        config: WorkerWebConfig,
        worker_core: Arc<tokio::sync::Mutex<crate::worker::core::WorkerCore>>,
        database_pool: Arc<PgPool>,
        system_config: TaskerConfig,
    ) -> TaskerResult<Self> {
        info!(
            enabled = config.enabled,
            bind_address = %config.bind_address,
            "Initializing worker web state"
        );

        // Cache the worker_id by locking once during initialization
        let worker_id = {
            let core = worker_core.lock().await;
            format!("worker-{}", core.core_id())
        };

        info!(worker_id = %worker_id, "Worker ID cached for web state");

        // Create unified message client using the shared database pool
        let message_client =
            Arc::new(UnifiedMessageClient::new_pgmq_with_pool((*database_pool).clone()).await);

        // Create task handler registry and task template manager
        let task_handler_registry = Arc::new(TaskHandlerRegistry::new((*database_pool).clone()));
        let task_template_manager = Arc::new(TaskTemplateManager::new(task_handler_registry));

        Ok(Self {
            worker_core,
            database_pool,
            message_client,
            task_template_manager,
            config,
            start_time: Instant::now(),
            system_config,
            worker_id,
        })
    }

    /// Get the uptime in seconds since the worker started
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Check if metrics collection is enabled
    pub fn metrics_enabled(&self) -> bool {
        self.config.metrics_enabled
    }

    /// Check if authentication is required
    pub fn authentication_enabled(&self) -> bool {
        self.config.authentication_enabled
    }

    /// Get worker identifier for status reporting
    ///
    /// Returns the cached worker ID that was set during initialization.
    /// This avoids needing to lock the mutex on every status check.
    pub fn worker_id(&self) -> String {
        self.worker_id.clone()
    }

    /// Get worker type classification
    pub fn worker_type(&self) -> String {
        "command_processor".to_string()
    }

    /// Get supported namespaces for this worker
    pub async fn supported_namespaces(&self) -> Vec<String> {
        self.task_template_manager.supported_namespaces().await
    }

    /// Get queue name for a namespace
    pub fn queue_name_for_namespace(&self, namespace: &str) -> String {
        format!("{}_queue", namespace)
    }

    /// Check if a namespace is supported by this worker
    pub async fn is_namespace_supported(&self, namespace: &str) -> bool {
        self.task_template_manager
            .is_namespace_supported(namespace)
            .await
    }

    /// Get task template manager reference for template operations
    pub fn task_template_manager(&self) -> &Arc<TaskTemplateManager> {
        &self.task_template_manager
    }

    /// Get task template manager statistics
    pub async fn template_cache_stats(&self) -> CacheStats {
        self.task_template_manager.cache_stats().await
    }

    /// Perform cache maintenance on task templates
    pub async fn maintain_template_cache(&self) {
        self.task_template_manager.maintain_cache().await;
    }
}
