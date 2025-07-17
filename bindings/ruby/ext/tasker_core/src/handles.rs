//! # Handle-Based FFI Architecture
//!
//! **OPTIMAL DESIGN**: This module eliminates FFI performance bottlenecks by maintaining
//! persistent references to Rust resources, preventing connection pool exhaustion
//! and eliminating repeated global lookups.
//!
//! ## Core Principle
//! - **ONE initialization** → **Many operations** via persistent handles
//! - **Zero global lookups** after handle creation
//! - **Shared resources** across all FFI calls
//! - **Production-ready** for high-throughput scenarios

use std::sync::Arc;
use std::time::SystemTime;
use magnus::{Error, Value, Module, Object, method, function};
use serde_json::json;
use crate::globals::OrchestrationSystem;
use crate::test_helpers::testing_factory::TestingFactory;
use crate::context::{json_to_ruby_value, ruby_value_to_json};

/// **PRIMARY HANDLE**: All orchestration operations flow through this handle
/// 
/// This eliminates the root cause of connection pool exhaustion by ensuring
/// all FFI operations share the same initialized Rust resources.
#[magnus::wrap(class = "TaskerCore::OrchestrationHandle")]
pub struct OrchestrationHandle {
    orchestration_system: Arc<OrchestrationSystem>,
    testing_factory: Arc<TestingFactory>, 
    handle_id: String,
    created_at: SystemTime,
}

impl OrchestrationHandle {
    /// **SINGLE INITIALIZATION POINT** - Creates handle with all resources
    pub fn new() -> Result<Self, String> {
        println!("🎯 HANDLE ARCHITECTURE: Creating orchestration handle with persistent references");
        
        // Initialize resources ONCE - these will be shared across all operations
        let orchestration_system = crate::globals::initialize_unified_orchestration_system();
        let testing_factory = crate::test_helpers::testing_factory::get_global_testing_factory();
        
        let handle_id = format!("handle_{}", 
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        
        println!("✅ HANDLE READY: {} with shared orchestration system and testing factory", handle_id);
        
        Ok(Self {
            orchestration_system,
            testing_factory,
            handle_id,
            created_at: SystemTime::now(),
        })
    }
    
    /// Validate handle is still usable (prevents stale handle usage)
    pub fn validate(&self) -> Result<(), String> {
        if self.created_at.elapsed().unwrap_or(std::time::Duration::MAX) > std::time::Duration::from_secs(7200) {
            return Err("Handle expired after 2 hours".to_string());
        }
        Ok(())
    }
    
    /// Get handle info for debugging and monitoring
    pub fn info(&self) -> Result<Value, Error> {
        json_to_ruby_value(json!({
            "handle_id": self.handle_id,
            "created_at": self.created_at.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default().as_secs(),
            "age_seconds": self.created_at.elapsed().unwrap_or_default().as_secs(),
            "orchestration_pool_size": self.orchestration_system.database_pool().size(),
            "status": "active"
        }))
    }
    
    // ========================================================================
    // TESTING FACTORY OPERATIONS (no global lookups!)
    // ========================================================================
    
    /// Create test task using persistent factory reference
    pub fn create_test_task(&self, options_value: Value) -> Result<Value, Error> {
        self.validate().map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;
        
        let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));
        
        let result = crate::globals::execute_async(async {
            // Use handle's testing factory directly - NO global lookup!
            self.testing_factory.create_task(options).await
        });
        
