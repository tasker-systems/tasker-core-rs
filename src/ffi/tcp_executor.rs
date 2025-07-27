//! FFI Interface for Embedded TCP Executor Control
//!
//! Provides language bindings the ability to start, stop, and control the TCP executor
//! from within their own process space, enabling:
//! 
//! 1. Single-process deployments (language runtime + Rust executor)
//! 2. Integration testing with programmatic server control
//! 3. Docker containers with single service management
//! 4. Development environments with embedded servers

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Runtime;
use tracing::{error, info, warn};

use crate::execution::tokio_tcp_executor::TokioTcpExecutor;

// Re-export for FFI usage
pub use crate::execution::tokio_tcp_executor::TcpExecutorConfig;
use crate::execution::command_handlers::WorkerManagementHandler;
use crate::execution::command::CommandType;

/// Embedded TCP executor manager for FFI control
pub struct EmbeddedTcpExecutor {
    /// Async runtime for the TCP server
    runtime: Arc<Runtime>,
    /// TCP executor instance (when running)
    executor: Arc<Mutex<Option<TokioTcpExecutor>>>,
    /// Server configuration
    config: TcpExecutorConfig,
    /// Running state
    running: Arc<Mutex<bool>>,
}

/// Server status information
#[derive(Debug, Clone)]
pub struct ServerStatus {
    pub running: bool,
    pub bind_address: String,
    pub total_connections: u64,
    pub active_connections: usize,
    pub uptime_seconds: u64,
    pub commands_processed: usize,
}

/// Result type for server operations
pub type ServerResult<T> = Result<T, ServerError>;

/// Server operation errors
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Server is already running")]
    AlreadyRunning,
    
    #[error("Server is not running")]
    NotRunning,
    
    #[error("Failed to start server: {0}")]
    StartupError(String),
    
    #[error("Failed to stop server: {0}")]
    ShutdownError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    #[error("Runtime error: {0}")]
    RuntimeError(String),
}

impl EmbeddedTcpExecutor {
    /// Create a new embedded TCP executor with default configuration
    pub fn new() -> ServerResult<Self> {
        Self::with_config(TcpExecutorConfig::default())
    }
    
    /// Create a new embedded TCP executor with custom configuration
    pub fn with_config(config: TcpExecutorConfig) -> ServerResult<Self> {
        let runtime = Arc::new(
            Runtime::new()
                .map_err(|e| ServerError::RuntimeError(format!("Failed to create runtime: {}", e)))?
        );
        
        Ok(Self {
            runtime,
            executor: Arc::new(Mutex::new(None)),
            config,
            running: Arc::new(Mutex::new(false)),
        })
    }
    
