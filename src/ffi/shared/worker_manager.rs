//! # Shared Worker Manager for FFI Architecture
//!
//! Language-agnostic worker lifecycle management that can be wrapped by Magnus (Ruby),
//! pyo3 (Python), or other FFI systems. Provides unified worker registration,
//! heartbeat management, and lifecycle coordination by integrating command_client
//! and command_listener into a cohesive worker management system.

use super::command_client::{SharedCommandClient, CommandClientConfig};
use super::command_listener::{SharedCommandListener, CommandListenerConfig};
use crate::execution::command::WorkerCapabilities;
use serde_json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// **SHARED WORKER MANAGER**: Language-agnostic worker lifecycle management for FFI wrappers
///
/// This provides unified worker management that can be wrapped by Magnus (Ruby),
/// pyo3 (Python), or other FFI systems without C-style exports. Integrates
/// command_client and command_listener to provide complete worker lifecycle management.
pub struct SharedWorkerManager {
    /// Worker manager configuration
    config: WorkerManagerConfig,
    /// Command client for server communication
    command_client: Option<Arc<RwLock<SharedCommandClient>>>,
    /// Command listener for incoming commands (optional for worker mode)
    command_listener: Option<Arc<RwLock<SharedCommandListener>>>,
    /// Worker state tracking
    worker_state: Arc<RwLock<WorkerState>>,
    /// Manager statistics
    stats: Arc<WorkerManagerStats>,
    /// Registered workers by ID
    registered_workers: Arc<Mutex<HashMap<String, WorkerInfo>>>,
    /// Heartbeat control
    heartbeat_control: Arc<HeartbeatControl>,
}

/// Worker manager configuration
#[derive(Debug, Clone)]
pub struct WorkerManagerConfig {
    pub server_host: String,
    pub server_port: u16,
    pub heartbeat_interval_seconds: u64,
    pub registration_timeout_seconds: u64,
    pub max_heartbeat_failures: u32,
    pub auto_reconnect: bool,
    pub worker_namespace: String,
    pub default_step_timeout_ms: u32,
}

impl Default for WorkerManagerConfig {
    fn default() -> Self {
        Self {
            server_host: "localhost".to_string(),
            server_port: 8080,
            heartbeat_interval_seconds: 30,
            registration_timeout_seconds: 10,
            max_heartbeat_failures: 3,
            auto_reconnect: true,
            worker_namespace: "default".to_string(),
            default_step_timeout_ms: 30000,
        }
    }
}

/// Worker state tracking
#[derive(Debug, Clone)]
pub struct WorkerState {
    pub mode: WorkerMode,
    pub status: WorkerStatus,
    pub registered_at: Option<SystemTime>,
    pub last_heartbeat_sent: Option<SystemTime>,
    pub last_heartbeat_received: Option<SystemTime>,
    pub heartbeat_failures: u32,
    pub current_load: u32,
    pub active_workers: Vec<String>,
}

/// Worker operation mode
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerMode {
    /// Worker mode - connects to server and registers as worker
    Worker,
    /// Server mode - accepts connections from workers
    Server,
    /// Hybrid mode - both worker and server capabilities
    Hybrid,
}

/// Worker status
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerStatus {
    Disconnected,
    Connecting,
    Connected,
    Registered,
    HeartbeatFailed,
    Error,
}

/// Manager statistics
#[derive(Debug)]
pub struct WorkerManagerStats {
    pub registrations_attempted: AtomicU64,
    pub registrations_successful: AtomicU64,
    pub heartbeats_sent: AtomicU64,
    pub heartbeats_failed: AtomicU64,
    pub commands_processed: AtomicU64,
    pub reconnection_attempts: AtomicU64,
}

impl Default for WorkerManagerStats {
    fn default() -> Self {
        Self {
            registrations_attempted: AtomicU64::new(0),
            registrations_successful: AtomicU64::new(0),
            heartbeats_sent: AtomicU64::new(0),
            heartbeats_failed: AtomicU64::new(0),
            commands_processed: AtomicU64::new(0),
            reconnection_attempts: AtomicU64::new(0),
        }
    }
}

