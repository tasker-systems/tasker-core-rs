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

// Note: OrchestrationLoopCoordinator removed as part of TAS-40 command pattern migration
// Bootstrap will be updated to use OrchestrationProcessor once implemented
use crate::orchestration::{
    EventDrivenCoordinatorConfig, EventDrivenOrchestrationCoordinator, OrchestrationCore,
};
use crate::web;
use crate::web::state::{AppState, WebServerConfig};
use std::sync::Arc;
use tasker_shared::config::ConfigManager;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};
use tokio::sync::oneshot;
use tracing::{error, info, warn};

/// Unified orchestration system handle for lifecycle management
pub struct OrchestrationSystemHandle {
    /// Core orchestration system
    pub orchestration_core: Arc<OrchestrationCore>,
    /// Event-driven coordination system (TAS-43)
    pub event_driven_coordinator:
        Option<Arc<tokio::sync::Mutex<EventDrivenOrchestrationCoordinator>>>,
    /// Web API state (optional)
    pub web_state: Option<Arc<AppState>>,
    /// Shutdown signal sender (Some when running, None when stopped)
    pub shutdown_sender: Option<oneshot::Sender<()>>,
    /// Runtime handle for async operations
    pub runtime_handle: tokio::runtime::Handle,
    /// System configuration manager
    pub config_manager: Arc<ConfigManager>,
    /// Bootstrap configuration
    pub bootstrap_config: BootstrapConfig,
}

impl OrchestrationSystemHandle {
    /// Create new orchestration system handle
    pub fn new(
        orchestration_core: Arc<OrchestrationCore>,
        event_driven_coordinator: Option<
            Arc<tokio::sync::Mutex<EventDrivenOrchestrationCoordinator>>,
        >,
        web_state: Option<Arc<AppState>>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        config_manager: Arc<ConfigManager>,
        bootstrap_config: BootstrapConfig,
    ) -> Self {
        Self {
            orchestration_core,
            event_driven_coordinator,
            web_state,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
            config_manager,
            bootstrap_config,
        }
    }

    /// Check if system is running
    pub fn is_running(&self) -> bool {
        self.shutdown_sender.is_some()
    }

    /// Stop the orchestration system
    pub async fn stop(&mut self) -> TaskerResult<()> {
        if self.shutdown_sender.is_some() {
            // Stop event-driven coordinator first
            if let Some(ref coordinator) = self.event_driven_coordinator {
                info!("🛑 Stopping event-driven orchestration coordinator");
                coordinator.lock().await.stop().await.map_err(|e| {
                    TaskerError::OrchestrationError(format!(
                        "Failed to stop event-driven coordinator: {}",
                        e
                    ))
                })?;
            }

            // Send shutdown signal
            if let Some(sender) = self.shutdown_sender.take() {
                sender.send(()).map_err(|_| {
                    TaskerError::OrchestrationError("Failed to send shutdown signal".to_string())
                })?;
            }

            info!("🛑 Orchestration system shutdown completed");
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
            circuit_breakers_enabled: self.orchestration_core.context.circuit_breakers_enabled(),
            database_pool_size: self.orchestration_core.context.database_pool().size(),
            database_pool_idle: self.orchestration_core.context.database_pool().num_idle(),
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
    /// Whether to start web API server
    pub enable_web_api: bool,
    /// Web API configuration (bind address, etc.)
    pub web_config: Option<WebServerConfig>,
    /// Enable event-driven orchestration coordination (TAS-43)
    pub enable_event_driven_coordination: bool,
    /// Event-driven coordinator configuration
    pub event_driven_config: Option<EventDrivenCoordinatorConfig>,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            namespaces: vec![],
            auto_start_processors: true,
            environment_override: None,
            enable_web_api: true,
            web_config: None,
            enable_event_driven_coordination: true,
            event_driven_config: Some(EventDrivenCoordinatorConfig::default()),
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
            enable_web_api: true,
            web_config: None, // Will be loaded from config manager
            enable_event_driven_coordination: true,
            event_driven_config: Some(EventDrivenCoordinatorConfig::default()),
        }
    }

