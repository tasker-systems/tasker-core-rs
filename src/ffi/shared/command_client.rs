//! # Shared Command Client for FFI Architecture
//!
//! Language-agnostic command client that can be wrapped by Magnus (Ruby),
//! pyo3 (Python), or other FFI systems. Provides high-performance command
//! operations by eliminating framework socket dependencies and integrating
//! directly with our command pattern architecture.

use crate::execution::command::{
    Command, CommandMetadata, CommandPayload, CommandPriority, CommandSource, CommandType,
    HealthCheckLevel, WorkerCapabilities, WorkerRuntimeInfo, SystemStats
};
use crate::models::core::task_request::TaskRequest;
use chrono::Utc;
use serde_json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{debug, error, info};
use uuid::Uuid;

/// **SHARED COMMAND CLIENT**: Language-agnostic command client for FFI wrappers
///
/// This provides high-performance command operations that can be wrapped by
/// Magnus (Ruby), pyo3 (Python), or other FFI systems without C-style exports.
/// Follows the established shared FFI pattern used throughout this codebase.
#[derive(Debug)]
pub struct SharedCommandClient {
    /// Client configuration
    config: CommandClientConfig,
    /// TCP stream for communication
    stream: Option<Arc<RwLock<TcpStream>>>,
    /// Connection state
    connection_state: Arc<RwLock<ConnectionState>>,
    /// Command counter for unique IDs
    command_counter: AtomicU64,
    /// Pending commands for correlation
    pending_commands: Arc<Mutex<HashMap<String, PendingCommand>>>,
}

/// Command client configuration
#[derive(Debug, Clone)]
pub struct CommandClientConfig {
    pub host: String,
    pub port: u16,
    pub timeout_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub read_timeout_seconds: u32,
    pub default_namespace: String,
}

impl Default for CommandClientConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8080,
            timeout_seconds: 30,
            connect_timeout_seconds: 5,
            read_timeout_seconds: 10,
            default_namespace: "default".to_string(),
        }
    }
}

/// Connection state tracking
#[derive(Debug, Clone)]
pub struct ConnectionState {
    pub connected: bool,
    pub connection_id: String,
    pub connected_at: Option<SystemTime>,
    pub last_activity: Option<SystemTime>,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            connected: false,
            connection_id: Uuid::new_v4().to_string(),
            connected_at: None,
            last_activity: None,
        }
    }
}

/// Pending command tracking for correlation
#[derive(Debug)]
pub struct PendingCommand {
    pub command_id: String,
    pub command_type: CommandType,
    pub sent_at: SystemTime,
}