    /// Start the TCP executor server
    pub fn start(&self) -> ServerResult<()> {
        let mut running = self.running.lock()
            .map_err(|e| ServerError::RuntimeError(format!("Lock error: {}", e)))?;
            
        if *running {
            return Err(ServerError::AlreadyRunning);
        }
        
        info!("Starting embedded TCP executor on {}", self.config.bind_address);
        
        // Create executor in the async runtime
        let config = self.config.clone();
        let executor_result = self.runtime.block_on(async {
            TokioTcpExecutor::new(config).await
        });
        
        let executor = executor_result
            .map_err(|e| ServerError::StartupError(format!("Failed to create executor: {}", e)))?;
        
        // Set up command handlers
        let worker_pool = executor.worker_pool();
        let router = executor.command_router();
        let worker_handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));
        
        // Register command handlers in the async runtime
        self.runtime.block_on(async {
            router.register_handler(CommandType::RegisterWorker, worker_handler.clone()).await?;
            router.register_handler(CommandType::UnregisterWorker, worker_handler.clone()).await?;
            router.register_handler(CommandType::WorkerHeartbeat, worker_handler.clone()).await?;
            router.register_handler(CommandType::HealthCheck, worker_handler).await?;
            
            // Start the executor
            executor.start().await
        }).map_err(|e| ServerError::StartupError(format!("Failed to start executor: {}", e)))?;
        
        // Store executor and mark as running
        let mut executor_guard = self.executor.lock()
            .map_err(|e| ServerError::RuntimeError(format!("Lock error: {}", e)))?;
        *executor_guard = Some(executor);
        *running = true;
        
        info!("Embedded TCP executor started successfully on {}", self.config.bind_address);
        Ok(())
    }
    
    /// Stop the TCP executor server
    pub fn stop(&self) -> ServerResult<()> {
        let mut running = self.running.lock()
            .map_err(|e| ServerError::RuntimeError(format!("Lock error: {}", e)))?;
            
        if !*running {
            return Err(ServerError::NotRunning);
        }
        
        info!("Stopping embedded TCP executor");
        
        // Get executor and stop it
        let mut executor_guard = self.executor.lock()
            .map_err(|e| ServerError::RuntimeError(format!("Lock error: {}", e)))?;
            
        if let Some(executor) = executor_guard.take() {
            self.runtime.block_on(async {
                executor.stop().await
            }).map_err(|e| ServerError::ShutdownError(format!("Failed to stop executor: {}", e)))?;
        }
        
        *running = false;
        info!("Embedded TCP executor stopped successfully");
        Ok(())
    }
    
    /// Check if the server is running
    pub fn is_running(&self) -> bool {
        self.running.lock()
            .map(|running| *running)
            .unwrap_or(false)
    }
    
    /// Get server status information
    pub fn get_status(&self) -> ServerResult<ServerStatus> {
        let running = self.is_running();
        
        if !running {
            return Ok(ServerStatus {
                running: false,
                bind_address: self.config.bind_address.clone(),
                total_connections: 0,
                active_connections: 0,
                uptime_seconds: 0,
                commands_processed: 0,
            });
        }
        
        let executor_guard = self.executor.lock()
            .map_err(|e| ServerError::RuntimeError(format!("Lock error: {}", e)))?;
            
        if let Some(executor) = executor_guard.as_ref() {
            let stats = self.runtime.block_on(async {
                executor.get_stats().await
            });
            
            Ok(ServerStatus {
                running: true,
                bind_address: self.config.bind_address.clone(),
                total_connections: stats.total_connections,
                active_connections: stats.active_connections,
                uptime_seconds: stats.uptime_seconds,
                commands_processed: stats.commands_processed,
            })
        } else {
            Err(ServerError::RuntimeError("Executor not found".to_string()))
        }
    }
    
    /// Wait for the server to start accepting connections
    pub fn wait_for_ready(&self, timeout_seconds: u64) -> ServerResult<bool> {
        if !self.is_running() {
            return Err(ServerError::NotRunning);
        }
        
        let timeout = Duration::from_secs(timeout_seconds);
        let start = std::time::Instant::now();
        
        while start.elapsed() < timeout {
            let status = self.get_status()?;
            if status.running {
                return Ok(true);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        
        Ok(false)
    }
    
    /// Get the bind address
    pub fn bind_address(&self) -> &str {
        &self.config.bind_address
    }
    
    /// Update configuration (only when stopped)
    pub fn update_config(&mut self, config: TcpExecutorConfig) -> ServerResult<()> {
        if self.is_running() {
            return Err(ServerError::RuntimeError("Cannot update config while running".to_string()));
        }
        
        self.config = config;
        Ok(())
    }
}


impl Drop for EmbeddedTcpExecutor {
    fn drop(&mut self) {
        if self.is_running() {
            if let Err(e) = self.stop() {
                warn!("Failed to stop TCP executor during drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_embedded_executor_lifecycle() {
        let mut executor = EmbeddedTcpExecutor::new().unwrap();
        
        // Initially not running
        assert!(!executor.is_running());
        
        // Start server
        executor.start().unwrap();
        assert!(executor.is_running());
        
        // Wait for ready
        assert!(executor.wait_for_ready(5).unwrap());
        
        // Get status
        let status = executor.get_status().unwrap();
        assert!(status.running);
        assert_eq!(status.bind_address, "127.0.0.1:8080");
        
        // Stop server
        executor.stop().unwrap();
        assert!(!executor.is_running());
    }
    
    #[test]
    fn test_custom_config() {
        let config = TcpExecutorConfig {
            bind_address: "127.0.0.1:0".to_string(), // OS-assigned port
            command_queue_size: 500,
            connection_timeout_ms: 15000,
            graceful_shutdown_timeout_ms: 3000,
            max_connections: 50,
        };
        
        let executor = EmbeddedTcpExecutor::with_config(config).unwrap();
        assert_eq!(executor.bind_address(), "127.0.0.1:0");
    }
    
    #[test]
    fn test_double_start_error() {
        let executor = EmbeddedTcpExecutor::new().unwrap();
        
        executor.start().unwrap();
        
        // Second start should fail
        let result = executor.start();
        assert!(matches!(result, Err(ServerError::AlreadyRunning)));
        
        executor.stop().unwrap();
    }
    
    #[test]
    fn test_stop_when_not_running() {
        let executor = EmbeddedTcpExecutor::new().unwrap();
        
        // Stop when not running should fail
        let result = executor.stop();
        assert!(matches!(result, Err(ServerError::NotRunning)));
    }
}