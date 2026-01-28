//! # Web API Application State
//!
//! Defines the shared state for the web API including database pools,
//! configuration, and circuit breaker health monitoring.

use crate::health::{BackpressureChecker, BackpressureMetrics, QueueDepthStatus};
use crate::orchestration::core::OrchestrationCore;
use crate::services::{
    AnalyticsService, HealthService, SharedApiServices, StepService, TaskService,
    TemplateQueryService,
};
use crate::web::circuit_breaker::WebDatabaseCircuitBreaker;
use sqlx::PgPool;
use std::sync::Arc;
use tasker_shared::config::web::WebConfig;
use tasker_shared::types::SecurityService;
// TAS-75: Import ApiError for backpressure status checking
use tasker_shared::types::web::{ApiError, DbOperationType, SystemOperationalState};
use tasker_shared::TaskerResult;
use tokio::sync::RwLock;
use tracing::info;

/// Database pool usage statistics for monitoring (TAS-37 Web Integration)
#[derive(Debug, Clone)]
pub struct DatabasePoolUsageStats {
    pub pool_name: String,
    pub active_connections: u32,
    pub max_connections: u32,
    pub usage_ratio: f64,
    pub is_healthy: bool,
}

/// Operational status tracking for web API integration
#[derive(Debug, Clone)]
pub struct OrchestrationStatus {
    pub running: bool,
    pub environment: String,
    pub operational_state: SystemOperationalState,
    pub database_pool_size: u32,
    pub last_health_check: std::time::Instant,
}

/// Shared application state for the web API
///
/// This state is shared across all request handlers and contains:
/// - Dedicated database pool for web operations
/// - Shared orchestration database pool reference
/// - Circuit breaker for web database health
/// - Configuration and orchestration status
///
/// TAS-177: AppState now wraps `SharedApiServices` for services that are shared
/// with the gRPC API (when both are enabled).
#[derive(Clone, Debug)]
pub struct AppState {
    /// Web server configuration (V2 OrchestrationWebConfig)
    pub config: Arc<WebConfig>,

    /// Authentication configuration (converted from V2's `Option<AuthConfig>` to WebAuthConfig adapter)
    /// This provides route matching methods needed by the auth middleware
    pub auth_config: Option<Arc<tasker_shared::config::WebAuthConfig>>,

    /// TAS-177: Shared services (same instances used by gRPC API when enabled)
    pub services: Arc<SharedApiServices>,
}

impl AppState {
    /// Create AppState from SharedApiServices.
    ///
    /// TAS-177: This is the recommended constructor when both REST and gRPC APIs are enabled.
    /// The services are created once by SharedApiServices and shared between both APIs.
    pub fn new(services: Arc<SharedApiServices>, web_config: WebConfig) -> Self {
        // Convert auth config for route matching
        let auth_config = web_config
            .auth
            .as_ref()
            .map(|auth| Arc::new(tasker_shared::config::WebAuthConfig::from(auth.clone())));

        info!(
            auth_enabled = services.is_auth_enabled(),
            "Web API application state created from shared services"
        );

        Self {
            config: Arc::new(web_config),
            auth_config,
            services,
        }
    }

    // ==========================================================================
    // Convenience accessors for shared services
    // ==========================================================================

    /// TAS-150: Unified security service for JWT + API key authentication
    pub fn security_service(&self) -> Option<&Arc<SecurityService>> {
        self.services.security_service.as_ref()
    }

    /// Dedicated database pool for web API operations (write pool)
    pub fn web_db_pool(&self) -> &PgPool {
        &self.services.write_pool
    }

    /// Shared orchestration database pool (for read operations)
    pub fn orchestration_db_pool(&self) -> &PgPool {
        &self.services.read_pool
    }

    /// Circuit breaker for web database health monitoring
    pub fn web_db_circuit_breaker(&self) -> &WebDatabaseCircuitBreaker {
        &self.services.circuit_breaker
    }

    /// Orchestration core reference
    pub fn orchestration_core(&self) -> &Arc<OrchestrationCore> {
        &self.services.orchestration_core
    }

