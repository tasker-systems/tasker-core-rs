use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SocketType {
    Tcp,
    Unix,
}

/// TCP executor configuration
#[derive(Debug, Clone)]
pub struct TcpExecutorConfig {
    /// Socket type
    pub socket_type: SocketType,

    /// TCP bind address (e.g., "127.0.0.1:8080")
    pub bind_address: String,

    /// Command queue size per connection
    pub command_queue_size: usize,

    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,

    /// Graceful shutdown timeout in milliseconds
    pub graceful_shutdown_timeout_ms: u64,

    /// Maximum concurrent connections
    pub max_connections: usize,
}

impl Default for TcpExecutorConfig {
    fn default() -> Self {
        Self {
            socket_type: SocketType::Tcp,
            bind_address: "127.0.0.1:8080".to_string(),
            command_queue_size: 1000,
            connection_timeout_ms: 30000,
            graceful_shutdown_timeout_ms: 5000,
            max_connections: 1000,
        }
    }
}

/// Unix datagram executor configuration
#[derive(Debug, Clone)]
pub struct UnixDatagramConfig {
    /// Socket type
    pub socket_type: SocketType,

    /// Unix socket path (e.g., "/tmp/tasker.sock")
    pub client_socket_path: String,

    /// Server socket path (e.g., "/tmp/tasker.sock")
    pub server_socket_path: String,

    /// Command queue size per connection
    pub command_queue_size: usize,

    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,

    /// Graceful shutdown timeout in milliseconds
    pub graceful_shutdown_timeout_ms: u64,

    /// Maximum concurrent connections
    pub max_connections: usize,
}

impl Default for UnixDatagramConfig {
    fn default() -> Self {
        Self {
            socket_type: SocketType::Unix,
            client_socket_path: "/tmp/tasker.sock".to_string(),
            server_socket_path: "/tmp/tasker.sock".to_string(),
            command_queue_size: 1000,
            connection_timeout_ms: 30000,
            graceful_shutdown_timeout_ms: 5000,
            max_connections: 1000,
        }
    }
}
