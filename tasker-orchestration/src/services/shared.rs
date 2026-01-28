//! Shared API services for both REST and gRPC APIs.
//!
//! `SharedApiServices` provides a single point of construction for all services
//! used by both the REST and gRPC APIs. This ensures:
//! - Services are created once, not duplicated
//! - Database pools are shared efficiently
//! - Both APIs have consistent behavior

use crate::orchestration::core::OrchestrationCoreStatus;
use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use crate::orchestration::lifecycle::task_initialization::TaskInitializer;
use crate::orchestration::OrchestrationCore;
use crate::services::{
    AnalyticsService, HealthService, StepService, TaskService, TemplateQueryService,
};
use crate::web::circuit_breaker::WebDatabaseCircuitBreaker;
use crate::web::state::OrchestrationStatus;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::types::web::SystemOperationalState;
use tasker_shared::types::SecurityService;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Error type for SharedApiServices initialization
#[derive(Debug, thiserror::Error)]
pub enum SharedApiServicesError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Security initialization failed: {0}")]
    Security(String),
    #[error("Service initialization failed: {0}")]
    Service(String),
}

/// Shared services for REST and gRPC APIs.
///
/// This struct holds all services that are common between the REST and gRPC APIs.
/// It is created once from `OrchestrationCore` and shared via `Arc` by both
/// `AppState` (REST) and `GrpcState` (gRPC).
#[derive(Clone, Debug)]
pub struct SharedApiServices {
    /// Security service for JWT/API key authentication
    pub security_service: Option<Arc<SecurityService>>,

    /// Database pool for write operations (dedicated API pool)
    pub write_pool: PgPool,

    /// Database pool for read operations (shared orchestration pool)
    pub read_pool: PgPool,

    /// Circuit breaker for database health monitoring
    pub circuit_breaker: WebDatabaseCircuitBreaker,

    /// Task service for task operations
    pub task_service: TaskService,

    /// Step service for step operations
    pub step_service: StepService,

    /// Health service for health checks
    pub health_service: HealthService,

    /// Template query service for template discovery
    pub template_query_service: TemplateQueryService,

    /// Analytics service for performance metrics
    pub analytics_service: Arc<AnalyticsService>,

    /// Orchestration core for backpressure checking and status
    pub orchestration_core: Arc<OrchestrationCore>,

    /// Task initializer for creating new tasks
    pub task_initializer: Arc<TaskInitializer>,

    /// Orchestration status for health reporting
    pub orchestration_status: Arc<RwLock<OrchestrationStatus>>,
}

