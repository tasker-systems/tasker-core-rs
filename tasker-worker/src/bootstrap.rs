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
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::{
    web::{state::WorkerWebConfig, WorkerWebState},
    worker::core::{WorkerCore, WorkerCoreStatus},
};
use tasker_client::api_clients::orchestration_client::OrchestrationApiConfig;
use tasker_shared::{
    config::tasker::TaskerConfigV2,
    config::TaskerConfig,
    errors::{TaskerError, TaskerResult},
    system_context::SystemContext,
};

/// Unified worker system handle for lifecycle management
pub struct WorkerSystemHandle {
    /// Core worker system
    pub worker_core: Arc<Mutex<WorkerCore>>,
    /// Web API state (optional)
    pub web_state: Option<Arc<WorkerWebState>>,
    /// Shutdown signal sender (Some when running, None when stopped)
    pub shutdown_sender: Option<oneshot::Sender<()>>,
    /// Runtime handle for async operations
    pub runtime_handle: tokio::runtime::Handle,
    /// Worker configuration
    pub worker_config: WorkerBootstrapConfig,
}

impl std::fmt::Debug for WorkerSystemHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerSystemHandle")
            .field("running", &self.shutdown_sender.is_some())
            .field("web_api_enabled", &self.web_state.is_some())
            .field("worker_config", &self.worker_config)
            .finish()
    }
}

impl WorkerSystemHandle {
    /// Create new worker system handle
    pub fn new(
        worker_core: Arc<Mutex<WorkerCore>>,
        web_state: Option<Arc<WorkerWebState>>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        worker_config: WorkerBootstrapConfig,
    ) -> Self {
        Self {
            worker_core,
            web_state,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
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
            info!("Worker system shutdown requested");
            Ok(())
        } else {
            warn!("Worker system already stopped");
            Ok(())
        }
    }

    /// Get system status information
    pub async fn status(&self) -> TaskerResult<WorkerSystemStatus> {
        let worker_core = self.worker_core.lock().await;
        let status = WorkerSystemStatus {
            running: self.is_running(),
            environment: worker_core.context.tasker_config.environment().to_string(),
            worker_core_status: worker_core.status().clone(),
            web_api_enabled: self.worker_config.web_config.enabled,
            supported_namespaces: worker_core
                .task_template_manager
                .supported_namespaces()
                .await,
            database_pool_size: worker_core.context.database_pool().size(),
            database_pool_idle: worker_core.context.database_pool().num_idle(),
            database_url_preview: worker_core
                .context
                .tasker_config
                .database_url()
                .chars()
                .take(30)
                .collect::<String>()
                + "...",
        };
        Ok(status)
    }

    pub async fn supported_namespaces(&self) -> Vec<String> {
        let worker_core = self.worker_core.lock().await;
        worker_core
            .task_template_manager
            .supported_namespaces()
            .await
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
            enable_web_api: true,
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: None,
            event_driven_enabled: true, // TAS-43: Enable event-driven processing by default
            deployment_mode_hint: Some("Hybrid".to_string()), // TAS-43: Default to hybrid mode
        }
    }
}

impl From<&TaskerConfig> for WorkerBootstrapConfig {
    fn from(config: &TaskerConfig) -> WorkerBootstrapConfig {
        WorkerBootstrapConfig {
            worker_id: format!("worker-{}", uuid::Uuid::new_v4()),
            enable_web_api: config
                .worker
                .as_ref()
                .map(|w| w.web.enabled)
                .unwrap_or(true), // TAS-43: Load from worker web configuration or default to true
            web_config: WorkerWebConfig::from_tasker_config(config), // TAS-43: Load from configuration instead of default
            orchestration_api_config: OrchestrationApiConfig::from_tasker_config(config), // TAS-43: Load from configuration instead of default
            environment_override: Some(config.environment().to_string()),
            event_driven_enabled: config
                .event_systems
                .worker
                .deployment_mode
                .has_event_driven(),
            deployment_mode_hint: Some(config.event_systems.worker.deployment_mode.to_string()),
        }
    }
}

