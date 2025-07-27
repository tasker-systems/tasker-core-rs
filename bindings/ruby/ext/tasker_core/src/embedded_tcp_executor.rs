//! Ruby FFI bindings for embedded TCP executor control using handle-based architecture

use magnus::{Error, RHash, RModule, TryConvert, function};
use tracing::info;

use tasker_core::ffi::tcp_executor::{EmbeddedTcpExecutor, ServerError};
use tasker_core::execution::executor::TcpExecutorConfig;
use tasker_core::ffi::shared::handles::SharedOrchestrationHandle;

/// Create a new embedded TCP executor with default config using handle-based architecture
fn create_embedded_executor() -> Result<bool, Error> {
    info!("🔧 Ruby FFI: Creating embedded TCP executor via handle");

    let handle = SharedOrchestrationHandle::get_global();
    let orchestration_system = handle.orchestration_system();

    // Ensure the embedded TCP executor container is initialized
    handle.ensure_embedded_tcp_executor()
        .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to initialize TCP executor container: {}", e)))?;

    // Get the executor container and create the actual executor
    if let Some(executor_arc) = orchestration_system.embedded_tcp_executor() {
        let mut executor_guard = executor_arc.lock()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Lock error: {}", e)))?;

        if executor_guard.is_some() {
            return Err(Error::new(magnus::exception::runtime_error(), "Executor already exists"));
        }

        let embedded_executor = EmbeddedTcpExecutor::new()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to create executor: {}", e)))?;

        *executor_guard = Some(embedded_executor);
        info!("✅ Ruby FFI: Created embedded TCP executor via handle");
        Ok(true)
    } else {
        Err(Error::new(magnus::exception::runtime_error(), "TCP executor container not available"))
    }
}

/// Create with custom configuration from Ruby hash using handle-based architecture
fn create_embedded_executor_with_config(config_hash: RHash) -> Result<bool, Error> {
    info!("🔧 Ruby FFI: Creating embedded TCP executor with custom config via handle");

    let config = parse_config_from_hash(config_hash)?;

    let handle = SharedOrchestrationHandle::get_global();
    let orchestration_system = handle.orchestration_system();

    // Ensure the embedded TCP executor container is initialized
    handle.ensure_embedded_tcp_executor()
        .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to initialize TCP executor container: {}", e)))?;

    // Get the executor container and create the actual executor
    if let Some(executor_arc) = orchestration_system.embedded_tcp_executor() {
        let mut executor_guard = executor_arc.lock()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Lock error: {}", e)))?;

        if executor_guard.is_some() {
            return Err(Error::new(magnus::exception::runtime_error(), "Executor already exists"));
        }

        let embedded_executor = EmbeddedTcpExecutor::with_config(config)
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to create executor: {}", e)))?;

        *executor_guard = Some(embedded_executor);
        info!("✅ Ruby FFI: Created embedded TCP executor with custom config via handle");
        Ok(true)
    } else {
        Err(Error::new(magnus::exception::runtime_error(), "TCP executor container not available"))
    }
}

/// Start the TCP executor server using handle-based architecture
fn start_embedded_executor() -> Result<bool, Error> {
    info!("🔧 Ruby FFI: Starting embedded TCP executor via handle");

    let handle = SharedOrchestrationHandle::get_global();
    let orchestration_system = handle.orchestration_system();

    // Ensure the embedded TCP executor container is initialized
    handle.ensure_embedded_tcp_executor()
        .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to initialize TCP executor container: {}", e)))?;

    // Get the executor container
    if let Some(executor_arc) = orchestration_system.embedded_tcp_executor() {
        let mut executor_guard = executor_arc.lock()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Lock error: {}", e)))?;

        if executor_guard.is_none() {
            // Create default executor if none exists
            let embedded_executor = EmbeddedTcpExecutor::new()
                .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to create executor: {}", e)))?;
            *executor_guard = Some(embedded_executor);
        }

        if let Some(executor) = executor_guard.as_ref() {
            executor.start()
                .map_err(|e| match e {
                    ServerError::AlreadyRunning => Error::new(magnus::exception::runtime_error(), "Server is already running"),
                    _ => Error::new(magnus::exception::runtime_error(), format!("Failed to start server: {}", e)),
                })?;
            info!("✅ Ruby FFI: Started embedded TCP executor via handle");
            Ok(true)
        } else {
            Err(Error::new(magnus::exception::runtime_error(), "No executor available"))
        }
    } else {
        Err(Error::new(magnus::exception::runtime_error(), "TCP executor container not available"))
    }
}

