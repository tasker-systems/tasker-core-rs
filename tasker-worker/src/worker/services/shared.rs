//! Shared API services for both REST and gRPC APIs.
//!
//! `SharedWorkerServices` provides a single point of construction for all services
//! used by both the REST and gRPC APIs. This ensures:
//! - Services are created once, not duplicated
//! - Database pools are shared efficiently
//! - Both APIs have consistent behavior
//!
//! TAS-177: Follows the same pattern as orchestration's `SharedApiServices`.

use crate::worker::core::WorkerCore;
use crate::worker::event_router::EventRouter;
use crate::worker::in_process_event_bus::InProcessEventBus;
use crate::worker::services::{
    ConfigQueryService, HealthService, MetricsService, SharedCircuitBreakerProvider,
    TemplateQueryService,
};
use crate::worker::task_template_manager::TaskTemplateManager;
use std::sync::Arc;
use std::time::Instant;
use tasker_shared::{
    config::tasker::TaskerConfig,
    database::DatabasePools,
    errors::TaskerError,
    messaging::client::MessageClient,
    messaging::service::{MessageRouterKind, MessagingProvider},
    types::SecurityService,
};
use tokio::sync::{Mutex, RwLock as TokioRwLock};
use tracing::info;

/// Error type for SharedWorkerServices initialization
#[derive(Debug, thiserror::Error)]
pub enum SharedWorkerServicesError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Security initialization failed: {0}")]
    Security(String),
    #[error("Service initialization failed: {0}")]
    Service(String),
}

impl From<SharedWorkerServicesError> for TaskerError {
    fn from(err: SharedWorkerServicesError) -> Self {
        TaskerError::ConfigurationError(err.to_string())
    }
}

/// Shared services for REST and gRPC APIs.
///
/// This struct holds all services that are common between the REST and gRPC APIs.
/// It is created once from `WorkerCore` and shared via `Arc` by both
/// `WorkerWebState` (REST) and `WorkerGrpcState` (gRPC).
#[derive(Clone, Debug)]
pub struct SharedWorkerServices {
    /// Security service for JWT/API key authentication
    pub security_service: Option<Arc<SecurityService>>,

    /// Database pools for Tasker and PGMQ operations
    pub database_pools: DatabasePools,

    /// Message client for queue metrics and monitoring (provider-agnostic)
    pub message_client: Arc<MessageClient>,

    /// Task template manager for template caching and validation
    pub task_template_manager: Arc<TaskTemplateManager>,

    /// Event router for domain event stats (cached to avoid mutex lock)
    pub event_router: Option<Arc<EventRouter>>,

    /// In-process event bus for domain event stats (cached to avoid mutex lock)
    pub in_process_bus: Arc<TokioRwLock<InProcessEventBus>>,

    /// Circuit breaker health provider (shared reference for FFI injection)
    pub circuit_breaker_health_provider: SharedCircuitBreakerProvider,

    /// Health service for health check operations
    pub health_service: Arc<HealthService>,

    /// Metrics service for metrics collection operations
    pub metrics_service: Arc<MetricsService>,

    /// Template query service for template operations
    pub template_query_service: Arc<TemplateQueryService>,

    /// Config query service for configuration operations
    pub config_query_service: Arc<ConfigQueryService>,

    /// Worker core reference for health checks and status
    pub worker_core: Arc<Mutex<WorkerCore>>,

    /// Cached worker ID (to avoid locking mutex on every status check)
    pub worker_id: String,

    /// Application start time for uptime calculations
    pub start_time: Instant,

    /// Worker system configuration
    pub system_config: TaskerConfig,
}