/// Individual worker information
#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub capabilities: WorkerCapabilities,
    pub registered_at: SystemTime,
    pub last_heartbeat: SystemTime,
    pub current_load: u32,
    pub status: String,
}

/// Heartbeat control for background task management
#[derive(Debug)]
pub struct HeartbeatControl {
    pub running: AtomicBool,
    pub interval_seconds: AtomicU64,
}

impl Default for HeartbeatControl {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            interval_seconds: AtomicU64::new(30),
        }
    }
}

impl SharedWorkerManager {
    /// Create new shared worker manager with configuration
    pub fn new(config: WorkerManagerConfig) -> Self {
        Self {
            config,
            command_client: None,
            command_listener: None,
            worker_state: Arc::new(RwLock::new(WorkerState {
                mode: WorkerMode::Worker,
                status: WorkerStatus::Disconnected,
                registered_at: None,
                last_heartbeat_sent: None,
                last_heartbeat_received: None,
                heartbeat_failures: 0,
                current_load: 0,
                active_workers: Vec::new(),
            })),
            stats: Arc::new(WorkerManagerStats::default()),
            registered_workers: Arc::new(Mutex::new(HashMap::new())),
            heartbeat_control: Arc::new(HeartbeatControl::default()),
        }
    }

    /// Create new shared worker manager with default configuration
    pub fn new_default() -> Self {
        Self::new(WorkerManagerConfig::default())
    }

    /// Initialize as worker (connects to server)
    pub async fn initialize_as_worker(&mut self) -> Result<bool, String> {
        info!(
            "👷 Shared WorkerManager: Initializing as worker connecting to {}:{}",
            self.config.server_host, self.config.server_port
        );

        // Set worker mode
        {
            let mut state = self.worker_state.write().await;
            state.mode = WorkerMode::Worker;
            state.status = WorkerStatus::Connecting;
        }

        // Create command client configuration
        let client_config = CommandClientConfig {
            host: self.config.server_host.clone(),
            port: self.config.server_port,
            timeout_seconds: self.config.registration_timeout_seconds as u32,
            connect_timeout_seconds: 10,
            read_timeout_seconds: 15,
            default_namespace: self.config.worker_namespace.clone(),
        };

        // Create and connect command client
        let mut client = SharedCommandClient::new(client_config);
        match client.connect().await {
            Ok(_) => {
                self.command_client = Some(Arc::new(RwLock::new(client)));
                
                // Update state
                {
                    let mut state = self.worker_state.write().await;
                    state.status = WorkerStatus::Connected;
                }

                info!("✅ Shared WorkerManager: Successfully connected to server as worker");
                Ok(true)
            }
            Err(e) => {
                {
                    let mut state = self.worker_state.write().await;
                    state.status = WorkerStatus::Error;
                }
                Err(format!("Failed to connect to server: {}", e))
            }
        }
    }

    /// Initialize as server (accepts worker connections)
    pub async fn initialize_as_server(&mut self, bind_port: Option<u16>) -> Result<bool, String> {
        let port = bind_port.unwrap_or(self.config.server_port);
        
        info!(
            "🖥️ Shared WorkerManager: Initializing as server on port {}",
            port
        );

        // Set server mode
        {
            let mut state = self.worker_state.write().await;
            state.mode = WorkerMode::Server;
            state.status = WorkerStatus::Connecting;
        }

        // Create command listener configuration
        let listener_config = CommandListenerConfig {
            bind_address: "127.0.0.1".to_string(),
            port,
            max_connections: 100,
            connection_timeout_ms: 30000,
            command_queue_size: 1000,
            graceful_shutdown_timeout_ms: 5000,
            health_check_interval_seconds: 30,
        };

        // Create and start command listener
        let mut listener = SharedCommandListener::new(listener_config);
        match listener.start().await {
            Ok(_) => {
                self.command_listener = Some(Arc::new(RwLock::new(listener)));
                
                // Update state
                {
                    let mut state = self.worker_state.write().await;
                    state.status = WorkerStatus::Connected;
                }

                info!("✅ Shared WorkerManager: Successfully started server on port {}", port);
                Ok(true)
            }
            Err(e) => {
                {
                    let mut state = self.worker_state.write().await;
                    state.status = WorkerStatus::Error;
                }
                Err(format!("Failed to start server: {}", e))
            }
        }
    }

