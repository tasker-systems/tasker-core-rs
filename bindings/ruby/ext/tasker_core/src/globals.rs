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
use tasker_core::orchestration::config::ConfigurationManager;
use tasker_core::database::sql_functions::SqlFunctionExecutor;
use tasker_core::registry::TaskHandlerRegistry;
use tasker_core::orchestration::task_config_finder::TaskConfigFinder;
use sqlx::PgPool;
use std::sync::Arc;

/// Global orchestration system singleton
static GLOBAL_ORCHESTRATION_SYSTEM: OnceLock<Arc<OrchestrationSystem>> = OnceLock::new();

/// Global database pool singleton (for simpler access during tests)
static GLOBAL_DATABASE_POOL: OnceLock<PgPool> = OnceLock::new();

/// Shared orchestration resources
pub struct OrchestrationSystem {
    pub database_pool: PgPool,
    pub event_publisher: EventPublisher,
    pub workflow_coordinator: WorkflowCoordinator,
    pub state_manager: StateManager,
    pub task_initializer: TaskInitializer,
    pub task_handler_registry: TaskHandlerRegistry,
    pub step_executor: StepExecutor,
    pub config_manager: Arc<ConfigurationManager>,
}

impl OrchestrationSystem {
    /// Create new orchestration system with all required components
    async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Don't create a new runtime - use the existing one
        // The runtime will be managed by the global initialization

        // Get database connection with larger pool for test environments
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

        // Configure pool options for better connection management
        let pool_options = sqlx::postgres::PgPoolOptions::new()
            .max_connections(50)  // Increase from default ~10 to handle test workload
            .min_connections(5)   // Maintain minimum connections
            .acquire_timeout(std::time::Duration::from_secs(30))  // Allow longer acquire timeout
            .idle_timeout(std::time::Duration::from_secs(300))    // Keep connections alive longer
            .max_lifetime(std::time::Duration::from_secs(3600));  // Rotate connections hourly

        let database_pool = pool_options.connect(&database_url).await?;

        // Create core components with FFI-compatible configuration
        let mut event_config = tasker_core::events::publisher::EventPublisherConfig::default();
        event_config.async_processing = false; // Disable async processing in FFI context  
        event_config.ffi_enabled = false; // Disable FFI bridge that might use tokio::spawn
        let event_publisher = EventPublisher::with_config(event_config);
        let sql_executor = SqlFunctionExecutor::new(database_pool.clone());
        let state_manager = StateManager::new(sql_executor, event_publisher.clone(), database_pool.clone());
        let task_handler_registry = TaskHandlerRegistry::with_event_publisher(event_publisher.clone());
        // Create config manager
        let config_manager = Arc::new(ConfigurationManager::new());
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
            config_manager,
        })
    }
}

/// Initialize the global orchestration system from within an existing runtime context
/// This should be called by tests that have their own Tokio runtime
pub fn initialize_global_orchestration_system_from_current_runtime() -> Arc<OrchestrationSystem> {
    GLOBAL_ORCHESTRATION_SYSTEM.get_or_init(|| {
        // We assume we're being called from an async context with an existing runtime
        // Use a channel to get the result from the async operation
        let (tx, rx) = std::sync::mpsc::channel();
        
        // Spawn a task in the current runtime to do the async initialization
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let system = OrchestrationSystem::new().await
                    .expect("Failed to initialize orchestration system");
                tx.send(system).expect("Failed to send initialization result");
            });
            
            // Wait for the async initialization to complete
            Arc::new(rx.recv().expect("Failed to receive initialization result"))
        } else {
            // Fallback: create our own runtime if no current runtime exists
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime for initialization");

            Arc::new(rt.block_on(async {
                OrchestrationSystem::new().await
                    .expect("Failed to initialize orchestration system")
            }))
        }
    }).clone()
}

/// Get or initialize the global orchestration system (production version)
pub fn get_global_orchestration_system() -> Arc<OrchestrationSystem> {
    GLOBAL_ORCHESTRATION_SYSTEM.get_or_init(|| {
        // Production: create our own runtime
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime for initialization");

        Arc::new(rt.block_on(async {
            OrchestrationSystem::new().await
                .expect("Failed to initialize orchestration system")
        }))
    }).clone()
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
    get_global_orchestration_system().event_publisher.clone()
}

/// Get the global database pool 
pub fn get_global_database_pool() -> PgPool {
    // First try to get the pool from the orchestration system if it's already initialized
    if let Some(system) = GLOBAL_ORCHESTRATION_SYSTEM.get() {
        return system.database_pool.clone();
    }
    
    // Fallback: create a standalone pool for operations that don't need the full orchestration system
    GLOBAL_DATABASE_POOL.get_or_init(|| {
        // Create just the database pool without the full orchestration system
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

        // Use a simpler approach - create the pool in a dedicated thread to avoid runtime conflicts
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime for pool initialization");

            rt.block_on(async {
                // Configure pool options for better connection management
                let pool_options = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(50)  // Increase from default ~10 to handle test workload
                    .min_connections(5)   // Maintain minimum connections
                    .acquire_timeout(std::time::Duration::from_secs(30))  // Allow longer acquire timeout
                    .idle_timeout(std::time::Duration::from_secs(300))    // Keep connections alive longer
                    .max_lifetime(std::time::Duration::from_secs(3600));  // Rotate connections hourly

                pool_options.connect(&database_url).await
                    .expect("Failed to connect to database")
            })
        });

        handle.join().expect("Failed to initialize database pool")
    }).clone()
}

/// Get the global task handler registry
pub fn get_global_task_handler_registry() -> TaskHandlerRegistry {
    get_global_orchestration_system().task_handler_registry.clone()
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

    Ok(())
}
