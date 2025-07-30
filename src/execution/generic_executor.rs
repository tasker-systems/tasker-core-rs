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
use crate::execution::transport::{
    ConnectionInfo, Transport, TransportConfig, TransportConnection, TransportListener,
};
use crate::execution::worker_pool::{WorkerPool, WorkerPoolError};

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

        info!(
            "Starting GenericExecutor on {}",
            self.transport.config().bind_address()
        );

        // Create transport listener
        let listener = self.transport.create_listener().await.map_err(|e| {
            GenericExecutorError::BindFailed {
                address: self.transport.config().bind_address().to_string(),
                error: e.to_string(),
            }
        })?;

        state.running = true;
        state.start_time = Some(chrono::Utc::now());
        drop(state);

        info!(
            "GenericExecutor listening on {}",
            self.transport.config().bind_address()
        );

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
        sleep(Duration::from_millis(
            self.transport.config().graceful_shutdown_timeout_ms(),
        ))
        .await;

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
            uptime_seconds: state
                .start_time
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
    async fn handle_connection(
        &self,
        connection_id: String,
        connection: T::Connection,
        connection_info: ConnectionInfo,
    ) {
        info!("🔌 CONNECTION {}: Starting from {} (transport: {:?})", connection_id, connection_info.peer_address, connection_info.transport_type);
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

        // Clone executor for immediate command processing
        let executor = self.clone();

        // Message reading loop - PROCESS COMMANDS IMMEDIATELY (no separate task)
        let mut line = String::new();
        let mut commands_processed = 0;
        info!("🔌 CONNECTION {}: Entering read loop", connection_id);
        
        loop {
            tokio::select! {
                // Read messages from connection
                read_result = reader.read_line(&mut line) => {
                    match read_result {
                        Ok(0) => {
                            // Connection closed by client
                            info!("🔌 CONNECTION {}: Closed by client after {} commands", connection_id, commands_processed);
                            break;
                        }
                        Ok(bytes_read) => {
                            info!("🔌 CONNECTION {}: Received {} bytes: '{}'", connection_id, bytes_read, line.trim());
                            
                            // Parse and IMMEDIATELY process command (no queuing)
                            if let Some(command) = self.parse_command(&line.trim()) {
                                commands_processed += 1;
                                info!("🔌 CONNECTION {}: Parsed command #{}: type={:?}, id={}", connection_id, commands_processed, command.command_type, command.command_id);
                                
                                // Log command details for debugging
                                if matches!(command.command_type, CommandType::InitializeTask) {
                                    info!("🎯 CONNECTION {}: InitializeTask command details: {:?}", connection_id, command);
                                    info!("🎯 CONNECTION {}: About to process InitializeTask", connection_id);
                                }
                                
                                // IMMEDIATE PROCESSING - no separate task, no queuing
                                let process_start = std::time::Instant::now();
                                executor.process_command(command.clone(), writer.clone()).await;
                                let process_time = process_start.elapsed();
                                
                                info!("🔌 CONNECTION {}: Finished processing command #{} (type={:?}) in {:?}", 
                                    connection_id, commands_processed, command.command_type, process_time);
                                    
                                if matches!(command.command_type, CommandType::InitializeTask) {
                                    info!("🎯 CONNECTION {}: InitializeTask processing completed in {:?}", connection_id, process_time);
                                }
                            } else {
                                warn!("🔌 CONNECTION {}: Failed to parse message: '{}'", connection_id, line.trim());
                                
                                // Send error response for unparseable commands
                                let error_response = Command::new(
                                    CommandType::Error,
                                    crate::execution::command::CommandPayload::Error {
                                        error_type: "ParseError".to_string(),
                                        message: "Failed to parse command from JSON".to_string(),
                                        details: Some({
                                            let mut details = std::collections::HashMap::new();
                                            details.insert("raw_message".to_string(), serde_json::Value::String(line.trim().to_string()));
                                            details
                                        }),
                                        retryable: false,
                                    },
                                    crate::execution::command::CommandSource::RustServer {
                                        id: "generic_executor".to_string(),
                                    },
                                );
                                executor.send_response(error_response, writer.clone()).await;
                            }
                            line.clear();
                        }
                        Err(e) => {
                            error!("🔌 CONNECTION {}: Error reading after {} commands: {}", connection_id, commands_processed, e);
                            break;
                        }
                    }
                }

                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("🔌 CONNECTION {}: Shutting down via signal after {} commands", connection_id, commands_processed);
                    break;
                }
            }
        }

        // No command processor task to abort - commands processed immediately

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

        info!("🔌 CONNECTION {}: Handler completed after processing {} commands", connection_id, commands_processed);
    }

    /// Process a command through the command router
    async fn process_command<W>(&self, command: Command, writer: Arc<tokio::sync::Mutex<W>>)
    where
        W: tokio::io::AsyncWriteExt + Send + Unpin,
    {
        debug!(
            "Processing command: type={:?}, id={}",
            command.command_type, command.command_id
        );
        
        // Enhanced debugging for InitializeTask
        if matches!(command.command_type, CommandType::InitializeTask) {
            info!("🎯 PROCESS_COMMAND: About to route InitializeTask to command router");
        }

        // Route command through command router
        match self.command_router.route_command(command.clone()).await {
            Ok(result) => {
                if matches!(command.command_type, CommandType::InitializeTask) {
                    info!("🎯 PROCESS_COMMAND: InitializeTask routed successfully, has_response={}", result.response.is_some());
                }
                
                // Send response if available
                if let Some(response) = result.response {
                    self.send_response(response, writer).await;
                } else {
                    warn!("Command {} returned no response - this violates command-response pattern!", command.command_id);
                    
                    // Always send a response - even if handler didn't provide one
                    let error_response = command.create_response(
                        CommandType::Error,
                        crate::execution::command::CommandPayload::Error {
                            error_type: "NoResponse".to_string(),
                            message: "Command handler did not provide a response".to_string(),
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
        info!("📤 SEND_RESPONSE: Sending response type={:?}, id={}", response.command_type, response.command_id);
        
        match serde_json::to_string(&response) {
            Ok(json) => {
                let message = format!("{}\n", json);
                info!("📤 SEND_RESPONSE: Serialized {} bytes for type={:?}", message.len(), response.command_type);

                let mut writer_guard = writer.lock().await;
                match writer_guard.write_all(message.as_bytes()).await {
                    Ok(_) => {
                        info!("📤 SEND_RESPONSE: Successfully sent response type={:?}", response.command_type);
                    }
                    Err(e) => {
                        error!("📤 SEND_RESPONSE: Failed to send response type={:?}: {}", response.command_type, e);
                    }
                }
            }
            Err(e) => {
                error!("📤 SEND_RESPONSE: Failed to serialize response type={:?}: {}", response.command_type, e);
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
