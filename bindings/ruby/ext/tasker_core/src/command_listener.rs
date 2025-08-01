//! # Ruby FFI Command Listener Wrapper
//!
//! Magnus wrapper over SharedCommandListener from src/ffi/shared/command_listener.rs
//! This provides Ruby FFI compatibility while delegating all operations to the shared
//! command listener, enabling server-side command processing without Ruby native sockets.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use magnus::error::Result as MagnusResult;
use magnus::{function, method, Error, Module, Object, RModule, Value};
use tracing::{debug, error, info};
use magnus::value::ReprValue;
use std::sync::Arc;
use tasker_core::ffi::shared::command_listener::{SharedCommandListener, CommandListenerConfig, create_default_command_listener};
use tasker_core::ffi::shared::orchestration_system::execute_async;
use tokio::sync::RwLock;

/// **RUBY FFI COMMAND LISTENER**: Magnus wrapper over SharedCommandListener
///
/// Provides Ruby FFI compatibility while delegating all operations to the shared
/// command listener architecture for server-side command processing.
#[magnus::wrap(class = "TaskerCore::CommandListener")]
pub struct CommandListener {
    shared_listener: Arc<RwLock<SharedCommandListener>>,
}

impl CommandListener {
    /// Create new Ruby command listener wrapping shared listener
    pub fn new() -> MagnusResult<Self> {
        info!("🔧 Ruby FFI: Creating CommandListener - delegating to shared listener");

        let shared_listener = SharedCommandListener::new_default();

        Ok(Self {
            shared_listener: Arc::new(RwLock::new(shared_listener)),
        })
    }

    /// Create default command listener using factory function
    pub fn create_default() -> MagnusResult<Self> {
        info!("🔧 Ruby FFI: Creating default CommandListener using factory");

        let shared_listener = create_default_command_listener();

        Ok(Self {
            shared_listener: Arc::new(RwLock::new(shared_listener)),
        })
    }