    /// Register worker with the server
    pub async fn register_worker(
        &self,
        worker_id: String,
        max_concurrent_steps: u32,
        supported_namespaces: Vec<String>,
        language_runtime: String,
        version: String,
        custom_capabilities: HashMap<String, serde_json::Value>,
    ) -> Result<HashMap<String, serde_json::Value>, String> {
        self.ensure_worker_mode().await?;

        info!(
            "📝 Shared WorkerManager: Registering worker {} with capabilities",
            worker_id
        );

        self.stats.registrations_attempted.fetch_add(1, Ordering::SeqCst);

        let client = self.get_command_client().await?;
        let client_guard = client.read().await;

        let result = client_guard.register_worker(
            worker_id.clone(),
            max_concurrent_steps,
            supported_namespaces.clone(),
            self.config.default_step_timeout_ms,
            true, // supports_retries
            language_runtime.clone(),
            version.clone(),
            custom_capabilities.clone(),
            None, // supported_tasks - TODO: Add parameter to register_worker_with_capabilities
        ).await;

        match result {
            Ok(response) => {
                // Update state
                {
                    let mut state = self.worker_state.write().await;
                    state.status = WorkerStatus::Registered;
                    state.registered_at = Some(SystemTime::now());
                }

                // Store worker info
                {
                    let mut workers = self.registered_workers.lock().map_err(|e| e.to_string())?;
                    let worker_capabilities = WorkerCapabilities {
                        worker_id: worker_id.clone(),
                        max_concurrent_steps: max_concurrent_steps as usize,
                        supported_namespaces,
                        step_timeout_ms: self.config.default_step_timeout_ms as u64,
                        supports_retries: true,
                        language_runtime,
                        version,
                        custom_capabilities,
                        connection_info: None,
                        runtime_info: None,
                        supported_tasks: None,
                    };

                    workers.insert(worker_id.clone(), WorkerInfo {
                        worker_id: worker_id.clone(),
                        capabilities: worker_capabilities,
                        registered_at: SystemTime::now(),
                        last_heartbeat: SystemTime::now(),
                        current_load: 0,
                        status: "registered".to_string(),
                    });
                }

                self.stats.registrations_successful.fetch_add(1, Ordering::SeqCst);

                info!("✅ Shared WorkerManager: Worker {} registered successfully", worker_id);
                Ok(response)
            }
            Err(e) => {
                error!("❌ Shared WorkerManager: Worker registration failed: {}", e);
                Err(e)
            }
        }
    }

    /// Send heartbeat to server
    pub async fn send_heartbeat(
        &self,
        worker_id: String,
        current_load: Option<u32>,
    ) -> Result<HashMap<String, serde_json::Value>, String> {
        self.ensure_worker_mode().await?;

        let load = current_load.unwrap_or(0);
        
        debug!(
            "💓 Shared WorkerManager: Sending heartbeat for worker {} (load: {})",
            worker_id, load
        );

        self.stats.heartbeats_sent.fetch_add(1, Ordering::SeqCst);

        let client = self.get_command_client().await?;
        let client_guard = client.read().await;

        // Create basic system stats
        let system_stats = Some(HashMap::from([
            ("cpu_usage_percent".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(15.0).unwrap_or_else(|| serde_json::Number::from(15)))),
            ("memory_usage_mb".to_string(), serde_json::Value::Number(serde_json::Number::from(256))),
            ("active_connections".to_string(), serde_json::Value::Number(serde_json::Number::from(1))),
            ("uptime_seconds".to_string(), serde_json::Value::Number(serde_json::Number::from(3600))),
        ]));

        let result = client_guard.send_heartbeat(worker_id.clone(), load, system_stats).await;