impl SharedWorkerServices {
    /// Create shared API services from WorkerCore.
    ///
    /// This creates all services needed by both REST and gRPC APIs.
    pub async fn from_worker_core(
        worker_core: Arc<Mutex<WorkerCore>>,
        database_pools: DatabasePools,
        system_config: TaskerConfig,
    ) -> Result<Self, SharedWorkerServicesError> {
        info!("Creating shared worker API services");

        let start_time = Instant::now();

        // Cache the worker_id by locking once during initialization
        let worker_id = {
            let core = worker_core.lock().await;
            format!("worker-{}", core.core_id())
        };

        info!(worker_id = %worker_id, "Worker ID cached for shared services");

        // Create message client using the provider-agnostic API
        let messaging_provider =
            Arc::new(MessagingProvider::new_pgmq_with_pool(database_pools.pgmq().clone()).await);
        let message_client = Arc::new(MessageClient::new(
            messaging_provider,
            MessageRouterKind::default(),
        ));

        // Extract task template manager and event components from WorkerCore
        // (shares same instances that were created during worker initialization)
        let (task_template_manager, event_router, in_process_bus) = {
            let core = worker_core.lock().await;
            (
                core.task_template_manager.clone(),
                core.event_router(),
                core.in_process_event_bus(),
            )
        };

        info!(
            namespaces = ?task_template_manager.supported_namespaces().await,
            has_event_router = event_router.is_some(),
            "SharedWorkerServices using shared TaskTemplateManager and event components"
        );

        // Create shared circuit breaker provider reference
        // This is shared between WorkerWebState and HealthService so that
        // when set_circuit_breaker_health_provider is called, both see the update.
        let circuit_breaker_health_provider: SharedCircuitBreakerProvider =
            Arc::new(TokioRwLock::new(None));

        // Create service instances
        let health_service = Arc::new(HealthService::new(
            worker_id.clone(),
            database_pools.clone(),
            worker_core.clone(),
            task_template_manager.clone(),
            circuit_breaker_health_provider.clone(),
            start_time,
        ));

        let metrics_service = Arc::new(MetricsService::new(
            worker_id.clone(),
            database_pools.clone(),
            message_client.clone(),
            task_template_manager.clone(),
            event_router.clone(),
            in_process_bus.clone(),
            start_time,
        ));

        let template_query_service =
            Arc::new(TemplateQueryService::new(task_template_manager.clone()));

        let config_query_service = Arc::new(ConfigQueryService::new(system_config.clone()));

        info!("Service instances created for shared worker services");

        // Build SecurityService from worker auth config
        // Fail-fast: if auth is explicitly enabled but init fails, refuse to start.
        let security_service = if let Some(auth_config) = system_config
            .worker
            .as_ref()
            .and_then(|w| w.web.as_ref())
            .and_then(|web| web.auth.as_ref())
        {
            match SecurityService::from_config(auth_config).await {
                Ok(svc) => Some(Arc::new(svc)),
                Err(e) => {
                    if auth_config.enabled {
                        tracing::error!(
                            error = %e,
                            "Worker SecurityService initialization failed with auth enabled. \
                             Refusing to start without authentication."
                        );
                        return Err(SharedWorkerServicesError::Security(format!(
                            "Security initialization failed: {e}"
                        )));
                    }
                    tracing::debug!(error = %e, "Worker SecurityService init skipped (auth disabled)");
                    None
                }
            }
        } else {
            None
        };

        info!(
            worker_id = %worker_id,
            auth_enabled = security_service.is_some(),
            "Shared worker API services created successfully"
        );

        Ok(Self {
            security_service,
            database_pools,
            message_client,
            task_template_manager,
            event_router,
            in_process_bus,
            circuit_breaker_health_provider,
            health_service,
            metrics_service,
            template_query_service,
            config_query_service,
            worker_core,
            worker_id,
            start_time,
            system_config,
        })
    }

    /// Check if authentication is enabled
    pub fn is_auth_enabled(&self) -> bool {
        self.security_service
            .as_ref()
            .map(|s| s.is_enabled())
            .unwrap_or(false)
    }

    /// Get the uptime in seconds since the worker started
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get supported namespaces for this worker
    pub async fn supported_namespaces(&self) -> Vec<String> {
        self.task_template_manager.supported_namespaces().await
    }
}
