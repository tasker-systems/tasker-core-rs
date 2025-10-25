//! # Unified Orchestration Bootstrap System
//!
//! Provides a unified way to bootstrap the orchestration system across all deployment modes:
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
    event_systems::{UnifiedCoordinatorConfig, UnifiedEventCoordinator},
    OrchestrationCore,
};
use crate::web;
use crate::web::state::AppState;
use std::sync::Arc;
use tasker_shared::config::TaskerConfig;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};
use tokio::sync::oneshot;
use tracing::{info, warn};

/// Unified orchestration system handle for lifecycle management
pub struct OrchestrationSystemHandle {
    /// Core orchestration system
    pub orchestration_core: Arc<OrchestrationCore>,
    /// Event-driven coordination system
    pub unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
    /// Web API state (optional)
    pub web_state: Option<Arc<AppState>>,
    /// Shutdown signal sender (Some when running, None when stopped)
    pub shutdown_sender: Option<oneshot::Sender<()>>,
    /// Runtime handle for async operations
    pub runtime_handle: tokio::runtime::Handle,
    /// System configuration manager
    pub tasker_config: Arc<TaskerConfig>,
    /// Bootstrap configuration
    pub bootstrap_config: BootstrapConfig,
}

impl OrchestrationSystemHandle {
    /// Create new orchestration system handle
    pub fn new(
        orchestration_core: Arc<OrchestrationCore>,
        unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
        web_state: Option<Arc<AppState>>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        tasker_config: Arc<TaskerConfig>,
        bootstrap_config: BootstrapConfig,
    ) -> Self {
        Self {
            orchestration_core,
            unified_event_coordinator,
            web_state,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
            tasker_config,
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
            // Stop unified event coordinator first
            if let Some(ref coordinator) = self.unified_event_coordinator {
                info!("Stopping unified event coordinator");
                coordinator.lock().await.stop().await.map_err(|e| {
                    TaskerError::OrchestrationError(format!(
                        "Failed to stop unified event coordinator: {}",
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

            info!("Orchestration system shutdown completed");
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
            environment: self.tasker_config.environment().to_string(),
            circuit_breakers_enabled: self.orchestration_core.context.circuit_breakers_enabled(),
            database_pool_size: self.orchestration_core.context.database_pool().size(),
            database_pool_idle: self.orchestration_core.context.database_pool().num_idle(),
            database_url_preview: self
                .tasker_config
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
    /// Environment override (None = auto-detect)
    pub environment_override: Option<String>,
    /// Whether to start web API server
    pub enable_web_api: bool,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            namespaces: vec![],
            environment_override: None,
            enable_web_api: true,
        }
    }
}

impl From<&TaskerConfig> for BootstrapConfig {
    fn from(config: &TaskerConfig) -> BootstrapConfig {
        BootstrapConfig {
            namespaces: vec![config.queues.orchestration_namespace.clone()],
            environment_override: Some(config.environment().to_string()),
            enable_web_api: config.orchestration.web_enabled(),
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
    pub async fn bootstrap() -> TaskerResult<OrchestrationSystemHandle> {
        info!(
            "Starting unified orchestration system bootstrap with context-specific configuration"
        );

        // TAS-50 Phase 2: Use orchestration-specific context loading
        // This loads only CommonConfig + OrchestrationConfig from context TOML files
        let system_context = Arc::new(SystemContext::new_for_orchestration().await?);

        info!(
            "Orchestration context loaded successfully for environment: {}",
            system_context.tasker_config.environment()
        );

        // Initialize OrchestrationCore with orchestration-specific configuration
        let orchestration_core = Arc::new(OrchestrationCore::new(system_context.clone()).await?);

        orchestration_core
            .context
            .initialize_orchestration_owned_queues()
            .await?;

        // Build bootstrap config from tasker_config
        let tasker_config = system_context.tasker_config.as_ref();
        let config: BootstrapConfig = tasker_config.into();

        info!("OrchestrationCore initialized with unified configuration");

        // Initialize namespace queues
        if !config.namespaces.is_empty() {
            let namespace_refs: Vec<&str> = config.namespaces.iter().map(|s| s.as_str()).collect();
            orchestration_core
                .context
                .initialize_queues(&namespace_refs)
                .await?;
            info!("Initialized queues for namespaces: {:?}", config.namespaces);
        }

        // Create web API state if enabled
        let web_state: Option<Arc<AppState>> = if config.enable_web_api {
            info!("Creating orchestration web API state");

            let web_config = tasker_config.orchestration.web.clone();

            if web_config.enabled {
                let app_state = Arc::new(
                    AppState::from_orchestration_core(orchestration_core.clone())
                        .await
                        .map_err(|e| {
                            TaskerError::ConfigurationError(format!(
                                "Failed to create AppState: {e}"
                            ))
                        })?,
                );

                info!("Orchestration web API state created successfully");
                Some(app_state)
            } else {
                info!("Orchestration web API state disabled");
                None
            }
        } else {
            info!("Orchestration web API disabled in bootstrap config");
            None
        };

        let coordinator_config: UnifiedCoordinatorConfig = {
            // Access task readiness configuration from unified event systems configuration
            let task_readiness_config = tasker_config.event_systems.task_readiness.clone();

            // Access orchestration configuration from unified event systems configuration
            let orchestration_config = tasker_config.event_systems.orchestration.clone();

            info!(
                orchestration_deployment_mode = %orchestration_config.deployment_mode,
                task_readiness_deployment_mode = %task_readiness_config.deployment_mode,
                "Loading UnifiedCoordinatorConfig from configuration"
            );

            UnifiedCoordinatorConfig {
                coordinator_id: "unified-event-coordinator".to_string(),
                orchestration_config,
                task_readiness_config,
            }
        };

        let coordinator = UnifiedEventCoordinator::new(
            coordinator_config,
            system_context.clone(),
            orchestration_core.clone(),
            orchestration_core.command_sender(),
        )
        .await
        .map_err(|e| {
            TaskerError::OrchestrationError(format!(
                "Failed to create unified event coordinator: {}",
                e
            ))
        })?;

        let coordinator_arc = Arc::new(tokio::sync::Mutex::new(coordinator));

        // Start the coordinator
        coordinator_arc.lock().await.start().await.map_err(|e| {
            TaskerError::OrchestrationError(format!(
                "Failed to start unified event coordinator: {}",
                e
            ))
        })?;

        info!("Unified event coordinator started successfully");
        let unified_event_coordinator = Some(coordinator_arc);

        // Create runtime handle
        let runtime_handle = tokio::runtime::Handle::current();

        // Start web server if enabled
        if let Some(ref web_state) = web_state {
            info!("Starting orchestration web server");

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

            info!("Orchestration web server started on {}", bind_address);
            info!("📖 API Documentation: http://{}/api-docs/ui", bind_address);
            info!("🏥 Health Check: http://{}/health", bind_address);
        }

        // Create shutdown channel
        let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

        // Spawn background task to handle shutdown
        tokio::spawn(async move {
            if let Ok(()) = shutdown_receiver.await {
                info!("Shutdown signal received");
            }
        });

        let handle = OrchestrationSystemHandle::new(
            orchestration_core,
            unified_event_coordinator,
            web_state,
            shutdown_sender,
            runtime_handle,
            system_context.tasker_config.clone(), // Use tasker_config from SystemContext
            config,
        );

        info!("Unified orchestration system bootstrap completed successfully with context-specific configuration (TAS-50)");
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bootstrap_config_default() {
        let config = BootstrapConfig::default();
        assert!(config.namespaces.is_empty());
        assert!(config.environment_override.is_none());
        assert!(config.enable_web_api);
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
            environment_override: Some("test".to_string()),
            enable_web_api: true,
        };

        assert_eq!(config.namespaces.len(), 1);
        assert_eq!(config.environment_override, Some("test".to_string()));
    }
}