    /// TAS-168: Analytics service with response caching
    pub fn analytics_service(&self) -> &Arc<AnalyticsService> {
        &self.services.analytics_service
    }

    /// TAS-76: Task service for task business logic
    pub fn task_service(&self) -> &TaskService {
        &self.services.task_service
    }

    /// TAS-76: Step service for step business logic
    pub fn step_service(&self) -> &StepService {
        &self.services.step_service
    }

    /// TAS-76: Health service for health check logic
    pub fn health_service(&self) -> &HealthService {
        &self.services.health_service
    }

    /// TAS-76: Template query service for template discovery
    pub fn template_query_service(&self) -> &TemplateQueryService {
        &self.services.template_query_service
    }

    /// Orchestration system operational status
    pub fn orchestration_status(&self) -> &Arc<RwLock<OrchestrationStatus>> {
        &self.services.orchestration_status
    }

    /// Select the appropriate database pool based on operation type
    ///
    /// This implements the smart pool selection strategy:
    /// - Write operations use dedicated web pool
    /// - Read operations can use shared orchestration pool
    pub fn select_db_pool(&self, operation_type: DbOperationType) -> &PgPool {
        match operation_type {
            DbOperationType::WebWrite | DbOperationType::WebCritical => &self.services.write_pool,
            DbOperationType::ReadOnly | DbOperationType::Analytics => &self.services.read_pool,
        }
    }

    /// Check if web database circuit breaker is healthy
    pub fn is_database_healthy(&self) -> bool {
        !self.services.circuit_breaker.is_circuit_open()
    }

    /// Record a successful database operation for circuit breaker
    pub fn record_database_success(&self) {
        self.services.circuit_breaker.record_success();
    }

    /// Record a failed database operation for circuit breaker
    pub fn record_database_failure(&self) {
        self.services.circuit_breaker.record_failure();
    }

    /// Update orchestration status (called periodically by health check)
    pub async fn update_orchestration_status(&self, new_status: OrchestrationStatus) {
        let mut status = self.services.orchestration_status.write().await;
        *status = new_status;
    }

    /// Get current orchestration operational state
    pub async fn operational_state(&self) -> SystemOperationalState {
        self.services
            .orchestration_status
            .read()
            .await
            .operational_state
            .clone()
    }

    /// Report web API database pool usage to health monitoring system (TAS-40 Web Integration)
    ///
    /// This method creates a pool usage report for external health monitoring.
    /// Uses TAS-40 command pattern architecture with simplified operational state.
    pub async fn report_pool_usage_stats(&self) -> TaskerResult<DatabasePoolUsageStats> {
        // Get current pool usage statistics (simplified for TAS-40)
        Ok(self.get_pool_usage_stats())
    }

    /// Get web API database pool usage statistics for external monitoring
    pub fn get_pool_usage_stats(&self) -> DatabasePoolUsageStats {
        let web_pool = &self.services.write_pool;
        let current_size = web_pool.size();
        let max_size = self.config.database_pools.web_api_max_connections;

        let usage_ratio = if max_size > 0 {
            current_size as f64 / max_size as f64
        } else {
            0.0
        };

        DatabasePoolUsageStats {
            pool_name: "web_api_pool".to_string(),
            active_connections: current_size,
            max_connections: max_size,
            usage_ratio,
            is_healthy: usage_ratio <= 0.75, // Using default warning threshold
        }
    }

    // ==========================================================================
    // TAS-75: Backpressure Status Checking (Cache-Based)
    // ==========================================================================

    /// TAS-75 Phase 5: Check command channel saturation level (cached)
    ///
    /// Returns the saturation percentage (0.0-100.0) of the orchestration command channel.
    /// This is used for backpressure detection and 503 response decisions.
    ///
    /// **Note**: This now reads from the background-updated cache rather than
    /// calculating inline. If the cache hasn't been evaluated, returns 0.0.
    pub fn get_command_channel_saturation(&self) -> f64 {
        // TAS-75 Phase 5: Use cached channel status from BackpressureChecker
        let channel_status = self
            .orchestration_core()
            .backpressure_checker()
            .try_get_channel_status();

        // If not evaluated yet, return 0.0 (fail-open)
        if !channel_status.evaluated {
            return 0.0;
        }

        channel_status.command_saturation_percent
    }

