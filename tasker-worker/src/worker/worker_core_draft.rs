//! # WorkerCore
//!
//! Worker-focused coordinator that manages processing executors for distributed
//! task execution. Implements the same CoordinatorCore pattern as OrchestrationCore
//! but optimized for worker-specific operations.

use crate::orchestration::{
    coordinator_core::CoordinatorCore,
    TaskInitializationResult,
};
use sqlx::PgPool;
use std::sync::Arc;
use tasker_shared::config::{CircuitBreakerConfig, ConfigManager};
use tasker_shared::coordinator::operational_state::{
    OperationalStateManager, SystemOperationalState,
};
use tasker_shared::executor::ProcessingExecutor;
use tasker_shared::messaging::{
    PgmqClient, PgmqClientTrait, ProtectedPgmqClient, UnifiedPgmqClient,
};
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::resilience::CircuitBreakerManager;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::info;
use uuid::Uuid;

/// Worker-focused orchestration core for distributed task execution
pub struct WorkerCore {
    /// Unified PGMQ client (either standard or circuit-breaker protected)
    pub pgmq_client: Arc<UnifiedPgmqClient>,

    /// Database connection pool
    pub database_pool: PgPool,

    /// Processing executors implementing the ProcessingExecutor trait
    pub processing_executors: Vec<Arc<dyn ProcessingExecutor>>,

    /// Task handler registry
    pub task_handler_registry: Arc<TaskHandlerRegistry>,

    /// Circuit breaker manager (optional, only when circuit breakers enabled)
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,

    /// Operational state manager for shutdown-aware health monitoring
    pub operational_state_manager: OperationalStateManager,
}

impl std::fmt::Debug for WorkerCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerCore")
            .field("pgmq_client", &"Arc<UnifiedPgmqClient>")
            .field(
                "database_pool",
                &format!("PgPool(size={})", self.database_pool.size()),
            )
            .field(
                "processing_executors",
                &format!("Vec<Arc<dyn ProcessingExecutor>>(len={})", self.processing_executors.len()),
            )
            .field("task_handler_registry", &"Arc<TaskHandlerRegistry>")
            .field(
                "circuit_breaker_manager",
                &self
                    .circuit_breaker_manager
                    .as_ref()
                    .map(|_| "Some(Arc<CircuitBreakerManager>)")
                    .unwrap_or("None"),
            )
            .field("operational_state_manager", &"OperationalStateManager")
            .finish()
    }
}