/// Stop the TCP executor server using handle-based architecture
fn stop_embedded_executor() -> Result<bool, Error> {
    info!("🔧 Ruby FFI: Stopping embedded TCP executor via handle");

    let handle = SharedOrchestrationHandle::get_global();
    let orchestration_system = handle.orchestration_system();

    if let Some(executor_arc) = orchestration_system.embedded_tcp_executor() {
        let executor_guard = executor_arc.lock()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Lock error: {}", e)))?;

        if let Some(executor) = executor_guard.as_ref() {
            executor.stop()
                .map_err(|e| match e {
                    ServerError::NotRunning => Error::new(magnus::exception::runtime_error(), "Server is not running"),
                    _ => Error::new(magnus::exception::runtime_error(), format!("Failed to stop server: {}", e)),
                })?;
            info!("✅ Ruby FFI: Stopped embedded TCP executor via handle");
            Ok(true)
        } else {
            Err(Error::new(magnus::exception::runtime_error(), "No executor available"))
        }
    } else {
        Ok(false) // No executor container means nothing to stop
    }
}

/// Check if the server is running using handle-based architecture
fn embedded_executor_running() -> Result<bool, Error> {
    let handle = SharedOrchestrationHandle::get_global();
    let orchestration_system = handle.orchestration_system();

    if let Some(executor_arc) = orchestration_system.embedded_tcp_executor() {
        let executor_guard = executor_arc.lock()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Lock error: {}", e)))?;

        if let Some(executor) = executor_guard.as_ref() {
            Ok(executor.is_running())
        } else {
            Ok(false)
        }
    } else {
        Ok(false)
    }
}

/// Get server status as Ruby hash using handle-based architecture
fn embedded_executor_status() -> Result<RHash, Error> {
    let handle = SharedOrchestrationHandle::get_global();
    let orchestration_system = handle.orchestration_system();

    if let Some(executor_arc) = orchestration_system.embedded_tcp_executor() {
        let executor_guard = executor_arc.lock()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Lock error: {}", e)))?;

        if let Some(executor) = executor_guard.as_ref() {
            let status = executor.get_status()
                .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to get status: {}", e)))?;

            let hash = RHash::new();
            hash.aset("running", status.running)?;
            hash.aset("bind_address", status.bind_address)?;
            hash.aset("total_connections", status.total_connections)?;
            hash.aset("active_connections", status.active_connections)?;
            hash.aset("uptime_seconds", status.uptime_seconds)?;
            hash.aset("commands_processed", status.commands_processed)?;

            Ok(hash)
        } else {
            // Return default status if no executor
            let hash = RHash::new();
            hash.aset("running", false)?;
            hash.aset("bind_address", "unknown")?;
            hash.aset("total_connections", 0)?;
            hash.aset("active_connections", 0)?;
            hash.aset("uptime_seconds", 0)?;
            hash.aset("commands_processed", 0)?;
            Ok(hash)
        }
    } else {
        // Return default status if no executor container
        let hash = RHash::new();
        hash.aset("running", false)?;
        hash.aset("bind_address", "unknown")?;
        hash.aset("total_connections", 0)?;
        hash.aset("active_connections", 0)?;
        hash.aset("uptime_seconds", 0)?;
        hash.aset("commands_processed", 0)?;
        Ok(hash)
    }
}

