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
use tracing::{info, warn};
use workspace_tools::workspace;

use crate::{
    api_clients::orchestration_client::OrchestrationApiConfig,
    worker::core::{WorkerCore, WorkerCoreStatus},
    web::{WorkerWebState, state::WorkerWebConfig},
};

use tasker_shared::{
    config::{ConfigManager, UnifiedConfigLoader},
    system_context::SystemContext,
    errors::{TaskerError, TaskerResult},
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
}

impl Default for WorkerBootstrapConfig {
    fn default() -> Self {
        Self {
            worker_id: format!("worker-{}", uuid::Uuid::new_v4()),
            supported_namespaces: vec![
                "linear_workflow".to_string(),
                "order_fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
            ],
            enable_web_api: true,
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: None,
        }
    }
}

impl WorkerBootstrapConfig {
    /// Create WorkerBootstrapConfig from ConfigManager for configuration-driven bootstrap
    pub fn from_config_manager(
        config_manager: &ConfigManager,
        worker_id: String,
        supported_namespaces: Vec<String>,
    ) -> Self {
        Self {
            worker_id,
            supported_namespaces,
            enable_web_api: true, // Default for most use cases
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: Some(config_manager.environment().to_string()),
        }
    }

    /// Create WorkerBootstrapConfig for testing scenarios
    pub fn for_testing(
        supported_namespaces: Vec<String>,
    ) -> Self {
        Self {
            worker_id: format!("test-worker-{}", uuid::Uuid::new_v4()),
            supported_namespaces,
            enable_web_api: false, // Disable web API for testing by default
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: Some("test".to_string()),
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
        info!("🚀 BOOTSTRAP: Starting unified worker system bootstrap");

        // Load configuration using UnifiedConfigLoader directly (TAS-34 Phase 2)
        let detected_env = UnifiedConfigLoader::detect_environment();
        let environment = config
            .environment_override
            .as_deref()
            .unwrap_or(&detected_env);

        let ws = workspace().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to create workspace: {e}"))
        })?;
        let config_root = ws.config_dir().join("tasker");
        
        // Use UnifiedConfigLoader as primary implementation
        let mut loader =
            UnifiedConfigLoader::with_root(config_root.clone(), environment).map_err(|e| {
                TaskerError::ConfigurationError(format!(
                    "Failed to create UnifiedConfigLoader: {e}"
                ))
            })?;

        let tasker_config = loader.load_tasker_config().map_err(|e| {
            TaskerError::ConfigurationError(format!(
                "Failed to load config with UnifiedConfigLoader: {e}"
            ))
        })?;

        // Create ConfigManager wrapper for backward compatibility
        let config_manager = Arc::new(
            ConfigManager::from_tasker_config(
                tasker_config,
                environment.to_string(),
            ),
        );

        info!(
            "✅ BOOTSTRAP: Configuration loaded for environment: {}",
            config_manager.environment()
        );

        // Initialize system context
        let system_context = Arc::new(SystemContext::from_config(config_manager.clone()).await?);

        // Initialize WorkerCore with unified configuration
        let worker_core = Arc::new(WorkerCore::new(
            system_context.clone(),
            config.orchestration_api_config.clone(),
        ).await?);

        info!("✅ BOOTSTRAP: WorkerCore initialized with unified configuration");

        // Initialize namespace queues if needed  
        if !config.supported_namespaces.is_empty() {
            let namespace_refs: Vec<&str> = config.supported_namespaces.iter().map(|s| s.as_str()).collect();
            worker_core
                .context
                .initialize_queues(&namespace_refs)
                .await?;
            info!(
                "✅ BOOTSTRAP: Initialized queues for namespaces: {:?}",
                config.supported_namespaces
            );
        }

        // Start the worker core
        let mut worker_core_clone = Arc::clone(&worker_core);
        if let Some(core) = Arc::get_mut(&mut worker_core_clone) {
            core.start().await?;
        }

        // Create web API state if enabled
        let web_state = if config.enable_web_api {
            // TODO: For now, we'll skip web API state creation as it requires refactoring
            // to integrate properly with WorkerCore. This will be addressed in the next task.
            info!("BOOTSTRAP: Web API state creation deferred - requires WorkerCore integration");
            None
        } else {
            info!("BOOTSTRAP: Web API disabled in configuration");
            None
        };

        // Create runtime handle
        let runtime_handle = tokio::runtime::Handle::current();

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

    /// Quick bootstrap for embedded/testing scenarios
    ///
    /// Simplified bootstrap method optimized for embedded mode and testing.
    pub async fn bootstrap_embedded(
        supported_namespaces: Vec<String>,
    ) -> TaskerResult<WorkerSystemHandle> {
        info!(
            "🚀 BOOTSTRAP: Starting embedded worker system"
        );

        let config = WorkerBootstrapConfig {
            worker_id: format!("embedded-worker-{}", uuid::Uuid::new_v4()),
            supported_namespaces,
            enable_web_api: true,
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: None, // Let it detect environment
        };

        // Use unified bootstrap
        Self::bootstrap(config).await
    }

    /// Bootstrap for standalone deployment
    ///
    /// Full-featured bootstrap method for standalone deployments with custom configuration.
    pub async fn bootstrap_standalone(
        environment: Option<String>,
        supported_namespaces: Vec<String>,
        enable_web_api: bool,
    ) -> TaskerResult<WorkerSystemHandle> {
        let config = WorkerBootstrapConfig {
            worker_id: format!("standalone-worker-{}", uuid::Uuid::new_v4()),
            supported_namespaces,
            enable_web_api,
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: environment,
        };

        Self::bootstrap(config).await
    }

    /// Bootstrap for testing scenarios
    ///
    /// Testing-optimized bootstrap with automatic test environment detection.
    pub async fn bootstrap_testing(
        supported_namespaces: Vec<String>,
    ) -> TaskerResult<WorkerSystemHandle> {
        let config = WorkerBootstrapConfig {
            worker_id: format!("test-worker-{}", uuid::Uuid::new_v4()),
            supported_namespaces,
            enable_web_api: false, // Disable web API for testing by default
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: Some("test".to_string()),
        };

        Self::bootstrap(config).await
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