    /// Create BootstrapConfig for executor testing with YAML-driven configuration
    pub fn for_executor_testing(namespaces: Vec<String>) -> Self {
        Self {
            namespaces,
            auto_start_processors: false, // Manual control for testing executors
            environment_override: Some("test".to_string()),
            enable_web_api: false, // Disable web API for testing
            web_config: None,
            enable_event_driven_coordination: false, // Disable for deterministic testing
            event_driven_config: None,
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

        let config_manager = ConfigManager::load().map_err(|e| {
            error!("Failed to load configuration: {e}");
            TaskerError::ConfigurationError(format!("Failed to load configuration: {e}"))
        })?;

        info!(
            "✅ BOOTSTRAP: Configuration loaded for environment: {}",
            config_manager.environment()
        );
        info!(
            "🛡️ BOOTSTRAP: Circuit breakers enabled: {}",
            config_manager.config().circuit_breakers.enabled
        );

        // Initialize system context
        let system_context = Arc::new(SystemContext::from_config(config_manager.clone()).await?);

        // Initialize OrchestrationCore with unified configuration
        let orchestration_core = Arc::new(OrchestrationCore::new(system_context.clone()).await?);

        info!("✅ BOOTSTRAP: OrchestrationCore initialized with unified configuration");

        // Initialize namespace queues
        if !config.namespaces.is_empty() {
            let namespace_refs: Vec<&str> = config.namespaces.iter().map(|s| s.as_str()).collect();
            orchestration_core
                .context
                .initialize_queues(&namespace_refs)
                .await?;
            info!(
                "✅ BOOTSTRAP: Initialized queues for namespaces: {:?}",
                config.namespaces
            );
        }

        // Create web API state if enabled
        let web_state = if config.enable_web_api {
            info!("BOOTSTRAP: Creating orchestration web API state");

            // Load web server config from configuration manager
            match WebServerConfig::from_config_manager(&config_manager).map_err(|e| {
                TaskerError::ConfigurationError(format!(
                    "Failed to load web server configuration: {e}"
                ))
            })? {
                Some(web_server_config) if web_server_config.enabled => {
                    let app_state = Arc::new(
                        AppState::from_orchestration_core(
                            web_server_config.clone(),
                            orchestration_core.clone(),
                            config_manager.clone(),
                        )
                        .await
                        .map_err(|e| {
                            TaskerError::ConfigurationError(format!(
                                "Failed to create AppState: {e}"
                            ))
                        })?,
                    );

                    info!("✅ BOOTSTRAP: Orchestration web API state created successfully");
                    Some(app_state)
                }
                _ => {
                    info!("BOOTSTRAP: Web API disabled in configuration");
                    None
                }
            }
        } else {
            info!("BOOTSTRAP: Web API disabled in bootstrap config");
            None
        };

        // Create event-driven coordinator if enabled (TAS-43)
        let event_driven_coordinator = if config.enable_event_driven_coordination {
            info!("BOOTSTRAP: Creating event-driven orchestration coordinator (TAS-43)");

            let coordinator_config = config
                .event_driven_config
                .clone()
                .unwrap_or_else(EventDrivenCoordinatorConfig::default);

            let coordinator = EventDrivenOrchestrationCoordinator::new(
                coordinator_config,
                system_context.clone(),
                orchestration_core.clone(),
                orchestration_core.command_sender(),
            )
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to create event-driven coordinator: {}",
                    e
                ))
            })?;

            let coordinator_arc = Arc::new(tokio::sync::Mutex::new(coordinator));

            // Start the coordinator
            coordinator_arc.lock().await.start().await.map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to start event-driven coordinator: {}",
                    e
                ))
            })?;

            info!("✅ BOOTSTRAP: Event-driven orchestration coordinator started successfully");
            Some(coordinator_arc)
        } else {
            info!("BOOTSTRAP: Event-driven coordination disabled in configuration");
            None
        };

        // Create runtime handle
        let runtime_handle = tokio::runtime::Handle::current();

        // Start web server if enabled
        if let Some(ref web_state) = web_state {
            info!("BOOTSTRAP: Starting orchestration web server");

            let app = web::create_app((**web_state).clone());
            let bind_address = web_state.config.bind_address.clone();

            // Start web server in background
            let listener = tokio::net::TcpListener::bind(&bind_address)
                .await
                .map_err(|e| {
                    TaskerError::OrchestrationError(format!(
                        "Failed to bind to {}: {}",
                        bind_address, e
                    ))
                })?;

            let server = axum::serve(listener, app);

            // Spawn web server in background
            tokio::spawn(async move {
                if let Err(e) = server.await {
                    tracing::error!("Orchestration web server error: {}", e);
                }
            });

            info!(
                "✅ BOOTSTRAP: Orchestration web server started on {}",
                bind_address
            );
            info!("📖 API Documentation: http://{}/api-docs/ui", bind_address);
            info!("🏥 Health Check: http://{}/health", bind_address);
        }

        // Create shutdown channel
        let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

        // Spawn background task to handle shutdown
        tokio::spawn(async move {
            if let Ok(()) = shutdown_receiver.await {
                info!("🛑 BOOTSTRAP: Shutdown signal received");
            }
        });

        let handle = OrchestrationSystemHandle::new(
            orchestration_core,
            event_driven_coordinator,
            web_state,
            shutdown_sender,
            runtime_handle,
            config_manager,
            config,
        );

        info!("🎉 BOOTSTRAP: Unified orchestration system bootstrap completed successfully");
        Ok(handle)
    }

    /// Bootstrap for standalone deployment
    ///
    /// Full-featured bootstrap method for standalone deployments with custom configuration.
    pub async fn bootstrap_standalone(
        environment: Option<String>,
        namespaces: Vec<String>,
        enable_web_api: bool,
    ) -> TaskerResult<OrchestrationSystemHandle> {
        let config = BootstrapConfig {
            namespaces,
            auto_start_processors: true,
            environment_override: environment,
            enable_web_api,
            web_config: None, // Will be loaded from config manager
            enable_event_driven_coordination: true,
            event_driven_config: Some(EventDrivenCoordinatorConfig::default()),
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
            enable_web_api: false, // Disable web API for testing by default
            web_config: None,
            enable_event_driven_coordination: false, // Disable for deterministic testing
            event_driven_config: None,
        };

        Self::bootstrap(config).await
    }

    // TODO: TAS-40 - Replace with OrchestrationProcessor startup method
    // This method will be reimplemented once OrchestrationProcessor is created
    /*
    async fn start_coordinator(
        coordinator: Arc<OrchestrationLoopCoordinator>,
        shutdown_receiver: oneshot::Receiver<()>,
    ) -> TaskerResult<()> {
        // Implementation removed - will be replaced with command pattern startup
    }
    */
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
            enable_web_api: true,
            web_config: None,
        };

        assert_eq!(config.namespaces.len(), 1);
        assert!(!config.auto_start_processors);
        assert_eq!(config.environment_override, Some("test".to_string()));
    }
}