        match result {
            Ok(response) => {
                // Update state
                {
                    let mut state = self.worker_state.write().await;
                    state.last_heartbeat_sent = Some(SystemTime::now());
                    state.current_load = load;
                    state.heartbeat_failures = 0; // Reset failures on success
                }

                // Update worker info
                {
                    let mut workers = self.registered_workers.lock().map_err(|e| e.to_string())?;
                    if let Some(worker) = workers.get_mut(&worker_id) {
                        worker.last_heartbeat = SystemTime::now();
                        worker.current_load = load;
                        worker.status = "active".to_string();
                    }
                }

                debug!("✅ Shared WorkerManager: Heartbeat sent successfully for worker {}", worker_id);
                Ok(response)
            }
            Err(e) => {
                self.stats.heartbeats_failed.fetch_add(1, Ordering::SeqCst);
                
                // Update failure count
                {
                    let mut state = self.worker_state.write().await;
                    state.heartbeat_failures += 1;
                    
                    if state.heartbeat_failures >= self.config.max_heartbeat_failures {
                        state.status = WorkerStatus::HeartbeatFailed;
                        warn!("⚠️ Shared WorkerManager: Worker {} exceeded max heartbeat failures", worker_id);
                    }
                }

                error!("❌ Shared WorkerManager: Heartbeat failed for worker {}: {}", worker_id, e);
                Err(e)
            }
        }
    }

    /// Unregister worker from server
    pub async fn unregister_worker(
        &self,
        worker_id: String,
        reason: Option<String>,
    ) -> Result<HashMap<String, serde_json::Value>, String> {
        self.ensure_worker_mode().await?;

        info!(
            "🗑️ Shared WorkerManager: Unregistering worker {} (reason: {:?})",
            worker_id, reason
        );

        let client = self.get_command_client().await?;
        let client_guard = client.read().await;

        let result = client_guard.unregister_worker(worker_id.clone(), reason).await;

        match result {
            Ok(response) => {
                // Remove from registered workers
                {
                    let mut workers = self.registered_workers.lock().map_err(|e| e.to_string())?;
                    workers.remove(&worker_id);
                }

                // Update state if this was the last worker
                {
                    let mut state = self.worker_state.write().await;
                    state.active_workers.retain(|id| id != &worker_id);
                    
                    if state.active_workers.is_empty() {
                        state.status = WorkerStatus::Connected;
                        state.registered_at = None;
                    }
                }

                info!("✅ Shared WorkerManager: Worker {} unregistered successfully", worker_id);
                Ok(response)
            }
            Err(e) => {
                error!("❌ Shared WorkerManager: Worker unregistration failed: {}", e);
                Err(e)
            }
        }
    }

    /// Start automatic heartbeat for a worker
    pub async fn start_heartbeat(
        &self,
        worker_id: String,
        interval_seconds: Option<u64>,
    ) -> Result<(), String> {
        let interval = interval_seconds.unwrap_or(self.config.heartbeat_interval_seconds);
        
        info!(
            "💓 Shared WorkerManager: Starting automatic heartbeat for worker {} (interval: {}s)",
            worker_id, interval
        );

        self.heartbeat_control.running.store(true, Ordering::SeqCst);
        self.heartbeat_control.interval_seconds.store(interval, Ordering::SeqCst);

        // In a real implementation, this would spawn a background task
        // For now, we just mark it as started
        debug!("Heartbeat background task would be started here for worker {}", worker_id);

        Ok(())
    }

    /// Stop automatic heartbeat
    pub async fn stop_heartbeat(&self) -> Result<(), String> {
        info!("💓 Shared WorkerManager: Stopping automatic heartbeat");

        self.heartbeat_control.running.store(false, Ordering::SeqCst);

        debug!("Heartbeat background task would be stopped here");

        Ok(())
    }

    /// Get worker manager status
    pub async fn get_status(&self) -> HashMap<String, serde_json::Value> {
        let state = self.worker_state.read().await;
        let mut status = HashMap::new();

        status.insert(
            "mode".to_string(),
            serde_json::Value::String(format!("{:?}", state.mode)),
        );
        status.insert(
            "status".to_string(),
            serde_json::Value::String(format!("{:?}", state.status)),
        );
        status.insert(
            "heartbeat_failures".to_string(),
            serde_json::Value::Number(state.heartbeat_failures.into()),
        );
        status.insert(
            "current_load".to_string(),
            serde_json::Value::Number(state.current_load.into()),
        );
        status.insert(
            "active_workers_count".to_string(),
            serde_json::Value::Number(state.active_workers.len().into()),
        );

        if let Some(registered_at) = state.registered_at {
            if let Ok(duration) = registered_at.duration_since(UNIX_EPOCH) {
                status.insert(
                    "registered_at".to_string(),
                    serde_json::Value::Number(duration.as_secs().into()),
                );
            }
        }

        if let Some(last_heartbeat) = state.last_heartbeat_sent {
            if let Ok(duration) = last_heartbeat.duration_since(UNIX_EPOCH) {
                status.insert(
                    "last_heartbeat_sent".to_string(),
                    serde_json::Value::Number(duration.as_secs().into()),
                );
            }
        }

        status
    }

    /// Get worker manager statistics
    pub async fn get_statistics(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();

        stats.insert(
            "registrations_attempted".to_string(),
            serde_json::Value::Number(self.stats.registrations_attempted.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "registrations_successful".to_string(),
            serde_json::Value::Number(self.stats.registrations_successful.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "heartbeats_sent".to_string(),
            serde_json::Value::Number(self.stats.heartbeats_sent.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "heartbeats_failed".to_string(),
            serde_json::Value::Number(self.stats.heartbeats_failed.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "commands_processed".to_string(),
            serde_json::Value::Number(self.stats.commands_processed.load(Ordering::SeqCst).into()),
        );
        stats.insert(
            "reconnection_attempts".to_string(),
            serde_json::Value::Number(self.stats.reconnection_attempts.load(Ordering::SeqCst).into()),
        );

        // Calculate success rates
        let total_registrations = self.stats.registrations_attempted.load(Ordering::SeqCst);
        let successful_registrations = self.stats.registrations_successful.load(Ordering::SeqCst);
        let registration_success_rate = if total_registrations > 0 {
            (successful_registrations as f64 / total_registrations as f64) * 100.0
        } else {
            100.0
        };

        stats.insert(
            "registration_success_rate_percentage".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(registration_success_rate).unwrap_or_else(|| serde_json::Number::from(0))),
        );

        let total_heartbeats = self.stats.heartbeats_sent.load(Ordering::SeqCst);
        let failed_heartbeats = self.stats.heartbeats_failed.load(Ordering::SeqCst);
        let heartbeat_success_rate = if total_heartbeats > 0 {
            ((total_heartbeats - failed_heartbeats) as f64 / total_heartbeats as f64) * 100.0
        } else {
            100.0
        };

        stats.insert(
            "heartbeat_success_rate_percentage".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(heartbeat_success_rate).unwrap_or_else(|| serde_json::Number::from(0))),
        );

        stats
    }

    // Private helper methods

    /// Ensure manager is in worker mode
    async fn ensure_worker_mode(&self) -> Result<(), String> {
        let state = self.worker_state.read().await;
        match state.mode {
            WorkerMode::Worker | WorkerMode::Hybrid => Ok(()),
            WorkerMode::Server => Err("Operation requires worker mode".to_string()),
        }
    }

    /// Get command client reference
    async fn get_command_client(&self) -> Result<Arc<RwLock<SharedCommandClient>>, String> {
        self.command_client
            .as_ref()
            .ok_or_else(|| "Command client not initialized".to_string())
            .map(|client| client.clone())
    }
}

/// Create new shared worker manager with default configuration
/// This is the preferred entry point for FFI wrappers (Magnus, pyo3, etc.)
pub fn create_default_worker_manager() -> SharedWorkerManager {
    SharedWorkerManager::new_default()
}

/// Create new shared worker manager with custom configuration
/// For FFI wrappers that need to specify server connection details
pub fn create_worker_manager(
    server_host: String,
    server_port: u16,
    heartbeat_interval_seconds: u64,
    worker_namespace: String,
) -> SharedWorkerManager {
    let config = WorkerManagerConfig {
        server_host,
        server_port,
        heartbeat_interval_seconds,
        registration_timeout_seconds: 10,
        max_heartbeat_failures: 3,
        auto_reconnect: true,
        worker_namespace,
        default_step_timeout_ms: 30000,
    };
    SharedWorkerManager::new(config)
}