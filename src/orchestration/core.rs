//! # Unified Orchestration Core
//!
//! Single-source-of-truth orchestration bootstrap system that eliminates
//! multiple entry points with different configuration assumptions.
//!
//! ## Architecture
//!
//! This module provides the unified bootstrap path that all orchestration
//! entry points use:
//! - embedded_bridge.rs uses OrchestrationCore::from_database_url()
//! - main orchestration_system.rs delegates to OrchestrationCore::from_config()
//! - Circuit breaker integration based on unified configuration
//! - Consistent component initialization everywhere
//!
//! ## Key Benefits
//!
//! 1. **Single Configuration Source**: Uses only src/config/mod.rs Config structure
//! 2. **Type Unification**: PgmqClientTrait allows seamless client switching
//! 3. **Consistent Bootstrap**: Same initialization logic everywhere
//! 4. **Circuit Breaker Integration**: config.circuit_breakers controls protection

use crate::config::{CircuitBreakerConfig, ConfigManager};
use crate::error::{Result, TaskerError};
use crate::messaging::{PgmqClient, PgmqClientTrait, ProtectedPgmqClient, UnifiedPgmqClient};
use crate::orchestration::{
    orchestration_loop::{OrchestrationLoop, OrchestrationLoopConfig},
    step_result_processor::StepResultProcessor,
    task_initializer::TaskInitializer,
    task_request_processor::{TaskRequestProcessor, TaskRequestProcessorConfig},
};
use crate::registry::TaskHandlerRegistry;
use crate::resilience::CircuitBreakerManager;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Unified orchestration core that all entry points use
pub struct OrchestrationCore {
    /// Unified PGMQ client (either standard or circuit-breaker protected)
    pub pgmq_client: Arc<UnifiedPgmqClient>,

    /// Database connection pool
    pub database_pool: PgPool,

    /// Task initializer for creating tasks from requests
    pub task_initializer: Arc<TaskInitializer>,

    /// Task request processor for handling incoming requests
    pub task_request_processor: Arc<TaskRequestProcessor>,

    /// Orchestration loop for continuous step enqueueing
    pub orchestration_loop: Arc<OrchestrationLoop>,

    /// Step result processor for handling completed steps
    pub step_result_processor: Arc<StepResultProcessor>,

    /// Task handler registry
    pub task_handler_registry: Arc<TaskHandlerRegistry>,

    /// Circuit breaker manager (optional, only when circuit breakers enabled)
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,
}