impl SharedApiServices {
    /// Create shared API services from OrchestrationCore.
    ///
    /// This creates a dedicated database pool for API write operations and
    /// initializes all services needed by both REST and gRPC APIs.
    pub async fn from_orchestration_core(
        orchestration_core: Arc<OrchestrationCore>,
    ) -> Result<Self, SharedApiServicesError> {
        info!("Creating shared API services");

        // Get database URL from common configuration
        let database_url = orchestration_core
            .context
            .tasker_config
            .common
            .database_url();

        // Get pool settings from web config if available, otherwise use defaults
        let (
            max_connections,
            min_connections,
            connection_timeout,
            idle_timeout,
            circuit_breaker_enabled,
        ) = orchestration_core
            .context
            .tasker_config
            .orchestration
            .as_ref()
            .and_then(|o| o.web.as_ref())
            .map(|web| {
                let cb_enabled = web
                    .resilience
                    .as_ref()
                    .map(|r| r.circuit_breaker_enabled)
                    .unwrap_or(false);
                (
                    web.database_pools.web_api_max_connections,
                    web.database_pools.web_api_pool_size / 2,
                    web.database_pools.web_api_connection_timeout_seconds as u64,
                    web.database_pools.web_api_idle_timeout_seconds as u64,
                    cb_enabled,
                )
            })
            .unwrap_or((30, 15, 30, 300, false));

        debug!(
            max_connections = max_connections,
            min_connections = min_connections,
            connection_timeout = connection_timeout,
            idle_timeout = idle_timeout,
            circuit_breaker_enabled = circuit_breaker_enabled,
            "Creating dedicated API database pool"
        );

        // Create dedicated database pool for API write operations
        let write_pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(Duration::from_secs(connection_timeout))
            .idle_timeout(Duration::from_secs(idle_timeout))
            .test_before_acquire(true)
            .connect(&database_url)
            .await
            .map_err(|e| {
                SharedApiServicesError::Database(format!("Failed to create API database pool: {e}"))
            })?;

        let read_pool = orchestration_core.context.database_pool().clone();

        // Create circuit breaker
        let circuit_breaker = if circuit_breaker_enabled {
            WebDatabaseCircuitBreaker::new(
                5,                       // failure_threshold
                Duration::from_secs(30), // recovery_timeout
                "api_database",
            )
        } else {
            // Disabled circuit breaker (always closed)
            WebDatabaseCircuitBreaker::new(u32::MAX, Duration::from_secs(1), "disabled")
        };

        // Get environment and create orchestration status
        let environment = orchestration_core
            .context
            .tasker_config
            .common
            .execution
            .environment
            .clone();
        let database_pool_size = read_pool.size();
        let core_status_snapshot = orchestration_core.status().read().await.clone();

        let operational_state = match &core_status_snapshot {
            OrchestrationCoreStatus::Running => SystemOperationalState::Normal,
            OrchestrationCoreStatus::Starting => SystemOperationalState::Startup,
            OrchestrationCoreStatus::Stopping => SystemOperationalState::GracefulShutdown,
            OrchestrationCoreStatus::Stopped => SystemOperationalState::Stopped,
            OrchestrationCoreStatus::Error(_) => SystemOperationalState::Emergency,
            OrchestrationCoreStatus::Created => SystemOperationalState::Startup,
        };

        let orchestration_status = Arc::new(RwLock::new(OrchestrationStatus {
            running: matches!(core_status_snapshot, OrchestrationCoreStatus::Running),
            environment,
            operational_state,
            database_pool_size,
            last_health_check: std::time::Instant::now(),
        }));

        // Create step enqueuer service
        let step_enqueuer = StepEnqueuerService::new(orchestration_core.context.clone())
            .await
            .map_err(|e| {
                SharedApiServicesError::Service(format!(
                    "Failed to create StepEnqueuerService: {e}"
                ))
            })?;
        let step_enqueuer = Arc::new(step_enqueuer);

        // Create task initializer
        let task_initializer = Arc::new(TaskInitializer::new(
            orchestration_core.context.clone(),
            step_enqueuer,
        ));

        // Build SecurityService from auth config (if present)
        let security_service = if let Some(auth) = orchestration_core
            .context
            .tasker_config
            .orchestration
            .as_ref()
            .and_then(|o| o.web.as_ref())
            .and_then(|w| w.auth.as_ref())
        {
            match SecurityService::from_config(auth).await {
                Ok(svc) => Some(Arc::new(svc)),
                Err(e) => {
                    if auth.enabled {
                        return Err(SharedApiServicesError::Security(format!(
                            "SecurityService init failed with auth enabled: {e}"
                        )));
                    }
                    debug!(error = %e, "SecurityService init skipped (auth disabled)");
                    None
                }
            }
        } else {
            None
        };

        // Create services
        let analytics_service = Arc::new(AnalyticsService::new(
            read_pool.clone(),
            orchestration_core.context.cache_provider.clone(),
            orchestration_core
                .context
                .tasker_config
                .common
                .cache
                .clone(),
        ));

        let task_service = TaskService::new(
            read_pool.clone(),
            write_pool.clone(),
            task_initializer.clone(),
            orchestration_core.context.clone(),
        );

        let step_service = StepService::new(
            read_pool.clone(),
            write_pool.clone(),
            orchestration_core.context.clone(),
        );

        let health_service = HealthService::new(
            write_pool.clone(),
            read_pool.clone(),
            circuit_breaker.clone(),
            orchestration_core.clone(),
            orchestration_status.clone(),
        );

        let template_query_service = TemplateQueryService::new(orchestration_core.context.clone());

        info!(
            write_pool_size = max_connections,
            read_pool_size = database_pool_size,
            circuit_breaker_enabled = circuit_breaker_enabled,
            auth_enabled = security_service.is_some(),
            "Shared API services created successfully"
        );

        Ok(Self {
            security_service,
            write_pool,
            read_pool,
            circuit_breaker,
            task_service,
            step_service,
            health_service,
            template_query_service,
            analytics_service,
            orchestration_core,
            task_initializer,
            orchestration_status,
        })
    }

    /// Check if authentication is enabled
    pub fn is_auth_enabled(&self) -> bool {
        self.security_service
            .as_ref()
            .map(|s| s.is_enabled())
            .unwrap_or(false)
    }

    /// Check backpressure status
    pub fn check_backpressure(&self) -> Option<String> {
        self.orchestration_core
            .backpressure_checker()
            .try_check_backpressure()
            .map(|e| e.to_string())
    }
}