impl SharedCommandClient {
    /// Create new shared command client with configuration
    pub fn new(config: CommandClientConfig) -> Self {
        let connection_id = Uuid::new_v4().to_string();
        info!("🎯 SHARED_CLIENT: Creating new SharedCommandClient instance with connection_id={}", connection_id);
        
        let connection_state = ConnectionState {
            connected: false,
            connection_id: connection_id.clone(),
            connected_at: None,
            last_activity: None,
        };
        
        Self {
            config,
            stream: None,
            connection_state: Arc::new(RwLock::new(connection_state)),
            command_counter: AtomicU64::new(0),
            pending_commands: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create new shared command client with default configuration
    pub fn new_default() -> Self {
        Self::new(CommandClientConfig::default())
    }

    /// Establish connection to Rust TCP executor
    pub async fn connect(&mut self) -> Result<bool, String> {
        // Check if already connected
        {
            let state = self.connection_state.read().await;
            if state.connected {
                return Ok(true);
            }
        }

        info!(
            "🔗 Shared CommandClient: Connecting to {}:{}",
            self.config.host, self.config.port
        );

        match timeout(
            Duration::from_secs(self.config.connect_timeout_seconds as u64),
            TcpStream::connect(format!("{}:{}", self.config.host, self.config.port)),
        )
        .await
        {
            Ok(Ok(stream)) => {
                self.stream = Some(Arc::new(RwLock::new(stream)));

                // Update connection state
                {
                    let mut state = self.connection_state.write().await;
                    state.connected = true;
                    state.connected_at = Some(SystemTime::now());
                    state.last_activity = Some(SystemTime::now());
                }

                info!(
                    "✅ Shared CommandClient: Connected to {}:{}",
                    self.config.host, self.config.port
                );
                Ok(true)
            }
            Ok(Err(e)) => {
                error!(
                    "❌ Shared CommandClient: Connection failed to {}:{} - {}",
                    self.config.host, self.config.port, e
                );
                Err(format!("Connection failed: {}", e))
            }
            Err(_) => {
                error!(
                    "⏰ Shared CommandClient: Connection timeout to {}:{}",
                    self.config.host, self.config.port
                );
                Err("Connection timeout".to_string())
            }
        }
    }

    /// Close connection to TCP executor
    pub async fn disconnect(&mut self) -> Result<(), String> {
        info!("🔌 Shared CommandClient: Disconnecting");

        // Clear stream
        self.stream = None;

        // Update connection state
        {
            let mut state = self.connection_state.write().await;
            state.connected = false;
            state.connected_at = None;
        }

        // Clear pending commands
        {
            let mut pending = self.pending_commands.lock().map_err(|e| e.to_string())?;
            pending.clear();
        }

        info!("✅ Shared CommandClient: Disconnected");
        Ok(())
    }

    /// Check if client is connected
    pub async fn is_connected(&self) -> bool {
        let state = self.connection_state.read().await;
        state.connected && self.stream.is_some()
    }

    /// Register a worker with the Rust orchestration system
    pub async fn register_worker(
        &self,
        worker_id: String,
        max_concurrent_steps: u32,
        supported_namespaces: Vec<String>,
        step_timeout_ms: u32,
        supports_retries: bool,
        language_runtime: String,
        version: String,
        custom_capabilities: HashMap<String, serde_json::Value>,
        supported_tasks: Option<Vec<crate::execution::command::TaskHandlerInfo>>,
    ) -> Result<HashMap<String, serde_json::Value>, String> {
        self.ensure_connected().await?;

        let command = self.build_register_worker_command(
            worker_id,
            max_concurrent_steps,
            supported_namespaces,
            step_timeout_ms,
            supports_retries,
            language_runtime,
            version,
            custom_capabilities,
            supported_tasks,
        )?;

        self.send_command(command).await
    }

    /// Send heartbeat to maintain worker connection
    pub async fn send_heartbeat(
        &self,
        worker_id: String,
        current_load: u32,
        system_stats: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<HashMap<String, serde_json::Value>, String> {
        self.ensure_connected().await?;

        let command = self.build_heartbeat_command(worker_id, current_load, system_stats)?;

        self.send_command(command).await
    }

    /// Unregister worker from orchestration system
    pub async fn unregister_worker(
        &self,
        worker_id: String,
        reason: Option<String>,
    ) -> Result<HashMap<String, serde_json::Value>, String> {
        self.ensure_connected().await?;

        let command = self.build_unregister_worker_command(worker_id, reason)?;

        self.send_command(command).await
    }

    /// Send health check command
    pub async fn health_check(
        &self,
        diagnostic_level: Option<String>,
    ) -> Result<HashMap<String, serde_json::Value>, String> {
        self.ensure_connected().await?;

        let command = self.build_health_check_command(diagnostic_level)?;

        self.send_command(command).await
    }

    /// Get connection information
    pub async fn connection_info(&self) -> HashMap<String, serde_json::Value> {
        let state = self.connection_state.read().await;
        let mut info = HashMap::new();

        info.insert(
            "connected".to_string(),
            serde_json::Value::Bool(state.connected),
        );
        info.insert(
            "connection_id".to_string(),
            serde_json::Value::String(state.connection_id.clone()),
        );
        info.insert(
            "host".to_string(),
            serde_json::Value::String(self.config.host.clone()),
        );
        info.insert(
            "port".to_string(),
            serde_json::Value::Number(self.config.port.into()),
        );

        if let Some(connected_at) = state.connected_at {
            if let Ok(duration) = connected_at.duration_since(UNIX_EPOCH) {
                info.insert(
                    "connected_at".to_string(),
                    serde_json::Value::Number(duration.as_secs().into()),
                );
            }
        }

        info
    }

    /// Send InitializeTask command to create a new task
    pub async fn initialize_task(&self, task_request: serde_json::Value) -> Result<serde_json::Value, String> {
        info!("🎯 SHARED_CLIENT: initialize_task() called");
        
        // Add debugging about connection state
        let connection_state = self.connection_state.read().await;
        info!("🎯 SHARED_CLIENT: Connection state: connected={}, has_stream={}, connection_id={}", 
               connection_state.connected, self.stream.is_some(), connection_state.connection_id);
        drop(connection_state);
        
        self.ensure_connected().await?;
        info!("🎯 SHARED_CLIENT: Connection confirmed for InitializeTask");

        let tr_request: TaskRequest = serde_json::from_value(task_request).map_err(|e| {
            error!("🎯 SHARED_CLIENT: Failed to parse task_request: {}", e);
            format!("Failed to parse task request: {}", e)
        })?;
        info!("🎯 SHARED_CLIENT: TaskRequest parsed successfully: namespace={}, name={}", tr_request.namespace, tr_request.name);

        let command = Command::new(
            CommandType::InitializeTask,
            CommandPayload::InitializeTask {
                task_request: tr_request,
            },
            CommandSource::RubyWorker { id: "ruby_client".to_string() },
        );
        info!("🎯 SHARED_CLIENT: Created InitializeTask command: id={}", command.command_id);

        info!("🎯 SHARED_CLIENT: About to call send_command for InitializeTask");
        let response = self.send_command(command).await?;
        info!("🎯 SHARED_CLIENT: send_command returned successfully for InitializeTask");
        
        Ok(serde_json::Value::Object(
            response.into_iter().collect()
        ))
    }

    /// Send TryTaskIfReady command to check if task has ready steps
    pub async fn try_task_if_ready(&self, task_id: i64) -> Result<serde_json::Value, String> {
        self.ensure_connected().await?;

        let command = Command::new(
            CommandType::TryTaskIfReady,
            CommandPayload::TryTaskIfReady { task_id },
            CommandSource::RubyWorker { id: "ruby_client".to_string() },
        );

        let response = self.send_command(command).await?;
        Ok(serde_json::Value::Object(
            response.into_iter().collect()
        ))
    }

    // Private implementation methods

    /// Ensure client is connected
    async fn ensure_connected(&self) -> Result<(), String> {
        if !self.is_connected().await {
            return Err("Client not connected".to_string());
        }
        Ok(())
    }

    /// Generate unique command ID
    fn generate_command_id(&self) -> String {
        let counter = self.command_counter.fetch_add(1, Ordering::SeqCst);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        format!("ffi_cmd_{}_{}_{}_{}", std::process::id(), counter, timestamp, Uuid::new_v4())
    }

    /// Build register worker command
    fn build_register_worker_command(
        &self,
        worker_id: String,
        max_concurrent_steps: u32,
        supported_namespaces: Vec<String>,
        step_timeout_ms: u32,
        supports_retries: bool,
        language_runtime: String,
        version: String,
        custom_capabilities: HashMap<String, serde_json::Value>,
        supported_tasks: Option<Vec<crate::execution::command::TaskHandlerInfo>>,
    ) -> Result<Command, String> {
        let worker_capabilities = WorkerCapabilities {
            worker_id: worker_id.clone(),
            max_concurrent_steps: max_concurrent_steps as usize,
            supported_namespaces,
            step_timeout_ms: step_timeout_ms as u64,
            supports_retries,
            language_runtime: language_runtime.clone(),
            version: version.clone(),
            custom_capabilities,
            connection_info: None, // Will be set by the server
            runtime_info: Some(WorkerRuntimeInfo {
                language_runtime: Some(language_runtime.clone()),
                language_version: Some(version.clone()),
                hostname: None,
                pid: Some(std::process::id()),
                started_at: Some(Utc::now().to_rfc3339()),
            }),
            supported_tasks,
        };

        Ok(Command {
            command_type: CommandType::RegisterWorker,
            command_id: self.generate_command_id(),
            correlation_id: None,
            metadata: CommandMetadata {
                timestamp: Utc::now(),
                source: CommandSource::RubyWorker { id: worker_id },
                target: None,
                timeout_ms: Some(step_timeout_ms as u64),
                retry_policy: None,
                namespace: None,
                priority: Some(CommandPriority::Normal),
            },
            payload: CommandPayload::RegisterWorker { worker_capabilities },
        })
    }

    /// Build heartbeat command
    fn build_heartbeat_command(
        &self,
        worker_id: String,
        current_load: u32,
        system_stats: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<Command, String> {
        let system_stats_converted = system_stats.and_then(|stats| {
            // Convert HashMap to SystemStats if possible
            Some(SystemStats {
                cpu_usage_percent: stats.get("cpu_usage_percent")?.as_f64().unwrap_or(0.0),
                memory_usage_mb: stats.get("memory_usage_mb")?.as_u64().unwrap_or(0),
                active_connections: stats.get("active_connections")?.as_u64().unwrap_or(0) as usize,
                uptime_seconds: stats.get("uptime_seconds")?.as_u64().unwrap_or(0),
            })
        });

        Ok(Command {
            command_type: CommandType::WorkerHeartbeat,
            command_id: self.generate_command_id(),
            correlation_id: None,
            metadata: CommandMetadata {
                timestamp: Utc::now(),
                source: CommandSource::RubyWorker { id: worker_id.clone() },
                target: None,
                timeout_ms: Some((self.config.timeout_seconds * 1000) as u64),
                retry_policy: None,
                namespace: None,
                priority: Some(CommandPriority::Normal),
            },
            payload: CommandPayload::WorkerHeartbeat {
                worker_id,
                current_load: current_load as usize,
                system_stats: system_stats_converted,
            },
        })
    }

    /// Build unregister worker command
    fn build_unregister_worker_command(
        &self,
        worker_id: String,
        reason: Option<String>,
    ) -> Result<Command, String> {
        let reason_text = reason.unwrap_or_else(|| "FFI client shutdown".to_string());

        Ok(Command {
            command_type: CommandType::UnregisterWorker,
            command_id: self.generate_command_id(),
            correlation_id: None,
            metadata: CommandMetadata {
                timestamp: Utc::now(),
                source: CommandSource::RubyWorker { id: worker_id.clone() },
                target: None,
                timeout_ms: Some((self.config.timeout_seconds * 1000) as u64),
                retry_policy: None,
                namespace: None,
                priority: Some(CommandPriority::Normal),
            },
            payload: CommandPayload::UnregisterWorker {
                worker_id,
                reason: reason_text,
            },
        })
    }

    /// Build health check command
    fn build_health_check_command(
        &self,
        diagnostic_level: Option<String>,
    ) -> Result<Command, String> {
        let level = match diagnostic_level.as_deref().unwrap_or("Basic") {
            "Basic" => HealthCheckLevel::Basic,
            "Detailed" => HealthCheckLevel::Detailed,
            "Full" => HealthCheckLevel::Full,
            _ => HealthCheckLevel::Basic,
        };

        Ok(Command {
            command_type: CommandType::HealthCheck,
            command_id: self.generate_command_id(),
            correlation_id: None,
            metadata: CommandMetadata {
                timestamp: Utc::now(),
                source: CommandSource::RubyWorker {
                    id: "health_check_client".to_string(),
                },
                target: None,
                timeout_ms: Some((self.config.timeout_seconds * 1000) as u64),
                retry_policy: None,
                namespace: None,
                priority: Some(CommandPriority::Normal),
            },
            payload: CommandPayload::HealthCheck {
                diagnostic_level: level,
            },
        })
    }

    /// Send command and wait for response (using persistent connection)
    async fn send_command(&self, command: Command) -> Result<HashMap<String, serde_json::Value>, String> {
        let stream = self.stream.as_ref().ok_or("No connection available")?;
        let command_id = command.command_id.clone();
        let command_type = command.command_type.clone();

        // Debug connection info
        let connection_state = self.connection_state.read().await;
        info!(
            "🎯 SEND_COMMAND: Sending command {:?} (ID: {}) on connection_id={}", 
            command_type, command_id, connection_state.connection_id
        );
        drop(connection_state);

        // Track pending command
        {
            let mut pending = self.pending_commands.lock().map_err(|e| e.to_string())?;
            pending.insert(
                command_id.clone(),
                PendingCommand {
                    command_id: command_id.clone(),
                    command_type: command_type.clone(),
                    sent_at: SystemTime::now(),
                },
            );
        }

        // Serialize and send command
        let command_json = serde_json::to_string(&command).map_err(|e| format!("Serialization failed: {}", e))?;

        let result = timeout(
            Duration::from_secs(self.config.timeout_seconds as u64),
            self.send_and_receive(stream, command_json, command_id.clone()),
        )
        .await;

        // Remove from pending commands
        {
            let mut pending = self.pending_commands.lock().map_err(|e| e.to_string())?;
            pending.remove(&command_id);
        }

        match result {
            Ok(Ok(response)) => {
                info!(
                    "🎯 SEND_COMMAND: ✅ Received response for command {} ({:?})",
                    command_id, command_type
                );

                // Update last activity
                {
                    let mut state = self.connection_state.write().await;
                    state.last_activity = Some(SystemTime::now());
                }

                Ok(response)
            }
            Ok(Err(e)) => {
                error!(
                    "❌ Shared CommandClient: Command {} failed: {}",
                    command_id, e
                );
                Err(e)
            }
            Err(_) => {
                error!(
                    "⏰ Shared CommandClient: Command {} timed out after {} seconds",
                    command_id, self.config.timeout_seconds
                );
                Err("Command timeout".to_string())
            }
        }
    }


    /// Send command data and receive response using persistent connection
    async fn send_and_receive(
        &self,
        stream: &Arc<RwLock<TcpStream>>,
        command_json: String,
        command_id: String,
    ) -> Result<HashMap<String, serde_json::Value>, String> {
        debug!("📤 Sending command {} ({} bytes) on persistent connection", 
               command_id, command_json.len());
        
        // Use write lock for both send and receive to ensure atomicity
        let mut stream_guard = stream.write().await;
        
        // Send command
        stream_guard.write_all(format!("{}\n", command_json).as_bytes()).await
            .map_err(|e| format!("Send failed: {}", e))?;
            
        debug!("📡 Command {} sent, waiting for response...", command_id);

        // Receive response on same connection
        let mut reader = BufReader::new(&mut *stream_guard);
        let mut response_line = String::new();
        
        match reader.read_line(&mut response_line).await {
            Ok(0) => {
                error!("❌ Command {}: Connection closed while waiting for response", command_id);
                Err(format!("Connection closed while waiting for response to command {}", command_id))
            },
            Ok(bytes_read) => {
                let response_str = response_line.trim();
                if response_str.is_empty() {
                    return Err("Empty response received".to_string());
                }

                debug!("📥 Command {}: Received response ({} bytes)", command_id, bytes_read);

                // Parse JSON response
                let response: serde_json::Value = serde_json::from_str(response_str)
                    .map_err(|e| format!("JSON parse failed: {}", e))?;

                // Convert to HashMap for FFI return
                match response {
                    serde_json::Value::Object(map) => {
                        let hashmap: HashMap<String, serde_json::Value> = map.into_iter().collect();
                        debug!("✅ Command {}: Successfully processed", command_id);
                        Ok(hashmap)
                    },
                    _ => Err("Response is not a JSON object".to_string()),
                }
            },
            Err(e) => {
                error!("❌ Command {}: Receive error - {}", command_id, e);
                Err(format!("Receive failed: {}", e))
            }
        }
    }
}

/// Create new shared command client with default configuration
/// This is the preferred entry point for FFI wrappers (Magnus, pyo3, etc.)
pub fn create_default_command_client() -> SharedCommandClient {
    SharedCommandClient::new_default()
}

/// Create new shared command client with custom configuration
/// For FFI wrappers that need to specify host, port, timeouts
pub fn create_command_client(
    host: String,
    port: u16,
    timeout_seconds: u32,
    connect_timeout_seconds: u32,
) -> SharedCommandClient {
    let config = CommandClientConfig {
        host,
        port,
        timeout_seconds,
        connect_timeout_seconds,
        read_timeout_seconds: 10,
        default_namespace: "default".to_string(),
    };
    SharedCommandClient::new(config)
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
