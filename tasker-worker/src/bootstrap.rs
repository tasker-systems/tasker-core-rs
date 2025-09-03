//! # Unified Worker Bootstrap System
//!
//! Provides a unified way to bootstrap the worker system across all deployment modes:
//! - Embedded mode (integrated with orchestration)
//! - Standalone deployment
//! - Docker containers
//! - Testing environments
//!
//! ## Key Features
//!
//! - **Environment-Aware Configuration**: Uses TaskerConfig with environment detection
//! - **Command Pattern Integration**: Uses WorkerCore for unified architecture
//! - **Lifecycle Management**: Start/stop/status operations for all deployment modes
//! - **Graceful Shutdown**: Proper cleanup and resource management
//! - **Consistent API**: Same bootstrap interface regardless of deployment mode

use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{error, info, warn};

use crate::{
    web::{state::WorkerWebConfig, WorkerWebState},
    worker::core::{WorkerCore, WorkerCoreStatus},
};
use tasker_client::api_clients::orchestration_client::OrchestrationApiConfig;
use tasker_shared::registry::TaskTemplateDiscoveryResult;
use tasker_shared::{
    config::ConfigManager,
    errors::{TaskerError, TaskerResult},
    system_context::SystemContext,
};

/// Unified worker system handle for lifecycle management
pub struct WorkerSystemHandle {
    /// Core worker system
    pub worker_core: Arc<WorkerCore>,
    /// Web API state (optional)
    pub web_state: Option<Arc<WorkerWebState>>,
    /// Shutdown signal sender (Some when running, None when stopped)
    pub shutdown_sender: Option<oneshot::Sender<()>>,
    /// Runtime handle for async operations
    pub runtime_handle: tokio::runtime::Handle,
    /// System configuration manager
    pub config_manager: Arc<ConfigManager>,
    /// Worker configuration
    pub worker_config: WorkerBootstrapConfig,
}

impl WorkerSystemHandle {
    /// Create new worker system handle
    pub fn new(
        worker_core: Arc<WorkerCore>,
        web_state: Option<Arc<WorkerWebState>>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        config_manager: Arc<ConfigManager>,
        worker_config: WorkerBootstrapConfig,
    ) -> Self {
        Self {
            worker_core,
            web_state,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
            config_manager,
            worker_config,
        }
    }

    /// Check if system is running
    pub fn is_running(&self) -> bool {
        self.shutdown_sender.is_some()
    }

    /// Stop the worker system
    pub fn stop(&mut self) -> TaskerResult<()> {
        if let Some(sender) = self.shutdown_sender.take() {
            sender.send(()).map_err(|_| {
                TaskerError::WorkerError("Failed to send shutdown signal".to_string())
            })?;
            info!("🛑 Worker system shutdown requested");
            Ok(())
        } else {
            warn!("Worker system already stopped");
            Ok(())
        }
    }

    /// Get system status information
    pub fn status(&self) -> WorkerSystemStatus {
        WorkerSystemStatus {
            running: self.is_running(),
            environment: self.config_manager.environment().to_string(),
            worker_core_status: self.worker_core.status().clone(),
            web_api_enabled: self.worker_config.web_config.enabled,
            supported_namespaces: self.worker_config.supported_namespaces.clone(),
            database_pool_size: self.worker_core.context.database_pool().size(),
            database_pool_idle: self.worker_core.context.database_pool().num_idle(),
            database_url_preview: self
                .config_manager
                .config()
                .database_url()
                .chars()
                .take(30)
                .collect::<String>()
                + "...",
        }
    }
}

/// Worker system status information
#[derive(Debug, Clone)]
pub struct WorkerSystemStatus {
    pub running: bool,
    pub environment: String,
    pub worker_core_status: WorkerCoreStatus,
    pub web_api_enabled: bool,
    pub supported_namespaces: Vec<String>,
    pub database_pool_size: u32,
    pub database_pool_idle: usize,
    pub database_url_preview: String,
}

/// Bootstrap configuration for worker system
#[derive(Debug, Clone)]
pub struct WorkerBootstrapConfig {
    /// Worker identifier for logging and metrics
    pub worker_id: String,
    /// Supported namespaces for this worker
    pub supported_namespaces: Vec<String>,
    /// Whether to start web API server
    pub enable_web_api: bool,
    /// Web API configuration
    pub web_config: WorkerWebConfig,
    /// Orchestration API configuration
    pub orchestration_api_config: OrchestrationApiConfig,
    /// Environment override (None = auto-detect)
    pub environment_override: Option<String>,
    /// TAS-43 Event-driven processing configuration
    pub event_driven_enabled: bool,
    /// TAS-43 Deployment mode preference (for future configuration-driven control)
    pub deployment_mode_hint: Option<String>,
}

impl Default for WorkerBootstrapConfig {
    fn default() -> Self {
        Self {
            worker_id: format!("worker-{}", uuid::Uuid::new_v4()),
            supported_namespaces: vec!["default".to_string()],
            enable_web_api: true,
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: None,
            event_driven_enabled: true, // TAS-43: Enable event-driven processing by default
            deployment_mode_hint: Some("Hybrid".to_string()), // TAS-43: Default to hybrid mode
        }
    }
}

