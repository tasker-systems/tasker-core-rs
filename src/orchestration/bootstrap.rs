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

use crate::config::ConfigManager;
use crate::error::{Result, TaskerError};
use crate::orchestration::OrchestrationCore;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{error, info, warn};

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
    pub fn stop(&mut self) -> Result<()> {
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
    /// Custom configuration directory (None = auto-detect)
    pub config_directory: Option<std::path::PathBuf>,
    /// Environment override (None = auto-detect)
    pub environment_override: Option<String>,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            namespaces: vec![],
            auto_start_processors: true,
            config_directory: None,
            environment_override: None,
        }
    }
}

impl BootstrapConfig {
    /// Create BootstrapConfig from ConfigManager for configuration-driven bootstrap
    pub fn from_config_manager(
        config_manager: &crate::config::ConfigManager,
        namespaces: Vec<String>,
    ) -> Self {
        Self {
            namespaces,
            auto_start_processors: true, // Default for most use cases
            config_directory: Some(config_manager.config_directory().to_path_buf()),
            environment_override: Some(config_manager.environment().to_string()),
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
    pub async fn bootstrap(config: BootstrapConfig) -> Result<OrchestrationSystemHandle> {
        info!("🚀 BOOTSTRAP: Starting unified orchestration system bootstrap");

        // Load configuration manager with environment detection
        let config_manager = if let Some(env) = &config.environment_override {
            if let Some(config_dir) = &config.config_directory {
                ConfigManager::load_from_directory_with_env(Some(config_dir.clone()), env).map_err(
                    |e| TaskerError::ConfigurationError(format!("Failed to load config: {e}")),
                )?
            } else {
                ConfigManager::load_from_directory_with_env(None, env).map_err(|e| {
                    TaskerError::ConfigurationError(format!("Failed to load config: {e}"))
                })?
            }
        } else if let Some(config_dir) = &config.config_directory {
            ConfigManager::load_from_directory(Some(config_dir.clone())).map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to load config: {e}"))
            })?
        } else {
            ConfigManager::load().map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to load config: {e}"))
            })?
        };

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

        // Start processors if auto-start is enabled
        if config.auto_start_processors {
            Self::start_processors(
                orchestration_core.clone(),
                shutdown_receiver,
                runtime_handle.clone(),
            )
            .await?;
        } else {
            info!("📋 BOOTSTRAP: Processors not auto-started - manual control mode");
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

    /// Start orchestration processors with sequential startup to avoid database connection race conditions
    ///
    /// This implements sequential startup with connection pool warming to prevent the
    /// "task was cancelled" errors that occur when all processors try to acquire
    /// database connections simultaneously during system initialization.
    async fn start_processors(
        orchestration_core: Arc<OrchestrationCore>,
        shutdown_receiver: oneshot::Receiver<()>,
        _runtime_handle: tokio::runtime::Handle,
    ) -> Result<()> {
        info!("🚀 BOOTSTRAP: Starting orchestration processors with sequential startup");

        // STEP 1: Warm up the database connection pool to prevent race conditions
        info!("🔥 BOOTSTRAP: Warming up database connection pool");
        match orchestration_core.database_pool().acquire().await {
            Ok(conn) => {
                info!("✅ BOOTSTRAP: Database connection pool warmed up successfully");
                drop(conn); // Release the connection back to the pool
            }
            Err(e) => {
                error!("❌ BOOTSTRAP: Failed to warm up connection pool: {}", e);
                return Err(TaskerError::DatabaseError(format!(
                    "Connection pool warmup failed: {e}"
                )));
            }
        }

        // STEP 2: Clone processor references for sequential startup
        let orchestration_loop = Arc::clone(&orchestration_core.orchestration_loop);
        let task_request_processor = Arc::clone(&orchestration_core.task_request_processor);
        let step_result_processor = Arc::clone(&orchestration_core.step_result_processor);

        // STEP 3: Start processors sequentially with small delays to prevent connection race conditions

        // Start orchestration loop first
        let orchestration_task = tokio::spawn(async move {
            info!("🔄 BOOTSTRAP: Orchestration loop started - beginning continuous operation");
            if let Err(e) = orchestration_loop.run_continuous().await {
                error!("❌ BOOTSTRAP: Orchestration loop failed: {}", e);
            } else {
                info!("✅ BOOTSTRAP: Orchestration loop completed successfully");
            }
        });
        let orchestration_abort_handle = orchestration_task.abort_handle();

        // Small delay to let orchestration loop initialize its connections
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Start task request processor second
        let task_request_task = tokio::spawn(async move {
            info!("🔄 BOOTSTRAP: Task request processor started");
            if let Err(e) = task_request_processor.start_processing_loop().await {
                error!("❌ BOOTSTRAP: Task request processor failed: {}", e);
            }
        });
        let task_request_abort_handle = task_request_task.abort_handle();

        // Small delay to let task request processor initialize its connections
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Start step result processor third
        let step_result_task = tokio::spawn(async move {
            info!("🔄 BOOTSTRAP: Step result processor started");
            if let Err(e) = step_result_processor.start_processing_loop().await {
                error!("❌ BOOTSTRAP: Step result processor failed: {}", e);
            }
        });
        let step_result_abort_handle = step_result_task.abort_handle();

        // Final small delay to ensure all processors have started before returning
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        info!("✅ BOOTSTRAP: All orchestration processors spawned successfully");

        // Spawn background task to manage processor lifecycle
        tokio::spawn(async move {
            // Wait for shutdown signal or any processor to complete
            tokio::select! {
                _ = shutdown_receiver => {
                    info!("🛑 BOOTSTRAP: Shutdown signal received, stopping processors");
                    orchestration_abort_handle.abort();
                    task_request_abort_handle.abort();
                    step_result_abort_handle.abort();
                }
                result = orchestration_task => {
                    match result {
                        Ok(_) => info!("✅ BOOTSTRAP: Orchestration loop completed normally"),
                        Err(e) => error!("❌ BOOTSTRAP: Orchestration loop task failed: {}", e),
                    }
                }
                result = task_request_task => {
                    match result {
                        Ok(_) => info!("✅ BOOTSTRAP: Task request processor completed normally"),
                        Err(e) => error!("❌ BOOTSTRAP: Task request processor task failed: {}", e),
                    }
                }
                result = step_result_task => {
                    match result {
                        Ok(_) => info!("✅ BOOTSTRAP: Step result processor completed normally"),
                        Err(e) => error!("❌ BOOTSTRAP: Step result processor task failed: {}", e),
                    }
                }
            }

            info!("🛑 BOOTSTRAP: Orchestration system shutting down");
        });

        Ok(())
    }

    /// Quick bootstrap for embedded/testing scenarios
    ///
    /// Simplified bootstrap method optimized for embedded mode and testing.
    pub async fn bootstrap_embedded(namespaces: Vec<String>) -> Result<OrchestrationSystemHandle> {
        let config = BootstrapConfig {
            namespaces,
            auto_start_processors: true,
            config_directory: None,
            environment_override: None, // Let it detect environment
        };

        Self::bootstrap(config).await
    }

    /// Bootstrap for standalone deployment
    ///
    /// Full-featured bootstrap method for standalone deployments with custom configuration.
    pub async fn bootstrap_standalone(
        config_directory: Option<std::path::PathBuf>,
        environment: Option<String>,
        namespaces: Vec<String>,
    ) -> Result<OrchestrationSystemHandle> {
        let config = BootstrapConfig {
            namespaces,
            auto_start_processors: true,
            config_directory,
            environment_override: environment,
        };

        Self::bootstrap(config).await
    }

    /// Bootstrap for testing scenarios
    ///
    /// Testing-optimized bootstrap with automatic test environment detection.
    pub async fn bootstrap_testing(namespaces: Vec<String>) -> Result<OrchestrationSystemHandle> {
        let config = BootstrapConfig {
            namespaces,
            auto_start_processors: true,
            config_directory: None,
            environment_override: Some("test".to_string()),
        };

        Self::bootstrap(config).await
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
        assert!(config.config_directory.is_none());
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
}
