//! TCP Executor Binary
//!
//! Standalone binary for running the Rust Command TCP Executor server.
//! This replaces ZeroMQ with native TCP communication for Ruby-Rust integration.

use std::sync::Arc;
use tokio::signal;
use tracing::info;

use tasker_core::execution::command::CommandType;
use tasker_core::execution::command_handlers::{
    ResultAggregationHandler, ResultHandlerConfig, WorkerManagementHandler,
};
use tasker_core::execution::generic_executor::TcpExecutor;
use tasker_core::execution::transport::{TcpTransport, TcpTransportConfig, Transport};
use tasker_core::ffi::shared::orchestration_system::initialize_unified_orchestration_system;
use tasker_core::orchestration::{task_finalizer::TaskFinalizer, OrchestrationResultProcessor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info,tasker_core=debug")
        .init();

    info!("Starting Rust TCP Command Executor");

    // Create TCP transport configuration
    let transport_config = TcpTransportConfig {
        bind_address: "127.0.0.1:8080".to_string(),
        command_queue_size: 1000,
        connection_timeout_ms: 30000,
        graceful_shutdown_timeout_ms: 5000,
        max_connections: 100,
    };

    // Create transport and executor
    let transport = TcpTransport::new(transport_config);
    let executor = TcpExecutor::new(transport).await?;

    // Set up command handlers
    let worker_pool = executor.worker_pool();
    let router = executor.command_router();

    // Initialize unified orchestration system for orchestration components
    let orchestration_system = initialize_unified_orchestration_system();

    // Create result processor from orchestration system components
    let result_processor = Arc::new(OrchestrationResultProcessor::new(
        orchestration_system.state_manager.clone(),
        TaskFinalizer::new(orchestration_system.database_pool.clone()),
        orchestration_system.database_pool.clone(),
    ));

    // Create handlers
    let worker_handler = Arc::new(WorkerManagementHandler::new(
        worker_pool.clone(),
        orchestration_system.database_pool.clone(),
        "tcp_executor_bin".to_string(),
    ));
    let result_handler = Arc::new(ResultAggregationHandler::new(
        worker_pool.clone(),
        result_processor,
        ResultHandlerConfig::default(),
    ));

    // Register command handlers
    router
        .register_handler(CommandType::RegisterWorker, worker_handler.clone())
        .await?;
    router
        .register_handler(CommandType::UnregisterWorker, worker_handler.clone())
        .await?;
    router
        .register_handler(CommandType::WorkerHeartbeat, worker_handler.clone())
        .await?;
    router
        .register_handler(CommandType::HealthCheck, worker_handler)
        .await?;

    // Register result aggregation handlers
    router
        .register_handler(CommandType::ReportPartialResult, result_handler.clone())
        .await?;
    router
        .register_handler(CommandType::ReportBatchCompletion, result_handler)
        .await?;

    info!(
        "All command handlers registered (worker management, batch execution, result aggregation)"
    );

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
