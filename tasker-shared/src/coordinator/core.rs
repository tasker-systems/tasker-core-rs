use crate::config::ConfigManager;
use crate::coordinator::operational_state::{OperationalStateManager, SystemOperationalState};
use crate::messaging::{PgmqClient, PgmqClientTrait, ProtectedPgmqClient, UnifiedPgmqClient};
use crate::registry::TaskHandlerRegistry;
use crate::resilience::CircuitBreakerManager;
use crate::{TaskerError, TaskerResult};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Unified orchestration core that all entry points use
pub struct CoordinatorCore {
    /// Coordinator ID
    pub coordinator_id: Uuid,

    /// Configuration manager
    pub config_manager: Arc<ConfigManager>,

    /// Unified PGMQ client (either standard or circuit-breaker protected)
    pub pgmq_client: Arc<UnifiedPgmqClient>,

    /// Database connection pool
    pub database_pool: PgPool,

    /// Task handler registry
    pub task_handler_registry: Arc<TaskHandlerRegistry>,

    /// Circuit breaker manager (optional, only when circuit breakers enabled)
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,

    /// Operational state manager for shutdown-aware health monitoring (TAS-37 Supplemental)
    pub operational_state_manager: OperationalStateManager,
}

impl std::fmt::Debug for CoordinatorCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoordinatorCore")
            .field("coordinator_id", &self.coordinator_id)
            .field("config_manager", &"Arc<ConfigManager>")
            .field("pgmq_client", &"Arc<UnifiedPgmqClient>")
            .field(
                "database_pool",
                &format!("PgPool(size={})", self.database_pool.size()),
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

impl CoordinatorCore {
    /// Create CoordinatorCore with environment-aware configuration loading
    ///
    /// This is the primary initialization method that auto-detects environment and loads
    /// the appropriate configuration, then bootstraps all orchestration components
    /// with the parsed configuration settings.
    ///
    /// # Returns
    /// Fully configured CoordinatorCore with all components initialized from configuration
    pub async fn new() -> TaskerResult<Self> {
        info!("🔧 Initializing CoordinatorCore with auto-detected environment configuration");

        // Auto-detect environment and load configuration
        let config_manager = ConfigManager::load().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to load configuration: {e}"))
        })?;

        Self::from_config(config_manager).await
    }

    /// Create CoordinatorCore from configuration manager
    ///
    /// This method provides the complete initialization path with configuration
    /// support, including circuit breaker integration based on config settings.
    ///
    /// # Arguments
    /// * `config_manager` - Loaded configuration manager (wrapped in Arc)
    ///
    /// # Returns
    /// Initialized CoordinatorCore with all configuration applied
    pub async fn from_config(config_manager: Arc<ConfigManager>) -> TaskerResult<Self> {
        info!("🔧 Initializing CoordinatorCore from configuration (environment-aware)");

        let config = config_manager.config();

        // Extract database connection settings from configuration
        let database_url = config.database_url();
        info!(
            "📊 CORE: Database URL derived from config: {} (pool options: {:?})",
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

        info!("✅ CORE: Database connection established from configuration");

        // Pass config_manager (already Arc) for component initialization
        Self::from_pool_and_config(database_pool, config_manager).await
    }

    /// Internal constructor with full configuration support
    ///
    /// This method contains the unified bootstrap logic that creates all orchestration
    /// components with proper configuration passed to each component.
    async fn from_pool_and_config(
        database_pool: PgPool,
        config_manager: Arc<ConfigManager>,
    ) -> TaskerResult<Self> {
        info!("🏗️ Creating orchestration components with unified configuration");

        let config = config_manager.config();

        // Circuit breaker configuration
        let circuit_breaker_config = if config.circuit_breakers.enabled {
            info!("🛡️ CORE: Circuit breakers enabled in configuration");
            Some(config.circuit_breakers.clone())
        } else {
            info!("📤 CORE: Circuit breakers disabled in configuration");
            None
        };

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

        // Create orchestration loop with environment-configured instance ID
        let coordinator_id = Uuid::now_v7();

        // Create operational state manager for shutdown-aware health monitoring (TAS-37 Supplemental)
        let operational_state_manager = OperationalStateManager::new();

        info!("✅ CoordinatorCore components created successfully");

        Ok(Self {
            coordinator_id,
            config_manager,
            pgmq_client,
            database_pool,
            task_handler_registry,
            circuit_breaker_manager,
            operational_state_manager,
        })
    }

    pub async fn start(&self) -> TaskerResult<()> {
        let config = self.config_manager.config();
        let namespaces: Vec<&str> = config
            .pgmq
            .default_namespaces
            .iter()
            .map(|ns| ns.as_str())
            .collect::<Vec<&str>>();

        // Initialize queues before transitioning to Normal operation
        self.initialize_queues(namespaces.as_slice()).await?;

        // Transition to Normal operation immediately after we initialize queues
        self.operational_state_manager
            .transition_to(SystemOperationalState::Normal)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to transition to normal operation state: {e}"
                ))
            })?;

        info!("✅ CORE: Operational state transitioned to Normal operation");
        Ok(())
    }

    /// Initialize standard namespace queues
    pub async fn initialize_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        info!("🏗️ CORE: Initializing namespace queues via unified client");

        self.pgmq_client
            .initialize_namespace_queues(namespaces)
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to initialize queues: {e}"))
            })?;

        info!("✅ CORE: All namespace queues initialized");
        Ok(())
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

    /// Transition the orchestration system to graceful shutdown state (TAS-37 Supplemental)
    ///
    /// This method signals that the system is intentionally shutting down, enabling
    /// context-aware health monitoring to suppress false alerts during planned operations.
    pub async fn stop(&self) -> TaskerResult<()> {
        info!(
            "🛑 CORE: Transitioning to graceful shutdown state for context-aware health monitoring"
        );

        self.operational_state_manager
            .transition_to(SystemOperationalState::GracefulShutdown)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to transition to graceful shutdown: {e}"
                ))
            })?;

        info!("✅ CORE: Operational state transitioned to graceful shutdown");
        Ok(())
    }

    /// Get current operational state (TAS-37 Supplemental)
    pub async fn operational_state(&self) -> SystemOperationalState {
        self.operational_state_manager.current_state().await
    }

    /// Check if system should suppress health alerts (TAS-37 Supplemental)
    pub async fn should_suppress_health_alerts(&self) -> bool {
        self.operational_state_manager
            .should_suppress_alerts()
            .await
    }
}
