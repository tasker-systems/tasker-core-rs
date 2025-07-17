//! # Global Resource Management
//!
//! Singleton pattern for shared orchestration resources to avoid recreating
//! expensive components (database connections, tokio runtime, event publisher)
//! on every FFI call.

use std::sync::OnceLock;
use tasker_core::events::EventPublisher;
use tasker_core::orchestration::workflow_coordinator::WorkflowCoordinator;
use tasker_core::orchestration::state_manager::StateManager;
use tasker_core::orchestration::task_initializer::TaskInitializer;
use tasker_core::orchestration::step_executor::StepExecutor;
use tasker_core::orchestration::config::{ConfigurationManager, DatabasePoolConfig};
use tasker_core::database::sql_functions::SqlFunctionExecutor;
use tasker_core::registry::TaskHandlerRegistry;
use tasker_core::orchestration::task_config_finder::TaskConfigFinder;
use sqlx::PgPool;
use std::sync::Arc;

/// Global orchestration system singleton
static GLOBAL_ORCHESTRATION_SYSTEM: OnceLock<Arc<OrchestrationSystem>> = OnceLock::new();

/// Shared orchestration resources
pub struct OrchestrationSystem {
    pub database_pool: PgPool,
    pub event_publisher: EventPublisher,
    pub workflow_coordinator: WorkflowCoordinator,
    pub state_manager: StateManager,
    pub task_initializer: TaskInitializer,
    pub task_handler_registry: TaskHandlerRegistry,
    pub step_executor: StepExecutor,
    pub config_manager: Arc<ConfigurationManager>
}

/// Check if we're running in a test environment
fn is_test_environment() -> bool {
    std::env::var("RAILS_ENV").unwrap_or_default() == "test"
        || std::env::var("APP_ENV").unwrap_or_default() == "test"
        || std::env::var("RACK_ENV").unwrap_or_default() == "test"
}

/// Create a database pool from configuration instead of hardcoded values
/// This replaces the hardcoded pool options in get_global_database_pool()
async fn create_pool_from_config(pool_config: &DatabasePoolConfig) -> Result<PgPool, sqlx::Error> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

    println!("🔧 CONFIG-DRIVEN POOL: Creating pool with config: max={}, min={}, acquire_timeout={}s",
        pool_config.max_connections,
        pool_config.min_connections,
        pool_config.acquire_timeout_seconds);

    sqlx::postgres::PgPoolOptions::new()
        .max_connections(pool_config.max_connections)
        .min_connections(pool_config.min_connections)
        .acquire_timeout(std::time::Duration::from_secs(pool_config.acquire_timeout_seconds))
        .idle_timeout(std::time::Duration::from_secs(pool_config.idle_timeout_seconds))
        .max_lifetime(std::time::Duration::from_secs(pool_config.max_lifetime_seconds))
        .connect(&database_url)
        .await
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
async fn create_unified_orchestration_system() -> Result<OrchestrationSystem, Box<dyn std::error::Error + Send + Sync>> {
    println!("🎯 UNIFIED ENTRY: Creating orchestration system from configuration");

    // 1. Load configuration from environment/files
    // Use TASKER_ENV as the primary environment variable
    let environment = std::env::var("TASKER_ENV")
        .unwrap_or_else(|_| "development".to_string());

    println!("🔧 UNIFIED CONFIG: Loading configuration for environment: {}", environment);

    // Load the config file based on environment (tasker-config-{env}.yaml)
    let config_filename = format!("config/tasker-config-{}.yaml", environment);

    // First check if we're in the Ruby bindings directory and need to go up
    let final_config_path = if std::path::Path::new("../../config").exists() {
        format!("../../{}", config_filename)
    } else {
        config_filename.to_string()
    };

    println!("🔧 UNIFIED CONFIG: Loading from file: {}", final_config_path);

    let config_manager = if std::path::Path::new(&final_config_path).exists() {
        // Load from YAML file
        match ConfigurationManager::load_from_file(&final_config_path).await {
            Ok(manager) => {
                println!("✅ UNIFIED CONFIG: Successfully loaded configuration from {}", final_config_path);
                Arc::new(manager)
            },
            Err(e) => {
                println!("⚠️  UNIFIED CONFIG: Failed to load config file: {}. Using defaults.", e);
                Arc::new(ConfigurationManager::new())
            }
        }
    } else {
        println!("⚠️  UNIFIED CONFIG: Config file not found at {}. Using defaults.", final_config_path);
        Arc::new(ConfigurationManager::new())
    };

    // 2. Get database pool configuration from config files (no environment-specific overrides)
    let pool_config = config_manager.system_config().database.pool.clone();

    println!("🔧 UNIFIED CONFIG: Using pool configuration from config files: max={}, min={}, acquire_timeout={}s",
        pool_config.max_connections,
        pool_config.min_connections,
        pool_config.acquire_timeout_seconds);

    // 4. Create pool from configuration (not hardcoded!)
    let database_pool = create_pool_from_config(&pool_config).await?;

    // 5. Create orchestration system components using the owned pool
    println!("🎯 UNIFIED ENTRY: Creating orchestration components with owned pool");

    let event_publisher = EventPublisher::new();
    let sql_function_executor = SqlFunctionExecutor::new(database_pool.clone());
    let state_manager = StateManager::new(sql_function_executor.clone(), event_publisher.clone(), database_pool.clone());
    let workflow_coordinator = WorkflowCoordinator::new(database_pool.clone());
    let task_initializer = TaskInitializer::new(database_pool.clone());
    let task_handler_registry = TaskHandlerRegistry::new();

    // Create config finder
    let task_config_finder = TaskConfigFinder::new(
        config_manager.clone(),
        Arc::new(task_handler_registry.clone())
    );

    let step_executor = StepExecutor::new(
        state_manager.clone(),
        task_handler_registry.clone(),
        event_publisher.clone(),
        task_config_finder,
    );

    // Create orchestration system with owned pool (NO testing components!)
    let orchestration_system = OrchestrationSystem {
        database_pool,
        event_publisher,
        workflow_coordinator,
        state_manager,
        task_initializer,
        task_handler_registry,
        step_executor,
        config_manager
    };

    println!("✅ UNIFIED ENTRY: Orchestration system created successfully");
    Ok(orchestration_system)
}

