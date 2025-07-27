//! Generic Transport-Agnostic Executor
//!
//! This module provides a generic executor that can work with any transport
//! implementation (TCP, Unix sockets, etc.) without knowing the specific details
//! of the underlying transport layer.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use crate::execution::command::{Command, CommandType};
use crate::execution::command_router::{CommandRouter, CommandRouterError};
use crate::execution::worker_pool::{WorkerPool, WorkerPoolError};
use crate::execution::transport::{Transport, TransportConfig, TransportListener, TransportConnection, ConnectionInfo};

/// Generic executor that works with any transport implementation
pub struct GenericExecutor<T: Transport> {
    /// Transport instance
    transport: T,

    /// Command router for handling all commands
    command_router: Arc<CommandRouter>,

    /// Worker pool for managing connected workers
    worker_pool: Arc<WorkerPool>,

    /// Active connections by connection ID
    connections: Arc<RwLock<HashMap<String, ConnectionState>>>,

    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,

    /// Shutdown signal receiver (for cloning)
    shutdown_rx: broadcast::Receiver<()>,

    /// Server state
    server_state: Arc<RwLock<ServerState>>,

    /// Phantom data to maintain type information
    _phantom: PhantomData<T>,
}

impl<T: Transport + 'static> GenericExecutor<T>
where
    T::Listener: 'static,
{
    /// Create new generic executor with transport
    pub async fn new(transport: T) -> Result<Self, GenericExecutorError> {
        let command_router = Arc::new(CommandRouter::new());
        let worker_pool = Arc::new(WorkerPool::new());
        let connections = Arc::new(RwLock::new(HashMap::new()));

        let (shutdown_tx, shutdown_rx) = broadcast::channel(16);

        let server_state = Arc::new(RwLock::new(ServerState {
            running: false,
            start_time: None,
            total_connections: 0,
            active_connections: 0,
        }));

        Ok(Self {
            transport,
            command_router,
            worker_pool,
            connections,
            shutdown_tx,
            shutdown_rx,
            server_state,
            _phantom: PhantomData,
        })
    }

    /// Start the server and begin accepting connections
    pub async fn start(&self) -> Result<(), GenericExecutorError> {
        let mut state = self.server_state.write().await;
        if state.running {
            return Err(GenericExecutorError::ServerAlreadyRunning);
        }

        info!("Starting GenericExecutor on {}", self.transport.config().bind_address());

        // Create transport listener
        let listener = self.transport.create_listener().await
            .map_err(|e| GenericExecutorError::BindFailed {
                address: self.transport.config().bind_address().to_string(),
                error: e.to_string(),
            })?;

        state.running = true;
        state.start_time = Some(chrono::Utc::now());
        drop(state);

        info!("GenericExecutor listening on {}", self.transport.config().bind_address());

        // Start connection acceptance loop
        let executor_clone = self.clone();
        tokio::spawn(async move {
            executor_clone.accept_connections(listener).await;
        });

        Ok(())
    }

    /// Stop the server gracefully
    pub async fn stop(&self) -> Result<(), GenericExecutorError> {
        let mut state = self.server_state.write().await;
        if !state.running {
            return Ok(());
        }

        info!("Stopping GenericExecutor gracefully");

        // Send shutdown signal
        let _ = self.shutdown_tx.send(());

        // Close all connections
        let connections = self.connections.read().await;
        for (connection_id, connection_state) in connections.iter() {
            info!("Closing connection: {}", connection_id);
            let _ = connection_state.shutdown_tx.send(());
        }
        drop(connections);

        // Wait for connections to close
        sleep(Duration::from_millis(self.transport.config().graceful_shutdown_timeout_ms())).await;

        state.running = false;
        info!("GenericExecutor stopped");

        Ok(())
    }

    /// Check if server is running
    pub async fn is_running(&self) -> bool {
        self.server_state.read().await.running
    }

    /// Get server statistics
    pub async fn get_stats(&self) -> ExecutorStats {
        let state = self.server_state.read().await;
        let connections = self.connections.read().await;
        let worker_stats = self.worker_pool.get_stats().await;
        let router_stats = self.command_router.get_stats().await;

        ExecutorStats {
            running: state.running,
            uptime_seconds: state.start_time
                .map(|start| (chrono::Utc::now() - start).num_seconds() as u64)
                .unwrap_or(0),
            total_connections: state.total_connections,
            active_connections: connections.len(),
            registered_workers: worker_stats.total_workers,
            commands_processed: router_stats.total_commands_processed,
            bind_address: self.transport.config().bind_address().to_string(),
        }
    }

    /// Get command router for registering handlers
    pub fn command_router(&self) -> Arc<CommandRouter> {
        self.command_router.clone()
    }

    /// Get worker pool for monitoring workers
    pub fn worker_pool(&self) -> Arc<WorkerPool> {
        self.worker_pool.clone()
    }

    /// Get transport reference
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Connection acceptance loop
    async fn accept_connections(&self, listener: T::Listener) {
        let mut shutdown_rx = self.shutdown_rx.resubscribe();

        loop {
            tokio::select! {
                // Handle new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((connection, connection_info)) => {
                            let connection_id = uuid::Uuid::new_v4().to_string();
                            info!("New connection: {} from {} ({})", 
                                connection_id, 
                                connection_info.peer_address,
                                format!("{:?}", connection_info.transport_type)
                            );

                            // Update server state
                            {
                                let mut state = self.server_state.write().await;
                                state.total_connections += 1;
                                state.active_connections += 1;
                            }

                            // Handle connection
                            self.handle_connection(connection_id, connection, connection_info).await;
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }

                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Connection acceptance loop shutting down");
                    break;
                }
            }
        }
    }

    /// Handle individual connection
    async fn handle_connection(&self, connection_id: String, connection: T::Connection, connection_info: ConnectionInfo) {
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel(16);

        // Create connection state
        let connection_state = ConnectionState {
            connection_id: connection_id.clone(),
            peer_address: connection_info.peer_address,
            connected_at: chrono::Utc::now(),
            shutdown_tx: shutdown_tx.clone(),
        };

        // Store connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(connection_id.clone(), connection_state);
        }

        // Split connection for reading and writing
        let (mut reader, writer) = connection.split();
        let writer = Arc::new(tokio::sync::Mutex::new(writer));

        // Create command processing channel
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(self.transport.config().command_queue_size());

        // Clone references for tasks
        let executor = self.clone();
        let writer_clone = writer.clone();

        // Spawn command processing task
        let cmd_processor = tokio::spawn(async move {
            while let Some(command) = cmd_rx.recv().await {
                executor.process_command(command, writer_clone.clone()).await;
            }
        });

        // Message reading loop
        let mut line = String::new();
        loop {
            tokio::select! {
                // Read messages from connection
                read_result = reader.read_line(&mut line) => {
                    match read_result {
                        Ok(0) => {
                            // Connection closed by client
                            info!("Connection {} closed by client", connection_id);
                            break;
                        }
                        Ok(_) => {
                            // Parse and send command for processing
                            if let Some(command) = self.parse_command(&line.trim()) {
                                if cmd_tx.send(command).await.is_err() {
                                    error!("Command queue full for connection {}", connection_id);
                                    break;
                                }
                            }
                            line.clear();
                        }
                        Err(e) => {
                            error!("Error reading from connection {}: {}", connection_id, e);
                            break;
                        }
                    }
                }

                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Connection {} shutting down", connection_id);
                    break;
                }
            }
        }

        // Cleanup
        cmd_processor.abort();

        // Remove connection from active connections
        {
            let mut connections = self.connections.write().await;
            connections.remove(&connection_id);
        }

        // Update server state
        {
            let mut state = self.server_state.write().await;
            state.active_connections = state.active_connections.saturating_sub(1);
        }

        info!("Connection {} handler completed", connection_id);
    }

    /// Process a command through the command router
    async fn process_command<W>(&self, command: Command, writer: Arc<tokio::sync::Mutex<W>>) 
    where
        W: tokio::io::AsyncWriteExt + Send + Unpin,
    {
        debug!("Processing command: type={:?}, id={}", command.command_type, command.command_id);

        // Route command through command router
        match self.command_router.route_command(command.clone()).await {
            Ok(result) => {
                // Send response if available
                if let Some(response) = result.response {
                    self.send_response(response, writer).await;
                }
            }
            Err(e) => {
                error!("Command routing failed: {}", e);

                // Send error response
                let error_response = command.create_response(
                    CommandType::Error,
                    crate::execution::command::CommandPayload::Error {
                        error_type: "CommandRoutingError".to_string(),
                        message: e.to_string(),
                        details: None,
                        retryable: false,
                    },
                    crate::execution::command::CommandSource::RustServer {
                        id: "generic_executor".to_string(),
                    },
                );

                self.send_response(error_response, writer).await;
            }
        }
    }

    /// Send response command over connection
    async fn send_response<W>(&self, response: Command, writer: Arc<tokio::sync::Mutex<W>>) 
    where
        W: tokio::io::AsyncWriteExt + Send + Unpin,
    {
        match serde_json::to_string(&response) {
            Ok(json) => {
                let message = format!("{}\n", json);

                let mut writer_guard = writer.lock().await;
                if let Err(e) = writer_guard.write_all(message.as_bytes()).await {
                    error!("Failed to send response: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to serialize response: {}", e);
            }
        }
    }

    /// Parse incoming line as command
    pub fn parse_command(&self, line: &str) -> Option<Command> {
        if line.is_empty() {
            return None;
        }

        match serde_json::from_str::<Command>(line) {
            Ok(command) => Some(command),
            Err(e) => {
                warn!("Failed to parse command: {} - line: {}", e, line);
                None
            }
        }
    }
}

