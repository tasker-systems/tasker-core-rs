//! # Shared Orchestration System
//!
//! Language-agnostic orchestration system core that can be shared across
//! Ruby, Python, Node.js, WASM, and JNI bindings while preserving the
//! handle-based architecture that eliminates connection pool exhaustion.

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::events::EventPublisher;
use crate::execution::zeromq_pub_sub_executor::ZmqPubSubExecutor;
use crate::orchestration::config::{ConfigurationManager, DatabasePoolConfig};
use crate::orchestration::state_manager::StateManager;
use crate::orchestration::task_initializer::{TaskInitializationConfig, TaskInitializer};
use crate::orchestration::workflow_coordinator::{WorkflowCoordinator, WorkflowCoordinatorConfig};
use crate::registry::TaskHandlerRegistry;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::OnceLock;
use tracing::{debug, info, warn};
use zmq::Context;

/// Global orchestration system singleton
static GLOBAL_ORCHESTRATION_SYSTEM: OnceLock<Arc<OrchestrationSystem>> = OnceLock::new();

/// Shared orchestration resources
pub struct OrchestrationSystem {
    pub database_pool: PgPool,
    pub event_publisher: EventPublisher,
    pub workflow_coordinator: WorkflowCoordinator,
    pub state_manager: StateManager,
    pub task_initializer: TaskInitializer,
    pub task_handler_registry: Arc<TaskHandlerRegistry>,
    pub config_manager: Arc<ConfigurationManager>,
    pub zmq_pub_sub_executor: Option<Arc<ZmqPubSubExecutor>>,
    pub zmq_context: Arc<Context>,
}

/// Check if we're running in a test environment
fn is_test_environment() -> bool {
    std::env::var("RAILS_ENV").unwrap_or_default() == "test"
        || std::env::var("APP_ENV").unwrap_or_default() == "test"
        || std::env::var("RACK_ENV").unwrap_or_default() == "test"
        || std::env::var("TASKER_ENV").unwrap_or_default() == "test"
}

/// Create a database pool from configuration instead of hardcoded values
/// This replaces the hardcoded pool options in get_global_database_pool()
async fn create_pool_from_config(pool_config: &DatabasePoolConfig) -> Result<PgPool, sqlx::Error> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

    info!(
        "🔧 CONFIG-DRIVEN POOL: Creating pool with config: max={}, min={}, acquire_timeout={}s",
        pool_config.max_connections,
        pool_config.min_connections,
        pool_config.acquire_timeout_seconds
    );

    // Test connection first before creating pool
    debug!("🔍 POOL: Testing connection to {}", database_url);
    let test_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&database_url)
        .await;

    match test_pool {
        Ok(pool) => {
            info!("✅ POOL: Test connection successful");
            pool.close().await;
        }
        Err(e) => {
            warn!("❌ POOL: Test connection failed: {}", e);
            return Err(e);
        }
    }

    // Create pool with full configuration
    let pool_options = sqlx::postgres::PgPoolOptions::new()
        .max_connections(pool_config.max_connections)
        .min_connections(pool_config.min_connections)
        .acquire_timeout(std::time::Duration::from_secs(
            pool_config.acquire_timeout_seconds,
        ))
        .idle_timeout(std::time::Duration::from_secs(
            pool_config.idle_timeout_seconds,
        ))
        .max_lifetime(std::time::Duration::from_secs(
            pool_config.max_lifetime_seconds,
        ));

    debug!(
        "🔍 POOL: Creating pool with options - max: {}, min: {}",
        pool_config.max_connections, pool_config.min_connections
    );

    let final_pool = pool_options.connect(&database_url).await?;

    info!(
        "✅ POOL: Created successfully - size: {}, max: {}",
        final_pool.size(),
        final_pool.options().get_max_connections()
    );

    Ok(final_pool)
}

