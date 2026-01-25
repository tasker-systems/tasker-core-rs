// TAS-61 Phase 6C/6D: V2 config is now canonical
// TAS-78: Added DatabasePools for separate PGMQ database support
// TAS-133e: Updated to use MessagingProvider and MessageClient
// TAS-156: Added CacheProvider for distributed template caching
use crate::cache::CacheProvider;
use crate::config::tasker::TaskerConfig;
use crate::config::ConfigManager;
use crate::database::DatabasePools;
use crate::events::EventPublisher;
use crate::messaging::client::MessageClient;
use crate::messaging::service::{MessageRouterKind, MessagingProvider};
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
/// - Messaging provider and client (TAS-133e: strategy pattern)
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
///
/// ## TAS-133e: Messaging Strategy Pattern
///
/// Uses `MessagingProvider` enum for low-level queue operations and `MessageClient`
/// struct for domain-level messaging. Provider selection is configuration-driven
/// via `common.queues.backend` ("pgmq" or "rabbitmq").
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

    /// Low-level messaging provider (TAS-133e)
    ///
    /// Provides direct access to queue operations. Use `message_client` for
    /// domain-level operations; use this only for advanced/low-level needs.
    pub messaging_provider: Arc<MessagingProvider>,

    /// Domain-level message client (TAS-133e)
    ///
    /// Provides Tasker-specific messaging methods (send_step_message, etc.).
    /// Wraps `messaging_provider` with queue routing and domain types.
    pub message_client: Arc<MessageClient>,

    /// Database connection pools (TAS-78: supports separate PGMQ database)
    ///
    /// Contains both the main Tasker pool and the PGMQ pool.
    /// In single-database deployments, both pools point to the same database.
    database_pools: DatabasePools,

    /// Task handler registry
    pub task_handler_registry: Arc<TaskHandlerRegistry>,

    /// TAS-156: Distributed cache provider
    pub cache_provider: Arc<CacheProvider>,

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
            .field(
                "messaging_provider",
                &format!(
                    "Arc<MessagingProvider::{}>",
                    self.messaging_provider.provider_name()
                ),
            )
            .field("message_client", &"Arc<MessageClient>")
            .field("database_pools", &self.database_pools)
            .field("task_handler_registry", &"Arc<TaskHandlerRegistry>")
            .field(
                "cache_provider",
                &format!(
                    "Arc<CacheProvider::{}>",
                    self.cache_provider.provider_name()
                ),
            )
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
    /// See config/tasker/generated/orchestration-test.toml for reference.
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
    /// See config/tasker/generated/worker-test.toml for reference.
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

    /// Create messaging provider and client based on configuration (TAS-133e)
    ///
    /// Reads `common.queues.backend` to select the provider:
    /// - "pgmq" (default): Uses PostgreSQL Message Queue
    /// - "rabbitmq": Uses RabbitMQ (requires `common.queues.rabbitmq` config)
    ///
    /// Returns tuple of (MessagingProvider, MessageClient) wrapped in Arc.
    async fn create_messaging(
        config: &TaskerConfig,
        pgmq_pool: PgPool,
    ) -> TaskerResult<(Arc<MessagingProvider>, Arc<MessageClient>)> {
        let backend = config.common.queues.backend.as_str();

        info!(backend = %backend, "Creating messaging provider (TAS-133e)");

        let provider = match backend {
            "pgmq" => {
                info!("Using PGMQ messaging provider with PostgreSQL database");
                MessagingProvider::new_pgmq_with_pool(pgmq_pool).await
            }
            "rabbitmq" => {
                let rabbitmq_config = config.common.queues.rabbitmq.as_ref().ok_or_else(|| {
                    TaskerError::ConfigurationError(
                        "RabbitMQ backend selected but [common.queues.rabbitmq] config not found"
                            .to_string(),
                    )
                })?;

                info!(
                    "Using RabbitMQ messaging provider: {}",
                    // Redact credentials from URL for logging
                    rabbitmq_config.url.split('@').next_back().unwrap_or("***")
                );

                MessagingProvider::new_rabbitmq(rabbitmq_config)
                    .await
                    .map_err(|e| {
                        TaskerError::ConfigurationError(format!(
                            "Failed to create RabbitMQ provider: {}",
                            e
                        ))
                    })?
            }
            other => {
                return Err(TaskerError::ConfigurationError(format!(
                    "Unknown messaging backend '{}'. Supported: 'pgmq', 'rabbitmq'",
                    other
                )));
            }
        };

        let provider = Arc::new(provider);

        // Create router from queue configuration
        let router = MessageRouterKind::from_config(&config.common.queues);

        // Create domain-level client wrapping the provider
        let client = Arc::new(MessageClient::new(provider.clone(), router));

        info!(
            provider_name = %provider.provider_name(),
            "Messaging provider and client created successfully"
        );

        Ok((provider, client))
    }

    /// Get circuit breaker manager if enabled (TAS-133e: separated from messaging)
    fn create_circuit_breaker_manager(config: &TaskerConfig) -> Option<Arc<CircuitBreakerManager>> {
        if config.common.circuit_breakers.enabled {
            info!("Circuit breaker manager enabled");
            Some(Arc::new(CircuitBreakerManager::from_config(
                &config.common.circuit_breakers,
            )))
        } else {
            info!("Circuit breaker manager disabled");
            None
        }
    }

    /// Internal constructor with full configuration support
    ///
    /// This method contains the unified bootstrap logic that creates all shared system
    /// components with proper configuration passed to each component.
    ///
    /// ## TAS-61 Phase 6C/6D: V2 Configuration (Canonical)
    /// ## TAS-78: DatabasePools for separate PGMQ database support
    /// ## TAS-133e: MessagingProvider and MessageClient
    ///
    /// Uses V2 config directly as the single source of truth.
    /// Messaging provider is selected based on `common.queues.backend`.
    async fn from_pools_and_config(
        database_pools: DatabasePools,
        config_manager: Arc<ConfigManager>,
    ) -> TaskerResult<Self> {
        info!("Creating system components with V2 configuration (TAS-61 Phase 6C/6D + TAS-133e)");

        // TAS-61 Phase 6C/6D: V2 config is the single source of truth
        let config = config_manager.config();
        let tasker_config = Arc::new(config.clone());

        // TAS-133e: Create messaging provider and client based on config
        let (messaging_provider, message_client) =
            Self::create_messaging(&tasker_config, database_pools.pgmq().clone()).await?;

        // Circuit breaker manager (optional, separated from messaging in TAS-133e)
        let circuit_breaker_manager = Self::create_circuit_breaker_manager(&tasker_config);

        // TAS-156: Create cache provider (graceful degradation if Redis unavailable)
        let cache_provider = Arc::new(Self::create_cache_provider(&tasker_config).await);

        // Create task handler registry with Tasker pool and cache
        let task_handler_registry = Arc::new(
            if let Some(cache_config) = tasker_config.common.cache.clone() {
                TaskHandlerRegistry::with_cache(
                    database_pools.tasker().clone(),
                    cache_provider.clone(),
                    cache_config,
                )
            } else {
                TaskHandlerRegistry::new(database_pools.tasker().clone())
            },
        );

        // Create system instance ID
        let system_id = Uuid::now_v7();

        // Create event publisher with bounded channel (TAS-51)
        let event_publisher_buffer_size = tasker_config
            .common
            .mpsc_channels
            .event_publisher
            .event_queue_buffer_size as usize;
        let event_publisher = Arc::new(EventPublisher::with_capacity(event_publisher_buffer_size));

        info!(
            provider = %messaging_provider.provider_name(),
            "SystemContext components created successfully"
        );

        Ok(Self {
            processor_uuid: system_id,
            tasker_config,
            messaging_provider,
            message_client,
            database_pools,
            task_handler_registry,
            cache_provider,
            circuit_breaker_manager,
            event_publisher,
        })
    }

    /// Create cache provider from configuration (TAS-156, TAS-171)
    ///
    /// Uses graceful degradation: if Redis is unavailable, returns NoOp provider.
    /// The system never fails to start due to cache issues.
    ///
    /// TAS-171: Passes circuit breaker config for distributed cache protection.
    async fn create_cache_provider(config: &TaskerConfig) -> CacheProvider {
        // TAS-171: Pass circuit breaker config if enabled
        let cb_config = if config.common.circuit_breakers.enabled {
            Some(&config.common.circuit_breakers)
        } else {
            None
        };

        match &config.common.cache {
            Some(cache_config) => {
                CacheProvider::from_config_graceful(cache_config, cb_config).await
            }
            None => {
                info!("No cache configuration found, using NoOp cache provider");
                CacheProvider::noop()
            }
        }
    }

    /// Initialize standard namespace queues
    pub async fn initialize_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        info!("Initializing namespace queues via MessageClient (TAS-133e)");

        self.message_client
            .initialize_namespace_queues(namespaces)
            .await?;

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
            self.message_client.ensure_queue(queue_name).await?;
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
            self.message_client.ensure_queue(&main_queue).await?;

            // Create DLQ for failed events
            let dlq_queue = format!("{}_domain_events_dlq", namespace);
            self.message_client.ensure_queue(&dlq_queue).await?;
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

    /// Get messaging provider reference (TAS-133e)
    ///
    /// Provides access to the low-level messaging provider for advanced operations.
    /// Prefer `message_client()` for domain-level messaging operations.
    pub fn messaging_provider(&self) -> &Arc<MessagingProvider> {
        &self.messaging_provider
    }

    /// Get message client reference (TAS-133e)
    ///
    /// Provides access to the domain-level messaging client with Tasker-specific
    /// methods like `send_step_message`, `send_task_request`, etc.
    pub fn message_client(&self) -> Arc<MessageClient> {
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

    /// Get cache provider reference (TAS-156)
    pub fn cache_provider(&self) -> &Arc<CacheProvider> {
        &self.cache_provider
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
    /// ## TAS-133e: Uses MessagingProvider and MessageClient
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

        // TAS-133e: Create messaging provider and client (uses PGMQ for tests)
        let (messaging_provider, message_client) =
            Self::create_messaging(&tasker_config, database_pools.pgmq().clone()).await?;

        // TAS-156: Create cache provider (graceful degradation for tests)
        let cache_provider = Arc::new(Self::create_cache_provider(&tasker_config).await);

        // Create task handler registry with Tasker pool and cache
        let task_handler_registry = Arc::new(
            if let Some(cache_config) = tasker_config.common.cache.clone() {
                TaskHandlerRegistry::with_cache(
                    database_pools.tasker().clone(),
                    cache_provider.clone(),
                    cache_config,
                )
            } else {
                TaskHandlerRegistry::new(database_pools.tasker().clone())
            },
        );

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
            messaging_provider,
            message_client,
            database_pools,
            task_handler_registry,
            cache_provider,
            circuit_breaker_manager: None, // Disabled for testing
            event_publisher,
        })
    }
}
