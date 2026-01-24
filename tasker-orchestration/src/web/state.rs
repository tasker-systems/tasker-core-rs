//! # Web API Application State
//!
//! Defines the shared state for the web API including database pools,
//! configuration, and circuit breaker health monitoring.

use crate::health::{BackpressureChecker, BackpressureMetrics, QueueDepthStatus};
use crate::orchestration::core::{OrchestrationCore, OrchestrationCoreStatus};
use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use crate::orchestration::lifecycle::task_initialization::TaskInitializer;
use crate::web::circuit_breaker::WebDatabaseCircuitBreaker;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::config::web::WebConfig;
use tasker_shared::types::SecurityService;
// TAS-75: Import ApiError for backpressure status checking
use tasker_shared::types::web::{ApiError, ApiResult, DbOperationType, SystemOperationalState};
use tasker_shared::TaskerResult;
use tokio::sync::RwLock;
use tracing::{debug, info};

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
#[derive(Clone, Debug)]
pub struct AppState {
    /// Web server configuration (V2 OrchestrationWebConfig)
    pub config: Arc<WebConfig>,

    /// Authentication configuration (converted from V2's `Option<AuthConfig>` to WebAuthConfig adapter)
    /// This provides route matching methods needed by the auth middleware
    pub auth_config: Option<Arc<tasker_shared::config::WebAuthConfig>>,

    /// TAS-150: Unified security service for JWT + API key authentication
    pub security_service: Option<Arc<SecurityService>>,

    /// Dedicated database pool for web API operations
    pub web_db_pool: PgPool,

    /// Shared orchestration database pool (for read operations)
    pub orchestration_db_pool: PgPool,

    /// Circuit breaker for web database health monitoring
    pub web_db_circuit_breaker: WebDatabaseCircuitBreaker,

    /// Shared task initializer component
    pub task_initializer: Arc<TaskInitializer>,

    /// Orchestration system operational status
    pub orchestration_status: Arc<RwLock<OrchestrationStatus>>,

    pub orchestration_core: Arc<OrchestrationCore>,
}

impl AppState {
    /// Create AppState from OrchestrationCore with dedicated web resources
    ///
    /// This method creates the web API's dedicated database pool while
    /// maintaining references to shared orchestration components.
    pub async fn from_orchestration_core(
        orchestration_core: Arc<OrchestrationCore>,
    ) -> ApiResult<Self> {
        info!("Creating web API application state with dedicated database pool");

        // TAS-61 V2: Access web config from orchestration context (must be present for web API)
        let web_config = orchestration_core
            .context
            .tasker_config
            .orchestration
            .as_ref()
            .and_then(|o| o.web.as_ref())
            .ok_or_else(|| {
                ApiError::internal_server_error(
                    "Web configuration not present in orchestration context",
                )
            })?
            .clone();

        // TAS-61 V2: Extract database URL from common configuration
        let database_url = orchestration_core
            .context
            .tasker_config
            .common
            .database_url();
        let pool_config = &web_config.database_pools;

        debug!(
            pool_size = pool_config.web_api_pool_size,
            max_connections = pool_config.web_api_max_connections,
            connection_timeout = pool_config.web_api_connection_timeout_seconds,
            idle_timeout = pool_config.web_api_idle_timeout_seconds,
            "Creating dedicated web API database pool"
        );

        let web_db_pool = PgPoolOptions::new()
            .max_connections(pool_config.web_api_max_connections)
            .min_connections(pool_config.web_api_pool_size / 2)
            .acquire_timeout(Duration::from_secs(
                pool_config.web_api_connection_timeout_seconds as u64,
            ))
            .idle_timeout(Duration::from_secs(
                pool_config.web_api_idle_timeout_seconds as u64,
            ))
            .test_before_acquire(true)
            .connect(&database_url)
            .await
            .map_err(|e| {
                ApiError::database_error(format!("Failed to create web database pool: {e}"))
            })?;

        // Create circuit breaker for web database health
        let circuit_breaker = if web_config
            .resilience
            .as_ref()
            .map(|r| r.circuit_breaker_enabled)
            .unwrap_or(false)
        {
            WebDatabaseCircuitBreaker::new(
                5,                       // failure_threshold
                Duration::from_secs(30), // recovery_timeout
                "web_database",          // component_name
            )
        } else {
            // Disabled circuit breaker (always closed)
            WebDatabaseCircuitBreaker::new(u32::MAX, Duration::from_secs(1), "disabled")
        };

        // Extract orchestration status from OrchestrationCore (TAS-40 simplified)
        let database_pool_size = orchestration_core.context.database_pool().size();
        // TAS-61 V2: Access environment from common.execution
        let environment = orchestration_core
            .context
            .tasker_config
            .common
            .execution
            .environment
            .clone();
        // Get initial status snapshot for logging (actual status is read dynamically via orchestration_core.status())
        let core_status_snapshot = orchestration_core.status().read().await.clone();

        // Convert OrchestrationCoreStatus to SystemOperationalState for initial snapshot
        let operational_state = match &core_status_snapshot {
            OrchestrationCoreStatus::Running => SystemOperationalState::Normal,
            OrchestrationCoreStatus::Starting => SystemOperationalState::Startup,
            OrchestrationCoreStatus::Stopping => SystemOperationalState::GracefulShutdown,
            OrchestrationCoreStatus::Stopped => SystemOperationalState::Stopped,
            OrchestrationCoreStatus::Error(_) => SystemOperationalState::Emergency,
            OrchestrationCoreStatus::Created => SystemOperationalState::Startup,
        };

        // Note: orchestration_status is kept for backward compatibility but health checks
        // should read directly from orchestration_core.status() for real-time status
        let orchestration_status = Arc::new(RwLock::new(OrchestrationStatus {
            running: matches!(core_status_snapshot, OrchestrationCoreStatus::Running),
            environment: environment.clone(),
            operational_state,
            database_pool_size,
            last_health_check: std::time::Instant::now(),
        }));

        let task_claim_step_enqueuer = StepEnqueuerService::new(orchestration_core.context.clone())
            .await
            .map_err(|e| {
                ApiError::database_error(format!("Failed to create TaskClaimStepEnqueuer: {e}"))
            })?;
        let task_claim_step_enqueuer = Arc::new(task_claim_step_enqueuer);

        // Create TaskInitializer with step enqueuer for immediate step enqueuing
        let task_initializer = Arc::new(TaskInitializer::new(
            orchestration_core.context.clone(),
            task_claim_step_enqueuer,
        ));

        info!(
            web_pool_size = pool_config.web_api_max_connections,
            orchestration_pool_size = database_pool_size,
            circuit_breaker_enabled = web_config.resilience.as_ref().map(|r| r.circuit_breaker_enabled).unwrap_or(false),
            environment = %environment,
            orchestration_status = ?orchestration_status,
            "Web API application state created successfully"
        );

        // TAS-61: Convert V2's Option<AuthConfig> to WebAuthConfig adapter for route matching
        let auth_config = web_config
            .auth
            .as_ref()
            .map(|auth| Arc::new(tasker_shared::config::WebAuthConfig::from(auth.clone())));

        // TAS-150: Build SecurityService from auth config
        let security_service = if let Some(auth) = &web_config.auth {
            match SecurityService::from_config(auth).await {
                Ok(svc) => Some(Arc::new(svc)),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to initialize SecurityService, auth disabled");
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            config: Arc::new(web_config),
            auth_config,
            security_service,
            web_db_pool: web_db_pool.clone(),
            orchestration_db_pool: orchestration_core.context.database_pool().clone(),
            web_db_circuit_breaker: circuit_breaker,
            task_initializer,
            orchestration_status,
            orchestration_core,
        })
    }

