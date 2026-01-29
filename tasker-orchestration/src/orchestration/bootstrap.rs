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

// Note: TAS-40 command pattern migration complete. TAS-148 unified the processor into
// OrchestrationCommandProcessorActor in the actors module.
use crate::orchestration::{
    event_systems::{UnifiedCoordinatorConfig, UnifiedEventCoordinator},
    OrchestrationCore,
};
#[cfg(any(feature = "web-api", feature = "grpc-api"))]
use crate::services::SharedApiServices;
#[cfg(feature = "web-api")]
use crate::web;
#[cfg(feature = "web-api")]
use crate::web::state::AppState;
use std::sync::Arc;
use tasker_shared::config::event_systems::{
    EventSystemConfig, OrchestrationEventSystemMetadata, TaskReadinessEventSystemMetadata,
};
// TAS-61 Phase 6C/6D: V2 configuration is canonical
use tasker_shared::config::tasker::TaskerConfig;
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
    /// Web API state (optional, requires web-api feature)
    #[cfg(feature = "web-api")]
    pub web_state: Option<Arc<AppState>>,
    /// gRPC server handle (optional, requires grpc-api feature)
    #[cfg(feature = "grpc-api")]
    pub grpc_server_handle: Option<crate::grpc::GrpcServerHandle>,
    /// Shutdown signal sender (Some when running, None when stopped)
    pub shutdown_sender: Option<oneshot::Sender<()>>,
    /// Runtime handle for async operations
    pub runtime_handle: tokio::runtime::Handle,
    /// System configuration manager
    pub tasker_config: Arc<TaskerConfig>,
    /// Bootstrap configuration
    pub bootstrap_config: BootstrapConfig,
}

impl std::fmt::Debug for OrchestrationSystemHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("OrchestrationSystemHandle");
        debug.field("has_orchestration_core", &true).field(
            "has_event_coordinator",
            &self.unified_event_coordinator.is_some(),
        );

        #[cfg(feature = "web-api")]
        debug.field("has_web_state", &self.web_state.is_some());

        #[cfg(feature = "grpc-api")]
        debug.field("has_grpc_server", &self.grpc_server_handle.is_some());

        debug
            .field("is_running", &self.shutdown_sender.is_some())
            .field("bootstrap_config", &self.bootstrap_config)
            .finish()
    }
}

impl OrchestrationSystemHandle {
    /// Create new orchestration system handle (both web-api and grpc-api)
    #[cfg(all(feature = "web-api", feature = "grpc-api"))]
    pub fn new(
        orchestration_core: Arc<OrchestrationCore>,
        unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
        web_state: Option<Arc<AppState>>,
        grpc_server_handle: Option<crate::grpc::GrpcServerHandle>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        tasker_config: Arc<TaskerConfig>,
        bootstrap_config: BootstrapConfig,
    ) -> Self {
        Self {
            orchestration_core,
            unified_event_coordinator,
            web_state,
            grpc_server_handle,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
            tasker_config,
            bootstrap_config,
        }
    }

    /// Create new orchestration system handle (grpc-api only, no web-api)
    #[cfg(all(feature = "grpc-api", not(feature = "web-api")))]
    pub fn new(
        orchestration_core: Arc<OrchestrationCore>,
        unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
        grpc_server_handle: Option<crate::grpc::GrpcServerHandle>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        tasker_config: Arc<TaskerConfig>,
        bootstrap_config: BootstrapConfig,
    ) -> Self {
        Self {
            orchestration_core,
            unified_event_coordinator,
            grpc_server_handle,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
            tasker_config,
            bootstrap_config,
        }
    }

    /// Create new orchestration system handle (web-api only, no gRPC)
    #[cfg(all(feature = "web-api", not(feature = "grpc-api")))]
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