/// Wait for server to be ready using handle-based architecture
fn embedded_executor_wait_for_ready(timeout_seconds: u64) -> Result<bool, Error> {
    let handle = SharedOrchestrationHandle::get_global();
    let orchestration_system = handle.orchestration_system();

    if let Some(executor_arc) = orchestration_system.embedded_tcp_executor() {
        let executor_guard = executor_arc.lock()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Lock error: {}", e)))?;

        if let Some(executor) = executor_guard.as_ref() {
            executor.wait_for_ready(timeout_seconds)
                .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Wait failed: {}", e)))
        } else {
            Ok(false)
        }
    } else {
        Ok(false)
    }
}

/// Get bind address using handle-based architecture
fn embedded_executor_bind_address() -> Result<String, Error> {
    let handle = SharedOrchestrationHandle::get_global();
    let orchestration_system = handle.orchestration_system();

    if let Some(executor_arc) = orchestration_system.embedded_tcp_executor() {
        let executor_guard = executor_arc.lock()
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Lock error: {}", e)))?;

        if let Some(executor) = executor_guard.as_ref() {
            Ok(executor.bind_address().to_string())
        } else {
            Ok("unknown".to_string())
        }
    } else {
        Ok("unknown".to_string())
    }
}

/// Parse configuration from Ruby hash
fn parse_config_from_hash(hash: RHash) -> Result<TcpExecutorConfig, Error> {
    let mut config = TcpExecutorConfig::default();

    if let Ok(addr) = hash.lookup::<&str, magnus::Value>("bind_address") {
        if let Ok(addr_str) = String::try_convert(addr) {
            config.bind_address = addr_str;
        }
    }

    if let Ok(queue_size) = hash.lookup::<&str, magnus::Value>("command_queue_size") {
        if let Ok(size) = usize::try_convert(queue_size) {
            config.command_queue_size = size;
        }
    }

    if let Ok(timeout) = hash.lookup::<&str, magnus::Value>("connection_timeout_ms") {
        if let Ok(timeout_ms) = u64::try_convert(timeout) {
            config.connection_timeout_ms = timeout_ms;
        }
    }

    if let Ok(shutdown_timeout) = hash.lookup::<&str, magnus::Value>("graceful_shutdown_timeout_ms") {
        if let Ok(timeout_ms) = u64::try_convert(shutdown_timeout) {
            config.graceful_shutdown_timeout_ms = timeout_ms;
        }
    }

    if let Ok(max_conn) = hash.lookup::<&str, magnus::Value>("max_connections") {
        if let Ok(max) = usize::try_convert(max_conn) {
            config.max_connections = max;
        }
    }

    Ok(config)
}

/// Initialize Ruby bindings for embedded TCP executor
pub fn init_embedded_tcp_executor(module: &RModule) -> Result<(), Error> {
    // Register module-level functions following the handle-based pattern used in this codebase
    module.define_module_function("create_embedded_executor", function!(create_embedded_executor, 0))?;
    module.define_module_function("create_embedded_executor_with_config", function!(create_embedded_executor_with_config, 1))?;
    module.define_module_function("start_embedded_executor", function!(start_embedded_executor, 0))?;
    module.define_module_function("stop_embedded_executor", function!(stop_embedded_executor, 0))?;
    module.define_module_function("embedded_executor_running?", function!(embedded_executor_running, 0))?;
    module.define_module_function("embedded_executor_status", function!(embedded_executor_status, 0))?;
    module.define_module_function("embedded_executor_wait_for_ready", function!(embedded_executor_wait_for_ready, 1))?;
    module.define_module_function("embedded_executor_bind_address", function!(embedded_executor_bind_address, 0))?;

    info!("✅ Ruby FFI: Initialized embedded TCP executor bindings using handle-based architecture");
    Ok(())
}
