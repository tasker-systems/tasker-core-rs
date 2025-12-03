//! # Ruby Worker Bootstrap
//!
//! Follows the same patterns as workers/rust/src/bootstrap.rs but adapted
//! for Ruby FFI integration with magnus.

use crate::{
    bridge::{RubyBridgeHandle, WORKER_SYSTEM},
    event_handler::RubyEventHandler,
    global_event_system::get_global_event_system,
};
use magnus::{value::ReprValue, Error, Value};
use std::sync::Arc;
use tasker_worker::worker::{InProcessEventBus, InProcessEventBusConfig};
use tasker_worker::WorkerBootstrap;
use tracing::{error, info};
use uuid::Uuid;

/// Bootstrap the worker system for Ruby
///
/// This follows the same pattern as the Rust worker bootstrap but stores
/// the handle in a global static for Ruby access.
///
/// Returns a handle ID that Ruby can use to reference the worker system.
pub fn bootstrap_worker() -> Result<Value, Error> {
    let worker_id = Uuid::new_v4();
    let worker_id_str = format!("ruby-worker-{}", worker_id);

    // Check if already running
    let mut handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    if handle_guard.is_some() {
        // Return existing handle info
        let ruby = magnus::Ruby::get().map_err(|err| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to get ruby system: {}", err),
            )
        })?;
        let hash = ruby.hash_new();
        hash.aset("handle_id", worker_id.to_string())?;
        hash.aset("status", "already_running")?;
        hash.aset("message", "Worker system already running")?;
        return Ok(hash.as_value());
    }

    // Create tokio runtime
    let runtime = tokio::runtime::Runtime::new().map_err(|e| {
        error!("Failed to create tokio runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    // TAS-65 Phase 2: Initialize telemetry in Tokio runtime context
    // This is phase 2 of the two-phase FFI telemetry initialization pattern.
    // If TELEMETRY_ENABLED=true, this will initialize OpenTelemetry with batch exporter.
    // If TELEMETRY_ENABLED=false, this is a no-op (console logging already initialized).
    runtime.block_on(async {
        tasker_shared::logging::init_tracing();
    });

    // TAS-50 Phase 3: Load worker-specific configuration from TASKER_CONFIG_PATH (single-file)
    // This respects TASKER_CONFIG_PATH environment variable for runtime overrides
    let system_context = runtime.block_on(async {
        tasker_shared::system_context::SystemContext::new_for_worker()
            .await
            .map_err(|e| {
                error!("Failed to load worker configuration: {}", e);
                Error::new(
                    magnus::exception::runtime_error(),
                    format!("Configuration load failed: {}", e),
                )
            })
    })?;

    let config = system_context.tasker_config.as_ref().clone();

    // Get global event system (shared singleton)
    let event_system = get_global_event_system();

    // TAS-61 Phase 6C/6D: Access mpsc_channels from common config
    // Create Ruby event handler with bounded channel (TAS-51)
    let buffer_size = config.common.mpsc_channels.ffi.ruby_event_buffer_size;
    let (ruby_event_handler, event_receiver) = RubyEventHandler::new(
        event_system.clone(),
        worker_id_str.clone(),
        buffer_size as usize,
    );
    let ruby_event_handler = Arc::new(ruby_event_handler);

    // Bootstrap within runtime context
    let (system_handle, event_handler) = runtime.block_on(async {
        // Start the Ruby event handler (subscribes to events)
        ruby_event_handler.start().await.map_err(|e| {
            error!("Failed to start Ruby event handler: {}", e);
            Error::new(
                magnus::exception::runtime_error(),
                format!("Event handler start failed: {}", e),
            )
        })?;

        info!("✅ Ruby event handler started and subscribed to events");

        // Bootstrap the worker using tasker-worker foundation
        let handle = WorkerBootstrap::bootstrap_with_event_system(Some(event_system))
            .await
            .map_err(|e| {
                error!("Failed to bootstrap worker system: {}", e);
                Error::new(
                    magnus::exception::runtime_error(),
                    format!("Worker bootstrap failed: {}", e),
                )
            })?;

        info!("✅ Worker system bootstrapped successfully");

        Ok::<_, Error>((handle, ruby_event_handler))
    })?;

    // TAS-65: Create domain event publisher from system context message client
    // Access message_client through async context to create publisher once
    let domain_event_publisher = runtime.block_on(async {
        let worker_core = system_handle.worker_core.lock().await;
        let message_client = worker_core.context.message_client.clone();
        Arc::new(tasker_shared::events::domain_events::DomainEventPublisher::new(message_client))
    });

    // TAS-65 Phase 4.1: Create in-process event bus for fast domain events
    // Ruby can poll this channel to receive events with delivery_mode: fast
    info!("⚡ Creating in-process event bus for fast domain events...");
    let in_process_bus = InProcessEventBus::new(InProcessEventBusConfig::default());
    let in_process_event_receiver = in_process_bus.subscribe_ffi();
    info!("✅ In-process event bus created with FFI subscriber");

    // Note: The InProcessEventBus is created but not yet integrated with EventRouter
    // for routing delivery_mode: fast events. This will be addressed when step handlers
    // use the EventRouter for dual-path delivery. For now, Ruby can poll this channel
    // for any events explicitly dispatched to the in-process bus.

    // Store the bridge handle with event receiver and domain event publisher
    *handle_guard = Some(RubyBridgeHandle::new(
        system_handle,
        event_handler,
        event_receiver,
        domain_event_publisher,
        Some(in_process_event_receiver),
        runtime,
    ));

    // Return handle info to Ruby
    let ruby = magnus::Ruby::get().map_err(|err| {
        Error::new(
            magnus::exception::runtime_error(),
            format!("Failed to get ruby system: {}", err),
        )
    })?;
    let hash = ruby.hash_new();
    hash.aset("handle_id", worker_id.to_string())?;
    hash.aset("status", "started")?;
    hash.aset("message", "Ruby worker system started successfully")?;
    hash.aset("worker_id", worker_id_str)?;

    Ok(hash.as_value())
}

/// Stop the worker system
pub fn stop_worker() -> Result<String, Error> {
    let mut handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    match handle_guard.as_mut() {
        Some(handle) => {
            handle.stop().map_err(|e| {
                error!("Failed to stop worker system: {}", e);
                Error::new(magnus::exception::runtime_error(), e)
            })?;
            *handle_guard = None;
            Ok("Worker system stopped".to_string())
        }
        None => Ok("Worker system not running".to_string()),
    }
}

/// Get worker system status
pub fn get_worker_status() -> Result<Value, Error> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let ruby = magnus::Ruby::get().map_err(|err| {
        error!("Failed to get ruby system: {err}");
        Error::new(
            magnus::exception::runtime_error(),
            "Failed to get ruby system: {err}",
        )
    })?;
    let hash = ruby.hash_new();

    if let Some(handle) = handle_guard.as_ref() {
        let runtime = handle.runtime_handle();
        let status = runtime
            .block_on(async { handle.status().await })
            .map_err(|err| {
                error!("Failed to get status from runtime: {err}");
                Error::new(
                    magnus::exception::runtime_error(),
                    "Failed to get status from runtime {err}",
                )
            })?;

        hash.aset("running", status.running)?;
        hash.aset("environment", status.environment)?;
        hash.aset(
            "worker_core_status",
            format!("{:?}", status.worker_core_status),
        )?;
        hash.aset("web_api_enabled", status.web_api_enabled)?;
        hash.aset("supported_namespaces", status.supported_namespaces)?;
        hash.aset("database_pool_size", status.database_pool_size)?;
        hash.aset("database_pool_idle", status.database_pool_idle)?;
    } else {
        hash.aset("running", false)?;
        hash.aset("error", "Worker system not initialized")?;
    }

    Ok(hash.as_value())
}

/// Transition to graceful shutdown
pub fn transition_to_graceful_shutdown() -> Result<String, Error> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        Error::new(
            magnus::exception::runtime_error(),
            "Worker system not running",
        )
    })?;

    let runtime = handle.runtime_handle();
    runtime.block_on(async {
        let mut worker_core = handle.system_handle.worker_core.lock().await;
        worker_core.stop().await.map_err(|e| {
            error!("Failed to transition to graceful shutdown: {}", e);
            Error::new(
                magnus::exception::runtime_error(),
                format!("Graceful shutdown failed: {}", e),
            )
        })
    })?;

    Ok("Worker system transitioned to graceful shutdown".to_string())
}