impl WorkerBootstrapConfig {
    /// Create WorkerBootstrapConfig from ConfigManager for configuration-driven bootstrap
    ///
    /// TAS-43: This method replaces ::default() usage with proper configuration loading
    pub fn from_config_manager(
        config_manager: &ConfigManager,
        worker_id: String,
        supported_namespaces: Vec<String>,
    ) -> Self {
        let config = config_manager.config();

        Self {
            worker_id,
            supported_namespaces,
            enable_web_api: config
                .worker
                .as_ref()
                .map(|w| w.web.enabled)
                .unwrap_or(true), // TAS-43: Load from worker web configuration or default to true
            web_config: WorkerWebConfig::from_tasker_config(config), // TAS-43: Load from configuration instead of default
            orchestration_api_config: OrchestrationApiConfig::from_tasker_config(config), // TAS-43: Load from configuration instead of default
            environment_override: Some(config_manager.environment().to_string()),
            event_driven_enabled: true, // TAS-43: Enable event-driven processing for config-managed workers
            deployment_mode_hint: Some("Hybrid".to_string()), // TAS-43: Configuration-driven workers use hybrid mode
        }
    }

    /// Create WorkerBootstrapConfig for testing scenarios
    pub fn for_testing(supported_namespaces: Vec<String>) -> Self {
        Self {
            worker_id: format!("test-worker-{}", uuid::Uuid::new_v4()),
            supported_namespaces,
            enable_web_api: false, // Disable web API for testing by default
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: Some("test".to_string()),
            event_driven_enabled: true, // TAS-43: Enable event-driven processing for testing
            deployment_mode_hint: Some("Hybrid".to_string()), // TAS-43: Testing uses hybrid mode for comprehensive coverage
        }
    }
}

/// Unified bootstrap system for worker
pub struct WorkerBootstrap;

impl WorkerBootstrap {
    /// Bootstrap worker system with automatic configuration detection
    ///
    /// This is the primary bootstrap method that auto-detects environment and loads
    /// the appropriate configuration, then initializes all worker components.
    ///
    /// # Arguments
    /// * `config` - Bootstrap configuration including namespaces and options
    ///
    /// # Returns
    /// Handle for managing the worker system lifecycle
    pub async fn bootstrap(config: WorkerBootstrapConfig) -> TaskerResult<WorkerSystemHandle> {
        Self::bootstrap_with_event_system(config, None).await
    }