impl OrchestrationSystem {
    /// Create new orchestration system with all required components
    async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        println!("🔍 ORCHESTRATION TRACE: OrchestrationSystem::new() called");

        // Get database connection with appropriate pool size for environment
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());
        println!("🔍 ORCHESTRATION TRACE: Database URL: {}", database_url);

        // Configure pool options based on environment
        let is_test = std::env::var("RAILS_ENV").unwrap_or_default() == "test"
            || std::env::var("APP_ENV").unwrap_or_default() == "test";

        let pool_options = if is_test {
            // Test environment: fail fast to debug pool issues quickly
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(10)  // Increased for debugging
                .min_connections(2)
                .acquire_timeout(std::time::Duration::from_secs(2))  // FAIL FAST: 2 second timeout
                .idle_timeout(std::time::Duration::from_secs(30))    // Shorter idle timeout
                .max_lifetime(std::time::Duration::from_secs(300))   // Shorter lifetime
        } else {
            // Production environment: larger pool
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(20)
                .min_connections(5)
                .acquire_timeout(std::time::Duration::from_secs(30))
                .idle_timeout(std::time::Duration::from_secs(300))
                .max_lifetime(std::time::Duration::from_secs(3600))
        };

        println!("🔍 ORCHESTRATION TRACE: Attempting to connect to database for orchestration system");
        let database_pool = pool_options.connect(&database_url).await?;
        println!("🔍 ORCHESTRATION TRACE: Successfully created orchestration system database pool with {} max connections", database_pool.size());

        Self::new_with_pool(database_pool).await
    }

    /// Create new orchestration system using an existing database pool
    /// CRITICAL: This prevents connection pool exhaustion by reusing the same pool
    async fn new_with_pool(database_pool: PgPool) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        println!("🔍 ORCHESTRATION TRACE: OrchestrationSystem::new_with_pool() called with pool size: {}", database_pool.size());

        // Create core components with FFI-compatible configuration
        let mut event_config = tasker_core::events::publisher::EventPublisherConfig::default();
        event_config.async_processing = false; // Disable async processing in FFI context
        event_config.ffi_enabled = false; // Disable FFI bridge that might use tokio::spawn
        let event_publisher = EventPublisher::with_config(event_config);
        // CRITICAL ISSUE: We're cloning the database pool multiple times during initialization
        println!("🔍 POOL TRACE: CLONE #1 - Creating SqlFunctionExecutor (pool size: {})", database_pool.size());
        let sql_executor = SqlFunctionExecutor::new(database_pool.clone());

        println!("🔍 POOL TRACE: CLONE #2 - Creating StateManager (pool size: {})", database_pool.size());
        let state_manager = StateManager::new(sql_executor, event_publisher.clone(), database_pool.clone());

        let task_handler_registry = TaskHandlerRegistry::with_event_publisher(event_publisher.clone());

        // Create config manager
        let config_manager = Arc::new(ConfigurationManager::new());

        println!("🔍 POOL TRACE: CLONE #3 - Creating WorkflowCoordinator (pool size: {})", database_pool.size());
        // Create workflow coordinator with the shared event publisher
        let workflow_coordinator = WorkflowCoordinator::with_config_manager_and_publisher(
            database_pool.clone(),
            Default::default(), // Use default WorkflowCoordinatorConfig
            config_manager.clone(),
            Some(event_publisher.clone()), // Pass the FFI-configured event publisher
        );
        let task_config_finder = TaskConfigFinder::new(config_manager.clone(), Arc::new(task_handler_registry.clone()));

        // Create step executor
        let step_executor = StepExecutor::new(
            state_manager.clone(),
            task_handler_registry.clone(),
            event_publisher.clone(),
            task_config_finder.clone()
        );

        println!("🔍 POOL TRACE: CLONE #4 - Creating TaskInitializer (pool size: {})", database_pool.size());
        // Create task initializer
        let task_initializer = TaskInitializer::with_state_manager(
            database_pool.clone(),
            Default::default(),
            event_publisher.clone()
        );

        Ok(OrchestrationSystem {
            database_pool,
            event_publisher,
            workflow_coordinator,
            state_manager,
            task_initializer,
            task_handler_registry,
            step_executor,
            config_manager
        })
    }

    /// Access the database pool owned by this orchestration system
    /// This is the new unified way to access the pool instead of global pool functions
    pub fn database_pool(&self) -> &PgPool {
        &self.database_pool
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
    println!("🎯 UNIFIED ENTRY: initialize_unified_orchestration_system called");
    GLOBAL_ORCHESTRATION_SYSTEM.get_or_init(|| {
        // Check if we're already in a runtime context
        let orchestration_system = if tokio::runtime::Handle::try_current().is_ok() {
            // Use the existing runtime context
            println!("🎯 UNIFIED ENTRY: Using existing runtime context");
            // Use Tokio's block_in_place for async calls in sync context
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(create_unified_orchestration_system())
            }).expect("Failed to create unified orchestration system")
        } else {
            // Create our own runtime
            println!("🎯 UNIFIED ENTRY: Creating new runtime context");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime for unified orchestration system");

            rt.block_on(async {
                create_unified_orchestration_system().await
                    .expect("Failed to create unified orchestration system")
            })
        };

        Arc::new(orchestration_system)
    }).clone()
}

