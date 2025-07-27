//! Transport Layer Abstraction
//!
//! This module provides a generic transport layer that abstracts over different
//! socket types (TCP, Unix datagram) so the executor can work with any transport
//! without knowing the underlying implementation details.

use async_trait::async_trait;
use std::fmt::Debug;
use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UnixDatagram};

/// Generic transport trait that abstracts over different socket types
#[async_trait]
pub trait Transport: Send + Sync + Debug + Clone {
    /// The type of the listener for this transport
    type Listener: TransportListener<Connection = Self::Connection> + Send + Sync;
    
    /// The type of connection for this transport
    type Connection: TransportConnection + Send + Sync;
    
    /// The configuration type for this transport
    type Config: TransportConfig + Send + Sync + Clone;
    
    /// Create a new transport instance with the given configuration
    fn new(config: Self::Config) -> Self;
    
    /// Create a listener that can accept connections
    async fn create_listener(&self) -> io::Result<Self::Listener>;
    
    /// Get the transport configuration
    fn config(&self) -> &Self::Config;
}

/// Trait for transport listeners that can accept connections
#[async_trait]
pub trait TransportListener: Send + Sync {
    type Connection: TransportConnection + Send + Sync;
    
    /// Accept a new connection
    async fn accept(&self) -> io::Result<(Self::Connection, ConnectionInfo)>;
}

/// Trait for individual transport connections
#[async_trait]
pub trait TransportConnection: Send + Sync {
    type Reader: AsyncBufReadExt + Send + Unpin;
    type Writer: AsyncWriteExt + Send + Unpin;
    
    /// Split the connection into separate reader and writer
    fn split(self) -> (Self::Reader, Self::Writer);
}

/// Configuration trait for transports
pub trait TransportConfig: Send + Sync + Clone + Debug {
    /// Get the bind address or path for this transport
    fn bind_address(&self) -> &str;
    
    /// Get the connection timeout in milliseconds
    fn connection_timeout_ms(&self) -> u64;
    
    /// Get the command queue size
    fn command_queue_size(&self) -> usize;
    
    /// Get the maximum number of connections
    fn max_connections(&self) -> usize;
    
    /// Get the graceful shutdown timeout in milliseconds
    fn graceful_shutdown_timeout_ms(&self) -> u64;
}

/// Information about a connection (peer address, etc.)
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub peer_address: String,
    pub transport_type: TransportType,
}

/// Transport type enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportType {
    Tcp,
    UnixDatagram,
}

/// TCP transport implementation
#[derive(Debug, Clone)]
pub struct TcpTransport {
    config: TcpTransportConfig,
}

/// TCP transport configuration
#[derive(Debug, Clone)]
pub struct TcpTransportConfig {
    pub bind_address: String,
    pub connection_timeout_ms: u64,
    pub command_queue_size: usize,
    pub max_connections: usize,
    pub graceful_shutdown_timeout_ms: u64,
}

impl Default for TcpTransportConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".to_string(),
            connection_timeout_ms: 30000,
            command_queue_size: 1000,
            max_connections: 1000,
            graceful_shutdown_timeout_ms: 5000,
        }
    }
}

impl TransportConfig for TcpTransportConfig {
    fn bind_address(&self) -> &str {
        &self.bind_address
    }
    
    fn connection_timeout_ms(&self) -> u64 {
        self.connection_timeout_ms
    }
    
    fn command_queue_size(&self) -> usize {
        self.command_queue_size
    }
    
    fn max_connections(&self) -> usize {
        self.max_connections
    }
    
    fn graceful_shutdown_timeout_ms(&self) -> u64 {
        self.graceful_shutdown_timeout_ms
    }
}

/// TCP listener wrapper
pub struct TcpTransportListener {
    listener: TcpListener,
}

/// TCP connection wrapper
pub struct TcpTransportConnection {
    stream: TcpStream,
}

#[async_trait]
impl Transport for TcpTransport {
    type Listener = TcpTransportListener;
    type Connection = TcpTransportConnection;
    type Config = TcpTransportConfig;
    
    fn new(config: Self::Config) -> Self {
        Self { config }
    }
    
    async fn create_listener(&self) -> io::Result<Self::Listener> {
        let listener = TcpListener::bind(&self.config.bind_address).await?;
        Ok(TcpTransportListener { listener })
    }
    
    fn config(&self) -> &Self::Config {
        &self.config
    }
}

#[async_trait]
impl TransportListener for TcpTransportListener {
    type Connection = TcpTransportConnection;
    
    async fn accept(&self) -> io::Result<(Self::Connection, ConnectionInfo)> {
        let (stream, addr) = self.listener.accept().await?;
        let connection = TcpTransportConnection { stream };
        let info = ConnectionInfo {
            peer_address: addr.to_string(),
            transport_type: TransportType::Tcp,
        };
        Ok((connection, info))
    }
}

#[async_trait]
impl TransportConnection for TcpTransportConnection {
    type Reader = BufReader<tokio::net::tcp::OwnedReadHalf>;
    type Writer = tokio::net::tcp::OwnedWriteHalf;
    
    fn split(self) -> (Self::Reader, Self::Writer) {
        let (reader, writer) = self.stream.into_split();
        (BufReader::new(reader), writer)
    }
}

/// Unix datagram transport implementation
#[derive(Debug, Clone)]
pub struct UnixDatagramTransport {
    config: UnixDatagramTransportConfig,
}

/// Unix datagram transport configuration
#[derive(Debug, Clone)]
pub struct UnixDatagramTransportConfig {
    pub socket_path: String,
    pub connection_timeout_ms: u64,
    pub command_queue_size: usize,
    pub max_connections: usize,
    pub graceful_shutdown_timeout_ms: u64,
}

