//! Worker Web Application State
//!
//! Contains shared state for the worker web API including database connections,
//! configuration, and metrics tracking.

use crate::worker::event_router::EventRouter;
use crate::worker::in_process_event_bus::InProcessEventBus;
use crate::worker::services::{
    ConfigQueryService, HealthService, MetricsService, SharedCircuitBreakerProvider,
    TemplateQueryService,
};
use crate::worker::task_template_manager::TaskTemplateManager;
use serde::Serialize;
use std::{sync::Arc, time::Instant};
use tasker_shared::{
    config::tasker::TaskerConfig,
    database::DatabasePools,
    errors::{TaskerError, TaskerResult},
    messaging::client::MessageClient,
    messaging::service::MessagingProvider,
    types::api::worker::CircuitBreakersHealth,
    types::base::CacheStats,
    types::web::{
        DomainEventStats, EventRouterStats as WebEventRouterStats,
        InProcessEventBusStats as WebInProcessEventBusStats,
    },
};
use tokio::sync::RwLock as TokioRwLock;
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
    pub config_endpoint_enabled: bool,
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
            config_endpoint_enabled: false,
        }
    }
}

// TAS-61 Phase 6D: Convert from canonical TaskerConfig
impl From<&tasker_shared::config::tasker::WorkerConfig> for WorkerWebConfig {
    fn from(worker_config: &tasker_shared::config::tasker::WorkerConfig) -> Self {
        // Use default if web config not present
        let web = match &worker_config.web {
            Some(w) => w,
            None => return Self::default(),
        };

        let health = &worker_config.health_monitoring;

        Self {
            enabled: web.enabled,
            bind_address: web.bind_address.clone(),
            request_timeout_ms: web.request_timeout_ms as u64,
            authentication_enabled: web.auth.is_some(),
            // TAS-61: CORS always enabled via hardcoded tower_http::cors::Any
            cors_enabled: true,
            metrics_enabled: health.performance_monitoring_enabled,
            health_check_interval_seconds: health.health_check_interval_seconds as u64,
            config_endpoint_enabled: web.config_endpoint_enabled,
        }
    }
}

/// Shared state for the worker web application
#[derive(Clone, Debug)]
pub struct WorkerWebState {
    /// Worker core reference for health checks and status
    /// Wrapped in Arc<Mutex<>> for thread-safe shared access
    pub worker_core: Arc<tokio::sync::Mutex<crate::worker::core::WorkerCore>>,

    /// TAS-164: Database pools for Tasker and PGMQ operations
    pub database_pools: DatabasePools,

    /// TAS-133e: Message client for queue metrics and monitoring (provider-agnostic)
    pub message_client: Arc<MessageClient>,

    /// Task template manager for template caching and validation
    pub task_template_manager: Arc<TaskTemplateManager>,

    /// TAS-65: Event router for domain event stats (cached to avoid mutex lock)
    event_router: Option<Arc<EventRouter>>,

    /// TAS-65: In-process event bus for domain event stats (cached to avoid mutex lock)
    in_process_bus: Arc<TokioRwLock<InProcessEventBus>>,

    /// TAS-75: Circuit breaker health provider (injected from FFI layer)
    /// This is set by the Ruby/Python worker binary when it creates the FfiDispatchChannel.
    /// Shared with HealthService so updates are visible to both.
    circuit_breaker_health_provider: SharedCircuitBreakerProvider,

    /// Web API configuration
    pub config: WorkerWebConfig,

    /// Application start time for uptime calculations
    pub start_time: Instant,

    /// Worker system configuration
    pub system_config: TaskerConfig,

    /// Cached worker ID (to avoid locking mutex on every status check)
    worker_id: String,

    // =========================================================================
    // TAS-77: Service instances for handler delegation
    // =========================================================================
    /// Health service for health check operations
    health_service: Arc<HealthService>,

    /// Metrics service for metrics collection operations
    metrics_service: Arc<MetricsService>,

    /// Template query service for template operations
    template_query_service: Arc<TemplateQueryService>,

    /// Config query service for configuration operations
    config_query_service: Arc<ConfigQueryService>,

    /// TAS-150: Unified security service for JWT + API key authentication
    pub security_service: Option<Arc<tasker_shared::types::SecurityService>>,
}

/// TAS-75: Trait for providing circuit breaker health information
///
/// This trait allows FFI workers (Ruby/Python) to inject their circuit breaker
/// health information into the worker web state without exposing implementation details.
pub trait CircuitBreakerHealthProvider: Send + Sync + std::fmt::Debug {
    /// Get the current circuit breaker health status
    fn get_circuit_breakers_health(&self) -> CircuitBreakersHealth;
}

