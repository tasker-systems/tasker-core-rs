// TAS-61 Phase 6C/6D: V2 config is now canonical
// TAS-78: Added DatabasePools for separate PGMQ database support
use crate::config::tasker::TaskerConfig;
use crate::config::ConfigManager;
use crate::database::DatabasePools;
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
/// ## TAS-61 Phase 6C/6D: V2 Configuration (Canonical)
///
/// Uses context-based configuration with optional orchestration and worker contexts.
/// Configuration structure supports three deployment modes:
/// - Orchestration-only: common + orchestration contexts
/// - Worker-only: common + worker contexts
/// - Full: common + orchestration + worker contexts
pub struct SystemContext {
    /// System instance ID
    pub processor_uuid: Uuid,

    /// Context-based Tasker Config (TAS-61 Phase 6C/6D: canonical config)
    ///
    /// Source of truth for all configuration. Contains:
    /// - `common`: Always-present shared configuration
    /// - `orchestration`: Optional orchestration-specific configuration
    /// - `worker`: Optional worker-specific configuration
    pub tasker_config: Arc<TaskerConfig>,

    /// Unified message queue client (PGMQ/RabbitMQ abstraction)
    pub message_client: Arc<UnifiedPgmqClient>,

    /// Database connection pools (TAS-78: supports separate PGMQ database)
    ///
    /// Contains both the main Tasker pool and the PGMQ pool.
    /// In single-database deployments, both pools point to the same database.
    database_pools: DatabasePools,

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
            .field("database_pools", &self.database_pools)
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
    /// The configuration uses a context-based structure with \[common\] and \[orchestration\] sections.
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
    /// The configuration uses a context-based structure with \[common\] and \[worker\] sections.
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

        // TAS-61 Phase 6C/6D: Use V2 config directly
        // TAS-78: Use DatabasePools for separate PGMQ database support
        let config = config_manager.config();

        // Create database pools (handles both single-DB and split-DB configurations)
        let database_pools = DatabasePools::from_config(config).await?;

        info!(
            "Database pools created: pgmq_is_separate={}",
            database_pools.pgmq_is_separate()
        );