impl Default for UnixDatagramTransportConfig {
    fn default() -> Self {
        Self {
            socket_path: "/tmp/tasker.sock".to_string(),
            connection_timeout_ms: 30000,
            command_queue_size: 1000,
            max_connections: 1000,
            graceful_shutdown_timeout_ms: 5000,
        }
    }
}

impl TransportConfig for UnixDatagramTransportConfig {
    fn bind_address(&self) -> &str {
        &self.socket_path
    }
    
    fn connection_timeout_ms(&self) -> u64 {
        self.connection_timeout_ms
    }
    
    fn command_queue_size(&self) -> usize {
        self.command_queue_size
    }
    
    fn max_connections(&self) -> usize {
        self.max_connections
    }
    
    fn graceful_shutdown_timeout_ms(&self) -> u64 {
        self.graceful_shutdown_timeout_ms
    }
}

/// Unix datagram listener - uses a custom implementation since UnixDatagram
/// doesn't have the same accept pattern as TCP
pub struct UnixDatagramListener {
    socket: UnixDatagram,
}

/// Unix datagram connection wrapper
pub struct UnixDatagramConnection {
    socket: UnixDatagram,
    peer_addr: Option<std::os::unix::net::SocketAddr>,
}

/// Custom reader/writer implementations for Unix datagram sockets
pub struct UnixDatagramReader {
    socket: std::sync::Arc<UnixDatagram>,
    buffer: Vec<u8>,
}

#[async_trait]
impl AsyncBufRead for UnixDatagramReader {
    fn poll_fill_buf<'a>(
        self: std::pin::Pin<&'a mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<&'a [u8]>> {
        let this = self.get_mut();
        let buffer = &mut this.buffer;
        match this.socket.try_recv(buffer) {
            Ok(len) => std::task::Poll::Ready(Ok(&buffer[..len])),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
            Err(e) => std::task::Poll::Ready(Err(e)),
        }
    }

    fn consume(self: std::pin::Pin<&mut Self>, _amt: usize) {
        // Nothing to do here for Unix Datagram as we have to consume manually in poll_fill_buf
    }
}

pub struct UnixDatagramWriter {
    socket: std::sync::Arc<UnixDatagram>,
    peer_addr: Option<std::os::unix::net::SocketAddr>,
}

#[async_trait]
impl Transport for UnixDatagramTransport {
    type Listener = UnixDatagramListener;
    type Connection = UnixDatagramConnection;
    type Config = UnixDatagramTransportConfig;
    
    fn new(config: Self::Config) -> Self {
        Self { config }
    }
    
    async fn create_listener(&self) -> io::Result<Self::Listener> {
        // Remove existing socket file if it exists
        let _ = std::fs::remove_file(&self.config.socket_path);
        
        let socket = UnixDatagram::bind(&self.config.socket_path)?;
        Ok(UnixDatagramListener { socket })
    }
    
    fn config(&self) -> &Self::Config {
        &self.config
    }
}

#[async_trait]
impl TransportListener for UnixDatagramListener {
    type Connection = UnixDatagramConnection;
    
    async fn accept(&self) -> io::Result<(Self::Connection, ConnectionInfo)> {
        // For Unix datagram sockets, we need to receive the first message
        // to establish the "connection" (really just the peer address)
        let mut buf = vec![0; 8192];
        let (_len, peer_addr) = self.socket.recv_from(&mut buf).await?;
        
        // Create a new socket for this "connection"
        let temp_path = format!("{}.{}", 
            std::env::temp_dir().join("tasker_worker").display(),
            uuid::Uuid::new_v4()
        );
let client_socket = UnixDatagram::bind(&temp_path)?;
        client_socket.connect(&peer_addr.as_pathname().unwrap())?;
        
        let peer_addr_for_info = format!("{:?}", peer_addr);
        let connection = UnixDatagramConnection {
            socket: client_socket,
            peer_addr: Some(peer_addr.into()),
        };
        
        let info = ConnectionInfo {
            peer_address: peer_addr_for_info,
            transport_type: TransportType::UnixDatagram,
        };
        
        Ok((connection, info))
    }
}

#[async_trait]
impl TransportConnection for UnixDatagramConnection {
    type Reader = UnixDatagramReader;
    type Writer = UnixDatagramWriter;
    
    fn split(self) -> (Self::Reader, Self::Writer) {
        let socket = std::sync::Arc::new(self.socket);
        let reader = UnixDatagramReader {
            socket: socket.clone(),
            buffer: vec![0; 8192],
        };
        let writer = UnixDatagramWriter {
            socket,
            peer_addr: self.peer_addr,
        };
        (reader, writer)
    }
}

// Implement AsyncRead trait for UnixDatagramReader
impl tokio::io::AsyncRead for UnixDatagramReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        use std::task::Poll;
        
        let mut temp_buffer = vec![0; buf.remaining()];
        match self.socket.try_recv(&mut temp_buffer) {
            Ok(len) => {
                buf.put_slice(&temp_buffer[..len]);
                Poll::Ready(Ok(()))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}


#[async_trait]
impl tokio::io::AsyncWrite for UnixDatagramWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        match self.socket.try_send(buf) {
            Ok(len) => std::task::Poll::Ready(Ok(len)),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::task::Poll::Pending
            }
            Err(e) => std::task::Poll::Ready(Err(e)),
        }
    }
    
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        // Unix datagram sockets don't need explicit flushing
        std::task::Poll::Ready(Ok(()))
    }
    
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