/// 🎯 UNIFIED ENTRY POINT: Single way to create orchestration system
/// This replaces all the scattered initialization methods with one clear path:
/// Configuration → Pool → OrchestrationSystem
///
/// # Architecture:
/// 1. Load configuration from files (not hardcoded)
/// 2. Create pool from configuration
/// 3. Create orchestration system with owned pool
/// 4. OrchestrationSystem owns the pool, TestingFramework references it
async fn create_unified_orchestration_system(
) -> Result<OrchestrationSystem, Box<dyn std::error::Error + Send + Sync>> {
    // Initialize structured logging first
    crate::logging::init_structured_logging();

    info!("🎯 UNIFIED ENTRY: Creating orchestration system from configuration");

    // CRITICAL FIX: Load environment variables from .env.test file
    let is_test = is_test_environment();
    debug!("🔍 ENV CHECK: is_test_environment() = {}", is_test);
    debug!(
        "🔍 ENV CHECK: RAILS_ENV = {:?}, APP_ENV = {:?}, RACK_ENV = {:?}, TASKER_ENV = {:?}",
        std::env::var("RAILS_ENV"),
        std::env::var("APP_ENV"),
        std::env::var("RACK_ENV"),
        std::env::var("TASKER_ENV")
    );

    // 1. Load configuration from environment/files
    // Use TASKER_ENV as the primary environment variable
    let environment = std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string());

    info!(
        "🔧 UNIFIED CONFIG: Loading configuration for environment: {}",
        environment
    );

    // Load the config file based on environment (tasker-config-{env}.yaml)
    let config_filename = format!("config/tasker-config-{environment}.yaml");

    // First check if we're in the Ruby bindings directory and need to go up
    let final_config_path = if std::path::Path::new("../../config").exists() {
        format!("../../{config_filename}")
    } else {
        config_filename.to_string()
    };

    debug!(
        "🔧 UNIFIED CONFIG: Loading from file: {}",
        final_config_path
    );

    let config_manager = if std::path::Path::new(&final_config_path).exists() {
        // Load from YAML file
        match ConfigurationManager::load_from_file(&final_config_path).await {
            Ok(manager) => {
                info!(
                    "✅ UNIFIED CONFIG: Successfully loaded configuration from {}",
                    final_config_path
                );
                Arc::new(manager)
            }
            Err(e) => {
                warn!(
                    "⚠️  UNIFIED CONFIG: Failed to load config file: {}. Using defaults.",
                    e
                );
                Arc::new(ConfigurationManager::new())
            }
        }
    } else {
        warn!(
            "⚠️  UNIFIED CONFIG: Config file not found at {}. Using defaults.",
            final_config_path
        );
        Arc::new(ConfigurationManager::new())
    };

    // 2. Get database pool configuration from config files (no environment-specific overrides)
    let pool_config = config_manager.system_config().database.pool.clone();

    debug!("🔧 UNIFIED CONFIG: Using pool configuration from config files: max={}, min={}, acquire_timeout={}s",
        pool_config.max_connections,
        pool_config.min_connections,
        pool_config.acquire_timeout_seconds);

    // 4. Create pool from configuration (not hardcoded!)
    let database_pool = create_pool_from_config(&pool_config).await?;

    // 5. Create orchestration system components using the owned pool
    info!("🎯 UNIFIED ENTRY: Creating orchestration components with owned pool");

    let event_publisher = EventPublisher::new();
    let sql_function_executor = SqlFunctionExecutor::new(database_pool.clone());
    let state_manager = StateManager::new(
        sql_function_executor.clone(),
        event_publisher.clone(),
        database_pool.clone(),
    );
    let shared_registry = Arc::new(TaskHandlerRegistry::with_event_publisher(
        event_publisher.clone(),
    ));

    // Store shared registry Arc for system storage
    let registry_for_system = shared_registry.clone();

    // Initialize ZeroMQ components if enabled
    let zmq_context = Arc::new(Context::new());
    let zmq_pub_sub_executor = if config_manager.system_config().zeromq.enabled {
        info!("🚀 ZeroMQ: Initializing ZmqPubSubExecutor with configuration");

        let zeromq_config = &config_manager.system_config().zeromq;

        // Create TaskFinalizer for ZmqPubSubExecutor
        let zmq_task_finalizer = crate::orchestration::task_finalizer::TaskFinalizer::with_event_publisher(
            database_pool.clone(),
            event_publisher.clone(),
        );
        
        // Create StateManager for ZmqPubSubExecutor
        let zmq_state_manager = StateManager::new(
            sql_function_executor.clone(),
            event_publisher.clone(),
            database_pool.clone(),
        );
        
        match ZmqPubSubExecutor::new(
            &zeromq_config.batch_endpoint,
            &zeromq_config.result_endpoint,
            database_pool.clone(),
            zmq_state_manager,
            zmq_task_finalizer,
            shared_registry.clone(),
        ).await {
            Ok(executor) => {
                info!("✅ ZeroMQ: ZmqPubSubExecutor initialized successfully");
                Some(Arc::new(executor))
            }
            Err(e) => {
                warn!("⚠️  ZeroMQ: Failed to initialize ZmqPubSubExecutor: {}", e);
                None
            }
        }
    } else {
        info!("ℹ️  ZeroMQ: ZmqPubSubExecutor disabled in configuration");
        None
    };

    // Create WorkflowCoordinator with ZmqPubSubExecutor if available
    let workflow_coordinator = if let Some(ref zmq_executor) = zmq_pub_sub_executor {
        info!("🚀 ZeroMQ: Creating WorkflowCoordinator with ZmqPubSubExecutor");
        WorkflowCoordinator::with_zmq_pub_sub_executor(
            database_pool.clone(),
            WorkflowCoordinatorConfig::default(),
            config_manager.clone(),
            event_publisher.clone(),
            shared_registry.as_ref().clone(),
            zmq_executor.clone(),
        )
    } else {
        info!("ℹ️  ZeroMQ: Creating WorkflowCoordinator without ZmqPubSubExecutor");
        WorkflowCoordinator::with_shared_registry(
            database_pool.clone(),
            WorkflowCoordinatorConfig::default(),
            config_manager.clone(),
            event_publisher.clone(),
            shared_registry.as_ref().clone(),
        )
    };
    let task_initializer = TaskInitializer::with_state_manager_and_registry(
        database_pool.clone(),
        TaskInitializationConfig::default(),
        event_publisher.clone(),
        shared_registry.clone(),
    );




    // Create orchestration system with owned pool and ZeroMQ components
    let orchestration_system = OrchestrationSystem {
        database_pool,
        event_publisher,
        workflow_coordinator,
        state_manager,
        task_initializer,
        task_handler_registry: registry_for_system,
        config_manager,
        zmq_pub_sub_executor,
        zmq_context,
    };

    info!("✅ UNIFIED ENTRY: Orchestration system created successfully");
    Ok(orchestration_system)
}

