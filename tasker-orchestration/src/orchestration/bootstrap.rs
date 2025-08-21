//! # Unified Orchestration Bootstrap System
//!
//! Provides a unified way to bootstrap the orchestration system across all deployment modes:
//! - Embedded mode (Ruby FFI)
//! - Standalone deployment
//! - Docker containers
//! - Testing environments
//!
//! ## Key Features
//!
//! - **Environment-Aware Configuration**: Uses TaskerConfig with environment detection
//! - **Circuit Breaker Integration**: Automatically configured based on settings
//! - **Lifecycle Management**: Start/stop/status operations for all deployment modes
//! - **Graceful Shutdown**: Proper cleanup and resource management
//! - **Consistent API**: Same bootstrap interface regardless of deployment mode

use crate::orchestration::coordinator::OrchestrationLoopCoordinator;
use crate::orchestration::OrchestrationCore;
use std::sync::Arc;
use tasker_shared::config::{ConfigManager, UnifiedConfigLoader};
use tasker_shared::{TaskerError, TaskerResult};
use tokio::sync::oneshot;
use tracing::{error, info, warn};
use workspace_tools::workspace;

/// Unified orchestration system handle for lifecycle management
pub struct OrchestrationSystemHandle {
    /// Core orchestration system
    pub orchestration_core: Arc<OrchestrationCore>,
    /// Shutdown signal sender (Some when running, None when stopped)
    pub shutdown_sender: Option<oneshot::Sender<()>>,
    /// Runtime handle for async operations
    pub runtime_handle: tokio::runtime::Handle,
    /// System configuration manager
    pub config_manager: Arc<ConfigManager>,
}

impl OrchestrationSystemHandle {
    /// Create new orchestration system handle
    pub fn new(
        orchestration_core: Arc<OrchestrationCore>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        config_manager: Arc<ConfigManager>,
    ) -> Self {
        Self {
            orchestration_core,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
            config_manager,
        }
    }

    /// Check if system is running
    pub fn is_running(&self) -> bool {
        self.shutdown_sender.is_some()
    }

    /// Stop the orchestration system
    pub fn stop(&mut self) -> TaskerResult<()> {
        if let Some(sender) = self.shutdown_sender.take() {
            sender.send(()).map_err(|_| {
                TaskerError::OrchestrationError("Failed to send shutdown signal".to_string())
            })?;
            info!("🛑 Orchestration system shutdown requested");
            Ok(())
        } else {
            warn!("Orchestration system already stopped");
            Ok(())
        }
    }

    /// Get system status information
    pub fn status(&self) -> SystemStatus {
        SystemStatus {
            running: self.is_running(),
            environment: self.config_manager.environment().to_string(),
            circuit_breakers_enabled: self.orchestration_core.circuit_breakers_enabled(),
            database_pool_size: self.orchestration_core.database_pool().size(),
            database_pool_idle: self.orchestration_core.database_pool().num_idle(),
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

/// System status information
#[derive(Debug, Clone)]
pub struct SystemStatus {
    pub running: bool,
    pub environment: String,
    pub circuit_breakers_enabled: bool,
    pub database_pool_size: u32,
    pub database_pool_idle: usize,
    pub database_url_preview: String,
}

/// Bootstrap configuration for orchestration system
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Namespaces to initialize queues for
    pub namespaces: Vec<String>,
    /// Whether to start processors immediately (vs manual control)
    pub auto_start_processors: bool,
    /// Environment override (None = auto-detect)
    pub environment_override: Option<String>,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            namespaces: vec![],
            auto_start_processors: true,
            environment_override: None,
        }
    }
}

impl BootstrapConfig {
    /// Create BootstrapConfig from ConfigManager for configuration-driven bootstrap
    pub fn from_config_manager(
        config_manager: &tasker_shared::config::ConfigManager,
        namespaces: Vec<String>,
    ) -> Self {
        Self {
            namespaces,
            auto_start_processors: true, // Default for most use cases
            environment_override: Some(config_manager.environment().to_string()),
        }
    }

    /// Create BootstrapConfig for executor testing with YAML-driven configuration
    pub fn for_executor_testing(namespaces: Vec<String>) -> Self {
        Self {
            namespaces,
            auto_start_processors: false, // Manual control for testing executors
            environment_override: Some("test".to_string()),
        }
    }
}

/// Unified bootstrap system for orchestration
pub struct OrchestrationBootstrap;

