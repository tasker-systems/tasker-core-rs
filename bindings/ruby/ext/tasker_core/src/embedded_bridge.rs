//! # Embedded FFI Bridge for Testing
//!
//! Minimal FFI interface for embedded mode testing. Provides lifecycle management
//! for the orchestration system without complex state sharing.
//!
//! Design Principles:
//! - Lightweight: Only lifecycle management (start/stop/status)
//! - No complex state sharing between Ruby and Rust
//! - Same pgmq-based architecture, just running in-process
//! - Ruby tests can start embedded orchestrator, run tests, stop orchestrator

use tasker_core::ffi::shared::orchestration_system::OrchestrationSystem;
use magnus::{function, prelude::*, Error, RHash, RModule, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tracing::{error, info, warn};

/// Global handle to the embedded orchestration system
static EMBEDDED_SYSTEM: Mutex<Option<EmbeddedOrchestrationHandle>> = Mutex::new(None);

/// Handle for managing embedded orchestration system lifecycle
struct EmbeddedOrchestrationHandle {
    system: Arc<OrchestrationSystem>,
    shutdown_sender: Option<oneshot::Sender<()>>,
    runtime_handle: tokio::runtime::Handle,
}

impl EmbeddedOrchestrationHandle {
    /// Create new embedded handle
    fn new(
        system: Arc<OrchestrationSystem>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            system,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
        }
    }

    /// Check if system is running
    fn is_running(&self) -> bool {
        self.shutdown_sender.is_some()
    }

    /// Stop the embedded system
    fn stop(&mut self) -> Result<(), String> {
        if let Some(sender) = self.shutdown_sender.take() {
            sender.send(()).map_err(|_| "Failed to send shutdown signal")?;
            info!("🛑 Embedded orchestration system shutdown requested");
            Ok(())
        } else {
            warn!("Embedded orchestration system already stopped");
            Ok(())
        }
    }

    /// Get system status information
    fn status(&self) -> EmbeddedSystemStatus {
        EmbeddedSystemStatus {
            running: self.is_running(),
            database_pool_size: self.system.database_pool().size(),
            database_pool_idle: self.system.database_pool().num_idle(),
        }
    }
}

/// System status information
#[derive(Debug)]
struct EmbeddedSystemStatus {
    running: bool,
    database_pool_size: u32,
    database_pool_idle: usize,
}

/// Start the embedded orchestration system for testing
///
/// This starts the orchestration system in a background thread using the same
/// pgmq architecture, but running in the same process as Ruby for testing.
///
/// # Arguments
/// * `namespaces` - Array of namespace strings to initialize queues for
///
/// # Returns
/// * Success message or error string
fn start_embedded_orchestration(namespaces: Vec<String>) -> Result<String, Error> {
    let mut handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(magnus::exception::runtime_error(), "Lock acquisition failed")
    })?;

    if handle_guard.is_some() {
        return Ok("Embedded orchestration system already running".to_string());
    }

    info!("🚀 Starting embedded orchestration system for testing");

    // Create tokio runtime for orchestration system
    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        error!("Failed to create tokio runtime: {}", e);
        Error::new(magnus::exception::runtime_error(), "Runtime creation failed")
    })?;

    let runtime_handle = rt.handle().clone();

    // Initialize orchestration system
    let system = rt.block_on(async {
        OrchestrationSystem::new().await.map_err(|e| {
            error!("Failed to initialize orchestration system: {}", e);
            format!("System initialization failed: {}", e)
        })
    }).map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Initialize namespace queues
    let namespaces_refs: Vec<&str> = namespaces.iter().map(|s| s.as_str()).collect();
    rt.block_on(async {
        system.initialize_queues(&namespaces_refs).await.map_err(|e| {
            error!("Failed to initialize queues: {}", e);
            format!("Queue initialization failed: {}", e)
        })
    }).map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Create shutdown channel
    let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

    // Spawn background task to keep runtime alive
    let system_clone = system.clone();
    std::thread::spawn(move || {
        rt.block_on(async {
            info!("✅ Embedded orchestration system started");

            // Wait for shutdown signal
            let shutdown_receiver = shutdown_receiver;
            let _ = shutdown_receiver.await;

            info!("🛑 Embedded orchestration system shutting down");
            // Graceful shutdown - just let the runtime drop
        });

        info!("✅ Embedded orchestration system stopped");
    });

    // Store handle
    *handle_guard = Some(EmbeddedOrchestrationHandle::new(
        system,
        shutdown_sender,
        runtime_handle,
    ));

    Ok("Embedded orchestration system started successfully".to_string())
}

