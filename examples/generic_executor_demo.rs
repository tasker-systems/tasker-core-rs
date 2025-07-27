//! Generic Executor Demo
//!
//! This example demonstrates how to use the new generic transport-based executor
//! that can work with both TCP and Unix datagram sockets transparently.

use std::time::Duration;
use tokio::time::sleep;
use tasker_core::execution::{
    GenericExecutor, TcpTransport, TcpTransportConfig, 
    UnixDatagramTransport, UnixDatagramTransportConfig,
    TransportType
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::init();
    
    println!("=== Generic Executor Demo ===");
    
    // Demo 1: TCP Transport
    println!("\n1. Creating TCP executor...");
    let tcp_config = TcpTransportConfig {
        bind_address: "127.0.0.1:0".to_string(), // Use OS-assigned port
        connection_timeout_ms: 30000,
        command_queue_size: 1000,
        max_connections: 100,
        graceful_shutdown_timeout_ms: 5000,
    };
    let tcp_transport = TcpTransport::new(tcp_config);
    let tcp_executor = GenericExecutor::new(tcp_transport).await?;
    
    println!("TCP executor created!");
    println!("Transport type: {:?}", TransportType::Tcp);
    
    // Start TCP executor
    tcp_executor.start().await?;
    println!("TCP executor started and listening");
    
    // Get stats
    let stats = tcp_executor.get_stats().await;
    println!("TCP executor stats: running={}, bind_address={}", 
             stats.running, stats.bind_address);
    
    // Stop TCP executor
    tcp_executor.stop().await?;
    println!("TCP executor stopped");
    
    // Demo 2: Unix Datagram Transport
    println!("\n2. Creating Unix datagram executor...");
    let unix_config = UnixDatagramTransportConfig {
        socket_path: "/tmp/tasker_demo.sock".to_string(),
        connection_timeout_ms: 30000,
        command_queue_size: 1000,
        max_connections: 100,
        graceful_shutdown_timeout_ms: 5000,
    };
    let unix_transport = UnixDatagramTransport::new(unix_config);
    let unix_executor = GenericExecutor::new(unix_transport).await?;
    
    println!("Unix datagram executor created!");
    println!("Transport type: {:?}", TransportType::UnixDatagram);
    
    // Start Unix executor
    unix_executor.start().await?;
    println!("Unix datagram executor started and listening");
    
    // Get stats
    let stats = unix_executor.get_stats().await;
    println!("Unix executor stats: running={}, bind_address={}", 
             stats.running, stats.bind_address);
    
    sleep(Duration::from_millis(500)).await;
    
    // Stop Unix executor
    unix_executor.stop().await?;
    println!("Unix datagram executor stopped");
    
    // Demo 3: Using type aliases for convenience
    println!("\n3. Using type aliases...");
    
    // Using TcpExecutor type alias
    let tcp_config2 = TcpTransportConfig {
        bind_address: "127.0.0.1:0".to_string(),
        ..TcpTransportConfig::default()
    };
    let tcp_transport2 = TcpTransport::new(tcp_config2);
    let tcp_executor2: tasker_core::execution::TcpExecutor = GenericExecutor::new(tcp_transport2).await?;
    println!("Created TcpExecutor using type alias");
    
    // Using UnixDatagramExecutor type alias  
    let unix_config2 = UnixDatagramTransportConfig {
        socket_path: "/tmp/tasker_demo2.sock".to_string(),
        ..UnixDatagramTransportConfig::default()
    };
    let unix_transport2 = UnixDatagramTransport::new(unix_config2);
    let unix_executor2: tasker_core::execution::UnixDatagramExecutor = GenericExecutor::new(unix_transport2).await?;
    println!("Created UnixDatagramExecutor using type alias");
    
    println!("\n=== Demo completed successfully! ===");
    println!("\nKey benefits of the generic approach:");
    println!("- Single executor implementation works with any transport");
    println!("- Easy to add new transport types (WebSocket, etc.)");
    println!("- Transport details are abstracted from command/execution logic");
    println!("- Type-safe with zero runtime overhead");
    
    Ok(())
}

// Example function showing how the generic approach enables transport-agnostic code
async fn start_executor_generic<T>(executor: &GenericExecutor<T>) -> Result<(), Box<dyn std::error::Error>>
where
    T: tasker_core::execution::Transport + 'static,
    T::Listener: 'static,
{
    executor.start().await?;
    println!("Started executor on: {}", executor.transport().config().bind_address());
    Ok(())
}