    /// Create new orchestration system handle (no API features)
    #[cfg(not(any(feature = "web-api", feature = "grpc-api")))]
    pub fn new(
        orchestration_core: Arc<OrchestrationCore>,
        unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        tasker_config: Arc<TaskerConfig>,
        bootstrap_config: BootstrapConfig,
    ) -> Self {
        Self {
            orchestration_core,
            unified_event_coordinator,
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

            // Stop gRPC server if running
            #[cfg(feature = "grpc-api")]
            if let Some(grpc_handle) = self.grpc_server_handle.take() {
                info!("Stopping gRPC server");
                if let Err(e) = grpc_handle.stop().await {
                    warn!("Failed to stop gRPC server cleanly: {}", e);
                } else {
                    info!("gRPC server stopped");
                }
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
            environment: self.tasker_config.common.execution.environment.clone(),
            circuit_breakers_enabled: self.orchestration_core.context.circuit_breakers_enabled(),
            database_pool_size: self.orchestration_core.context.database_pool().size(),
            database_pool_idle: self.orchestration_core.context.database_pool().num_idle(),
            database_url_preview: self
                .tasker_config
                .common
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
            // TAS-61 V2: Access queues from common config
            namespaces: vec![config.common.queues.orchestration_namespace.clone()],
            // TAS-61 V2: Access environment from common.execution
            environment_override: Some(config.common.execution.environment.clone()),
            // TAS-61 V2: Access web config from orchestration context (optional)
            enable_web_api: config
                .orchestration
                .as_ref()
                .and_then(|o| o.web.as_ref())
                .map(|web| web.enabled)
                .unwrap_or(true),
        }
    }
}

/// Unified bootstrap system for orchestration
#[derive(Debug)]
pub struct OrchestrationBootstrap;

impl OrchestrationBootstrap {
    /// Bootstrap orchestration system with automatic configuration detection
    ///
    /// This is the primary bootstrap method that auto-detects environment and loads
    /// the appropriate configuration, then initializes all orchestration components.
    ///
    /// # Returns
    /// Handle for managing the orchestration system lifecycle
    pub async fn bootstrap() -> TaskerResult<OrchestrationSystemHandle> {
        info!(
            "Starting unified orchestration system bootstrap with context-specific configuration"
        );

        // Step 1: Initialize system context and orchestration core
        let (system_context, orchestration_core) = Self::initialize_orchestration_core().await?;

        // Step 2: Initialize orchestration and namespace queues
        let config = Self::initialize_queues(&orchestration_core, &system_context).await?;

        let tasker_config = system_context.tasker_config.as_ref();

        // Step 3: Create shared services if any API is enabled
        #[cfg(any(feature = "web-api", feature = "grpc-api"))]
        let shared_services =
            Self::create_shared_services_if_needed(&orchestration_core, tasker_config).await?;

        // Step 4: Create web API state if enabled
        #[cfg(feature = "web-api")]
        let web_state =
            Self::create_web_state_if_enabled(&config, tasker_config, &shared_services)?;

        // Step 5: Set up unified event coordinator
        let unified_event_coordinator = Self::create_and_start_event_coordinator(
            tasker_config,
            &system_context,
            &orchestration_core,
        )
        .await?;

        // Step 6: Start web server if enabled
        #[cfg(feature = "web-api")]
        if let Some(ref state) = web_state {
            Self::start_web_server(state).await?;
        }

        // Step 7: Start gRPC server if enabled
        #[cfg(feature = "grpc-api")]
        let grpc_server_handle = {
            // Determine if web API is enabled for logging purposes
            #[cfg(feature = "web-api")]
            let web_api_enabled = web_state.is_some();
            #[cfg(not(feature = "web-api"))]
            let web_api_enabled = false;

            Self::start_grpc_server_if_enabled(tasker_config, &shared_services, web_api_enabled)?
        };

        // Step 8: Create shutdown channel and handler
        let (shutdown_sender, runtime_handle) = Self::setup_shutdown_handler();

        // Step 9: Create and return orchestration system handle
        // There are 4 mutually exclusive feature combinations:
        //   1. Both grpc-api and web-api enabled
        //   2. grpc-api only (no web-api)
        //   3. web-api only (no grpc-api)
        //   4. Neither API enabled
        #[cfg(all(feature = "grpc-api", feature = "web-api"))]
        let handle = Self::create_system_handle(
            orchestration_core,
            unified_event_coordinator,
            web_state,
            grpc_server_handle,
            shutdown_sender,
            runtime_handle,
            system_context.tasker_config.clone(),
            config,
        );

        #[cfg(all(feature = "grpc-api", not(feature = "web-api")))]
        let handle = Self::create_system_handle(
            orchestration_core,
            unified_event_coordinator,
            grpc_server_handle,
            shutdown_sender,
            runtime_handle,
            system_context.tasker_config.clone(),
            config,
        );

        #[cfg(all(feature = "web-api", not(feature = "grpc-api")))]
        let handle = Self::create_system_handle(
            orchestration_core,
            unified_event_coordinator,
            web_state,
            shutdown_sender,
            runtime_handle,
            system_context.tasker_config.clone(),
            config,
        );

        #[cfg(not(any(feature = "web-api", feature = "grpc-api")))]
        let handle = Self::create_system_handle(
            orchestration_core,
            unified_event_coordinator,
            shutdown_sender,
            runtime_handle,
            system_context.tasker_config.clone(),
            config,
        );

        info!("Unified orchestration system bootstrap completed successfully with context-specific configuration (TAS-50)");
        Ok(handle)
    }