/// Stop the embedded orchestration system
fn stop_embedded_orchestration() -> Result<String, Error> {
    let mut handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(magnus::exception::runtime_error(), "Lock acquisition failed")
    })?;

    match handle_guard.as_mut() {
        Some(handle) => {
            handle.stop().map_err(|e| {
                error!("Failed to stop embedded system: {}", e);
                Error::new(magnus::exception::runtime_error(), e)
            })?;
            *handle_guard = None;
            Ok("Embedded orchestration system stopped".to_string())
        }
        None => Ok("Embedded orchestration system not running".to_string()),
    }
}

/// Get status of the embedded orchestration system
fn get_embedded_orchestration_status() -> Result<Value, Error> {
    let handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(magnus::exception::runtime_error(), "Lock acquisition failed")
    })?;

    let status = match handle_guard.as_ref() {
        Some(handle) => handle.status(),
        None => EmbeddedSystemStatus {
            running: false,
            database_pool_size: 0,
            database_pool_idle: 0,
        },
    };

    // Convert to Ruby hash
    let hash = RHash::new();
    hash.aset("running", status.running)?;
    hash.aset("database_pool_size", status.database_pool_size)?;
    hash.aset("database_pool_idle", status.database_pool_idle)?;

    Ok(hash.as_value())
}

/// Enqueue ready steps for a task (testing helper)
///
/// This provides a simple way for Ruby tests to trigger step enqueueing
/// without complex orchestration setup.
fn enqueue_task_steps(task_id: i64) -> Result<String, Error> {
    let handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(magnus::exception::runtime_error(), "Lock acquisition failed")
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        Error::new(magnus::exception::runtime_error(), "Embedded system not running")
    })?;

    // Execute step enqueueing on the runtime
    let system = handle.system.clone();
    let runtime_handle = handle.runtime_handle.clone();

    let result = runtime_handle.block_on(async {
        system.enqueue_ready_steps(task_id).await.map_err(|e| {
            error!("Failed to enqueue steps for task {}: {}", task_id, e);
            format!("Step enqueueing failed: {}", e)
        })
    }).map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    Ok(format!("Enqueued steps for task {}: {:?}", task_id, result))
}

/// Initialize embedded FFI bridge
pub fn init_embedded_bridge(tasker_core_module: &RModule) -> Result<(), Error> {
    info!("🔌 Initializing embedded FFI bridge");

    // Define module functions for lifecycle management
    tasker_core_module.define_singleton_method("start_embedded_orchestration", function!(start_embedded_orchestration, 1))?;
    tasker_core_module.define_singleton_method("stop_embedded_orchestration", function!(stop_embedded_orchestration, 0))?;
    tasker_core_module.define_singleton_method("embedded_orchestration_status", function!(get_embedded_orchestration_status, 0))?;
    tasker_core_module.define_singleton_method("enqueue_task_steps", function!(enqueue_task_steps, 1))?;

    info!("✅ Embedded FFI bridge initialized");
    Ok(())
}

/// Cleanup embedded system on process exit
pub fn cleanup_embedded_system() {
    if let Ok(mut handle_guard) = EMBEDDED_SYSTEM.lock() {
        if let Some(mut handle) = handle_guard.take() {
            let _ = handle.stop();
            info!("🧹 Embedded orchestration system cleaned up");
        }
    }
}