impl OrchestrationCore {
    /// Create OrchestrationCore with environment-aware configuration loading
    ///
    /// This is the primary initialization method that auto-detects environment and loads
    /// the appropriate configuration, then bootstraps all orchestration components
    /// with the parsed configuration settings.
    ///
    /// # Returns
    /// Fully configured OrchestrationCore with all components initialized from configuration
    pub async fn new() -> Result<Self> {
        info!("🔧 Initializing OrchestrationCore with auto-detected environment configuration");

        // Auto-detect environment and load configuration
        let config_manager = crate::config::ConfigManager::load().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to load configuration: {e}"))
        })?;

        Self::from_config(config_manager).await
    }

    /// Create OrchestrationCore from configuration manager
    ///
    /// This method provides the complete initialization path with configuration
    /// support, including circuit breaker integration based on config settings.
    ///
    /// # Arguments
    /// * `config_manager` - Loaded configuration manager (wrapped in Arc)
    ///
    /// # Returns
    /// Initialized OrchestrationCore with all configuration applied
    pub async fn from_config(config_manager: Arc<ConfigManager>) -> Result<Self> {
        info!("🔧 Initializing OrchestrationCore from configuration (environment-aware)");

        let config = config_manager.config();

        // Extract database connection settings from configuration
        let database_url = config.database_url();
        info!(
            "📊 CORE: Database URL derived from config: {} (pool_size: {})",
            database_url.chars().take(30).collect::<String>(),
            config.database.pool
        );

        // Create database connection pool with configuration
        let database_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.database.pool)
            .acquire_timeout(std::time::Duration::from_secs(
                config.database.checkout_timeout,
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

        // Circuit breaker configuration
        let circuit_breaker_config = if config.circuit_breakers.enabled {
            info!("🛡️ CORE: Circuit breakers enabled in configuration");
            Some(config.circuit_breakers.clone())
        } else {
            info!("📤 CORE: Circuit breakers disabled in configuration");
            None
        };

        // Pass config_manager (already Arc) for component initialization
        Self::from_pool_and_config(database_pool, config_manager, circuit_breaker_config).await
    }

    /// Internal constructor with full configuration support
    ///
    /// This method contains the unified bootstrap logic that creates all orchestration
    /// components with proper configuration passed to each component.
    async fn from_pool_and_config(
        database_pool: PgPool,
        config_manager: Arc<ConfigManager>,
        circuit_breaker_config: Option<CircuitBreakerConfig>,
    ) -> Result<Self> {
        info!("🏗️ Creating orchestration components with unified configuration");

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

        // Create task initializer with proper configuration support using for_testing approach
        // This approach sets up both TaskHandlerRegistry and TaskConfigFinder correctly
        let task_initializer = Arc::new(TaskInitializer::for_testing(database_pool.clone()));

        // Create orchestration loop configuration from environment config
        let orchestration_loop_config =
            OrchestrationLoopConfig::from_config_manager(&config_manager);

        // Create orchestration loop with environment-configured instance ID
        let orchestrator_id = format!(
            "orch-{}-{}",
            config.execution.environment,
            Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("default")
        );

        let orchestration_loop = Arc::new({
            info!("🛡️ CORE: OrchestrationLoop using unified client with circuit breaker support");
            OrchestrationLoop::with_unified_client(
                database_pool.clone(),
                pgmq_client.clone(),
                orchestrator_id,
                orchestration_loop_config.clone(),
            )
            .await?
        });

        // Create task request processor configuration from environment config
        let task_request_config = TaskRequestProcessorConfig {
            request_queue_name: config.orchestration.task_requests_queue_name.clone(),
            polling_interval_seconds: (config.orchestration.task_request_polling_interval_ms
                / 1000),
            visibility_timeout_seconds: config.orchestration.task_request_visibility_timeout_seconds
                as i32,
            batch_size: config.orchestration.task_request_batch_size as i32,
            max_processing_attempts: 3, // Keep default for now
        };

        let task_request_processor = Arc::new(TaskRequestProcessor::new(
            pgmq_client.clone(),
            task_handler_registry.clone(),
            task_initializer.clone(),
            task_request_config,
        ));

        // Create step result processor with configuration
        let step_result_processor =
            Arc::new(StepResultProcessor::new(database_pool.clone(), pgmq_client.clone()).await?);

        info!("✅ OrchestrationCore components created successfully");

        Ok(Self {
            pgmq_client,
            database_pool,
            task_initializer,
            task_request_processor,
            orchestration_loop,
            step_result_processor,
            task_handler_registry,
            circuit_breaker_manager,
        })
    }

    /// Initialize a task using the unified task initializer
    pub async fn initialize_task(
        &self,
        task_request: crate::models::core::task_request::TaskRequest,
    ) -> Result<crate::orchestration::TaskInitializationResult> {
        info!(
            namespace = %task_request.namespace,
            name = %task_request.name,
            version = %task_request.version,
            "🚀 CORE: Initializing task via unified orchestration core"
        );

        let result = self
            .task_initializer
            .create_task_from_request(task_request)
            .await?;

        info!(
            task_id = result.task_id,
            step_count = result.step_count,
            "✅ CORE: Task initialized successfully"
        );

        Ok(result)
    }

    /// Initialize standard namespace queues
    pub async fn initialize_queues(&self, namespaces: &[&str]) -> Result<()> {
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

    /// Enqueue ready steps for a task (compatibility method for embedded testing)
    ///
    /// This method triggers a single orchestration cycle to enqueue any steps that
    /// are ready for the specified task. In production, the OrchestrationLoop handles
    /// this automatically through continuous processing.
    pub async fn enqueue_ready_steps(
        &self,
        task_id: i64,
    ) -> Result<crate::orchestration::OrchestrationCycleResult> {
        info!(
            task_id = task_id,
            "🚀 CORE: Enqueueing ready steps for task via orchestration cycle"
        );

        // Run a single orchestration cycle which will discover and enqueue ready steps
        let result = self.orchestration_loop.run_cycle().await?;

        info!(
            task_id = task_id,
            tasks_processed = result.tasks_processed,
            steps_enqueued = result.total_steps_enqueued,
            "✅ CORE: Orchestration cycle completed - steps enqueued"
        );

        Ok(result)
    }
}