    // =========================================================================
    // Step 1: Initialize system context and orchestration core
    // =========================================================================

    /// Initialize the system context and orchestration core, then start the core.
    async fn initialize_orchestration_core(
    ) -> TaskerResult<(Arc<SystemContext>, Arc<OrchestrationCore>)> {
        // TAS-50 Phase 2: Use orchestration-specific context loading
        // This loads only CommonConfig + OrchestrationConfig from context TOML files
        let system_context = Arc::new(SystemContext::new_for_orchestration().await?);

        // TAS-61 V2: Access environment from common.execution
        info!(
            "Orchestration context loaded successfully for environment: {}",
            system_context.tasker_config.common.execution.environment
        );

        // Initialize OrchestrationCore with orchestration-specific configuration
        let mut orchestration_core = OrchestrationCore::new(system_context.clone()).await?;

        // Start the orchestration core (transitions status to Running)
        orchestration_core.start().await?;

        info!("OrchestrationCore initialized with unified configuration");

        Ok((system_context, Arc::new(orchestration_core)))
    }

    // =========================================================================
    // Step 2: Initialize queues
    // =========================================================================

    /// Initialize orchestration-owned queues and namespace queues.
    async fn initialize_queues(
        orchestration_core: &Arc<OrchestrationCore>,
        system_context: &Arc<SystemContext>,
    ) -> TaskerResult<BootstrapConfig> {
        // Initialize orchestration-owned queues
        orchestration_core
            .context
            .initialize_orchestration_owned_queues()
            .await?;

        // Build bootstrap config from tasker_config
        let tasker_config = system_context.tasker_config.as_ref();
        let config: BootstrapConfig = tasker_config.into();

        // Initialize namespace queues
        if !config.namespaces.is_empty() {
            let namespace_refs: Vec<&str> = config.namespaces.iter().map(|s| s.as_str()).collect();
            orchestration_core
                .context
                .initialize_queues(&namespace_refs)
                .await?;
            info!("Initialized queues for namespaces: {:?}", config.namespaces);
        }

        Ok(config)
    }

    // =========================================================================
    // Step 3: Create shared services (conditionally)
    // =========================================================================

