//! # Web API Application State
//!
//! Defines the shared state for the web API including database pools,
//! configuration, and circuit breaker health monitoring.

use crate::orchestration::core::{OrchestrationCore, OrchestrationCoreStatus};
use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use crate::orchestration::lifecycle::task_initialization::TaskInitializer;
use crate::web::circuit_breaker::WebDatabaseCircuitBreaker;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::config::web::WebConfig;
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

        Ok(Self {
            config: Arc::new(web_config),
            auth_config,
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
    // TAS-75: Backpressure Status Checking
    // ==========================================================================

    /// TAS-75: Check command channel saturation level
    ///
    /// Returns the saturation percentage (0.0-100.0) of the orchestration command channel.
    /// This is used for backpressure detection and 503 response decisions.
    pub fn get_command_channel_saturation(&self) -> f64 {
        let channel_monitor = self.orchestration_core.command_channel_monitor();
        let sender = self.orchestration_core.command_sender();
        let available_capacity = sender.capacity();

        channel_monitor.calculate_saturation(available_capacity) * 100.0
    }

    /// TAS-75: Check if command channel is under backpressure (>80% saturated)
    pub fn is_command_channel_saturated(&self) -> bool {
        self.get_command_channel_saturation() >= 80.0
    }

    /// TAS-75: Check if command channel is critically saturated (>95%)
    pub fn is_command_channel_critical(&self) -> bool {
        self.get_command_channel_saturation() >= 95.0
    }

    /// TAS-75: Get backpressure status for API operations
    ///
    /// Returns `Some(ApiError)` if backpressure is active, `None` if system is healthy.
    /// Checks in order of severity:
    /// 1. Circuit breaker open
    /// 2. Command channel critical saturation (>95%)
    /// 3. Command channel degraded saturation (>80%)
    ///
    /// When backpressure is detected, returns `ApiError::Backpressure` with
    /// appropriate Retry-After value based on severity.
    pub fn check_backpressure_status(&self) -> Option<ApiError> {
        // Check 1: Circuit breaker open (most severe)
        if self.web_db_circuit_breaker.is_circuit_open() {
            // Circuit breaker recovery timeout is 30s by default
            return Some(ApiError::circuit_breaker_with_retry(30));
        }

        // Check 2: Command channel saturation
        let saturation = self.get_command_channel_saturation();
        if saturation >= 95.0 {
            // Critical saturation - suggest longer wait
            return Some(ApiError::channel_saturated(
                "orchestration_command",
                saturation,
            ));
        } else if saturation >= 80.0 {
            // Degraded saturation - suggest shorter wait
            return Some(ApiError::channel_saturated(
                "orchestration_command",
                saturation,
            ));
        }

        // System is healthy
        None
    }

    /// TAS-75: Get detailed backpressure metrics for health endpoint
    pub fn get_backpressure_metrics(&self) -> BackpressureMetrics {
        let channel_monitor = self.orchestration_core.command_channel_monitor();
        let sender = self.orchestration_core.command_sender();
        let available_capacity = sender.capacity();
        let channel_metrics = channel_monitor.metrics();

        BackpressureMetrics {
            circuit_breaker_open: self.web_db_circuit_breaker.is_circuit_open(),
            circuit_breaker_failures: self.web_db_circuit_breaker.current_failures(),
            command_channel_saturation_percent: self.get_command_channel_saturation(),
            command_channel_available_capacity: available_capacity,
            command_channel_messages_sent: channel_metrics.messages_sent,
            command_channel_overflow_events: channel_metrics.overflow_events,
            backpressure_active: self.check_backpressure_status().is_some(),
        }
    }
}

/// TAS-75: Backpressure metrics for health endpoint and monitoring
#[derive(Debug, Clone)]
pub struct BackpressureMetrics {
    /// Whether the web database circuit breaker is open
    pub circuit_breaker_open: bool,
    /// Current failure count on circuit breaker
    pub circuit_breaker_failures: u32,
    /// Command channel saturation percentage (0-100)
    pub command_channel_saturation_percent: f64,
    /// Available slots in command channel
    pub command_channel_available_capacity: usize,
    /// Total messages sent through command channel
    pub command_channel_messages_sent: u64,
    /// Total overflow events on command channel
    pub command_channel_overflow_events: u64,
    /// Whether any backpressure condition is active
    pub backpressure_active: bool,
}
