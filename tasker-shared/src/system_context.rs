use crate::config::{ConfigManager, TaskerConfig};
use crate::events::EventPublisher;
use crate::messaging::{PgmqClient, PgmqClientTrait, ProtectedPgmqClient, UnifiedPgmqClient};
use crate::registry::TaskHandlerRegistry;
use crate::resilience::CircuitBreakerManager;
use crate::{TaskerError, TaskerResult};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Shared system dependencies and configuration
///
/// This serves as a dependency injection container providing access to:
/// - Database connection pool
/// - Configuration manager
/// - Message queue clients (unified PGMQ/future RabbitMQ)
/// - Task handler registry
/// - Circuit breaker management
/// - Operational state management
pub struct SystemContext {
    /// System instance ID
    pub system_id: Uuid,

    /// Configuration manager with environment-aware loading
    pub config_manager: Arc<ConfigManager>,

    /// Unified message queue client (PGMQ/RabbitMQ abstraction)
    pub message_client: Arc<UnifiedPgmqClient>,

    /// Database connection pool
    pub database_pool: PgPool,

    /// Task handler registry
    pub task_handler_registry: Arc<TaskHandlerRegistry>,

    /// Circuit breaker manager (optional)
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,

    /// Event publisher
    pub event_publisher: Arc<EventPublisher>,
}

impl std::fmt::Debug for SystemContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemContext")
            .field("system_id", &self.system_id)
            .field("config_manager", &"Arc<ConfigManager>")
            .field("message_client", &"Arc<UnifiedMessageClient>")
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
            .finish()
    }
}

impl SystemContext {
    /// Create SystemContext with environment-aware configuration loading
    ///
    /// This is the primary initialization method that auto-detects environment and loads
    /// the appropriate configuration, then bootstraps all shared system components
    /// with the parsed configuration settings.
    ///
    /// # Returns
    /// Fully configured SystemContext with all components initialized from configuration
    pub async fn new() -> TaskerResult<Self> {
        info!("🔧 Initializing SystemContext with auto-detected environment configuration");

        // Auto-detect environment and load configuration
        let config_manager = ConfigManager::load().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to load configuration: {e}"))
        })?;

        Self::from_config(config_manager).await
    }

    /// Create SystemContext from configuration manager
    ///
    /// This method provides the complete initialization path with configuration
    /// support, including circuit breaker integration based on config settings.
    ///
    /// # Arguments
    /// * `config_manager` - Loaded configuration manager (wrapped in Arc)
    ///
    /// # Returns
    /// Initialized SystemContext with all configuration applied
    pub async fn from_config(config_manager: Arc<ConfigManager>) -> TaskerResult<Self> {
        info!("🔧 Initializing SystemContext from configuration (environment-aware)");

        let config = config_manager.config();

        // Extract database connection settings from configuration
        let database_url = config.database_url();
        info!(
            "📊 CORE: Database URL derived from config: {} (pool options: {:?})",
            database_url.chars().take(30).collect::<String>(),
            config.database.pool
        );

        // Create database connection pool with configuration
        let database_pool = Self::get_pg_pool_options(config)
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

    fn get_pg_pool_options(config: &TaskerConfig) -> sqlx::postgres::PgPoolOptions {
        sqlx::postgres::PgPoolOptions::new()
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
    }

    async fn get_circuit_breaker_and_queue_client(
        config: &TaskerConfig,
        database_pool: PgPool,
    ) -> (Option<Arc<CircuitBreakerManager>>, Arc<UnifiedPgmqClient>) {
        // Circuit breaker configuration
        let circuit_breaker_config = if config.circuit_breakers.enabled {
            info!("🛡️ CORE: Circuit breakers enabled in configuration");
            Some(config.circuit_breakers.clone())
        } else {
            info!("📤 CORE: Circuit breakers disabled in configuration");
            None
        };

        // Create circuit breaker manager if enabled
        let (circuit_breaker_manager, message_client): (
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
                Arc::new(UnifiedPgmqClient::new_protected(protected_client)),
            )
        } else {
            info!("📤 Circuit breakers disabled - using standard PgmqClient");
            let standard_client = PgmqClient::new_with_pool(database_pool.clone()).await;
            (
                None,
                Arc::new(UnifiedPgmqClient::new_standard(standard_client)),
            )
        };
        (circuit_breaker_manager, message_client)
    }

    /// Internal constructor with full configuration support
    ///
    /// This method contains the unified bootstrap logic that creates all shared system
    /// components with proper configuration passed to each component.
    async fn from_pool_and_config(
        database_pool: PgPool,
        config_manager: Arc<ConfigManager>,
    ) -> TaskerResult<Self> {
        info!("🏗️ Creating system components with unified configuration");

        let config = config_manager.config();

        let (circuit_breaker_manager, message_client) =
            Self::get_circuit_breaker_and_queue_client(config, database_pool.clone()).await;

        // Create task handler registry with configuration
        let task_handler_registry = Arc::new(TaskHandlerRegistry::new(database_pool.clone()));

        // Create system instance ID
        let system_id = Uuid::now_v7();

        let event_publisher = Arc::new(EventPublisher::new());

        info!("✅ SystemContext components created successfully");

        Ok(Self {
            system_id,
            config_manager,
            message_client,
            database_pool,
            task_handler_registry,
            circuit_breaker_manager,
            event_publisher,
        })
    }

    /// Initialize standard namespace queues
    pub async fn initialize_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        info!("🏗️ CORE: Initializing namespace queues via unified client");

        self.message_client
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

    /// Get message client reference
    pub fn message_client(&self) -> Arc<UnifiedPgmqClient> {
        self.message_client.clone()
    }

    /// Get circuit breaker manager if enabled
    pub fn circuit_breaker_manager(&self) -> Option<Arc<CircuitBreakerManager>> {
        self.circuit_breaker_manager.clone()
    }

    /// Check if circuit breakers are enabled
    pub fn circuit_breakers_enabled(&self) -> bool {
        self.circuit_breaker_manager.is_some()
    }

    /// Create a minimal SystemContext for testing with provided database pool
    ///
    /// This bypasses full configuration loading and creates a basic context
    /// suitable for unit tests that need database access.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn with_pool(database_pool: sqlx::PgPool) -> TaskerResult<Self> {
        use uuid::Uuid;

        let system_id = Uuid::new_v4();

        // Create minimal default configuration for testing
        // Auto-detect environment and load configuration
        let config_manager = ConfigManager::load().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to load configuration: {e}"))
        })?;

        // Create standard PGMQ client (no circuit breaker for simplicity)
        let message_client = Arc::new(UnifiedPgmqClient::new_standard(
            PgmqClient::new_with_pool(database_pool.clone()).await,
        ));

        // Create task handler registry
        let task_handler_registry = Arc::new(TaskHandlerRegistry::new(database_pool.clone()));

        let event_publisher = Arc::new(EventPublisher::new());

        Ok(Self {
            system_id,
            config_manager,
            message_client,
            database_pool,
            task_handler_registry,
            circuit_breaker_manager: None, // Disabled for testing
            event_publisher,
        })
    }
}