    /// Determine if any API (REST or gRPC) is enabled in configuration.
    /// Also checks if the corresponding feature is compiled in.
    #[cfg(any(feature = "web-api", feature = "grpc-api"))]
    fn is_any_api_enabled(tasker_config: &TaskerConfig) -> bool {
        let orchestration = tasker_config.orchestration.as_ref();

        #[cfg(feature = "web-api")]
        let web_enabled = orchestration
            .and_then(|o| o.web.as_ref())
            .map(|web| web.enabled)
            .unwrap_or(false);

        #[cfg(not(feature = "web-api"))]
        let web_enabled = false;

        #[cfg(feature = "grpc-api")]
        let grpc_enabled = orchestration
            .and_then(|o| o.grpc.as_ref())
            .map(|grpc| grpc.enabled)
            .unwrap_or(false);

        #[cfg(not(feature = "grpc-api"))]
        let grpc_enabled = false;

        web_enabled || grpc_enabled
    }

    /// Create shared services only if at least one API (REST or gRPC) is enabled.
    ///
    /// TAS-177: SharedApiServices are only needed when REST or gRPC APIs are enabled.
    /// The orchestration core has its own services, so we don't need to create shared
    /// services if neither API is being used.
    #[cfg(any(feature = "web-api", feature = "grpc-api"))]
    async fn create_shared_services_if_needed(
        orchestration_core: &Arc<OrchestrationCore>,
        tasker_config: &TaskerConfig,
    ) -> TaskerResult<Option<Arc<SharedApiServices>>> {
        if !Self::is_any_api_enabled(tasker_config) {
            info!("No APIs enabled, skipping SharedApiServices creation");
            return Ok(None);
        }

        let shared_services = Arc::new(
            SharedApiServices::from_orchestration_core(orchestration_core.clone())
                .await
                .map_err(|e| {
                    TaskerError::ConfigurationError(format!(
                        "Failed to create SharedApiServices: {e}"
                    ))
                })?,
        );
        info!("Shared API services created successfully");

        Ok(Some(shared_services))
    }

    // =========================================================================
    // Step 4: Create web API state
    // =========================================================================

    /// Create web API state if the web API is enabled.
    #[cfg(feature = "web-api")]
    fn create_web_state_if_enabled(
        config: &BootstrapConfig,
        tasker_config: &TaskerConfig,
        shared_services: &Option<Arc<SharedApiServices>>,
    ) -> TaskerResult<Option<Arc<AppState>>> {
        if !config.enable_web_api {
            info!("Orchestration web API disabled in bootstrap config");
            return Ok(None);
        }

        info!("Creating orchestration web API state");

        // TAS-61 V2: Access web config from orchestration context (optional)
        let web_config_opt = tasker_config
            .orchestration
            .as_ref()
            .and_then(|o| o.web.as_ref());

        let Some(web_config) = web_config_opt else {
            info!("Orchestration web config not present");
            return Ok(None);
        };

        if !web_config.enabled {
            info!("Orchestration web API state disabled");
            return Ok(None);
        }

        let Some(services) = shared_services else {
            // This shouldn't happen if is_any_api_enabled is working correctly
            return Err(TaskerError::ConfigurationError(
                "Web API enabled but SharedApiServices not created".to_string(),
            ));
        };

        // TAS-177: Create AppState from shared services
        let app_state = Arc::new(AppState::new(services.clone(), web_config.clone()));

        info!("Orchestration web API state created successfully");
        Ok(Some(app_state))
    }

    // =========================================================================
    // Step 5: Create and start unified event coordinator
    // =========================================================================