    /// Select the appropriate database pool based on operation type
    ///
    /// This implements the smart pool selection strategy:
    /// - Write operations use dedicated web pool
    /// - Read operations can use shared orchestration pool
    pub fn select_db_pool(&self, operation_type: DbOperationType) -> &PgPool {
        match operation_type {
            DbOperationType::WebWrite | DbOperationType::WebCritical => &self.web_db_pool,
            DbOperationType::ReadOnly | DbOperationType::Analytics => &self.orchestration_db_pool,
        }
    }

    /// Check if web database circuit breaker is healthy
    pub fn is_database_healthy(&self) -> bool {
        !self.web_db_circuit_breaker.is_circuit_open()
    }

    /// Record a successful database operation for circuit breaker
    pub fn record_database_success(&self) {
        self.web_db_circuit_breaker.record_success();
    }

    /// Record a failed database operation for circuit breaker
    pub fn record_database_failure(&self) {
        self.web_db_circuit_breaker.record_failure();
    }

    /// Update orchestration status (called periodically by health check)
    pub async fn update_orchestration_status(&self, new_status: OrchestrationStatus) {
        let mut status = self.orchestration_status.write().await;
        *status = new_status;
    }

    /// Get current orchestration operational state
    pub async fn operational_state(&self) -> SystemOperationalState {
        self.orchestration_status
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
        let web_pool = &self.web_db_pool;
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
            .orchestration_core
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
            .orchestration_core
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
            .orchestration_core
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
        self.orchestration_core
            .backpressure_checker()
            .try_check_backpressure()
    }

    /// TAS-75: Get queue depth status (synchronous, cache-based)
    ///
    /// Returns the cached queue depth status from the background evaluator.
    /// This is the fix for the "cardinal sin" - now returns real data instead of None.
    pub fn try_get_queue_depth_status(&self) -> QueueDepthStatus {
        // TAS-75 Phase 5: Delegate to BackpressureChecker (cache-based)
        self.orchestration_core
            .backpressure_checker()
            .try_get_queue_depth_status()
    }

    /// TAS-75: Get the backpressure checker for advanced use cases
    pub fn backpressure_checker(&self) -> &BackpressureChecker {
        self.orchestration_core.backpressure_checker()
    }

    /// TAS-75: Get detailed backpressure metrics for health endpoint
    ///
    /// This delegates to the `BackpressureChecker` which aggregates metrics from caches.
    pub async fn get_backpressure_metrics(&self) -> BackpressureMetrics {
        // TAS-75 Phase 5: Delegate to BackpressureChecker
        self.orchestration_core
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