/// ⚠️ DEPRECATED: Use initialize_unified_orchestration_system() instead
/// This function is kept for backward compatibility but should be migrated
pub fn initialize_global_orchestration_system_from_current_runtime() -> Arc<OrchestrationSystem> {
    println!("⚠️  DEPRECATED: Use initialize_unified_orchestration_system() instead");
    initialize_unified_orchestration_system()
}

/// ⚠️ DEPRECATED: Use initialize_unified_orchestration_system() instead
/// Get or initialize the global orchestration system (production version)
pub fn get_global_orchestration_system() -> Arc<OrchestrationSystem> {
    println!("⚠️  DEPRECATED: get_global_orchestration_system() - use initialize_unified_orchestration_system() instead");
    initialize_unified_orchestration_system()
}

/// Execute async operation using the current or global runtime
pub fn execute_async<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    // Check if we're already in a Tokio runtime context
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // Use existing runtime
        handle.block_on(future)
    } else {
        // Create a temporary runtime for this operation
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime for async operation");
        rt.block_on(future)
    }
}

/// Get the global event publisher
pub fn get_global_event_publisher() -> EventPublisher {
    initialize_unified_orchestration_system().event_publisher.clone()
}

/// Get the global database pool through the unified orchestration system
/// 🎯 UNIFIED ARCHITECTURE: All pool access goes through orchestration system
pub fn get_global_database_pool() -> PgPool {
    println!("🔍 POOL TRACE: get_global_database_pool() called - delegating to orchestration system");

    // Get the pool from the unified orchestration system
    let orchestration_system = initialize_unified_orchestration_system();

    // Return a clone of the orchestration system's pool
    orchestration_system.database_pool().clone()
}

