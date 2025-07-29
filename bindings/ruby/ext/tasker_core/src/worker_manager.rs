//! # Ruby FFI Worker Manager Wrapper
//!
//! Magnus wrapper over SharedWorkerManager from src/ffi/shared/worker_manager.rs
//! This provides Ruby FFI compatibility while delegating all operations to the shared
//! worker manager, enabling unified worker lifecycle management without Ruby dependencies.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use magnus::error::Result as MagnusResult;
use magnus::{function, method, Error, Module, Object, RModule, Value};
use std::sync::Arc;
use tasker_core::ffi::shared::worker_manager::{SharedWorkerManager, WorkerManagerConfig, create_default_worker_manager};
use tasker_core::ffi::shared::orchestration_system::execute_async;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// **RUBY FFI WORKER MANAGER**: Magnus wrapper over SharedWorkerManager
///
/// Provides Ruby FFI compatibility while delegating all operations to the shared
/// worker manager architecture for unified worker lifecycle management.
#[magnus::wrap(class = "TaskerCore::WorkerManager")]
pub struct WorkerManager {
    shared_manager: Arc<RwLock<SharedWorkerManager>>,
}

impl WorkerManager {
    /// Create new Ruby worker manager wrapping shared manager
    pub fn new() -> MagnusResult<Self> {
        info!("🔧 Ruby FFI: Creating WorkerManager - delegating to shared manager");

        let shared_manager = SharedWorkerManager::new_default();

        Ok(Self {
            shared_manager: Arc::new(RwLock::new(shared_manager)),
        })
    }

    /// Create default worker manager using factory function
    pub fn create_default() -> MagnusResult<Self> {
        info!("🔧 Ruby FFI: Creating default WorkerManager using factory");

        let shared_manager = create_default_worker_manager();

        Ok(Self {
            shared_manager: Arc::new(RwLock::new(shared_manager)),
        })
    }

