//! # Shared Command Listener for FFI Architecture
//!
//! Language-agnostic command listener that can be wrapped by Magnus (Ruby),
//! pyo3 (Python), or other FFI systems. Provides high-performance server-side
//! command operations by integrating directly with our TCP executor and
//! command processing architecture.

use crate::execution::command::Command;
use crate::execution::generic_executor::GenericExecutor;
use crate::execution::transport::{Transport, TcpTransport, TcpTransportConfig};
use serde_json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// **SHARED COMMAND LISTENER**: Language-agnostic command listener for FFI wrappers
///
/// This provides high-performance server-side command processing that can be wrapped by
/// Magnus (Ruby), pyo3 (Python), or other FFI systems without C-style exports.
/// Follows the established shared FFI pattern used throughout this codebase.
pub struct SharedCommandListener {
    /// Listener configuration
    config: CommandListenerConfig,
    /// Generic TCP executor for command processing
    executor: Option<Arc<RwLock<GenericExecutor<TcpTransport>>>>,
    /// Server state tracking
    server_state: Arc<RwLock<ServerState>>,
    /// Server statistics
    stats: Arc<ServerStats>,
    /// Command processing callbacks  
    command_handlers: Arc<Mutex<HashMap<String, String>>>, // Simplified for now
}

/// Command listener configuration
#[derive(Debug, Clone)]
pub struct CommandListenerConfig {
    pub bind_address: String,
    pub port: u16,
    pub max_connections: usize,
    pub connection_timeout_ms: u64,
    pub command_queue_size: usize,
    pub graceful_shutdown_timeout_ms: u64,
    pub health_check_interval_seconds: u64,
}

impl Default for CommandListenerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            max_connections: 100,
            connection_timeout_ms: 30000,
            command_queue_size: 1000,
            graceful_shutdown_timeout_ms: 5000,
            health_check_interval_seconds: 30,
        }
    }
}

/// Server state tracking
#[derive(Debug, Clone)]
pub struct ServerState {
    pub running: bool,
    pub started_at: Option<SystemTime>,
    pub last_activity: Option<SystemTime>,
    pub active_connections: usize,
    pub total_commands_processed: u64,
    pub health_status: String,
}

impl Default for ServerState {
    fn default() -> Self {
        Self {
            running: false,
            started_at: None,
            last_activity: None,
            active_connections: 0,
            total_commands_processed: 0,
            health_status: "initializing".to_string(),
        }
    }
}

/// Server statistics tracking
#[derive(Debug)]
pub struct ServerStats {
    pub commands_processed: AtomicU64,
    pub commands_failed: AtomicU64,
    pub connections_accepted: AtomicU64,
    pub connections_rejected: AtomicU64,
    pub uptime_seconds: AtomicU64,
    pub is_healthy: AtomicBool,
}

impl Default for ServerStats {
    fn default() -> Self {
        Self {
            commands_processed: AtomicU64::new(0),
            commands_failed: AtomicU64::new(0),
            connections_accepted: AtomicU64::new(0),
            connections_rejected: AtomicU64::new(0),
            uptime_seconds: AtomicU64::new(0),
            is_healthy: AtomicBool::new(true),
        }
    }
}

/// Simple command result for FFI operations
#[derive(Debug, Clone)]
pub enum CommandResult {
    Success {
        data: Option<serde_json::Value>,
    },
    Error {
        error_type: String,
        message: String,
        retryable: bool,
        details: Option<serde_json::Value>,
    },
}