// TAS-61 Phase 6B: V2 configuration support
impl From<&TaskerConfigV2> for WorkerBootstrapConfig {
    fn from(config: &TaskerConfigV2) -> WorkerBootstrapConfig {
        WorkerBootstrapConfig {
            worker_id: format!("worker-{}", uuid::Uuid::new_v4()),
            enable_web_api: config
                .worker
                .as_ref()
                .and_then(|w| w.web.as_ref())
                .map(|web| web.enabled)
                .unwrap_or(true),
            // TAS-61 Phase 6B: Use default configs for now (helper methods will be migrated in Phase 6C)
            web_config: WorkerWebConfig::default(),
            orchestration_api_config: OrchestrationApiConfig::default(),
            environment_override: Some(config.common.execution.environment.clone()),
            // TAS-61 Phase 6B: Default to event-driven mode (will be properly migrated in Phase 6C)
            event_driven_enabled: true,
            deployment_mode_hint: Some("Hybrid".to_string()),
        }
    }
}

/// Unified bootstrap system for worker
#[derive(Debug)]
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
    pub async fn bootstrap() -> TaskerResult<WorkerSystemHandle> {
        Self::bootstrap_with_event_system(None).await
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
        event_system: Option<Arc<tasker_shared::events::WorkerEventSystem>>,
    ) -> TaskerResult<WorkerSystemHandle> {
        info!(
            "Starting unified worker system bootstrap with context-specific configuration (TAS-50)"
        );

        // TAS-50 Phase 2: Use worker-specific context loading
        // This loads only CommonConfig + WorkerConfig from context TOML files
        let system_context = Arc::new(SystemContext::new_for_worker().await?);

        info!(
            "Worker context loaded successfully for environment: {}",
            system_context.tasker_config.environment()
        );

        let config: WorkerBootstrapConfig = system_context.tasker_config.as_ref().into();

        // Create worker core (not wrapped in Arc yet - we need to start it first)
        let mut worker_core = WorkerCore::new_with_event_system(
            system_context.clone(),
            config.orchestration_api_config.clone(),
            event_system, // Use provided event system for cross-component coordination
        )
        .await?;

        info!("WorkerCore initialized with WorkerEventSystem architecture",);
        info!("   - Event-driven processing enabled with deployment modes support",);
        info!("   - Fallback polling for reliability and hybrid deployment mode",);

        // Start the worker core before wrapping in Arc<Mutex<>>
        worker_core
            .start()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Failed to start worker core: {}", e)))?;

        // Now wrap in Arc<Mutex<>> for shared access across web API and handle
        let worker_core = Arc::new(Mutex::new(worker_core));

        info!("WorkerCore started successfully with background processing");

        // Create web API state if enabled (after starting worker core)
        let web_state = if config.enable_web_api {
            info!("Creating worker web API state");

            // Clone the Arc<Mutex<WorkerCore>> for web API
            let web_worker_core = worker_core.clone();

            // Get database pool (need to lock briefly to access context)
            let database_pool = {
                let core = worker_core.lock().await;
                Arc::new(core.context.database_pool().clone())
            };

            let web_state = Arc::new(
                WorkerWebState::new(
                    config.web_config.clone(),
                    web_worker_core,
                    database_pool,
                    (*system_context.tasker_config).clone(),
                )
                .await?,
            );

            info!("Worker web API state created successfully");
            Some(web_state)
        } else {
            info!("Web API disabled in configuration");
            None
        };

        // Create runtime handle
        let runtime_handle = tokio::runtime::Handle::current();

        // Start web server if enabled
        if let Some(ref web_state) = web_state {
            info!("Starting worker web server");

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

            info!("Worker web server started on {}", bind_address);
        }

        // Create shutdown channel
        let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

        // Spawn background task to handle shutdown
        tokio::spawn(async move {
            if let Ok(()) = shutdown_receiver.await {
                info!("Worker shutdown signal received");
            }
        });

        let handle = WorkerSystemHandle::new(
            worker_core,
            web_state,
            shutdown_sender,
            runtime_handle,
            config,
        );

        info!("Unified worker system bootstrap completed successfully");
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
        assert!(config.enable_web_api);
        assert!(config.web_config.enabled);
        assert!(config.environment_override.is_none());
    }

    #[test]
    fn test_worker_bootstrap_config_customization() {
        let config = WorkerBootstrapConfig {
            worker_id: "test-worker".to_string(),
            enable_web_api: false,
            ..Default::default()
        };

        assert_eq!(config.worker_id, "test-worker");
        assert!(!config.enable_web_api);
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
