//! # Global Resource Management
//!
//! Singleton pattern for shared orchestration resources to avoid recreating
//! expensive components (database connections, tokio runtime, event publisher)
//! on every FFI call.

use std::sync::OnceLock;
use tokio::runtime::Runtime;
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

/// Shared orchestration resources
pub struct OrchestrationSystem {
    pub runtime: Runtime,
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
        // Create tokio runtime for async operations
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        // Get database connection
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

        let database_pool = PgPool::connect(&database_url).await?;

        // Create core components
        let event_publisher = EventPublisher::new();
        let sql_executor = SqlFunctionExecutor::new(database_pool.clone());
        let state_manager = StateManager::new(sql_executor, event_publisher.clone(), database_pool.clone());
        let task_handler_registry = TaskHandlerRegistry::with_event_publisher(event_publisher.clone());
        // Create config manager
        let config_manager = Arc::new(ConfigurationManager::new());
        // Create workflow coordinator
        let workflow_coordinator = WorkflowCoordinator::new(database_pool.clone());
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
            runtime,
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

/// Get or initialize the global orchestration system
pub fn get_global_orchestration_system() -> Arc<OrchestrationSystem> {
    GLOBAL_ORCHESTRATION_SYSTEM.get_or_init(|| {
        // We need to block on async initialization
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

/// Execute async operation using the global runtime
pub fn execute_async<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    let orchestration = get_global_orchestration_system();
    orchestration.runtime.block_on(future)
}

/// Get the global event publisher
pub fn get_global_event_publisher() -> EventPublisher {
    get_global_orchestration_system().event_publisher.clone()
}

/// Get the global database pool
pub fn get_global_database_pool() -> PgPool {
    get_global_orchestration_system().database_pool.clone()
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

    Ok(())
}