    /// Build the UnifiedCoordinatorConfig from tasker configuration.
    fn build_coordinator_config(tasker_config: &TaskerConfig) -> UnifiedCoordinatorConfig {
        // TAS-61 V2: Access event_systems from orchestration context
        let event_systems = tasker_config
            .orchestration
            .as_ref()
            .map(|o| o.event_systems.clone())
            .unwrap_or_default();

        let task_readiness_event_system = event_systems.task_readiness.clone();
        let orchestration_event_system = event_systems.orchestration.clone();

        info!(
            orchestration_deployment_mode = %orchestration_event_system.deployment_mode,
            task_readiness_deployment_mode = %task_readiness_event_system.deployment_mode,
            "Loading UnifiedCoordinatorConfig from configuration"
        );

        // Convert V2 configs to legacy EventSystemConfig types
        let orchestration_config = EventSystemConfig::<OrchestrationEventSystemMetadata> {
            system_id: orchestration_event_system.system_id,
            deployment_mode: orchestration_event_system.deployment_mode,
            timing: orchestration_event_system.timing,
            processing: orchestration_event_system.processing,
            health: orchestration_event_system.health,
            metadata: OrchestrationEventSystemMetadata { _reserved: None },
        };

        let task_readiness_config = EventSystemConfig::<TaskReadinessEventSystemMetadata> {
            system_id: task_readiness_event_system.system_id,
            deployment_mode: task_readiness_event_system.deployment_mode,
            timing: task_readiness_event_system.timing,
            processing: task_readiness_event_system.processing,
            health: task_readiness_event_system.health,
            metadata: TaskReadinessEventSystemMetadata { _reserved: None },
        };

        UnifiedCoordinatorConfig {
            coordinator_id: "unified-event-coordinator".to_string(),
            orchestration_config,
            task_readiness_config,
        }
    }

    /// Create and start the unified event coordinator.
    async fn create_and_start_event_coordinator(
        tasker_config: &TaskerConfig,
        system_context: &Arc<SystemContext>,
        orchestration_core: &Arc<OrchestrationCore>,
    ) -> TaskerResult<Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>> {
        let coordinator_config = Self::build_coordinator_config(tasker_config);

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
        Ok(Some(coordinator_arc))
    }

    // =========================================================================
    // Step 6: Start web server
    // =========================================================================

    /// Start the web server for the given web state.
    #[cfg(feature = "web-api")]
    async fn start_web_server(web_state: &Arc<AppState>) -> TaskerResult<()> {
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

        // TAS-158: Named spawn for tokio-console visibility
        tasker_shared::spawn_named!("orchestration_web_server", async move {
            if let Err(e) = server.await {
                tracing::error!("Orchestration web server error: {}", e);
            }
        });

        info!("Orchestration web server started on {}", bind_address);
        info!("   API Documentation: http://{}/api-docs/ui", bind_address);
        info!("   Health Check: http://{}/health", bind_address);

        Ok(())
    }

    // =========================================================================
    // Step 7: Start gRPC server (feature-gated)
    // =========================================================================