impl OrchestrationSystem {
    /// Access the database pool owned by this orchestration system
    /// This is the new unified way to access the pool instead of global pool functions
    pub fn database_pool(&self) -> &PgPool {
        &self.database_pool
    }

    /// Access the ZeroMQ pub-sub executor for comprehensive batch execution
    pub fn zmq_pub_sub_executor(&self) -> Option<&Arc<ZmqPubSubExecutor>> {
        self.zmq_pub_sub_executor.as_ref()
    }

    /// Access the shared ZeroMQ context
    pub fn zmq_context(&self) -> &Arc<Context> {
        &self.zmq_context
    }

    /// Check if ZeroMQ batch processing is enabled and available
    pub fn is_zeromq_enabled(&self) -> bool {
        self.zmq_pub_sub_executor.is_some()
    }
}

/// 🎯 UNIFIED ENTRY POINT: Single way to initialize the global orchestration system
///
/// This replaces all scattered initialization methods with one clear path:
/// Configuration → Pool → OrchestrationSystem
///
/// # New Architecture:
/// - Configuration-driven pool creation (not hardcoded)
/// - OrchestrationSystem owns the database pool
/// - TestingFramework will reference the orchestration system (not be embedded)
/// - Single initialization path prevents multiple instances
pub fn initialize_unified_orchestration_system() -> Arc<OrchestrationSystem> {
    info!("🎯 UNIFIED ENTRY: initialize_unified_orchestration_system called");
    GLOBAL_ORCHESTRATION_SYSTEM
        .get_or_init(|| {
            info!("🎯 UNIFIED ENTRY: Using global runtime for consistent execution context");
            let runtime = get_global_runtime();

            Arc::new(
                runtime
                    .block_on(create_unified_orchestration_system())
                    .expect("Failed to create unified orchestration system"),
            )
        })
        .clone()
}

/// Execute async operation using the current or global runtime
/// Global runtime for consistent async execution context
static GLOBAL_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Get or create the global runtime
fn get_global_runtime() -> &'static tokio::runtime::Runtime {
    GLOBAL_RUNTIME.get_or_init(|| {
        info!("🔧 RUNTIME: Creating global Tokio runtime for consistent execution context");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4) // Small number for FFI context
            .thread_name("tasker-core-runtime")
            .build()
            .expect("Failed to create global runtime")
    })
}

pub fn execute_async<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    // CRITICAL FIX: Always use the same global runtime to avoid pool context issues
    let runtime = get_global_runtime();

    // RUBY THREADING FIX: Use LocalSet to maintain Ruby thread context for spawn_local
    // This ensures that spawn_local tasks have access to the Ruby interpreter
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(future))
}

/// Get the global event publisher
pub fn get_global_event_publisher() -> EventPublisher {
    initialize_unified_orchestration_system()
        .event_publisher
        .clone()
}

/// Get the global database pool through the unified orchestration system
/// 🎯 UNIFIED ARCHITECTURE: All pool access goes through orchestration system
pub fn get_global_database_pool() -> PgPool {
    debug!("🔍 POOL TRACE: get_global_database_pool() called - delegating to orchestration system");

    // Get the pool from the unified orchestration system
    let orchestration_system = initialize_unified_orchestration_system();

    // Return a clone of the orchestration system's pool
    orchestration_system.database_pool().clone()
}

/// Get the global task handler registry
pub fn get_global_task_handler_registry() -> Arc<TaskHandlerRegistry> {
    initialize_unified_orchestration_system()
        .task_handler_registry
        .clone()
}

// ===== SHARED CORE LOGIC ENDS HERE =====
// Ruby-specific wrapper functions have been moved to bindings/ruby/
// Language bindings should implement their own wrapper functions that
// call the shared core functions above.
