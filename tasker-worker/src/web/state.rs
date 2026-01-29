//! Worker Web Application State
//!
//! Contains shared state for the worker web API including database connections,
//! configuration, and metrics tracking.
//!
//! TAS-177: Refactored to use `SharedWorkerServices` for service sharing with gRPC.

use crate::worker::services::{
    ConfigQueryService, HealthService, MetricsService, SharedCircuitBreakerProvider,
    SharedWorkerServices, TemplateQueryService,
};
use crate::worker::task_template_manager::TaskTemplateManager;
use serde::Serialize;
use std::{sync::Arc, time::Instant};
use tasker_shared::{
    config::tasker::TaskerConfig,
    database::DatabasePools,
    messaging::client::MessageClient,
    types::api::worker::CircuitBreakersHealth,
    types::base::CacheStats,
    types::web::{
        DomainEventStats, EventRouterStats as WebEventRouterStats,
        InProcessEventBusStats as WebInProcessEventBusStats,
    },
};
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
///
/// TAS-177: Now wraps `SharedWorkerServices` for service sharing with gRPC.
#[derive(Clone, Debug)]
pub struct WorkerWebState {
    /// TAS-177: Shared services (also used by gRPC)
    pub shared_services: Arc<SharedWorkerServices>,

    /// Web API configuration
    pub config: WorkerWebConfig,
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
    /// Create new worker web state from shared services
    ///
    /// # Arguments
    /// * `shared_services` - Shared services created from WorkerCore
    /// * `config` - Web API configuration
    pub fn new(shared_services: Arc<SharedWorkerServices>, config: WorkerWebConfig) -> Self {
        info!(
            enabled = config.enabled,
            bind_address = %config.bind_address,
            "Initializing worker web state from shared services"
        );

        Self {
            shared_services,
            config,
        }
    }

    // =========================================================================
    // Delegated accessors to SharedWorkerServices
    // =========================================================================

    /// Worker core reference for health checks and status
    pub fn worker_core(&self) -> &Arc<tokio::sync::Mutex<crate::worker::core::WorkerCore>> {
        &self.shared_services.worker_core
    }

    /// TAS-164: Database pools for Tasker and PGMQ operations
    pub fn database_pools(&self) -> &DatabasePools {
        &self.shared_services.database_pools
    }

    /// TAS-133e: Message client for queue metrics and monitoring (provider-agnostic)
    pub fn message_client(&self) -> &Arc<MessageClient> {
        &self.shared_services.message_client
    }

    /// Task template manager for template caching and validation
    pub fn task_template_manager(&self) -> &Arc<TaskTemplateManager> {
        &self.shared_services.task_template_manager
    }

    /// TAS-75: Circuit breaker health provider (injected from FFI layer)
    pub fn circuit_breaker_health_provider(&self) -> &SharedCircuitBreakerProvider {
        &self.shared_services.circuit_breaker_health_provider
    }

    /// Application start time for uptime calculations
    pub fn start_time(&self) -> Instant {
        self.shared_services.start_time
    }

    /// Worker system configuration
    pub fn system_config(&self) -> &TaskerConfig {
        &self.shared_services.system_config
    }

    /// Cached worker ID (to avoid locking mutex on every status check)
    pub fn worker_id(&self) -> String {
        self.shared_services.worker_id.clone()
    }

    /// Security service for authentication
    pub fn security_service(&self) -> Option<&Arc<tasker_shared::types::SecurityService>> {
        self.shared_services.security_service.as_ref()
    }

    // =========================================================================
    // TAS-77: Service Accessors
    // =========================================================================

    /// Get the health service for health check operations
    pub fn health_service(&self) -> &Arc<HealthService> {
        &self.shared_services.health_service
    }

    /// Get the metrics service for metrics collection operations
    pub fn metrics_service(&self) -> &Arc<MetricsService> {
        &self.shared_services.metrics_service
    }

    /// Get the template query service for template operations
    pub fn template_query_service(&self) -> &Arc<TemplateQueryService> {
        &self.shared_services.template_query_service
    }

    /// Get the config query service for configuration operations
    pub fn config_query_service(&self) -> &Arc<ConfigQueryService> {
        &self.shared_services.config_query_service
    }

    // =========================================================================
    // Convenience methods
    // =========================================================================

    /// TAS-75: Get the circuit breaker health status
    ///
    /// Returns the current health of all circuit breakers in the worker.
    /// If no provider is set, returns a default (empty) health status.
    pub async fn circuit_breakers_health(&self) -> CircuitBreakersHealth {
        let guard = self
            .shared_services
            .circuit_breaker_health_provider
            .read()
            .await;
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
        self.shared_services
            .circuit_breaker_health_provider
            .read()
            .await
            .is_some()
    }

    /// Get the uptime in seconds since the worker started
    pub fn uptime_seconds(&self) -> u64 {
        self.shared_services.start_time.elapsed().as_secs()
    }

    /// Check if metrics collection is enabled
    pub fn metrics_enabled(&self) -> bool {
        self.config.metrics_enabled
    }

    /// Check if authentication is required
    pub fn authentication_enabled(&self) -> bool {
        self.config.authentication_enabled
    }

    /// Get worker type classification
    pub fn worker_type(&self) -> String {
        "command_processor".to_string()
    }

    /// Get supported namespaces for this worker
    pub async fn supported_namespaces(&self) -> Vec<String> {
        self.shared_services
            .task_template_manager
            .supported_namespaces()
            .await
    }

    /// Get queue name for a namespace
    pub fn queue_name_for_namespace(&self, namespace: &str) -> String {
        format!("{}_queue", namespace)
    }

    /// Check if a namespace is supported by this worker
    pub async fn is_namespace_supported(&self, namespace: &str) -> bool {
        self.shared_services
            .task_template_manager
            .is_namespace_supported(namespace)
            .await
    }

    /// Get task template manager statistics
    pub async fn template_cache_stats(&self) -> CacheStats {
        self.shared_services
            .task_template_manager
            .cache_stats()
            .await
    }

    /// Perform cache maintenance on task templates
    pub async fn maintain_template_cache(&self) {
        self.shared_services
            .task_template_manager
            .maintain_cache()
            .await;
    }

    /// TAS-65: Get domain event statistics without locking the worker core
    ///
    /// Returns combined statistics from the EventRouter and InProcessEventBus.
    /// This method uses cached references to avoid locking the worker core mutex.
    pub async fn domain_event_stats(&self) -> DomainEventStats {
        // Get router stats
        let router_stats = if let Some(ref router) = self.shared_services.event_router {
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
            let bus = self.shared_services.in_process_bus.read().await;
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
            worker_id: self.shared_services.worker_id.clone(),
        }
    }
}
