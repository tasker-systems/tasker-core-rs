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
    /// Web server configuration
    pub config: Arc<WebConfig>,

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

        let web_config = orchestration_core
            .context
            .tasker_config
            .orchestration
            .web
            .clone();
        // Extract database URL from orchestration configuration
        let database_url = orchestration_core.context.tasker_config.database_url();
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
                pool_config.web_api_connection_timeout_seconds,
            ))
            .idle_timeout(Duration::from_secs(
                pool_config.web_api_idle_timeout_seconds,
            ))
            .test_before_acquire(true)
            .connect(&database_url)
            .await
            .map_err(|e| {
                ApiError::database_error(format!("Failed to create web database pool: {e}"))
            })?;

        // Create circuit breaker for web database health
        let circuit_breaker = if web_config.resilience.circuit_breaker_enabled {
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
        let environment = orchestration_core
            .context
            .tasker_config
            .environment()
            .to_string();
        let core_status = orchestration_core.status();

        // Convert OrchestrationCoreStatus to SystemOperationalState
        let operational_state = match core_status {
            OrchestrationCoreStatus::Running => SystemOperationalState::Normal,
            OrchestrationCoreStatus::Starting => SystemOperationalState::Startup,
            OrchestrationCoreStatus::Stopping => SystemOperationalState::GracefulShutdown,
            OrchestrationCoreStatus::Stopped => SystemOperationalState::Stopped,
            OrchestrationCoreStatus::Error(_) => SystemOperationalState::Emergency,
            OrchestrationCoreStatus::Created => SystemOperationalState::Startup,
        };

        let orchestration_status = Arc::new(RwLock::new(OrchestrationStatus {
            running: matches!(core_status, OrchestrationCoreStatus::Running),
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
            circuit_breaker_enabled = web_config.resilience.circuit_breaker_enabled,
            environment = %environment,
            orchestration_status = ?orchestration_status,
            "Web API application state created successfully"
        );

        Ok(Self {
            config: Arc::new(web_config),
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
}
