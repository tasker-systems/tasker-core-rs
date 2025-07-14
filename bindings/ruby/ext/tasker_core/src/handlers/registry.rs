//! # Handler Registry - Ruby FFI
//!
//! Ruby FFI wrapper for the core TaskHandlerRegistry, providing Rails integration
//! for handler discovery and metadata retrieval.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use magnus::{Error, RModule, Ruby, Value, TryConvert};
use magnus::value::ReprValue;
use tasker_core::registry::TaskHandlerRegistry;
use tasker_core::models::core::task_request::TaskRequest;
use tasker_core::events::EventPublisher;
use std::sync::{Arc, OnceLock};

/// Global singleton TaskHandlerRegistry for FFI operations
static GLOBAL_REGISTRY: OnceLock<Arc<TaskHandlerRegistry>> = OnceLock::new();

/// Get or initialize the global TaskHandlerRegistry
fn get_global_registry() -> Arc<TaskHandlerRegistry> {
    GLOBAL_REGISTRY.get_or_init(|| {
        let event_publisher = EventPublisher::new();
        Arc::new(TaskHandlerRegistry::with_event_publisher(event_publisher))
    }).clone()
}

/// Create a new registry (actually returns reference to singleton)
fn registry_new_wrapper() -> Result<Value, Error> {
    let registry = get_global_registry();
    
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
fn register_ffi_handler_wrapper(handler_data_value: Value) -> Result<Value, Error> {
    let handler_data = ruby_value_to_json(handler_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid handler data: {}", e)))?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to create tokio runtime for handler registration",
            )
        })?;

    let result = runtime.block_on(async {
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
        let registry = get_global_registry();
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
fn find_handler_wrapper(task_request_value: Value) -> Result<Value, Error> {
    let task_request_data = ruby_value_to_json(task_request_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid task request: {}", e)))?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to create tokio runtime for handler lookup",
            )
        })?;

    let result: Result<serde_json::Value, String> = runtime.block_on(async {
        // Parse TaskRequest
        let task_request: TaskRequest = serde_json::from_value(task_request_data)
            .map_err(|e| format!("Failed to parse TaskRequest: {}", e))?;

        // Use core registry to find handler
        let registry = get_global_registry();
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
fn list_handlers_wrapper(namespace_value: Value) -> Result<Value, Error> {
    let namespace: Option<String> = if namespace_value.is_nil() {
        None
    } else {
        Some(String::try_convert(namespace_value)
            .map_err(|_| Error::new(Ruby::get().unwrap().exception_runtime_error(), "Invalid namespace"))?)
    };

    let registry = get_global_registry();
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
fn contains_handler_wrapper(handler_key_value: Value) -> Result<Value, Error> {
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

    let registry = get_global_registry();
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