// Clone implementation to support Arc usage
impl<T: Transport> Clone for GenericExecutor<T> {
    fn clone(&self) -> Self {
        Self {
            transport: self.transport.clone(),
            command_router: self.command_router.clone(),
            worker_pool: self.worker_pool.clone(),
            connections: self.connections.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
            shutdown_rx: self.shutdown_tx.subscribe(),
            server_state: self.server_state.clone(),
            _phantom: PhantomData,
        }
    }
}

/// Connection state information
#[derive(Debug)]
struct ConnectionState {
    connection_id: String,
    peer_address: String,
    connected_at: chrono::DateTime<chrono::Utc>,
    shutdown_tx: broadcast::Sender<()>,
}

/// Server state information
#[derive(Debug)]
struct ServerState {
    running: bool,
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    total_connections: u64,
    active_connections: usize,
}

/// Generic executor statistics
#[derive(Debug, Clone)]
pub struct ExecutorStats {
    pub running: bool,
    pub uptime_seconds: u64,
    pub total_connections: u64,
    pub active_connections: usize,
    pub registered_workers: usize,
    pub commands_processed: usize,
    pub bind_address: String,
}

/// Generic executor errors
#[derive(Debug, thiserror::Error)]
pub enum GenericExecutorError {
    #[error("Server is already running")]
    ServerAlreadyRunning,

