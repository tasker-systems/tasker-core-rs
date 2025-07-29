//! # Ruby FFI Command Client Wrapper
//!
//! Magnus wrapper over SharedCommandClient from src/ffi/shared/command_client.rs
//! This provides Ruby FFI compatibility while delegating all operations to the shared
//! command client, eliminating Ruby socket dependencies and providing better integration
//! with our command pattern architecture.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use magnus::error::Result as MagnusResult;
use magnus::{function, method, Error, Module, Object, RModule, Value};
use std::sync::Arc;
use tasker_core::ffi::shared::command_client::{SharedCommandClient, CommandClientConfig, execute_async};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// **RUBY FFI COMMAND CLIENT**: Magnus wrapper over SharedCommandClient
///
/// Provides Ruby FFI compatibility while delegating all operations to the shared
/// command client architecture, eliminating Ruby socket dependencies.
#[magnus::wrap(class = "TaskerCore::CommandClient")]
pub struct CommandClient {
    shared_client: Arc<RwLock<SharedCommandClient>>,
}

impl CommandClient {
    /// Create new Ruby command client wrapping shared client
    pub fn new() -> MagnusResult<Self> {
        info!("🔧 Ruby FFI: Creating CommandClient - delegating to shared client");

        let shared_client = SharedCommandClient::new_default();

        Ok(Self {
            shared_client: Arc::new(RwLock::new(shared_client)),
        })
    }

    /// Create new Ruby command client with custom configuration
    pub fn new_with_config(options: Value) -> MagnusResult<Self> {
        info!("🔧 Ruby FFI: Creating CommandClient with custom config");

        // Convert Ruby options to configuration
        let options_json = ruby_value_to_json(options).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert options: {e}"),
            )
        })?;

        let host = options_json
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("localhost")
            .to_string();
        let port = options_json
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(8080) as u16;
        let timeout_seconds = options_json
            .get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as u32;
        let connect_timeout_seconds = options_json
            .get("connect_timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32;

        let config = CommandClientConfig {
            host,
            port,
            timeout_seconds,
            connect_timeout_seconds,
            read_timeout_seconds: 10,
            default_namespace: options_json
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string(),
        };

        let shared_client = SharedCommandClient::new(config);

        Ok(Self {
            shared_client: Arc::new(RwLock::new(shared_client)),
        })
    }

    /// Connect to the Rust TCP executor
    pub fn connect(&self) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: connect() - delegating to shared client");

        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let mut client_guard = client.write().await;
            client_guard.connect().await
        });

        match result {
            Ok(connected) => Ok(connected),
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("Connection failed: {e}"),
            )),
        }
    }

    /// Disconnect from the TCP executor
    pub fn disconnect(&self) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: disconnect() - delegating to shared client");

        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let mut client_guard = client.write().await;
            client_guard.disconnect().await
        });

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("Disconnect failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Check if client is connected
    pub fn connected(&self) -> MagnusResult<bool> {
        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let client_guard = client.read().await;
            client_guard.is_connected().await
        });

        Ok(result)
    }

    /// Register a worker with the server
    pub fn register_worker(&self, options: Value) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: register_worker() - delegating to shared client");

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

        let step_timeout_ms = options_json
            .get("step_timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000) as u32;

        let supports_retries = options_json
            .get("supports_retries")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

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

        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let client_guard = client.read().await;
            client_guard
                .register_worker(
                    worker_id,
                    max_concurrent_steps,
                    supported_namespaces,
                    step_timeout_ms,
                    supports_retries,
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
    pub fn send_heartbeat(&self, options: Value) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: send_heartbeat() - delegating to shared client");

        // Convert Ruby options to parameters
        let options_json = ruby_value_to_json(options).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert heartbeat options: {e}"),
            )
        })?;

        let worker_id = options_json
            .get("worker_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::new(
                    magnus::exception::runtime_error(),
                    "Heartbeat options must include 'worker_id'",
                )
            })?
            .to_string();

        let current_load = options_json
            .get("current_load")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let system_stats = options_json
            .get("system_stats")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            });

        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let client_guard = client.read().await;
            client_guard
                .send_heartbeat(worker_id, current_load, system_stats)
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
                format!("Heartbeat failed: {e}"),
            )),
        }
    }

    /// Unregister worker from server
    pub fn unregister_worker(&self, options: Value) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: unregister_worker() - delegating to shared client");

        // Convert Ruby options to parameters
        let options_json = ruby_value_to_json(options).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert unregister options: {e}"),
            )
        })?;

        let worker_id = options_json
            .get("worker_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::new(
                    magnus::exception::runtime_error(),
                    "Unregister options must include 'worker_id'",
                )
            })?
            .to_string();

        let reason = options_json
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let client_guard = client.read().await;
            client_guard.unregister_worker(worker_id, reason).await
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

    /// Send health check to server
    pub fn health_check(&self, diagnostic_level: Option<String>) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: health_check() - delegating to shared client");

        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let client_guard = client.read().await;
            client_guard.health_check(diagnostic_level).await
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
                format!("Health check failed: {e}"),
            )),
        }
    }

    /// Get connection information
    pub fn connection_info(&self) -> MagnusResult<Value> {
        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let client_guard = client.read().await;
            client_guard.connection_info().await
        });

        let response_json = serde_json::Value::Object(
            result.into_iter().collect()
        );
        json_to_ruby_value(response_json).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert connection info: {e}"),
            )
        })
    }

    /// Send InitializeTask command to create a new task
    pub fn initialize_task(&self, task_request: Value) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: initialize_task() - delegating to shared client");

        // Convert Ruby task_request to JSON
        let task_request_json = ruby_value_to_json(task_request).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert task_request: {e}"),
            )
        })?;

        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let client_guard = client.read().await;
            client_guard.initialize_task(task_request_json).await
        });

        match result {
            Ok(response) => json_to_ruby_value(response).map_err(|e| {
                Error::new(
                    magnus::exception::runtime_error(),
                    format!("Failed to convert response: {e}"),
                )
            }),
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("InitializeTask failed: {e}"),
            )),
        }
    }

    /// Send TryTaskIfReady command to check if task has ready steps
    pub fn try_task_if_ready(&self, task_id: i64) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: try_task_if_ready() - delegating to shared client");

        let client = self.shared_client.clone();
        let result = execute_async(async move {
            let client_guard = client.read().await;
            client_guard.try_task_if_ready(task_id).await
        });

        match result {
            Ok(response) => json_to_ruby_value(response).map_err(|e| {
                Error::new(
                    magnus::exception::runtime_error(),
                    format!("Failed to convert response: {e}"),
                )
            }),
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("TryTaskIfReady failed: {e}"),
            )),
        }
    }
}