    /// Start the gRPC server if enabled in configuration.
    #[cfg(feature = "grpc-api")]
    fn start_grpc_server_if_enabled(
        tasker_config: &TaskerConfig,
        shared_services: &Option<Arc<SharedApiServices>>,
        web_api_enabled: bool,
    ) -> TaskerResult<Option<crate::grpc::GrpcServerHandle>> {
        // Access gRPC config from orchestration context
        let grpc_config_opt = tasker_config
            .orchestration
            .as_ref()
            .and_then(|o| o.grpc.as_ref());

        let Some(grpc_config) = grpc_config_opt else {
            info!("gRPC configuration not present");
            return Ok(None);
        };

        if !grpc_config.enabled {
            info!("gRPC server disabled in configuration");
            return Ok(None);
        }

        let Some(services) = shared_services else {
            // This shouldn't happen if is_any_api_enabled is working correctly
            return Err(TaskerError::ConfigurationError(
                "gRPC API enabled but SharedApiServices not created".to_string(),
            ));
        };

        info!("Starting gRPC server");

        // TAS-177: Create GrpcState from shared services
        // Services are shared with REST API (when enabled)
        let grpc_state = crate::grpc::GrpcState::new(services.clone(), grpc_config.clone());
        info!(
            "   Mode: shared services (REST API {})",
            if web_api_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );

        // Create and spawn the gRPC server
        let grpc_server = crate::grpc::GrpcServer::new(grpc_config.clone(), grpc_state);
        let grpc_handle = grpc_server.spawn();

        let grpc_bind_address = grpc_handle.bind_address().to_string();
        info!("gRPC server started on {}", grpc_bind_address);
        info!(
            "   TLS: {}",
            if grpc_config.tls_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        info!(
            "   Reflection: {}",
            if grpc_config.enable_reflection {
                "enabled"
            } else {
                "disabled"
            }
        );

        Ok(Some(grpc_handle))
    }

    // =========================================================================
    // Step 8: Setup shutdown handler
    // =========================================================================

    /// Create shutdown channel and spawn the shutdown handler.
    fn setup_shutdown_handler() -> (oneshot::Sender<()>, tokio::runtime::Handle) {
        let runtime_handle = tokio::runtime::Handle::current();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

        // TAS-158: Named spawn for tokio-console visibility
        tasker_shared::spawn_named!("orchestration_shutdown_handler", async move {
            if let Ok(()) = shutdown_receiver.await {
                info!("Shutdown signal received");
            }
        });

        (shutdown_sender, runtime_handle)
    }

    // =========================================================================
    // Step 9: Create system handle
    // =========================================================================

    /// Create the orchestration system handle (both grpc-api and web-api).
    #[cfg(all(feature = "grpc-api", feature = "web-api"))]
    fn create_system_handle(
        orchestration_core: Arc<OrchestrationCore>,
        unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
        web_state: Option<Arc<AppState>>,
        grpc_server_handle: Option<crate::grpc::GrpcServerHandle>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        tasker_config: Arc<TaskerConfig>,
        config: BootstrapConfig,
    ) -> OrchestrationSystemHandle {
        OrchestrationSystemHandle::new(
            orchestration_core,
            unified_event_coordinator,
            web_state,
            grpc_server_handle,
            shutdown_sender,
            runtime_handle,
            tasker_config,
            config,
        )
    }

    /// Create the orchestration system handle (grpc-api only, no web-api).
    #[cfg(all(feature = "grpc-api", not(feature = "web-api")))]
    fn create_system_handle(
        orchestration_core: Arc<OrchestrationCore>,
        unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
        grpc_server_handle: Option<crate::grpc::GrpcServerHandle>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        tasker_config: Arc<TaskerConfig>,
        config: BootstrapConfig,
    ) -> OrchestrationSystemHandle {
        OrchestrationSystemHandle::new(
            orchestration_core,
            unified_event_coordinator,
            grpc_server_handle,
            shutdown_sender,
            runtime_handle,
            tasker_config,
            config,
        )
    }

    /// Create the orchestration system handle (web-api only, no gRPC).
    #[cfg(all(feature = "web-api", not(feature = "grpc-api")))]
    fn create_system_handle(
        orchestration_core: Arc<OrchestrationCore>,
        unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
        web_state: Option<Arc<AppState>>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        tasker_config: Arc<TaskerConfig>,
        config: BootstrapConfig,
    ) -> OrchestrationSystemHandle {
        OrchestrationSystemHandle::new(
            orchestration_core,
            unified_event_coordinator,
            web_state,
            shutdown_sender,
            runtime_handle,
            tasker_config,
            config,
        )
    }

    /// Create the orchestration system handle (no API features).
    #[cfg(not(any(feature = "web-api", feature = "grpc-api")))]
    fn create_system_handle(
        orchestration_core: Arc<OrchestrationCore>,
        unified_event_coordinator: Option<Arc<tokio::sync::Mutex<UnifiedEventCoordinator>>>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        tasker_config: Arc<TaskerConfig>,
        config: BootstrapConfig,
    ) -> OrchestrationSystemHandle {
        OrchestrationSystemHandle::new(
            orchestration_core,
            unified_event_coordinator,
            shutdown_sender,
            runtime_handle,
            tasker_config,
            config,
        )
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
