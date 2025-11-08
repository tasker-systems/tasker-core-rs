use crate::config::tasker::TaskerConfigV2;
use crate::config::{ConfigManager, TaskerConfig};
use crate::events::EventPublisher;
use crate::messaging::{PgmqClientTrait, UnifiedPgmqClient};
use crate::registry::TaskHandlerRegistry;
use crate::resilience::CircuitBreakerManager;
use crate::{TaskerError, TaskerResult};
use pgmq_notify::PgmqClient;
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
///
/// ## TAS-61 Phase 6A: Parallel Config Support
///
/// During Phase 6A-6C migration, both legacy and V2 configs are available:
/// - `tasker_config`: Legacy monolithic config (via bridge conversion)
/// - `tasker_config_v2`: Context-based configuration (primary going forward)
///
/// The legacy config will be removed in Phase 6D once all consumers migrate to V2.
pub struct SystemContext {
    /// System instance ID
    pub processor_uuid: Uuid,

    /// Legacy Tasker Config (TAS-61 Phase 6: will be removed in Phase 6D)
    ///
    /// This is populated via bridge conversion from `tasker_config_v2` for backward compatibility.
    /// All new code should use `tasker_config_v2` directly.
    pub tasker_config: Arc<TaskerConfig>,

    /// Context-based Tasker Config (TAS-61 Phase 6: primary config)
    ///
    /// This is the source of truth for configuration. Use this for all new code.
    /// Contains optional orchestration and worker contexts.
    pub tasker_config_v2: Arc<TaskerConfigV2>,

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
            .field("processor_uuid", &self.processor_uuid)
            .field("tasker_config", &"Arc<TaskerConfig>")
            .field("tasker_config_v2", &"Arc<TaskerConfigV2>")
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
    /// Create SystemContext for orchestration using single-file configuration (TAS-50 Phase 3)
    ///
    /// This method loads orchestration configuration from context-based TOML files
    /// specified by the TASKER_CONFIG_PATH environment variable.
    ///
    /// The configuration uses a context-based structure with [common] and [orchestration] sections.
    /// See config/tasker/orchestration-test.toml for reference.
    ///
    /// # Returns
    /// Fully configured SystemContext for orchestration with validated configuration
    ///
    /// # Errors
    /// Returns TaskerError::ConfigurationError with explicit failure reason if:
    /// - Environment variable not set
    /// - File not found
    /// - Parse/validation errors
    ///
    /// # Example
    /// ```no_run
    /// # use tasker_shared::system_context::SystemContext;
    /// # tokio_test::block_on(async {
    /// let system_context = SystemContext::new_for_orchestration().await.unwrap();
    /// # })
    /// ```
    pub async fn new_for_orchestration() -> TaskerResult<Self> {
        info!("Initializing SystemContext for orchestration");

        let environment = crate::config::ConfigLoader::detect_environment();
        let config_manager = ConfigManager::load_from_env(&environment).map_err(|e| {
            TaskerError::ConfigurationError(format!(
                "Failed to load orchestration configuration: {}",
                e
            ))
        })?;

        info!(
            "Orchestration configuration loaded successfully: environment={}",
            config_manager.environment()
        );

        Self::from_config(config_manager).await
    }