    /// Create new Ruby worker manager with custom configuration
    pub fn new_with_config(options: Value) -> MagnusResult<Self> {
        info!("🔧 Ruby FFI: Creating WorkerManager with custom config");

        // Convert Ruby options to configuration
        let options_json = ruby_value_to_json(options).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert options: {e}"),
            )
        })?;

        let server_host = options_json
            .get("server_host")
            .and_then(|v| v.as_str())
            .unwrap_or("localhost")
            .to_string();
        let server_port = options_json
            .get("server_port")
            .and_then(|v| v.as_u64())
            .unwrap_or(8080) as u16;
        let heartbeat_interval_seconds = options_json
            .get("heartbeat_interval_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        let registration_timeout_seconds = options_json
            .get("registration_timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(10);
        let max_heartbeat_failures = options_json
            .get("max_heartbeat_failures")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;
        let auto_reconnect = options_json
            .get("auto_reconnect")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let worker_namespace = options_json
            .get("worker_namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        let config = WorkerManagerConfig {
            server_host,
            server_port,
            heartbeat_interval_seconds,
            registration_timeout_seconds,
            max_heartbeat_failures,
            auto_reconnect,
            worker_namespace,
            default_step_timeout_ms: 30000,
        };

        let shared_manager = SharedWorkerManager::new(config);

        Ok(Self {
            shared_manager: Arc::new(RwLock::new(shared_manager)),
        })
    }

    /// Initialize as worker (connects to server)
    pub fn initialize_as_worker(&self) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: initialize_as_worker() - delegating to shared manager");

        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let mut manager_guard = manager.write().await;
            manager_guard.initialize_as_worker().await
        });

        match result {
            Ok(initialized) => Ok(initialized),
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to initialize as worker: {e}"),
            )),
        }
    }

    /// Initialize as server (accepts worker connections)
    pub fn initialize_as_server(&self, bind_port: Option<u64>) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: initialize_as_server() - delegating to shared manager");

        let port = bind_port.map(|p| p as u16);
        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let mut manager_guard = manager.write().await;
            manager_guard.initialize_as_server(port).await
        });

        match result {
            Ok(initialized) => Ok(initialized),
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to initialize as server: {e}"),
            )),
        }
    }

    /// Register worker with the server
    pub fn register_worker(&self, options: Value) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: register_worker() - delegating to shared manager");

        // Convert Ruby options to parameters
        let options_json = ruby_value_to_json(options).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert worker options: {e}"),
            )
        })?;

        let worker_id = options_json
            .get("worker_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::new(
                    magnus::exception::runtime_error(),
                    "Worker options must include 'worker_id'",
                )
            })?
            .to_string();

        let max_concurrent_steps = options_json
            .get("max_concurrent_steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32;

        let supported_namespaces = options_json
            .get("supported_namespaces")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| vec!["default".to_string()]);

        let language_runtime = options_json
            .get("language_runtime")
            .and_then(|v| v.as_str())
            .unwrap_or("ruby")
            .to_string();

        let version = options_json
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("3.0.0")
            .to_string();

        let custom_capabilities = options_json
            .get("custom_capabilities")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_else(|| std::collections::HashMap::new());

        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let manager_guard = manager.read().await;
            manager_guard
                .register_worker(
                    worker_id,
                    max_concurrent_steps,
                    supported_namespaces,
                    language_runtime,
                    version,
                    custom_capabilities,
                )
                .await
        });

        match result {
            Ok(response) => {
                let response_json = serde_json::Value::Object(
                    response.into_iter().collect()
                );
                json_to_ruby_value(response_json).map_err(|e| {
                    Error::new(
                        magnus::exception::runtime_error(),
                        format!("Failed to convert response: {e}"),
                    )
                })
            }
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("Worker registration failed: {e}"),
            )),
        }
    }

    /// Send heartbeat to server
    pub fn send_heartbeat(&self, worker_id: String, current_load: Option<u64>) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: send_heartbeat() - delegating to shared manager");

        let load = current_load.map(|l| l as u32);
        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let manager_guard = manager.read().await;
            manager_guard.send_heartbeat(worker_id, load).await
        });

        match result {
            Ok(response) => {
                let response_json = serde_json::Value::Object(
                    response.into_iter().collect()
                );
                json_to_ruby_value(response_json).map_err(|e| {
                    Error::new(
                        magnus::exception::runtime_error(),
                        format!("Failed to convert response: {e}"),
                    )
                })
            }
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("Heartbeat failed: {e}"),
            )),
        }
    }

    /// Unregister worker from server
    pub fn unregister_worker(&self, worker_id: String, reason: Option<String>) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: unregister_worker() - delegating to shared manager");

        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let manager_guard = manager.read().await;
            manager_guard.unregister_worker(worker_id, reason).await
        });

        match result {
            Ok(response) => {
                let response_json = serde_json::Value::Object(
                    response.into_iter().collect()
                );
                json_to_ruby_value(response_json).map_err(|e| {
                    Error::new(
                        magnus::exception::runtime_error(),
                        format!("Failed to convert response: {e}"),
                    )
                })
            }
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("Worker unregistration failed: {e}"),
            )),
        }
    }

    /// Start automatic heartbeat
    pub fn start_heartbeat(&self, worker_id: String, interval_seconds: Option<u64>) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: start_heartbeat() - delegating to shared manager");

        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let manager_guard = manager.read().await;
            manager_guard.start_heartbeat(worker_id, interval_seconds).await
        });

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("Start heartbeat failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Stop automatic heartbeat
    pub fn stop_heartbeat(&self) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: stop_heartbeat() - delegating to shared manager");

        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let manager_guard = manager.read().await;
            manager_guard.stop_heartbeat().await
        });

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("Stop heartbeat failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Get worker manager status
    pub fn get_status(&self) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: get_status() - delegating to shared manager");

        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let manager_guard = manager.read().await;
            manager_guard.get_status().await
        });

        let response_json = serde_json::Value::Object(
            result.into_iter().collect()
        );
        json_to_ruby_value(response_json).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert status: {e}"),
            )
        })
    }

    /// Get worker manager statistics
    pub fn get_statistics(&self) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: get_statistics() - delegating to shared manager");

        let manager = self.shared_manager.clone();
        let result = execute_async(async move {
            let manager_guard = manager.read().await;
            manager_guard.get_statistics().await
        });

        let response_json = serde_json::Value::Object(
            result.into_iter().collect()
        );
        json_to_ruby_value(response_json).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert statistics: {e}"),
            )
        })
    }
}

/// Register the WorkerManager class with Ruby
pub fn register_worker_manager(module: &RModule) -> MagnusResult<()> {
    let class = module.define_class("WorkerManager", magnus::class::object())?;

    // Constructor methods
    class.define_singleton_method("new", function!(WorkerManager::new, 0))?;
    class.define_singleton_method(
        "create_default",
        function!(WorkerManager::create_default, 0),
    )?;
    class.define_singleton_method(
        "new_with_config",
        function!(WorkerManager::new_with_config, 1),
    )?;

    // Initialization methods
    class.define_method(
        "initialize_as_worker",
        method!(WorkerManager::initialize_as_worker, 0),
    )?;
    class.define_method(
        "initialize_as_server",
        method!(WorkerManager::initialize_as_server, 1),
    )?;

    // Worker operations
    class.define_method(
        "register_worker",
        method!(WorkerManager::register_worker, 1),
    )?;
    class.define_method(
        "send_heartbeat",
        method!(WorkerManager::send_heartbeat, 2),
    )?;
    class.define_method(
        "unregister_worker",
        method!(WorkerManager::unregister_worker, 2),
    )?;

    // Heartbeat management
    class.define_method(
        "start_heartbeat",
        method!(WorkerManager::start_heartbeat, 2),
    )?;
    class.define_method(
        "stop_heartbeat",
        method!(WorkerManager::stop_heartbeat, 0),
    )?;

    // Status and statistics
    class.define_method("get_status", method!(WorkerManager::get_status, 0))?;
    class.define_method("get_statistics", method!(WorkerManager::get_statistics, 0))?;

    Ok(())
}