/// Create a temporary database pool for single operations
/// CRITICAL: This pool should be explicitly closed after use
pub fn create_temporary_database_pool() -> PgPool {
    println!("🔍 TEMP POOL TRACE: Creating temporary database pool for single operation");

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

    let pool_options = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)  // Single connection for temporary use
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .idle_timeout(std::time::Duration::from_secs(30))
        .max_lifetime(std::time::Duration::from_secs(60));

    // Create pool synchronously
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();
    let database_url_clone = database_url.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime for database connection");

        let result = rt.block_on(async {
            pool_options.connect(&database_url_clone).await
        });

        tx.send(result).expect("Failed to send pool creation result");
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(pool)) => {
            println!("🔍 TEMP POOL TRACE: Successfully created temporary database pool with {} connections", pool.size());
            pool
        },
        Ok(Err(e)) => panic!("Failed to create temporary database pool: {}", e),
        Err(_) => panic!("Temporary database pool creation timeout after 5 seconds"),
    }
}

/// Explicitly close a database pool and wait for all connections to be closed
/// CRITICAL: This is required for proper connection cleanup per SQLx documentation
pub fn close_database_pool(pool: PgPool) {
    println!("🔍 POOL CLOSE TRACE: Explicitly closing database pool with {} connections", pool.size());

    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime for pool close");

        rt.block_on(async {
            pool.close().await;
            println!("🔍 POOL CLOSE TRACE: Database pool explicitly closed");
        });

        // Ignore send errors - receiver may have timed out and been dropped
        let _ = tx.send(());
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(()) => println!("🔍 POOL CLOSE TRACE: Pool close completed successfully"),
        Err(_) => println!("🔍 POOL CLOSE TRACE: Pool close timeout - connections may still be open"),
    }
}

/// Get the global task handler registry
pub fn get_global_task_handler_registry() -> TaskHandlerRegistry {
    initialize_unified_orchestration_system().task_handler_registry.clone()
}

/// Registry FFI Functions - moved from registry.rs for consolidation

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use magnus::{Error, RModule, Ruby, Value, TryConvert};
use magnus::value::ReprValue;
use tasker_core::models::core::task_request::TaskRequest;

/// Create a new registry (actually returns reference to singleton)
pub fn registry_new_wrapper() -> Result<Value, Error> {
    let registry = get_global_task_handler_registry();

    let stats = registry.stats().map_err(|e| {
        Error::new(
            Ruby::get().unwrap().exception_runtime_error(),
            format!("Failed to get registry stats: {}", e)
        )
    })?;

    json_to_ruby_value(serde_json::json!({
        "type": "TaskHandlerRegistry",
        "status": "initialized",
        "total_handlers": stats.total_handlers,
        "total_ffi_handlers": stats.total_ffi_handlers,
        "namespaces": stats.namespaces
    }))
}