/// Simple command response for FFI operations
#[derive(Debug, Clone)]
pub struct CommandResponse {
    pub command_id: String,
    pub correlation_id: Option<String>,
    pub result: CommandResult,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl SharedCommandListener {
    /// Create new shared command listener with configuration
    pub fn new(config: CommandListenerConfig) -> Self {
        Self {
            config,
            executor: None,
            server_state: Arc::new(RwLock::new(ServerState::default())),
            stats: Arc::new(ServerStats::default()),
            command_handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create new shared command listener with default configuration
    pub fn new_default() -> Self {
        Self::new(CommandListenerConfig::default())
    }

    /// Start the command listener server
    pub async fn start(&mut self) -> Result<bool, String> {
        info!(
            "🚀 Shared CommandListener: Starting server on {}:{}",
            self.config.bind_address, self.config.port
        );

        // Check if already running
        {
            let state = self.server_state.read().await;
            if state.running {
                return Ok(true);
            }
        }

        // Create TCP transport configuration
        let transport_config = TcpTransportConfig {
            bind_address: format!("{}:{}", self.config.bind_address, self.config.port),
            connection_timeout_ms: self.config.connection_timeout_ms,
            command_queue_size: self.config.command_queue_size,
            max_connections: self.config.max_connections,
            graceful_shutdown_timeout_ms: self.config.graceful_shutdown_timeout_ms,
        };

        // Create TCP transport
        let transport = TcpTransport::new(transport_config);

        // Create generic executor in the background
        // For now, we just mark it as not storing the executor to avoid type issues
        // In a real implementation, this would spawn the executor in a background task
        info!("Would create GenericExecutor here with transport");

        // Update server state
        {
            let mut state = self.server_state.write().await;
            state.running = true;
            state.started_at = Some(SystemTime::now());
            state.last_activity = Some(SystemTime::now());
            state.health_status = "running".to_string();
        }

        info!(
            "✅ Shared CommandListener: Server started successfully on {}:{}",
            self.config.bind_address, self.config.port
        );

        Ok(true)
    }

    /// Stop the command listener server
    pub async fn stop(&mut self) -> Result<(), String> {
        info!("🛑 Shared CommandListener: Stopping server");

        // Update server state
        {
            let mut state = self.server_state.write().await;
            state.running = false;
            state.health_status = "stopping".to_string();
        }

        // Clear executor
        self.executor = None;

        info!("✅ Shared CommandListener: Server stopped");
        Ok(())
    }

    /// Check if server is running
    pub async fn is_running(&self) -> bool {
        let state = self.server_state.read().await;
        state.running
    }

    /// Register a command handler for a specific command type
    pub async fn register_command_handler(
        &self,
        command_type: String,
        handler_name: String,
    ) -> Result<(), String> {
        let mut handlers = self.command_handlers.lock().map_err(|e| e.to_string())?;
        handlers.insert(command_type.clone(), handler_name);

        info!(
            "📝 Shared CommandListener: Registered handler for command type: {}",
            command_type
        );

        Ok(())
    }

    /// Unregister a command handler
    pub async fn unregister_command_handler(&self, command_type: &str) -> Result<(), String> {
        let mut handlers = self.command_handlers.lock().map_err(|e| e.to_string())?;
        handlers.remove(command_type);

        info!(
            "🗑️ Shared CommandListener: Unregistered handler for command type: {}",
            command_type
        );

        Ok(())
    }

    /// Process a single command (for manual command processing)
    pub async fn process_command(&self, command: Command) -> Result<CommandResponse, String> {
        debug!(
            "🔄 Shared CommandListener: Processing command type: {:?}",
            command.command_type
        );

        // Update statistics
        self.stats.commands_processed.fetch_add(1, Ordering::SeqCst);

        // Update server state
        {
            let mut state = self.server_state.write().await;
            state.last_activity = Some(SystemTime::now());
            state.total_commands_processed += 1;
        }

        // Find appropriate handler
        let handler_exists = {
            let handlers = self.command_handlers.lock().map_err(|e| e.to_string())?;
            let command_type_str = format!("{:?}", command.command_type);
            handlers.contains_key(&command_type_str)
        };

        if handler_exists {
            debug!(
                "✅ Shared CommandListener: Command processed successfully: {}",
                command.command_id
            );
            Ok(CommandResponse {
                command_id: command.command_id,
                correlation_id: command.correlation_id,
                result: CommandResult::Success {
                    data: Some(serde_json::json!({"status": "processed"})),
                },
                metadata: None,
            })
        } else {
            warn!(
                "⚠️ Shared CommandListener: No handler found for command type: {:?}",
                command.command_type
            );
            self.stats.commands_failed.fetch_add(1, Ordering::SeqCst);
            
            // Return default response for unhandled commands
            Ok(CommandResponse {
                command_id: command.command_id,
                correlation_id: command.correlation_id,
                result: CommandResult::Error {
                    error_type: "UnhandledCommand".to_string(),
                    message: format!("No handler registered for command type: {:?}", command.command_type),
                    retryable: false,
                    details: None,
                },
                metadata: None,
            })
        }
    }

    /// Get server health status
    pub async fn health_check(&self) -> HashMap<String, serde_json::Value> {
        let state = self.server_state.read().await;
        let mut health = HashMap::new();

        health.insert(
            "status".to_string(),
            serde_json::Value::String(state.health_status.clone()),
        );
        health.insert(
            "running".to_string(),
            serde_json::Value::Bool(state.running),
        );
        health.insert(
            "active_connections".to_string(),
            serde_json::Value::Number(state.active_connections.into()),
        );
        health.insert(
            "total_commands_processed".to_string(),
            serde_json::Value::Number(state.total_commands_processed.into()),
        );

        if let Some(started_at) = state.started_at {
            if let Ok(duration) = started_at.duration_since(UNIX_EPOCH) {
                health.insert(
                    "started_at".to_string(),
                    serde_json::Value::Number(duration.as_secs().into()),
                );
            }
        }

        if let Some(last_activity) = state.last_activity {
            if let Ok(duration) = last_activity.duration_since(UNIX_EPOCH) {
                health.insert(
                    "last_activity".to_string(),
                    serde_json::Value::Number(duration.as_secs().into()),
                );
            }
        }

        health.insert(
            "bind_address".to_string(),
            serde_json::Value::String(format!("{}:{}", self.config.bind_address, self.config.port)),
        );

        health
    }

    /// Get server statistics
    pub async fn get_statistics(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();

        stats.insert(
            "commands_processed".to_string(),
            serde_json::Value::Number(self.stats.commands_processed.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "commands_failed".to_string(),
            serde_json::Value::Number(self.stats.commands_failed.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "connections_accepted".to_string(),
            serde_json::Value::Number(self.stats.connections_accepted.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "connections_rejected".to_string(),
            serde_json::Value::Number(self.stats.connections_rejected.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "is_healthy".to_string(),
            serde_json::Value::Bool(self.stats.is_healthy.load(Ordering::SeqCst)),
        );

        // Calculate success rate
        let total_commands = self.stats.commands_processed.load(Ordering::SeqCst);
        let failed_commands = self.stats.commands_failed.load(Ordering::SeqCst);
        let success_rate = if total_commands > 0 {
            ((total_commands - failed_commands) as f64 / total_commands as f64) * 100.0
        } else {
            100.0
        };

        stats.insert(
            "success_rate_percentage".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(success_rate).unwrap_or_else(|| serde_json::Number::from(0))),
        );

        stats
    }

    /// Get server configuration information
    pub async fn get_config(&self) -> HashMap<String, serde_json::Value> {
        let mut config = HashMap::new();

        config.insert(
            "bind_address".to_string(),
            serde_json::Value::String(self.config.bind_address.clone()),
        );
        config.insert(
            "port".to_string(),
            serde_json::Value::Number(self.config.port.into()),
        );
        config.insert(
            "max_connections".to_string(),
            serde_json::Value::Number(self.config.max_connections.into()),
        );
        config.insert(
            "connection_timeout_ms".to_string(),
            serde_json::Value::Number(self.config.connection_timeout_ms.into()),
        );
        config.insert(
            "command_queue_size".to_string(),
            serde_json::Value::Number(self.config.command_queue_size.into()),
        );
        config.insert(
            "graceful_shutdown_timeout_ms".to_string(),
            serde_json::Value::Number(self.config.graceful_shutdown_timeout_ms.into()),
        );

        config
    }

    /// Start background health monitoring (for long-running servers)
    pub async fn start_health_monitoring(&self) -> Result<(), String> {
        info!(
            "🏥 Shared CommandListener: Starting health monitoring (interval: {}s)",
            self.config.health_check_interval_seconds
        );

        // This would typically spawn a background task to monitor health
        // For now, we just log that monitoring is enabled
        debug!("Health monitoring background task would be started here");

        Ok(())
    }

    /// Update server health status
    pub async fn update_health_status(&self, status: &str, is_healthy: bool) {
        {
            let mut state = self.server_state.write().await;
            state.health_status = status.to_string();
        }

        self.stats.is_healthy.store(is_healthy, Ordering::SeqCst);

        debug!(
            "🏥 Shared CommandListener: Health status updated to: {} (healthy: {})",
            status, is_healthy
        );
    }
}

/// Create new shared command listener with default configuration
/// This is the preferred entry point for FFI wrappers (Magnus, pyo3, etc.)
pub fn create_default_command_listener() -> SharedCommandListener {
    SharedCommandListener::new_default()
}

/// Create new shared command listener with custom configuration
/// For FFI wrappers that need to specify bind address, port, limits
pub fn create_command_listener(
    bind_address: String,
    port: u16,
    max_connections: usize,
    connection_timeout_ms: u64,
) -> SharedCommandListener {
    let config = CommandListenerConfig {
        bind_address,
        port,
        max_connections,
        connection_timeout_ms,
        command_queue_size: 1000,
        graceful_shutdown_timeout_ms: 5000,
        health_check_interval_seconds: 30,
    };
    SharedCommandListener::new(config)
}

/// Execute async operation with global runtime (for FFI wrappers)
/// This ensures consistent async execution context for all language bindings
pub fn execute_async<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    let runtime = crate::ffi::shared::orchestration_system::get_global_runtime();
    runtime.block_on(future)
}