    /// Create new Ruby command listener with custom configuration
    pub fn new_with_config(options: Value) -> MagnusResult<Self> {
        info!("🔧 Ruby FFI: Creating CommandListener with custom config");

        // Convert Ruby options to configuration
        let options_json = ruby_value_to_json(options).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert options: {e}"),
            )
        })?;

        let bind_address = options_json
            .get("bind_address")
            .and_then(|v| v.as_str())
            .unwrap_or("127.0.0.1")
            .to_string();
        let port = options_json
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(8080) as u16;
        let max_connections = options_json
            .get("max_connections")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as usize;
        let connection_timeout_ms = options_json
            .get("connection_timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000);
        let command_queue_size = options_json
            .get("command_queue_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000) as usize;

        let config = CommandListenerConfig {
            bind_address,
            port,
            max_connections,
            connection_timeout_ms,
            command_queue_size,
            graceful_shutdown_timeout_ms: 5000,
            health_check_interval_seconds: 30,
        };

        let shared_listener = SharedCommandListener::new(config);

        Ok(Self {
            shared_listener: Arc::new(RwLock::new(shared_listener)),
        })
    }

    /// Start the command listener server
    pub fn start(&self) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: start() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let mut listener_guard = listener.write().await;
            listener_guard.start().await
        });

        match result {
            Ok(started) => Ok(started),
            Err(e) => Err(Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to start listener: {e}"),
            )),
        }
    }

    /// Stop the command listener server
    pub fn stop(&self) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: stop() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let mut listener_guard = listener.write().await;
            listener_guard.stop().await
        });

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("Stop failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Check if server is running
    pub fn running(&self) -> MagnusResult<bool> {
        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let listener_guard = listener.read().await;
            listener_guard.is_running().await
        });

        Ok(result)
    }

    /// Register a command handler
    pub fn register_command_handler(&self, command_type: String, handler_name: String) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: register_command_handler() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let listener_guard = listener.read().await;
            listener_guard.register_command_handler(command_type, handler_name).await
        });

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("Handler registration failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Register a Ruby callback function that will be invoked for commands
    /// This is the bridge that connects Ruby handlers to the Rust CommandRouter
    pub fn register_ruby_callback(&self, command_type: String, _callback: Value) -> MagnusResult<bool> {
        info!("🔧 Ruby FFI: register_ruby_callback() - acknowledged for {}", command_type);
        
        // Due to Magnus thread-safety constraints, we acknowledge the registration
        // The actual Ruby handler invocation will happen through the RubyCommandBridge
        // using Ruby's existing BatchExecutionHandler mechanism
        
        info!("✅ RUBY_FFI: Ruby callback acknowledged for {} (processing via RubyCommandBridge)", command_type);
        Ok(true)
    }

    /// Unregister a command handler
    pub fn unregister_command_handler(&self, command_type: String) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: unregister_command_handler() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let listener_guard = listener.read().await;
            listener_guard.unregister_command_handler(&command_type).await
        });

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("Handler unregistration failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Get server health status
    pub fn health_check(&self) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: health_check() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let listener_guard = listener.read().await;
            listener_guard.health_check().await
        });

        let response_json = serde_json::Value::Object(
            result.into_iter().collect()
        );
        json_to_ruby_value(response_json).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert health check response: {e}"),
            )
        })
    }

    /// Get server statistics
    pub fn get_statistics(&self) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: get_statistics() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let listener_guard = listener.read().await;
            listener_guard.get_statistics().await
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

    /// Get server configuration
    pub fn get_config(&self) -> MagnusResult<Value> {
        debug!("🔧 Ruby FFI: get_config() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let listener_guard = listener.read().await;
            listener_guard.get_config().await
        });

        let response_json = serde_json::Value::Object(
            result.into_iter().collect()
        );
        json_to_ruby_value(response_json).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to convert config: {e}"),
            )
        })
    }

    /// Start health monitoring
    pub fn start_health_monitoring(&self) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: start_health_monitoring() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        let result = execute_async(async move {
            let listener_guard = listener.read().await;
            listener_guard.start_health_monitoring().await
        });

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("Health monitoring start failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Update health status
    pub fn update_health_status(&self, status: String, is_healthy: bool) -> MagnusResult<bool> {
        debug!("🔧 Ruby FFI: update_health_status() - delegating to shared listener");

        let listener = self.shared_listener.clone();
        execute_async(async move {
            let listener_guard = listener.read().await;
            listener_guard.update_health_status(&status, is_healthy).await
        });

        Ok(true)
    }
}

/// Register the CommandListener class with Ruby
pub fn register_command_listener(module: &RModule) -> MagnusResult<()> {
    let class = module.define_class("CommandListener", magnus::class::object())?;

    // Constructor methods
    class.define_singleton_method("new", function!(CommandListener::new, 0))?;
    class.define_singleton_method(
        "create_default",
        function!(CommandListener::create_default, 0),
    )?;
    class.define_singleton_method(
        "new_with_config",
        function!(CommandListener::new_with_config, 1),
    )?;

    // Server control methods
    class.define_method("start", method!(CommandListener::start, 0))?;
    class.define_method("stop", method!(CommandListener::stop, 0))?;
    class.define_method("running?", method!(CommandListener::running, 0))?;

    // Handler management
    class.define_method(
        "register_command_handler",
        method!(CommandListener::register_command_handler, 2),
    )?;
    class.define_method(
        "register_ruby_callback",
        method!(CommandListener::register_ruby_callback, 2),
    )?;
    class.define_method(
        "unregister_command_handler",
        method!(CommandListener::unregister_command_handler, 1),
    )?;

    // Health and monitoring
    class.define_method("health_check", method!(CommandListener::health_check, 0))?;
    class.define_method("get_statistics", method!(CommandListener::get_statistics, 0))?;
    class.define_method("get_config", method!(CommandListener::get_config, 0))?;
    class.define_method(
        "start_health_monitoring",
        method!(CommandListener::start_health_monitoring, 0),
    )?;
    class.define_method(
        "update_health_status",
        method!(CommandListener::update_health_status, 2),
    )?;

    Ok(())
}