impl WorkerWebState {
    /// Create new worker web state with all required components
    ///
    /// # Arguments
    /// * `config` - Web API configuration
    /// * `worker_core` - Worker core for processing stats
    /// * `database_pools` - TAS-164: Database pools (Tasker + PGMQ)
    /// * `system_config` - System configuration
    pub async fn new(
        config: WorkerWebConfig,
        worker_core: Arc<tokio::sync::Mutex<crate::worker::core::WorkerCore>>,
        database_pools: DatabasePools,
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

        // TAS-133e: Create message client using the new provider-agnostic API
        let messaging_provider =
            Arc::new(MessagingProvider::new_pgmq_with_pool(database_pools.pgmq().clone()).await);
        let message_client = Arc::new(MessageClient::new(
            messaging_provider,
            tasker_shared::messaging::service::MessageRouterKind::default(),
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
            "WorkerWebState using shared TaskTemplateManager and event components"
        );

        let start_time = Instant::now();

        // TAS-77: Create shared circuit breaker provider reference
        // This is shared between WorkerWebState and HealthService so that
        // when set_circuit_breaker_health_provider is called, both see the update.
        let circuit_breaker_health_provider: SharedCircuitBreakerProvider =
            Arc::new(TokioRwLock::new(None));

        // TAS-77: Create service instances for handler delegation
        // TAS-164: Pass DatabasePools for pool utilization and connectivity checks
        // TAS-169: Pass TaskTemplateManager for distributed cache status in detailed health
        let health_service = Arc::new(HealthService::new(
            worker_id.clone(),
            database_pools.clone(),
            worker_core.clone(),
            task_template_manager.clone(),
            circuit_breaker_health_provider.clone(), // Shared reference
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

        info!("TAS-77: Service instances created for web handlers");

        // TAS-150: Build SecurityService from worker auth config
        // Fail-fast: if auth is explicitly enabled but init fails, refuse to start.
        let security_service = if let Some(auth_config) = system_config
            .worker
            .as_ref()
            .and_then(|w| w.web.as_ref())
            .and_then(|web| web.auth.as_ref())
        {
            match tasker_shared::types::SecurityService::from_config(auth_config).await {
                Ok(svc) => Some(Arc::new(svc)),
                Err(e) => {
                    if auth_config.enabled {
                        tracing::error!(
                            error = %e,
                            "Worker SecurityService initialization failed with auth enabled. \
                             Refusing to start without authentication."
                        );
                        return Err(TaskerError::ConfigurationError(format!(
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

        Ok(Self {
            worker_core,
            database_pools,
            message_client,
            task_template_manager,
            event_router,
            in_process_bus,
            circuit_breaker_health_provider, // Shared reference with HealthService
            config,
            start_time,
            system_config,
            worker_id,
            health_service,
            metrics_service,
            template_query_service,
            config_query_service,
            security_service,
        })
    }

    /// TAS-75: Get the circuit breaker health status
    ///
    /// Returns the current health of all circuit breakers in the worker.
    /// If no provider is set, returns a default (empty) health status.
    pub async fn circuit_breakers_health(&self) -> CircuitBreakersHealth {
        let guard = self.circuit_breaker_health_provider.read().await;
        guard
            .as_ref()
            .map(|p| p.get_circuit_breakers_health())
            .unwrap_or_else(|| CircuitBreakersHealth {
                all_healthy: true,
                closed_count: 0,
                open_count: 0,
                half_open_count: 0,
                circuit_breakers: Vec::new(),
            })
    }

    /// TAS-75: Check if a circuit breaker health provider is configured
    pub async fn has_circuit_breaker_provider(&self) -> bool {
        self.circuit_breaker_health_provider.read().await.is_some()
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

    /// TAS-65: Get domain event statistics without locking the worker core
    ///
    /// Returns combined statistics from the EventRouter and InProcessEventBus.
    /// This method uses cached references to avoid locking the worker core mutex.
    pub async fn domain_event_stats(&self) -> DomainEventStats {
        // Get router stats
        let router_stats = if let Some(ref router) = self.event_router {
            let stats = router.get_statistics();
            WebEventRouterStats {
                total_routed: stats.total_routed,
                durable_routed: stats.durable_routed,
                fast_routed: stats.fast_routed,
                broadcast_routed: stats.broadcast_routed,
                fast_delivery_errors: stats.fast_delivery_errors,
                routing_errors: stats.routing_errors,
            }
        } else {
            WebEventRouterStats::default()
        };

        // Get in-process bus stats
        let bus_stats = {
            let bus = self.in_process_bus.read().await;
            let stats = bus.get_statistics();
            WebInProcessEventBusStats {
                total_events_dispatched: stats.total_events_dispatched,
                rust_handler_dispatches: stats.rust_handler_dispatches,
                ffi_channel_dispatches: stats.ffi_channel_dispatches,
                rust_handler_errors: stats.rust_handler_errors,
                ffi_channel_drops: stats.ffi_channel_drops,
                rust_subscriber_patterns: stats.rust_subscriber_patterns,
                rust_handler_count: stats.rust_handler_count,
                ffi_subscriber_count: stats.ffi_subscriber_count,
            }
        };

        DomainEventStats {
            router: router_stats,
            in_process_bus: bus_stats,
            captured_at: chrono::Utc::now(),
            worker_id: self.worker_id.clone(),
        }
    }

    // =========================================================================
    // TAS-77: Service Accessors
    // =========================================================================

    /// Get the health service for health check operations
    pub fn health_service(&self) -> &Arc<HealthService> {
        &self.health_service
    }

    /// Get the metrics service for metrics collection operations
    pub fn metrics_service(&self) -> &Arc<MetricsService> {
        &self.metrics_service
    }

    /// Get the template query service for template operations
    pub fn template_query_service(&self) -> &Arc<TemplateQueryService> {
        &self.template_query_service
    }

    /// Get the config query service for configuration operations
    pub fn config_query_service(&self) -> &Arc<ConfigQueryService> {
        &self.config_query_service
    }
}