/// Register the CommandClient class with Ruby
pub fn register_command_client(module: &RModule) -> MagnusResult<()> {
    let class = module.define_class("CommandClient", magnus::class::object())?;

    // Constructor methods
    class.define_singleton_method("new", function!(CommandClient::new, 0))?;
    class.define_singleton_method(
        "new_with_config",
        function!(CommandClient::new_with_config, 1),
    )?;

    // Connection methods
    class.define_method("connect", method!(CommandClient::connect, 0))?;
    class.define_method("disconnect", method!(CommandClient::disconnect, 0))?;
    class.define_method("connected?", method!(CommandClient::connected, 0))?;

    // Worker operations
    class.define_method(
        "register_worker",
        method!(CommandClient::register_worker, 1),
    )?;
    class.define_method(
        "send_heartbeat",
        method!(CommandClient::send_heartbeat, 1),
    )?;
    class.define_method(
        "unregister_worker",
        method!(CommandClient::unregister_worker, 1),
    )?;

    // Task operations
    class.define_method(
        "initialize_task",
        method!(CommandClient::initialize_task, 1),
    )?;
    class.define_method(
        "try_task_if_ready",
        method!(CommandClient::try_task_if_ready, 1),
    )?;

    // Health and info
    class.define_method("health_check", method!(CommandClient::health_check, 1))?;
    class.define_method(
        "connection_info",
        method!(CommandClient::connection_info, 0),
    )?;

    Ok(())
}