        json_to_ruby_value(result)
    }
    
    /// Create test workflow step using persistent factory reference
    pub fn create_test_workflow_step(&self, options_value: Value) -> Result<Value, Error> {
        self.validate().map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;
        
        let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));
        
        let result = crate::globals::execute_async(async {
            self.testing_factory.create_workflow_step(options).await
        });
        
        json_to_ruby_value(result)
    }
    
    /// Create test foundation using persistent factory reference
    pub fn create_test_foundation(&self, options_value: Value) -> Result<Value, Error> {
        self.validate().map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;
        
        let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));
        
        let result = crate::globals::execute_async(async {
            self.testing_factory.create_foundation(options).await
        });
        
        json_to_ruby_value(result)
    }
    
    // ========================================================================
    // ORCHESTRATION OPERATIONS (no global lookups!)  
    // ========================================================================
    
    /// Register FFI handler using persistent orchestration system reference
    pub fn register_ffi_handler(&self, handler_data_value: Value) -> Result<Value, Error> {
        self.validate().map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;
        
        let handler_data = ruby_value_to_json(handler_data_value)
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Invalid handler data: {}", e)))?;
        
        let result = crate::globals::execute_async(async {
            let namespace = handler_data.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
            let name = handler_data.get("name").and_then(|v| v.as_str()).ok_or("Missing handler name")?;
            let version = handler_data.get("version").and_then(|v| v.as_str()).unwrap_or("0.1.0");
            let handler_class = handler_data.get("handler_class").and_then(|v| v.as_str()).ok_or("Missing handler_class")?;
            let config_schema = handler_data.get("config_schema").cloned();
            
            // Use handle's orchestration system directly - NO global lookup!
            self.orchestration_system.task_handler_registry.register_ffi_handler(
                namespace, name, version, handler_class, config_schema
            ).await.map_err(|e| format!("Handler registration failed: {}", e))?;
            
            Ok::<serde_json::Value, String>(json!({
                "status": "registered",
                "namespace": namespace,
                "name": name,
                "version": version,
                "handler_class": handler_class,
                "handle_id": self.handle_id
            }))
        });
        
        match result {
            Ok(success_data) => json_to_ruby_value(success_data),
            Err(error_msg) => json_to_ruby_value(json!({
                "status": "error",
                "error": error_msg
            }))
        }
    }
    
    /// Find handler using persistent orchestration system reference
    pub fn find_handler(&self, task_request_value: Value) -> Result<Value, Error> {
        self.validate().map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;
        
        let task_request_data = ruby_value_to_json(task_request_value)
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Invalid task request: {}", e)))?;
        
        let result: Result<serde_json::Value, String> = crate::globals::execute_async(async {
            let task_request: tasker_core::models::core::task_request::TaskRequest = 
                serde_json::from_value(task_request_data)
                    .map_err(|e| format!("Failed to parse TaskRequest: {}", e))?;
            
            // Use handle's orchestration system directly - NO global lookup!
            match self.orchestration_system.task_handler_registry.resolve_handler(&task_request) {
                Ok(metadata) => Ok(json!({
                    "found": true,
                    "namespace": metadata.namespace,
                    "name": metadata.name,
                    "version": metadata.version,
                    "ruby_class_name": metadata.handler_class,
                    "config_schema": metadata.config_schema,
                    "registered_at": metadata.registered_at.to_rfc3339(),
                    "handle_id": self.handle_id
                })),
                Err(_) => Ok(json!({
                    "found": false,
                    "namespace": task_request.namespace,
                    "name": task_request.name,
                    "version": task_request.version
                }))
            }
        });
        
        match result {
            Ok(success_data) => json_to_ruby_value(success_data),
            Err(error_msg) => json_to_ruby_value(json!({
                "found": false,
                "error": error_msg
            }))
        }
    }
}

/// Register handle-based FFI functions with Ruby
pub fn register_handle_functions(module: magnus::RModule) -> Result<(), magnus::Error> {
    // Register the OrchestrationHandle class manually (like BaseTaskHandler does)
    let ruby = magnus::Ruby::get().unwrap();
    let handle_class = module.define_class("OrchestrationHandle", ruby.class_object())?;
    
    // Register constructor
    handle_class.define_singleton_method("new", function!(create_orchestration_handle_wrapper, 0))?;
    
    // Register instance methods
    handle_class.define_method("info", method!(OrchestrationHandle::info, 0))?;
    handle_class.define_method("create_test_task", method!(OrchestrationHandle::create_test_task, 1))?;
    handle_class.define_method("create_test_workflow_step", method!(OrchestrationHandle::create_test_workflow_step, 1))?;
    handle_class.define_method("create_test_foundation", method!(OrchestrationHandle::create_test_foundation, 1))?;
    handle_class.define_method("register_ffi_handler", method!(OrchestrationHandle::register_ffi_handler, 1))?;
    handle_class.define_method("find_handler", method!(OrchestrationHandle::find_handler, 1))?;
    
    // Also register static functions for convenience 
    module.define_module_function(
        "create_orchestration_handle",
        function!(create_orchestration_handle_wrapper, 0),
    )?;
    
    Ok(())
}

/// Wrapper function to create OrchestrationHandle from Ruby
fn create_orchestration_handle_wrapper() -> Result<OrchestrationHandle, magnus::Error> {
    OrchestrationHandle::new()
        .map_err(|e| magnus::Error::new(magnus::exception::runtime_error(), e))
}