        // Pass config_manager (already Arc) for component initialization
        Self::from_pools_and_config(database_pools, config_manager).await
    }

    async fn get_circuit_breaker_and_queue_client(
        config: &TaskerConfig,
        pgmq_pool: PgPool,
    ) -> (Option<Arc<CircuitBreakerManager>>, Arc<UnifiedPgmqClient>) {
        // TAS-61 Phase 6C/6D: Use V2 config structure (common.circuit_breakers)
        let circuit_breaker_config = if config.common.circuit_breakers.enabled {
            info!("Circuit breakers enabled in configuration");
            Some(config.common.circuit_breakers.clone())
        } else {
            info!("Circuit breakers disabled in configuration");
            None
        };

        // Create circuit breaker manager if enabled (deprecated - circuit breakers are now redundant with sqlx)
        let circuit_breaker_manager = if let Some(cb_config) = circuit_breaker_config {
            info!("Circuit breaker configuration found but ignored - sqlx provides this functionality");
            Some(Arc::new(CircuitBreakerManager::from_config(&cb_config)))
        } else {
            info!("Using standard PgmqClient with sqlx connection pooling");
            None
        };

        // TAS-78: Create unified client with PGMQ pool (may be separate from Tasker pool)
        let standard_client = PgmqClient::new_with_pool(pgmq_pool).await;
        let message_client = Arc::new(UnifiedPgmqClient::new_standard(standard_client));
        (circuit_breaker_manager, message_client)
    }

    /// Internal constructor with full configuration support
    ///
    /// This method contains the unified bootstrap logic that creates all shared system
    /// components with proper configuration passed to each component.
    ///
    /// ## TAS-61 Phase 6C/6D: V2 Configuration (Canonical)
    /// ## TAS-78: DatabasePools for separate PGMQ database support
    ///
    /// Uses V2 config directly as the single source of truth.
    /// No bridge conversion - all components use V2 config structure.
    async fn from_pools_and_config(
        database_pools: DatabasePools,
        config_manager: Arc<ConfigManager>,
    ) -> TaskerResult<Self> {
        info!("Creating system components with V2 configuration (TAS-61 Phase 6C/6D: canonical config)");

        // TAS-61 Phase 6C/6D: V2 config is the single source of truth
        let config = config_manager.config();
        let tasker_config = Arc::new(config.clone());

        // TAS-78: Use PGMQ pool for message client
        let (circuit_breaker_manager, message_client) = Self::get_circuit_breaker_and_queue_client(
            &tasker_config,
            database_pools.pgmq().clone(),
        )
        .await;

        // Create task handler registry with Tasker pool
        let task_handler_registry =
            Arc::new(TaskHandlerRegistry::new(database_pools.tasker().clone()));

        // Create system instance ID
        let system_id = Uuid::now_v7();

        // Create event publisher with bounded channel (TAS-51)
        let event_publisher_buffer_size = tasker_config
            .common
            .mpsc_channels
            .event_publisher
            .event_queue_buffer_size as usize;
        let event_publisher = Arc::new(EventPublisher::with_capacity(event_publisher_buffer_size));

        info!("SystemContext components created successfully with V2 config");

        Ok(Self {
            processor_uuid: system_id,
            tasker_config,
            message_client,
            database_pools,
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

        // TAS-61 Phase 6C/6D: Use V2 config (now canonical)
        let queue_config = self.tasker_config.common.queues.clone();

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

    /// Initialize domain event queues for given namespaces
    ///
    /// Creates main queue ({namespace}_domain_events) and DLQ ({namespace}_domain_events_dlq)
    /// for each namespace to support TAS-65 domain event publishing.
    ///
    /// # Arguments
    ///
    /// * `namespaces` - Array of namespace identifiers to create queues for
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all queues were created successfully, or an error if any queue creation failed.
    pub async fn initialize_domain_event_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        info!(
            "Initializing domain event queues for namespaces: {:?}",
            namespaces
        );

        for namespace in namespaces {
            // Create main domain events queue
            let main_queue = format!("{}_domain_events", namespace);
            self.message_client
                .create_queue(&main_queue)
                .await
                .map_err(|e| {
                    TaskerError::MessagingError(format!(
                        "Failed to create domain events queue {}: {}",
                        main_queue, e
                    ))
                })?;

            // Create DLQ for failed events
            let dlq_queue = format!("{}_domain_events_dlq", namespace);
            self.message_client
                .create_queue(&dlq_queue)
                .await
                .map_err(|e| {
                    TaskerError::MessagingError(format!(
                        "Failed to create domain events DLQ {}: {}",
                        dlq_queue, e
                    ))
                })?;
        }

        info!("Domain event queues initialized successfully");
        Ok(())
    }

    /// Get main Tasker database pool reference (backward compatible)
    ///
    /// This returns the Tasker pool for task, step, and transition operations.
    /// For PGMQ-specific operations, use `pgmq_pool()`.
    pub fn database_pool(&self) -> &PgPool {
        self.database_pools.tasker()
    }

    /// Get PGMQ database pool reference (TAS-78)
    ///
    /// This returns the PGMQ pool for queue operations.
    /// May be the same as `database_pool()` in single-database deployments.
    pub fn pgmq_pool(&self) -> &PgPool {
        self.database_pools.pgmq()
    }

    /// Get database pools reference (TAS-78)
    ///
    /// Provides access to the full `DatabasePools` struct for advanced usage.
    pub fn database_pools(&self) -> &DatabasePools {
        &self.database_pools
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
    /// ## TAS-61 Phase 6C/6D: V2 Configuration (Canonical)
    /// ## TAS-78: Uses provided pool for both Tasker and PGMQ (single-DB test mode)
    ///
    /// Uses V2 config directly as the single source of truth.
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

        // TAS-61 Phase 6C/6D: V2 config is the single source of truth
        let config = config_manager.config();
        let tasker_config = Arc::new(config.clone());

        // TAS-78: Create DatabasePools with the provided pool (uses config for PGMQ if separate)
        let database_pools = DatabasePools::with_tasker_pool(config, database_pool.clone()).await?;

        // Create standard PGMQ client (no circuit breaker for simplicity)
        // TAS-78: Use PGMQ pool from DatabasePools
        let message_client = Arc::new(UnifiedPgmqClient::new_standard(
            PgmqClient::new_with_pool(database_pools.pgmq().clone()).await,
        ));

        // Create task handler registry with Tasker pool
        let task_handler_registry =
            Arc::new(TaskHandlerRegistry::new(database_pools.tasker().clone()));

        // Create event publisher with bounded channel (TAS-51)
        let event_publisher_buffer_size = tasker_config
            .common
            .mpsc_channels
            .event_publisher
            .event_queue_buffer_size as usize;
        let event_publisher = Arc::new(EventPublisher::with_capacity(event_publisher_buffer_size));

        Ok(Self {
            processor_uuid: system_id,
            tasker_config,
            message_client,
            database_pools,
            task_handler_registry,
            circuit_breaker_manager: None, // Disabled for testing
            event_publisher,
        })
    }
}
