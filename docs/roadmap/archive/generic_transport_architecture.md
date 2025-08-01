# Generic Transport Architecture

This document explains the new generic transport-based executor architecture that abstracts over different socket types (TCP, Unix datagram) so the executor and command system don't need to know or care which transport is being used.

## Overview

The new architecture introduces a trait-based abstraction layer that allows the execution system to work with any transport implementation without being tightly coupled to specific socket types.

## Key Components

### 1. Transport Traits

#### `Transport`
The main trait that defines a transport implementation:
```rust
pub trait Transport: Send + Sync + Debug + Clone {
    type Listener: TransportListener<Connection = Self::Connection> + Send + Sync;
    type Connection: TransportConnection + Send + Sync;
    type Config: TransportConfig + Send + Sync + Clone;
    
    fn new(config: Self::Config) -> Self;
    async fn create_listener(&self) -> io::Result<Self::Listener>;
    fn config(&self) -> &Self::Config;
}
```

#### `TransportListener`
Defines how to accept connections:
```rust
pub trait TransportListener: Send + Sync {
    type Connection: TransportConnection + Send + Sync;
    async fn accept(&self) -> io::Result<(Self::Connection, ConnectionInfo)>;
}
```

#### `TransportConnection`
Defines how to split connections into readers and writers:
```rust
pub trait TransportConnection: Send + Sync {
    type Reader: AsyncBufReadExt + Send + Unpin;
    type Writer: AsyncWriteExt + Send + Unpin;
    fn split(self) -> (Self::Reader, Self::Writer);
}
```

#### `TransportConfig`
Provides configuration methods that all transports must implement:
```rust
pub trait TransportConfig: Send + Sync + Clone + Debug {
    fn bind_address(&self) -> &str;
    fn connection_timeout_ms(&self) -> u64;
    fn command_queue_size(&self) -> usize;
    fn max_connections(&self) -> usize;
    fn graceful_shutdown_timeout_ms(&self) -> u64;
}
```

### 2. Concrete Transport Implementations

#### TCP Transport
- `TcpTransport`: Wraps Tokio's TCP functionality
- `TcpTransportConfig`: Configuration for TCP sockets
- `TcpTransportListener`: Wraps `TcpListener`
- `TcpTransportConnection`: Wraps `TcpStream`

#### Unix Datagram Transport
- `UnixDatagramTransport`: Wraps Tokio's Unix datagram functionality
- `UnixDatagramTransportConfig`: Configuration for Unix datagram sockets
- `UnixDatagramListener`: Custom listener implementation for datagram sockets
- `UnixDatagramConnection`: Wraps `UnixDatagram`

### 3. Generic Executor

The `GenericExecutor<T: Transport>` is transport-agnostic and works with any transport implementation:

```rust
pub struct GenericExecutor<T: Transport> {
    transport: T,
    command_router: Arc<CommandRouter>,
    worker_pool: Arc<WorkerPool>,
    // ... other fields
}
```

## Usage Examples

### Basic Usage

```rust
use tasker_core::execution::{
    GenericExecutor, TcpTransport, TcpTransportConfig,
    UnixDatagramTransport, UnixDatagramTransportConfig
};

// TCP Transport
let tcp_config = TcpTransportConfig {
    bind_address: "127.0.0.1:8080".to_string(),
    connection_timeout_ms: 30000,
    command_queue_size: 1000,
    max_connections: 100,
    graceful_shutdown_timeout_ms: 5000,
};
let tcp_transport = TcpTransport::new(tcp_config);
let tcp_executor = GenericExecutor::new(tcp_transport).await?;

// Unix Datagram Transport
let unix_config = UnixDatagramTransportConfig {
    socket_path: "/tmp/tasker.sock".to_string(),
    connection_timeout_ms: 30000,
    command_queue_size: 1000,
    max_connections: 100,
    graceful_shutdown_timeout_ms: 5000,
};
let unix_transport = UnixDatagramTransport::new(unix_config);
let unix_executor = GenericExecutor::new(unix_transport).await?;
```

### Type Aliases for Convenience

```rust
use tasker_core::execution::{TcpExecutor, UnixDatagramExecutor};

// These are equivalent to GenericExecutor<TcpTransport> and GenericExecutor<UnixDatagramTransport>
let tcp_executor: TcpExecutor = GenericExecutor::new(tcp_transport).await?;
let unix_executor: UnixDatagramExecutor = GenericExecutor::new(unix_transport).await?;
```

### Transport-Agnostic Functions

```rust
async fn start_any_executor<T>(executor: &GenericExecutor<T>) -> Result<(), Box<dyn std::error::Error>>
where
    T: Transport + 'static,
    T::Listener: 'static,
{
    executor.start().await?;
    println!("Started executor on: {}", executor.transport().config().bind_address());
    Ok(())
}
```

## Architecture Benefits

### 1. **Single Implementation**
- One executor implementation works with any transport
- No need to duplicate logic for different socket types
- Easier to maintain and extend

### 2. **Type Safety**
- All transport switching happens at compile time
- Zero runtime overhead from the abstraction
- Compiler ensures all required traits are implemented

### 3. **Extensibility**
- Easy to add new transport types (WebSocket, QUIC, etc.)
- Just implement the transport traits
- No changes needed to executor or command routing logic

### 4. **Abstraction**
- Command routing and execution logic is completely decoupled from transport
- Business logic doesn't need to know about socket details
- Clean separation of concerns

### 5. **Flexibility**
- Can choose transport at runtime based on configuration
- Easy to switch between transports for testing
- Support for different deployment scenarios

## Migration Path

### From Existing Code
If you're currently using `TokioTcpExecutor`:

**Before:**
```rust
let config = TcpExecutorConfig::default();
let executor = TokioTcpExecutor::new(config).await?;
```

**After:**
```rust
let config = TcpTransportConfig::default();
let transport = TcpTransport::new(config);
let executor = GenericExecutor::new(transport).await?;
// Or using the type alias:
let executor: TcpExecutor = GenericExecutor::new(transport).await?;
```

### Adding New Transports
To add a new transport (e.g., WebSocket):

1. Create transport config struct implementing `TransportConfig`
2. Create transport struct implementing `Transport`
3. Create listener struct implementing `TransportListener`
4. Create connection struct implementing `TransportConnection`
5. Implement required AsyncRead/AsyncWrite traits for reader/writer types

## Testing

The generic approach makes testing easier:

```rust
// Mock transport for testing
struct MockTransport { /* ... */ }
impl Transport for MockTransport { /* ... */ }

// Test with mock transport
let mock_transport = MockTransport::new();
let executor = GenericExecutor::new(mock_transport).await?;
// Run tests...
```

## Future Extensions

The architecture is designed to support future transport types:

- **WebSocket Transport**: For web-based clients
- **QUIC Transport**: For high-performance, low-latency scenarios  
- **In-Memory Transport**: For testing and embedded scenarios
- **HTTP/2 Transport**: For REST-like interfaces
- **Message Queue Transport**: For integration with external message brokers

Each new transport only requires implementing the four core traits, with no changes to the executor or command processing logic.