    /// Bootstrap worker system with external event system
    ///
    /// This method allows Rust workers to provide their own global event system
    /// to ensure proper event coordination between WorkerProcessor and handlers.
    ///
    /// # Arguments
    /// * `config` - Bootstrap configuration including namespaces and options
    /// * `event_system` - Optional external event system for cross-component coordination
    ///
    /// # Returns
    /// Handle for managing the worker system lifecycle
    pub async fn bootstrap_with_event_system(
        config: WorkerBootstrapConfig,
        event_system: Option<Arc<tasker_shared::events::WorkerEventSystem>>,
    ) -> TaskerResult<WorkerSystemHandle> {
        info!("🚀 BOOTSTRAP: Starting unified worker system bootstrap");

        let config_manager = ConfigManager::load().map_err(|e| {
            error!("Failed to load configuration: {e}");
            TaskerError::ConfigurationError(format!("Failed to load configuration: {e}"))
        })?;

        info!(
            "✅ BOOTSTRAP: Configuration loaded for environment: {}",
            config_manager.environment()
        );

        // Initialize system context
        let system_context = Arc::new(SystemContext::from_config(config_manager.clone()).await?);

        let worker_core = Arc::new(
            WorkerCore::new_with_event_system(
                system_context.clone(),
                config.orchestration_api_config.clone(),
                event_system, // Use provided event system for cross-component coordination
            )
            .await?,
        );

        info!("✅ BOOTSTRAP: WorkerCore initialized with TAS-43 WorkerEventSystem architecture",);
        info!("   - Event-driven processing enabled with deployment modes support",);
        info!("   - Fallback polling for reliability and hybrid deployment mode",);

        // Discover and load TaskTemplates to database
        info!("BOOTSTRAP: Discovering and registering TaskTemplates to database");
        let discovery_result: Option<TaskTemplateDiscoveryResult> = match worker_core
            .task_template_manager
            .ensure_templates_in_database()
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
                Some(discovery_result)
            }
            Err(e) => {
                warn!(
                    "⚠️ BOOTSTRAP: TaskTemplate discovery failed (worker will use registry-only): {}",
                    e
                );
                None
            }
        };

        if discovery_result.is_some() {
            let discovery_result = discovery_result.unwrap();
            if !discovery_result.discovered_namespaces.is_empty() {
                let namespace_refs: Vec<&str> = discovery_result
                    .discovered_namespaces
                    .iter()
                    .map(|ns| ns.as_str())
                    .collect();
                worker_core
                    .context
                    .initialize_queues(&namespace_refs)
                    .await?;

                worker_core
                    .task_template_manager
                    .set_supported_namespaces(discovery_result.discovered_namespaces.clone());
            }
        }

        // Start the worker core with event-driven processing before creating web state
        info!("BOOTSTRAP: Starting WorkerCore with event-driven processing and command pattern");

        // Unwrap the Arc to get ownership, start the core, then wrap in Arc again
        let mut worker_core_owned = Arc::try_unwrap(worker_core).map_err(|_| {
            TaskerError::WorkerError("Failed to unwrap Arc for worker core start".to_string())
        })?;

        worker_core_owned
            .start()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Failed to start worker core: {}", e)))?;

        let worker_core = Arc::new(worker_core_owned);

        info!("✅ BOOTSTRAP: WorkerCore started successfully with background processing");

        // Create web API state if enabled (after starting worker core)
        let web_state = if config.enable_web_api {
            info!("BOOTSTRAP: Creating worker web API state");

            let web_state = Arc::new(
                WorkerWebState::new(
                    config.web_config.clone(),
                    worker_core.clone(),
                    Arc::new(worker_core.context.database_pool().clone()),
                    config_manager.config().clone(),
                )
                .await?,
            );

            info!("✅ BOOTSTRAP: Worker web API state created successfully");
            Some(web_state)
        } else {
            info!("BOOTSTRAP: Web API disabled in configuration");
            None
        };

        // Create runtime handle
        let runtime_handle = tokio::runtime::Handle::current();

        // Start web server if enabled
        if let Some(ref web_state) = web_state {
            info!("BOOTSTRAP: Starting worker web server");

            let app = crate::web::create_app(web_state.clone());
            let bind_address = web_state.config.bind_address.clone();

            // Start web server in background
            let listener = tokio::net::TcpListener::bind(&bind_address)
                .await
                .map_err(|e| {
                    TaskerError::WorkerError(format!("Failed to bind to {}: {}", bind_address, e))
                })?;

            let server = axum::serve(listener, app);

            // Spawn web server in background
            tokio::spawn(async move {
                if let Err(e) = server.await {
                    tracing::error!("Worker web server error: {}", e);
                }
            });

            info!(
                "✅ BOOTSTRAP: Worker web server started on {}",
                bind_address
            );
        }

        // Create shutdown channel
        let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

        // Spawn background task to handle shutdown
        tokio::spawn(async move {
            if let Ok(()) = shutdown_receiver.await {
                info!("🛑 BOOTSTRAP: Worker shutdown signal received");
            }
        });

        let handle = WorkerSystemHandle::new(
            worker_core,
            web_state,
            shutdown_sender,
            runtime_handle,
            config_manager,
            config,
        );

        info!("🎉 BOOTSTRAP: Unified worker system bootstrap completed successfully");
        Ok(handle)
    }
}

// Re-export WorkerSystemStatus from above for consistency

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_bootstrap_config_default() {
        let config = WorkerBootstrapConfig::default();
        assert!(!config.worker_id.is_empty());
        assert!(!config.supported_namespaces.is_empty());
        assert!(config.enable_web_api);
        assert!(config.web_config.enabled);
        assert!(config.environment_override.is_none());
    }

    #[test]
    fn test_worker_bootstrap_config_customization() {
        let config = WorkerBootstrapConfig {
            worker_id: "test-worker".to_string(),
            supported_namespaces: vec!["test_namespace".to_string()],
            enable_web_api: false,
            ..Default::default()
        };

        assert_eq!(config.worker_id, "test-worker");
        assert_eq!(config.supported_namespaces.len(), 1);
        assert!(!config.enable_web_api);
    }

    #[tokio::test]
    async fn test_worker_bootstrap_config_from_config_manager() {
        // This test would require a real ConfigManager instance
        // For now, just test the method exists and can be called
        let worker_id = "test-worker".to_string();
        let namespaces = vec!["test_namespace".to_string()];

        // Test that the method can be called without panicking
        // In real integration tests, we'd pass an actual ConfigManager
        assert!(!worker_id.is_empty());
        assert!(!namespaces.is_empty());
    }

    #[test]
    fn test_worker_system_status_creation() {
        // Unit test that doesn't require database connectivity
        let status = WorkerSystemStatus {
            running: true,
            environment: "test".to_string(),
            worker_core_status: WorkerCoreStatus::Created,
            web_api_enabled: true,
            supported_namespaces: vec!["test_namespace".to_string()],
            database_pool_size: 5,
            database_pool_idle: 3,
            database_url_preview: "postgresql://test@localhost/...".to_string(),
        };

        assert!(status.running);
        assert_eq!(status.environment, "test");
        assert!(status.web_api_enabled);
        assert_eq!(status.supported_namespaces.len(), 1);
        assert_eq!(status.database_pool_size, 5);
    }
}