    /// TAS-75 Phase 5: Check if command channel is under backpressure (cached)
    ///
    /// Returns true if saturation >= 80%. Uses pre-computed flag from cache.
    pub fn is_command_channel_saturated(&self) -> bool {
        // TAS-75 Phase 5: Use cached is_saturated flag
        let channel_status = self
            .orchestration_core()
            .backpressure_checker()
            .try_get_channel_status();

        // If not evaluated, return false (fail-open)
        channel_status.evaluated && channel_status.is_saturated
    }

    /// TAS-75 Phase 5: Check if command channel is critically saturated (cached)
    ///
    /// Returns true if saturation >= 95%. Uses pre-computed flag from cache.
    pub fn is_command_channel_critical(&self) -> bool {
        // TAS-75 Phase 5: Use cached is_critical flag
        let channel_status = self
            .orchestration_core()
            .backpressure_checker()
            .try_get_channel_status();

        // If not evaluated, return false (fail-open)
        channel_status.evaluated && channel_status.is_critical
    }

    /// TAS-75: Get backpressure status for API operations
    ///
    /// Returns `Some(ApiError)` if backpressure is active, `None` if system is healthy.
    /// This method delegates to the `BackpressureChecker` which reads from caches
    /// updated by the background `StatusEvaluator`.
    ///
    /// This is a synchronous, non-blocking method - no database queries in the hot path.
    ///
    /// # Returns
    /// - `Some(ApiError::Backpressure { ... })` if backpressure is active
    /// - `None` if the system is healthy and can accept requests
    pub fn check_backpressure_status(&self) -> Option<ApiError> {
        // TAS-75 Phase 5: Delegate to BackpressureChecker (cache-based)
        self.services
            .orchestration_core
            .backpressure_checker()
            .try_check_backpressure()
    }

    /// TAS-75: Get queue depth status (synchronous, cache-based)
    ///
    /// Returns the cached queue depth status from the background evaluator.
    /// This is the fix for the "cardinal sin" - now returns real data instead of None.
    pub fn try_get_queue_depth_status(&self) -> QueueDepthStatus {
        // TAS-75 Phase 5: Delegate to BackpressureChecker (cache-based)
        self.services
            .orchestration_core
            .backpressure_checker()
            .try_get_queue_depth_status()
    }

    /// TAS-75: Get the backpressure checker for advanced use cases
    pub fn backpressure_checker(&self) -> &BackpressureChecker {
        self.services.orchestration_core.backpressure_checker()
    }

    /// TAS-75: Get detailed backpressure metrics for health endpoint
    ///
    /// This delegates to the `BackpressureChecker` which aggregates metrics from caches.
    pub async fn get_backpressure_metrics(&self) -> BackpressureMetrics {
        // TAS-75 Phase 5: Delegate to BackpressureChecker
        self.services
            .orchestration_core
            .backpressure_checker()
            .get_backpressure_metrics()
            .await
    }

    // ==========================================================================
    // TAS-75 Phase 5: Queue Depth Monitoring (Cache-Based Preferred)
    // ==========================================================================

    /// TAS-75 Phase 5: Check queue depth status (cached, preferred)
    ///
    /// Returns the cached queue depth status from the background evaluator.
    /// This is the recommended method for API hot-path operations.
    ///
    /// **Note**: This delegates to `try_get_queue_depth_status()` for consistency.
    /// The async signature is maintained for backward compatibility but the
    /// implementation is synchronous (cache read).
    pub async fn check_queue_depth_status(&self) -> QueueDepthStatus {
        // TAS-75 Phase 5: Delegate to cached version
        self.try_get_queue_depth_status()
    }
}