/// Register an FFI handler with the core registry
pub fn register_ffi_handler_wrapper(handler_data_value: Value) -> Result<Value, Error> {
    let handler_data = ruby_value_to_json(handler_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid handler data: {}", e)))?;

    let result = execute_async(async {
        // Extract handler registration parameters
        let namespace = handler_data.get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let name = handler_data.get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing handler name")?;
        let version = handler_data.get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.1.0");
        let handler_class = handler_data.get("handler_class")
            .and_then(|v| v.as_str())
            .ok_or("Missing handler_class")?;
        let config_schema = handler_data.get("config_schema").cloned();

        // Register with the core registry
        let registry = get_global_task_handler_registry();
        registry.register_ffi_handler(
            namespace,
            name,
            version,
            handler_class,
            config_schema
        ).await.map_err(|e| format!("Handler registration failed: {}", e))?;

        Ok::<serde_json::Value, String>(serde_json::json!({
            "status": "registered",
            "namespace": namespace,
            "name": name,
            "version": version,
            "handler_class": handler_class
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// Find handler using the core registry
pub fn find_handler_wrapper(task_request_value: Value) -> Result<Value, Error> {
    let task_request_data = ruby_value_to_json(task_request_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid task request: {}", e)))?;

    let result: Result<serde_json::Value, String> = execute_async(async {
        // Parse TaskRequest
        let task_request: TaskRequest = serde_json::from_value(task_request_data)
            .map_err(|e| format!("Failed to parse TaskRequest: {}", e))?;

        // Use core registry to find handler
        let registry = get_global_task_handler_registry();
        match registry.resolve_handler(&task_request) {
            Ok(metadata) => Ok(serde_json::json!({
                "found": true,
                "namespace": metadata.namespace,
                "name": metadata.name,
                "version": metadata.version,
                "ruby_class_name": metadata.handler_class,
                "config_schema": metadata.config_schema,
                "registered_at": metadata.registered_at.to_rfc3339()
            })),
            Err(_) => Ok(serde_json::json!({
                "found": false,
                "namespace": task_request.namespace,
                "name": task_request.name,
                "version": task_request.version
            }))
        }
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "found": false,
            "error": error_msg
        }))
    }
}

/// List all handlers in a namespace
pub fn list_handlers_wrapper(namespace_value: Value) -> Result<Value, Error> {
    let namespace: Option<String> = if namespace_value.is_nil() {
        None
    } else {
        Some(String::try_convert(namespace_value)
            .map_err(|_| Error::new(Ruby::get().unwrap().exception_runtime_error(), "Invalid namespace"))?)
    };

    let registry = get_global_task_handler_registry();
    let handlers = registry.list_handlers(namespace.as_deref()).map_err(|e| {
        Error::new(
            Ruby::get().unwrap().exception_runtime_error(),
            format!("Failed to list handlers: {}", e)
        )
    })?;

    let handlers_json: Vec<serde_json::Value> = handlers.into_iter().map(|metadata| {
        serde_json::json!({
            "namespace": metadata.namespace,
            "name": metadata.name,
            "version": metadata.version,
            "handler_class": metadata.handler_class,
            "config_schema": metadata.config_schema,
            "registered_at": metadata.registered_at.to_rfc3339()
        })
    }).collect();

    json_to_ruby_value(serde_json::json!({
        "handlers": handlers_json,
        "count": handlers_json.len()
    }))
}

/// Check if a handler exists
pub fn contains_handler_wrapper(handler_key_value: Value) -> Result<Value, Error> {
    let handler_key = ruby_value_to_json(handler_key_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid handler key: {}", e)))?;

    let namespace = handler_key.get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let name = handler_key.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::new(Ruby::get().unwrap().exception_runtime_error(), "Missing handler name"))?;
    let version = handler_key.get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.1.0");

    let registry = get_global_task_handler_registry();
    let exists = registry.contains_handler(namespace, name, version);

    json_to_ruby_value(serde_json::json!({
        "exists": exists,
        "namespace": namespace,
        "name": name,
        "version": version
    }))
}

/// Initialize orchestration system from current runtime (for tests)
pub fn initialize_orchestration_system_from_current_runtime_wrapper() -> Result<Value, Error> {
    let system = initialize_global_orchestration_system_from_current_runtime();

    json_to_ruby_value(serde_json::json!({
        "status": "initialized",
        "type": "OrchestrationSystem",
        "message": "Global orchestration system initialized from current runtime context"
    }))
}

/// 🎯 UNIFIED FFI WRAPPER: New unified orchestration system initialization
/// Use this instead of the deprecated initialize_orchestration_system_from_current_runtime
pub fn initialize_unified_orchestration_system_wrapper() -> Result<Value, Error> {
    let system = initialize_unified_orchestration_system();

    json_to_ruby_value(serde_json::json!({
        "status": "initialized",
        "type": "UnifiedOrchestrationSystem",
        "message": "Unified orchestration system initialized with configuration-driven pool",
        "architecture": "config_driven",
        "pool_source": "configuration_file",
        "testing_framework": "separated"
    }))
}
/// Register registry functions for Ruby FFI
pub fn register_registry_functions(module: RModule) -> Result<(), Error> {
    module.define_module_function(
        "registry_new",
        magnus::function!(registry_new_wrapper, 0),
    )?;

    module.define_module_function(
        "register_ffi_handler",
        magnus::function!(register_ffi_handler_wrapper, 1),
    )?;

    module.define_module_function(
        "find_handler",
        magnus::function!(find_handler_wrapper, 1),
    )?;

    module.define_module_function(
        "list_handlers",
        magnus::function!(list_handlers_wrapper, 1),
    )?;

    module.define_module_function(
        "contains_handler",
        magnus::function!(contains_handler_wrapper, 1),
    )?;

    module.define_module_function(
        "initialize_orchestration_system_from_current_runtime",
        magnus::function!(initialize_orchestration_system_from_current_runtime_wrapper, 0),
    )?;

    // 🎯 NEW UNIFIED ENTRY POINT: Register the new unified orchestration system initialization
    module.define_module_function(
        "initialize_unified_orchestration_system",
        magnus::function!(initialize_unified_orchestration_system_wrapper, 0),
    )?;

    Ok(())
}
