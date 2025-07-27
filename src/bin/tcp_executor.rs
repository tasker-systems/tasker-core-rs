//! TCP Executor Binary
//! 
//! Standalone binary for running the Rust Command TCP Executor server.
//! This replaces ZeroMQ with native TCP communication for Ruby-Rust integration.

use std::sync::Arc;
use tokio::signal;
use tracing::info;

use tasker_core::execution::tokio_tcp_executor::{TokioTcpExecutor, TcpExecutorConfig};
use tasker_core::execution::command_handlers::WorkerManagementHandler;
use tasker_core::execution::command::CommandType;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info,tasker_core=debug")
        .init();

    info!("Starting Rust TCP Command Executor");

    // Create TCP executor configuration
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:8080".to_string(),
        command_queue_size: 1000,
        connection_timeout_ms: 30000,
        graceful_shutdown_timeout_ms: 5000,
        max_connections: 100,
    };

    // Create executor
    let executor = TokioTcpExecutor::new(config).await?;
    
    // Set up command handlers
    let worker_pool = executor.worker_pool();
    let router = executor.command_router();
    let worker_handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    // Register command handlers
    router.register_handler(CommandType::RegisterWorker, worker_handler.clone()).await?;
    router.register_handler(CommandType::UnregisterWorker, worker_handler.clone()).await?;
    router.register_handler(CommandType::WorkerHeartbeat, worker_handler.clone()).await?;
    router.register_handler(CommandType::HealthCheck, worker_handler).await?;

    info!("Command handlers registered");

    // Start the executor
    executor.start().await?;
    info!("TCP Executor started on 127.0.0.1:8080");
    info!("Ready to accept Ruby worker connections!");

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    info!("Shutdown signal received");

    // Stop the executor
    executor.stop().await?;
    info!("TCP Executor stopped");

    Ok(())
}