impl OrchestrationBootstrap {
    /// Bootstrap orchestration system with automatic configuration detection
    ///
    /// This is the primary bootstrap method that auto-detects environment and loads
    /// the appropriate configuration, then initializes all orchestration components.
    ///
    /// # Arguments
    /// * `config` - Bootstrap configuration including namespaces and options
    ///
    /// # Returns
    /// Handle for managing the orchestration system lifecycle
    pub async fn bootstrap(config: BootstrapConfig) -> TaskerResult<OrchestrationSystemHandle> {
        info!("🚀 BOOTSTRAP: Starting unified orchestration system bootstrap");

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

        // Create ConfigManager wrapper for backward compatibility only
        let config_manager = Arc::new(
            tasker_shared::config::manager::ConfigManager::from_tasker_config(
                tasker_config,
                environment.to_string(),
            ),
        );

        info!(
            "✅ BOOTSTRAP: Configuration loaded for environment: {}",
            config_manager.environment()
        );
        info!(
            "🛡️ BOOTSTRAP: Circuit breakers enabled: {}",
            config_manager.config().circuit_breakers.enabled
        );

        // Initialize OrchestrationCore with unified configuration
        let orchestration_core =
            Arc::new(OrchestrationCore::from_config(config_manager.clone()).await?);

        info!("✅ BOOTSTRAP: OrchestrationCore initialized with unified configuration");

        // Initialize namespace queues
        if !config.namespaces.is_empty() {
            let namespace_refs: Vec<&str> = config.namespaces.iter().map(|s| s.as_str()).collect();
            orchestration_core
                .initialize_queues(&namespace_refs)
                .await?;
            info!(
                "✅ BOOTSTRAP: Initialized queues for namespaces: {:?}",
                config.namespaces
            );
        }

        // Create runtime handle
        let runtime_handle = tokio::runtime::Handle::current();

        // Create shutdown channel
        let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

        // Start coordinator if auto-start is enabled (NEW UNIFIED ARCHITECTURE)
        if config.auto_start_processors {
            // Create OrchestrationLoopCoordinator for unified architecture
            let coordinator = Arc::new(
                OrchestrationLoopCoordinator::new(
                    config_manager.clone(),
                    orchestration_core.clone(),
                )
                .await?,
            );

            info!("✅ BOOTSTRAP: Using OrchestrationLoopCoordinator for unified architecture");

            Self::start_coordinator(coordinator, shutdown_receiver).await?;
        } else {
            info!("📋 BOOTSTRAP: Coordinator not auto-started - manual control mode");
            // If not auto-starting, we need to consume the receiver somehow
            drop(shutdown_receiver);
        }

        let handle = OrchestrationSystemHandle::new(
            orchestration_core,
            shutdown_sender,
            runtime_handle,
            config_manager,
        );

        info!("🎉 BOOTSTRAP: Unified orchestration system bootstrap completed successfully");
        Ok(handle)
    }

    /// Quick bootstrap for embedded/testing scenarios
    ///
    /// Simplified bootstrap method optimized for embedded mode and testing.
    /// Uses OrchestrationLoopCoordinator for unified architecture.
    pub async fn bootstrap_embedded(
        namespaces: Vec<String>,
    ) -> TaskerResult<OrchestrationSystemHandle> {
        info!(
            "🚀 BOOTSTRAP: Starting embedded orchestration with unified coordinator architecture"
        );

        let config = BootstrapConfig {
            namespaces,
            auto_start_processors: true,
            environment_override: None, // Let it detect environment
        };

        // Use unified bootstrap which now defaults to coordinator
        Self::bootstrap(config).await
    }

    /// Bootstrap for standalone deployment
    ///
    /// Full-featured bootstrap method for standalone deployments with custom configuration.
    pub async fn bootstrap_standalone(
        environment: Option<String>,
        namespaces: Vec<String>,
    ) -> TaskerResult<OrchestrationSystemHandle> {
        let config = BootstrapConfig {
            namespaces,
            auto_start_processors: true,
            environment_override: environment,
        };

        Self::bootstrap(config).await
    }

    /// Bootstrap for testing scenarios
    ///
    /// Testing-optimized bootstrap with automatic test environment detection.
    pub async fn bootstrap_testing(
        namespaces: Vec<String>,
    ) -> TaskerResult<OrchestrationSystemHandle> {
        let config = BootstrapConfig {
            namespaces,
            auto_start_processors: true,
            environment_override: Some("test".to_string()),
        };

        Self::bootstrap(config).await
    }

    /// Start OrchestrationLoopCoordinator with lifecycle management
    async fn start_coordinator(
        coordinator: Arc<OrchestrationLoopCoordinator>,
        shutdown_receiver: oneshot::Receiver<()>,
    ) -> TaskerResult<()> {
        info!("🚀 BOOTSTRAP: Starting OrchestrationLoopCoordinator");

        // Start the coordinator
        coordinator.start().await?;
        info!("✅ BOOTSTRAP: OrchestrationLoopCoordinator started successfully");

        // Spawn background task to manage coordinator lifecycle
        tokio::spawn(async move {
            // Wait for shutdown signal
            if let Ok(()) = shutdown_receiver.await {
                info!("🛑 BOOTSTRAP: Shutdown signal received, stopping coordinator");

                // Stop coordinator with timeout
                let timeout = std::time::Duration::from_secs(30);
                if let Err(e) = coordinator.stop(timeout).await {
                    error!("❌ BOOTSTRAP: Failed to stop coordinator gracefully: {}", e);
                } else {
                    info!("✅ BOOTSTRAP: OrchestrationLoopCoordinator stopped successfully");
                }
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bootstrap_config_default() {
        let config = BootstrapConfig::default();
        assert!(config.namespaces.is_empty());
        assert!(config.auto_start_processors);
        assert!(config.environment_override.is_none());
    }

    #[tokio::test]
    async fn test_system_status_creation() {
        // This is a unit test that doesn't require database connectivity
        let status = SystemStatus {
            running: true,
            environment: "test".to_string(),
            circuit_breakers_enabled: false,
            database_pool_size: 5,
            database_pool_idle: 3,
            database_url_preview: "postgresql://test@localhost/...".to_string(),
        };

        assert!(status.running);
        assert_eq!(status.environment, "test");
        assert!(!status.circuit_breakers_enabled);
        assert_eq!(status.database_pool_size, 5);
        assert_eq!(status.database_pool_idle, 3);
    }

    #[tokio::test]
    async fn test_coordinator_bootstrap_config() {
        // Test that coordinator bootstrap config is properly structured
        let config = BootstrapConfig {
            namespaces: vec!["test_namespace".to_string()],
            auto_start_processors: false, // Don't auto-start for testing
            environment_override: Some("test".to_string()),
        };

        assert_eq!(config.namespaces.len(), 1);
        assert!(!config.auto_start_processors);
        assert_eq!(config.environment_override, Some("test".to_string()));
    }
}