impl WorkerCore {
    /// Create WorkerCore with environment-aware configuration loading
    pub async fn new() -> TaskerResult<Self> {
        info!("🔧 Initializing WorkerCore with auto-detected environment configuration");

        // Auto-detect environment and load configuration
        let config_manager = tasker_shared::config::ConfigManager::load().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to load configuration: {e}"))
        })?;

        Self::from_config(config_manager).await
    }

    /// Create WorkerCore from configuration manager
    pub async fn from_config(config_manager: Arc<ConfigManager>) -> TaskerResult<Self> {
        info!("🔧 Initializing WorkerCore from configuration (environment-aware)");

        let config = config_manager.config();

        // Extract database connection settings from configuration
        let database_url = config.database_url();
        info!(
            "📊 WORKER: Database URL derived from config: {} (pool options: {:?})",
            database_url.chars().take(30).collect::<String>(),
            config.database.pool
        );

        // Create database connection pool with configuration
        let database_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.database.pool.max_connections)
            .min_connections(config.database.pool.min_connections)
            .acquire_timeout(std::time::Duration::from_secs(
                config.database.checkout_timeout,
            ))
            .max_lifetime(std::time::Duration::from_secs(
                config.database.pool.max_lifetime_seconds,
            ))
            .idle_timeout(Some(std::time::Duration::from_secs(
                config.database.reaping_frequency,
            )))
            .connect(&database_url)
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!(
                    "Failed to connect to database with config: {e}"
                ))
            })?;

        info!("✅ WORKER: Database connection established from configuration");

        // Circuit breaker configuration
        let circuit_breaker_config = if config.circuit_breakers.enabled {
            info!("🛡️ WORKER: Circuit breakers enabled in configuration");
            Some(config.circuit_breakers.clone())
        } else {
            info!("📤 WORKER: Circuit breakers disabled in configuration");
            None
        };

        Self::from_pool_and_config(database_pool, config_manager, circuit_breaker_config).await
    }

    /// Internal constructor with full configuration support
    async fn from_pool_and_config(
        database_pool: PgPool,
        config_manager: Arc<ConfigManager>,
        circuit_breaker_config: Option<CircuitBreakerConfig>,
    ) -> TaskerResult<Self> {
        info!("🏗️ Creating worker components with unified configuration");

        let config = config_manager.config();

        // Create circuit breaker manager if enabled
        let (circuit_breaker_manager, pgmq_client): (
            Option<Arc<CircuitBreakerManager>>,
            Arc<UnifiedPgmqClient>,
        ) = if let Some(cb_config) = circuit_breaker_config {
            info!("🛡️ Circuit breakers enabled - creating ProtectedPgmqClient with configuration");
            let manager = Arc::new(CircuitBreakerManager::from_config(&cb_config));
            let protected_client =
                ProtectedPgmqClient::new_with_pool_and_config(database_pool.clone(), &cb_config)
                    .await;
            (
                Some(manager),
                Arc::new(UnifiedPgmqClient::Protected(protected_client)),
            )
        } else {
            info!("📤 Circuit breakers disabled - using standard PgmqClient");
            let standard_client = PgmqClient::new_with_pool(database_pool.clone()).await;
            (None, Arc::new(UnifiedPgmqClient::Standard(standard_client)))
        };

        // Create task handler registry with configuration
        let task_handler_registry = Arc::new(TaskHandlerRegistry::new(database_pool.clone()));

        // For WorkerCore, we would create different executors (e.g., StepExecutor, TaskExecutor)
        // This is just a placeholder - actual implementation would depend on worker-specific needs
        let processing_executors = Vec::new(); // TODO: Create worker-specific executors using WorkerExecutorFactory

        info!(
            "✅ WORKER: Created {} processing executors via factory",
            processing_executors.len()
        );

        // Create operational state manager for shutdown-aware health monitoring
        let operational_state_manager = OperationalStateManager::new();

        // Transition to Normal operation immediately after successful component initialization
        if let Err(e) = operational_state_manager
            .transition_to(SystemOperationalState::Normal)
            .await
        {
            return Err(TaskerError::OrchestrationError(format!(
                "Failed to transition to normal operation state: {e}"
            )));
        }

        info!("✅ WORKER: Operational state transitioned to Normal operation");

        info!("✅ WorkerCore components created successfully");

        Ok(Self {
            pgmq_client,
            database_pool,
            processing_executors,
            task_handler_registry,
            circuit_breaker_manager,
            operational_state_manager,
        })
    }

    /// Get database pool reference
    pub fn database_pool(&self) -> &PgPool {
        &self.database_pool
    }

    /// Get PGMQ client reference
    pub fn pgmq_client(&self) -> Arc<UnifiedPgmqClient> {
        self.pgmq_client.clone()
    }

    /// Get circuit breaker manager if enabled
    pub fn circuit_breaker_manager(&self) -> Option<Arc<CircuitBreakerManager>> {
        self.circuit_breaker_manager.clone()
    }

    /// Check if circuit breakers are enabled
    pub fn circuit_breakers_enabled(&self) -> bool {
        self.circuit_breaker_manager.is_some()
    }

    /// Transition the worker system to graceful shutdown state
    pub async fn transition_to_graceful_shutdown(&self) -> TaskerResult<()> {
        info!(
            "🛑 WORKER: Transitioning to graceful shutdown state for context-aware health monitoring"
        );

        self.operational_state_manager
            .transition_to(SystemOperationalState::GracefulShutdown)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to transition to graceful shutdown: {e}"
                ))
            })?;

        info!("✅ WORKER: Operational state transitioned to graceful shutdown");
        Ok(())
    }

    /// Get current operational state
    pub async fn operational_state(&self) -> SystemOperationalState {
        self.operational_state_manager.current_state().await
    }

    /// Check if system should suppress health alerts
    pub async fn should_suppress_health_alerts(&self) -> bool {
        self.operational_state_manager
            .should_suppress_alerts()
            .await
    }
}

// Implementation of CoordinatorCore trait for WorkerCore
impl CoordinatorCore for WorkerCore {
    fn processing_executors(&self) -> &Vec<Arc<dyn ProcessingExecutor>> {
        &self.processing_executors
    }
}