    #[error("Failed to bind to address {address}: {error}")]
    BindFailed { address: String, error: String },

    #[error("Connection error: {message}")]
    ConnectionError { message: String },

    #[error("Command processing error: {error}")]
    CommandProcessingError { error: String },

    #[error("Worker pool error: {0}")]
    WorkerPoolError(#[from] WorkerPoolError),

    #[error("Command router error: {0}")]
    CommandRouterError(#[from] CommandRouterError),
}

// Type aliases for convenience
pub type TcpExecutor = GenericExecutor<crate::execution::transport::TcpTransport>;
pub type UnixDatagramExecutor = GenericExecutor<crate::execution::transport::UnixDatagramTransport>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::transport::{TcpTransport, TcpTransportConfig};

    #[tokio::test]
    async fn test_generic_executor_creation() {
        let transport = TcpTransport::new(TcpTransportConfig::default());
        let executor = GenericExecutor::new(transport).await.unwrap();

        assert!(!executor.is_running().await);

        let stats = executor.get_stats().await;
        assert!(!stats.running);
        assert_eq!(stats.active_connections, 0);
    }

    #[tokio::test]
    async fn test_generic_executor_start_stop() {
        let config = TcpTransportConfig {
            bind_address: "127.0.0.1:0".to_string(), // Use OS-assigned port
            ..TcpTransportConfig::default()
        };
        let transport = TcpTransport::new(config);
        let executor = GenericExecutor::new(transport).await.unwrap();

        // Start should succeed
        assert!(executor.start().await.is_ok());
        assert!(executor.is_running().await);

        // Start again should fail
        assert!(executor.start().await.is_err());

        // Stop should succeed
        assert!(executor.stop().await.is_ok());
        assert!(!executor.is_running().await);
    }
}