    /// Create SystemContext for worker using context-based configuration
    ///
    /// This method loads worker configuration from context-based TOML files
    /// specified by the TASKER_CONFIG_PATH environment variable.
    ///
    /// The configuration uses a context-based structure with [common] and [worker] sections.
    /// See config/tasker/worker-test.toml for reference.
    ///
    /// # Returns
    /// Fully configured SystemContext for worker with validated configuration
    ///
    /// # Errors
    /// Returns TaskerError::ConfigurationError with explicit failure reason if:
    /// - Environment variable not set
    /// - File not found
    /// - Parse/validation errors
    ///
    /// # Example
    /// ```no_run
    /// # use tasker_shared::system_context::SystemContext;
    /// # tokio_test::block_on(async {
    /// let system_context = SystemContext::new_for_worker().await.unwrap();
    /// # })
    /// ```
    pub async fn new_for_worker() -> TaskerResult<Self> {
        info!("Initializing SystemContext for worker");

        let environment = crate::config::ConfigLoader::detect_environment();
        let config_manager = ConfigManager::load_from_env(&environment).map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to load worker configuration: {}", e))
        })?;

        info!(
            "Worker configuration loaded successfully: environment={}",
            config_manager.environment()
        );

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
        info!("Initializing SystemContext from configuration (environment-aware)");

        let config = config_manager.config();

        // Extract database connection settings from configuration
        let database_url = config.database_url();
        info!(
            "Database URL derived from config: {} (pool options: {:?})",
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

        info!("Database connection established from configuration");

        // Pass config_manager (already Arc) for component initialization
        Self::from_pool_and_config(database_pool, config_manager).await
    }

    fn get_pg_pool_options(config: &TaskerConfig) -> sqlx::postgres::PgPoolOptions {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.database.pool.max_connections)
            .min_connections(config.database.pool.min_connections)
            .acquire_timeout(std::time::Duration::from_secs(
                config.database.pool.acquire_timeout_seconds,
            ))
            .idle_timeout(Some(std::time::Duration::from_secs(
                config.database.pool.idle_timeout_seconds,
            )))
            .max_lifetime(Some(std::time::Duration::from_secs(
                config.database.pool.max_lifetime_seconds,
            )))
    }

    async fn get_circuit_breaker_and_queue_client(
        config: &TaskerConfig,
        database_pool: PgPool,
    ) -> (Option<Arc<CircuitBreakerManager>>, Arc<UnifiedPgmqClient>) {
        // Circuit breaker configuration
        let circuit_breaker_config = if config.circuit_breakers.enabled {
            info!("Circuit breakers enabled in configuration");
            Some(config.circuit_breakers.clone())
        } else {
            info!("📤 Circuit breakers disabled in configuration");
            None
        };

        // Create circuit breaker manager if enabled (deprecated - circuit breakers are now redundant with sqlx)
        let circuit_breaker_manager = if let Some(cb_config) = circuit_breaker_config {
            info!("Circuit breaker configuration found but ignored - sqlx provides this functionality");
            Some(Arc::new(CircuitBreakerManager::from_config(&cb_config)))
        } else {
            info!("📤 Using standard PgmqClient with sqlx connection pooling");
            None
        };

        // Create unified client with sqlx pool (which handles retries, timeouts, etc.)
        let standard_client = PgmqClient::new_with_pool(database_pool.clone()).await;
        let message_client = Arc::new(UnifiedPgmqClient::new_standard(standard_client));
        (circuit_breaker_manager, message_client)
    }

    /// Internal constructor with full configuration support
    ///
    /// This method contains the unified bootstrap logic that creates all shared system
    /// components with proper configuration passed to each component.
    ///
    /// ## TAS-61 Phase 6A: Parallel Config Support
    ///
    /// Populates BOTH `tasker_config` (legacy) and `tasker_config_v2` (primary):
    /// - `tasker_config_v2`: Source of truth from ConfigManager
    /// - `tasker_config`: Generated via bridge conversion for backward compatibility
    async fn from_pool_and_config(
        database_pool: PgPool,
        config_manager: Arc<ConfigManager>,
    ) -> TaskerResult<Self> {
        info!("Creating system components with unified configuration (TAS-61 Phase 6A: parallel config support)");

        // TAS-61 Phase 6A: Get V2 config (source of truth)
        let config_v2 = config_manager.config_v2();
        let tasker_config_v2 = Arc::new(config_v2.clone());

        // TAS-61 Phase 6A: Generate legacy config via bridge for backward compatibility
        let config_legacy = config_manager.config(); // Already converted via bridge in ConfigManager
        let tasker_config = Arc::new(config_legacy.clone());

        let (circuit_breaker_manager, message_client) =
            Self::get_circuit_breaker_and_queue_client(config_legacy, database_pool.clone()).await;

        // Create task handler registry with configuration
        let task_handler_registry = Arc::new(TaskHandlerRegistry::new(database_pool.clone()));

        // Create system instance ID
        let system_id = Uuid::now_v7();

        // Create event publisher with bounded channel (TAS-51)
        let event_publisher_buffer_size = config_legacy
            .mpsc_channels
            .shared
            .event_publisher
            .event_queue_buffer_size;
        let event_publisher = Arc::new(EventPublisher::with_capacity(event_publisher_buffer_size));

        info!("SystemContext components created successfully (both configs available)");

        Ok(Self {
            processor_uuid: system_id,
            tasker_config,
            tasker_config_v2,
            message_client,
            database_pool,
            task_handler_registry,
            circuit_breaker_manager,
            event_publisher,
        })
    }

    /// Initialize standard namespace queues
    pub async fn initialize_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        info!("Initializing namespace queues via unified client");

        self.message_client
            .initialize_namespace_queues(namespaces)
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to initialize queues: {e}"))
            })?;

        info!("All namespace queues initialized");
        Ok(())
    }

    /// Initialize owned queues for the system context
    pub async fn initialize_orchestration_owned_queues(&self) -> TaskerResult<()> {
        info!("Initializing owned queues");

        // TAS-61 Phase 6C: Use V2 config
        let queue_config = self.tasker_config_v2.common.queues.clone();

        let orchestration_owned_queues = vec![
            queue_config.orchestration_queues.step_results.as_str(),
            queue_config.orchestration_queues.task_requests.as_str(),
            queue_config
                .orchestration_queues
                .task_finalizations
                .as_str(),
        ];

        for queue_name in orchestration_owned_queues {
            self.message_client
                .create_queue(queue_name)
                .await
                .map_err(|e| {
                    TaskerError::MessagingError(format!("Failed to initialize owned queues: {e}"))
                })?;
        }

        info!("All owned queues initialized");
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

    pub fn processor_uuid(&self) -> Uuid {
        self.processor_uuid
    }

    /// Create a minimal SystemContext for testing with provided database pool
    ///
    /// This bypasses full configuration loading and creates a basic context
    /// suitable for unit tests that need database access.
    ///
    /// ## TAS-61 Phase 6A: Parallel Config Support
    ///
    /// Populates BOTH configs for test compatibility during migration.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn with_pool(database_pool: sqlx::PgPool) -> TaskerResult<Self> {
        let system_id = Uuid::new_v4();

        // Create minimal default configuration for testing using V2 loader
        // Requires TASKER_CONFIG_PATH to be set
        let environment = crate::config::ConfigLoader::detect_environment();
        let config_manager = ConfigManager::load_from_env(&environment).map_err(|e| {
            TaskerError::ConfigurationError(format!(
                "Failed to load test configuration (ensure TASKER_CONFIG_PATH is set): {}",
                e
            ))
        })?;

        // TAS-61 Phase 6A: Get both configs
        let config_v2 = config_manager.config_v2();
        let tasker_config_v2 = Arc::new(config_v2.clone());

        let config_legacy = config_manager.config();
        let tasker_config = Arc::new(config_legacy.clone());

        // Create standard PGMQ client (no circuit breaker for simplicity)
        let message_client = Arc::new(UnifiedPgmqClient::new_standard(
            PgmqClient::new_with_pool(database_pool.clone()).await,
        ));

        // Create task handler registry
        let task_handler_registry = Arc::new(TaskHandlerRegistry::new(database_pool.clone()));

        // Create event publisher with bounded channel (TAS-51)
        let event_publisher_buffer_size = tasker_config
            .mpsc_channels
            .shared
            .event_publisher
            .event_queue_buffer_size;
        let event_publisher = Arc::new(EventPublisher::with_capacity(event_publisher_buffer_size));

        Ok(Self {
            processor_uuid: system_id,
            tasker_config,
            tasker_config_v2,
            message_client,
            database_pool,
            task_handler_registry,
            circuit_breaker_manager: None, // Disabled for testing
            event_publisher,
        })
    }
}
