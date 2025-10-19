use crate::config::{ConfigManager, TaskerConfig};
use crate::events::EventPublisher;
use crate::messaging::{PgmqClientTrait, UnifiedPgmqClient};
use crate::registry::TaskHandlerRegistry;
use crate::resilience::CircuitBreakerManager;
use crate::{TaskerError, TaskerResult};
use pgmq_notify::PgmqClient;
use sqlx::PgPool;
use std::path::PathBuf;
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
    pub processor_uuid: Uuid,

    /// Tasker Config
    pub tasker_config: Arc<TaskerConfig>,

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
    /// This method loads orchestration configuration from a single merged TOML file
    /// specified by the TASKER_CONFIG_PATH environment variable. The file should contain:
    /// - CommonConfig (database, queues, circuit breakers)
    /// - OrchestrationConfig (backoff, orchestration system, task readiness)
    ///
    /// The configuration file should be generated using the tasker-cli config merge command.
    ///
    /// **FAIL LOUDLY**: This method will fail with explicit errors if:
    /// - TASKER_CONFIG_PATH environment variable is not set
    /// - Configuration file doesn't exist at the specified path
    /// - TOML cannot be parsed or deserialized into orchestration config structs
    ///
    /// # Returns
    /// Fully configured SystemContext for orchestration with validated configuration
    ///
    /// # Errors
    /// Returns TaskerError::ConfigurationError with explicit failure reason if:
    /// - Environment variable not set
    /// - File not found
    /// - Parse/validation errors
    pub async fn new_for_orchestration() -> TaskerResult<Self> {
        info!("Initializing SystemContext for orchestration with single-file configuration (TAS-50 Phase 3)");

        // TAS-50: Configuration path resolution with convention-based fallback
        // Precedence:
        //   1. TASKER_CONFIG_PATH (explicit single file) - Docker/production deployment
        //   2. TASKER_CONFIG_ROOT/{context}-{environment}.toml - Test/development convention
        let (path, source) = if let Ok(config_path) = std::env::var("TASKER_CONFIG_PATH") {
            (PathBuf::from(&config_path), "TASKER_CONFIG_PATH")
        } else {
            // Fall back to convention-based path
            let config_root = std::env::var("TASKER_CONFIG_ROOT").map_err(|_| {
                TaskerError::ConfigurationError(
                    "Neither TASKER_CONFIG_PATH nor TASKER_CONFIG_ROOT is set. \
                        For Docker/production: set TASKER_CONFIG_PATH to the merged config file. \
                        For tests/development: set TASKER_CONFIG_ROOT to the config directory."
                        .to_string(),
                )
            })?;

            let environment = crate::config::UnifiedConfigLoader::detect_environment();
            let convention_path = PathBuf::from(&config_root)
                .join("tasker")
                .join("orchestration-{}.toml".replace("{}", &environment));

            info!(
                "Using convention-based config path: {} (environment={})",
                convention_path.display(),
                environment
            );

            (convention_path, "TASKER_CONFIG_ROOT (convention)")
        };

        info!(
            "📋 Loading orchestration configuration from: {} (source: {})",
            path.display(),
            source
        );

        // Load orchestration context configuration from single file - FAIL LOUDLY on errors
        let context_manager = ConfigManager::load_from_single_file(
            &path,
            crate::config::contexts::ConfigContext::Orchestration,
        )
        .map_err(|e| {
            TaskerError::ConfigurationError(format!(
                "Failed to load orchestration configuration from {}: {}",
                path.display(),
                e
            ))
        })?;

        // Build TaskerConfig from context configs for backward compatibility
        let tasker_config = context_manager.as_tasker_config().ok_or_else(|| {
            TaskerError::ConfigurationError(format!(
                "Failed to build TaskerConfig from orchestration configuration in {}",
                path.display()
            ))
        })?;

        // Wrap in Arc<ConfigManager> for compatibility with existing code
        let config_manager = Arc::new(ConfigManager::from_tasker_config(
            tasker_config,
            context_manager.environment().to_string(),
        ));

        info!(
            "Orchestration configuration loaded successfully from {}: environment={}",
            path.display(),
            config_manager.environment()
        );

        Self::from_config(config_manager).await
    }

    /// Create SystemContext for worker using single-file configuration (TAS-50 Phase 3)
    ///
    /// This method loads worker configuration from a single merged TOML file
    /// specified by the TASKER_CONFIG_PATH environment variable. The file should contain:
    /// - CommonConfig (database, queues, circuit breakers)
    /// - WorkerConfig (worker system, step processing, web API)
    ///
    /// The configuration file should be generated using the tasker-cli config merge command.
    ///
    /// **FAIL LOUDLY**: This method will fail with explicit errors if:
    /// - TASKER_CONFIG_PATH environment variable is not set
    /// - Configuration file doesn't exist at the specified path
    /// - TOML cannot be parsed or deserialized into worker config structs
    ///
    /// # Returns
    /// Fully configured SystemContext for worker with validated configuration
    ///
    /// # Errors
    /// Returns TaskerError::ConfigurationError with explicit failure reason if:
    /// - Environment variable not set
    /// - File not found
    /// - Parse/validation errors
    pub async fn new_for_worker() -> TaskerResult<Self> {
        info!(
            "Initializing SystemContext for worker with single-file configuration (TAS-50 Phase 3)"
        );

        // TAS-50: Configuration path resolution with convention-based fallback
        // Precedence:
        //   1. TASKER_CONFIG_PATH (explicit single file) - Docker/production deployment
        //   2. TASKER_CONFIG_ROOT/{context}-{environment}.toml - Test/development convention
        let (path, source) = if let Ok(config_path) = std::env::var("TASKER_CONFIG_PATH") {
            (PathBuf::from(&config_path), "TASKER_CONFIG_PATH")
        } else {
            // Fall back to convention-based path
            let config_root = std::env::var("TASKER_CONFIG_ROOT").map_err(|_| {
                TaskerError::ConfigurationError(
                    "Neither TASKER_CONFIG_PATH nor TASKER_CONFIG_ROOT is set. \
                        For Docker/production: set TASKER_CONFIG_PATH to the merged config file. \
                        For tests/development: set TASKER_CONFIG_ROOT to the config directory."
                        .to_string(),
                )
            })?;

            let environment = crate::config::UnifiedConfigLoader::detect_environment();
            let convention_path = PathBuf::from(&config_root)
                .join("tasker")
                .join("worker-{}.toml".replace("{}", &environment));

            info!(
                "Using convention-based config path: {} (environment={})",
                convention_path.display(),
                environment
            );

            (convention_path, "TASKER_CONFIG_ROOT (convention)")
        };

        info!(
            "📋 Loading worker configuration from: {} (source: {})",
            path.display(),
            source
        );

        // Load worker context configuration from single file - FAIL LOUDLY on errors
        let context_manager = ConfigManager::load_from_single_file(
            &path,
            crate::config::contexts::ConfigContext::Worker,
        )
        .map_err(|e| {
            TaskerError::ConfigurationError(format!(
                "Failed to load worker configuration from {}: {}",
                path.display(),
                e
            ))
        })?;

        // Build TaskerConfig from context configs for backward compatibility
        let tasker_config = context_manager.as_tasker_config().ok_or_else(|| {
            TaskerError::ConfigurationError(format!(
                "Failed to build TaskerConfig from worker configuration in {}",
                path.display()
            ))
        })?;

        // Wrap in Arc<ConfigManager> for compatibility with existing code
        let config_manager = Arc::new(ConfigManager::from_tasker_config(
            tasker_config,
            context_manager.environment().to_string(),
        ));

        info!(
            "Worker configuration loaded successfully from {}: environment={}",
            path.display(),
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
                config.database.checkout_timeout,
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
    async fn from_pool_and_config(
        database_pool: PgPool,
        config_manager: Arc<ConfigManager>,
    ) -> TaskerResult<Self> {
        info!("Creating system components with unified configuration");

        let config = config_manager.config();

        let (circuit_breaker_manager, message_client) =
            Self::get_circuit_breaker_and_queue_client(config, database_pool.clone()).await;

        // Create task handler registry with configuration
        let task_handler_registry = Arc::new(TaskHandlerRegistry::new(database_pool.clone()));

        // Create system instance ID
        let system_id = Uuid::now_v7();

        // Create event publisher with bounded channel (TAS-51)
        let event_publisher_buffer_size = config
            .mpsc_channels
            .shared
            .event_publisher
            .event_queue_buffer_size;
        let event_publisher = Arc::new(EventPublisher::with_capacity(event_publisher_buffer_size));

        info!("SystemContext components created successfully");

        let tasker_config = Arc::new(config.clone());

        Ok(Self {
            processor_uuid: system_id,
            tasker_config,
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

        let queue_config = self.tasker_config.queues.clone();

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
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn with_pool(database_pool: sqlx::PgPool) -> TaskerResult<Self> {
        use uuid::Uuid;

        let system_id = Uuid::new_v4();

        // Create minimal default configuration for testing using context-specific loading (TAS-50 Phase 2)
        // Use Combined context for test compatibility (includes all configuration)
        let context_manager =
            ConfigManager::load_context_direct(crate::config::contexts::ConfigContext::Combined)
                .map_err(|e| {
                    TaskerError::ConfigurationError(format!(
                        "Failed to load test configuration: {e}"
                    ))
                })?;

        // Build TaskerConfig from context configs for backward compatibility with tests
        let tasker_config_inner = context_manager.as_tasker_config().ok_or_else(|| {
            TaskerError::ConfigurationError(
                "Failed to build TaskerConfig from combined context for testing".to_string(),
            )
        })?;

        let tasker_config = Arc::new(tasker_config_inner);

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
            message_client,
            database_pool,
            task_handler_registry,
            circuit_breaker_manager: None, // Disabled for testing
            event_publisher,
        })
    }
}
