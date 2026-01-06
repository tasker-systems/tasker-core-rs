//! # Ruby Worker Bootstrap
//!
//! TAS-67: Follows the same patterns as workers/rust/src/bootstrap.rs but adapted
//! for Ruby FFI integration with magnus. Uses FfiDispatchChannel for step event
//! dispatch instead of the legacy RubyEventHandler.
//!
//! ## Architecture
//!
//! ```text
//! WorkerBootstrap → WorkerSystemHandle → take_dispatch_handles()
//!                                              │
//!                                              ▼
//!                                    FfiDispatchChannel
//!                                              │
//!                             ┌────────────────┴────────────────┐
//!                             ▼                                 ▼
//!                     poll_step_events()              complete_step_event()
//!                        (Ruby FFI)                      (Ruby FFI)
//! ```

use crate::bridge::{RubyBridgeHandle, WORKER_SYSTEM};
use crate::global_event_system::get_global_event_system;
use magnus::{value::ReprValue, Error, ExceptionClass, Ruby, Value};
use std::sync::Arc;

/// Helper to get RuntimeError exception class
/// Uses the new magnus 0.8 API pattern
fn runtime_error_class() -> ExceptionClass {
    Ruby::get()
        .expect("Ruby runtime should be available")
        .exception_runtime_error()
}
use tasker_worker::worker::{
    services::CheckpointService, DomainEventCallback, FfiDispatchChannel, FfiDispatchChannelConfig,
    StepEventPublisherRegistry,
};
use tasker_worker::WorkerBootstrap;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

/// Bootstrap the worker system for Ruby
///
/// TAS-67: This now uses FfiDispatchChannel for step event dispatch,
/// matching the architecture of the Rust worker but adapted for FFI.
///
/// Returns a handle ID that Ruby can use to reference the worker system.
pub fn bootstrap_worker() -> Result<Value, Error> {
    let worker_id = Uuid::new_v4();
    let worker_id_str = format!("ruby-worker-{}", worker_id);

    // Check if already running
    let mut handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    if handle_guard.is_some() {
        // Return existing handle info
        let ruby = magnus::Ruby::get().map_err(|err| {
            Error::new(
                runtime_error_class(),
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
        Error::new(runtime_error_class(), "Runtime creation failed")
    })?;

    // TAS-65 Phase 2: Initialize telemetry in Tokio runtime context
    runtime.block_on(async {
        tasker_shared::logging::init_tracing();
    });

    // Get global event system (shared singleton)
    let event_system = get_global_event_system();

    // Bootstrap the worker using tasker-worker foundation
    let mut system_handle = runtime.block_on(async {
        WorkerBootstrap::bootstrap_with_event_system(Some(event_system))
            .await
            .map_err(|e| {
                error!("Failed to bootstrap worker system: {}", e);
                Error::new(
                    runtime_error_class(),
                    format!("Worker bootstrap failed: {}", e),
                )
            })
    })?;

    info!("✅ Worker system bootstrapped successfully");

    // TAS-67: Create domain event callback for step completion
    // This MUST be done BEFORE taking dispatch handles
    info!("🔔 Setting up step event publisher registry for domain events...");
    let (domain_event_publisher, domain_event_callback) = runtime.block_on(async {
        let worker_core = system_handle.worker_core.lock().await;

        // Get the message client for durable events
        let message_client = worker_core.context.message_client.clone();
        let publisher = Arc::new(
            tasker_shared::events::domain_events::DomainEventPublisher::new(message_client),
        );

        // TAS-67: Get EventRouter from WorkerCore for stats tracking
        let event_router = worker_core
            .event_router()
            .expect("EventRouter should be available from WorkerCore");

        // Create registry with EventRouter for dual-path delivery (durable + fast)
        let step_event_registry =
            StepEventPublisherRegistry::with_event_router(publisher.clone(), event_router);

        let registry = Arc::new(RwLock::new(step_event_registry));
        let callback = Arc::new(DomainEventCallback::new(registry));

        (publisher, callback)
    });
    info!("✅ Domain event callback created with EventRouter for stats tracking");

    // TAS-67: Take dispatch handles and create FfiDispatchChannel with callback
    let ffi_dispatch_channel = if let Some(dispatch_handles) = system_handle.take_dispatch_handles()
    {
        info!("🔗 Creating FfiDispatchChannel from dispatch handles...");

        // Create config with runtime handle for executing async callbacks from FFI threads
        let config = FfiDispatchChannelConfig::new(runtime.handle().clone())
            .with_service_id(worker_id_str.clone())
            .with_completion_timeout(std::time::Duration::from_secs(30));

        // TAS-125: Get database pool for checkpoint service
        let db_pool = runtime.block_on(async {
            let worker_core = system_handle.worker_core.lock().await;
            worker_core.context.database_pool.clone()
        });

        // TAS-125: Create checkpoint service for batch processing handlers
        let checkpoint_service = CheckpointService::new(db_pool);

        let channel = FfiDispatchChannel::new(
            dispatch_handles.dispatch_receiver,
            dispatch_handles.completion_sender,
            config,
            domain_event_callback,
        )
        // TAS-125: Enable checkpoint support for batch processing
        .with_checkpoint_support(checkpoint_service, dispatch_handles.dispatch_sender);

        info!("✅ FfiDispatchChannel created with domain event callback and checkpoint support for Ruby step dispatch");
        Arc::new(channel)
    } else {
        error!("Failed to get dispatch handles from WorkerSystemHandle");
        return Err(Error::new(
            runtime_error_class(),
            "Dispatch handles not available",
        ));
    };

    // TAS-65 Phase 4.1: Get in-process event receiver from WorkerCore's event bus
    // IMPORTANT: We must use WorkerCore's bus, not create a new one, otherwise the
    // sender is dropped when the local bus goes out of scope.
    info!("⚡ Subscribing to WorkerCore's in-process event bus for fast domain events...");
    let in_process_event_receiver = runtime.block_on(async {
        let worker_core = system_handle.worker_core.lock().await;
        // Get the in-process bus from WorkerCore and subscribe for FFI
        let bus = worker_core.in_process_event_bus();
        let bus_guard = bus.write().await;
        bus_guard.subscribe_ffi()
    });
    info!("✅ Subscribed to WorkerCore's in-process event bus for FFI domain events");

    // Store the bridge handle with FfiDispatchChannel
    *handle_guard = Some(RubyBridgeHandle::new(
        system_handle,
        ffi_dispatch_channel,
        domain_event_publisher,
        Some(in_process_event_receiver),
        runtime,
    ));

    // Return handle info to Ruby
    let ruby = magnus::Ruby::get().map_err(|err| {
        Error::new(
            runtime_error_class(),
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
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    match handle_guard.as_mut() {
        Some(handle) => {
            handle.stop().map_err(|e| {
                error!("Failed to stop worker system: {}", e);
                Error::new(runtime_error_class(), e)
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
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let ruby = magnus::Ruby::get().map_err(|err| {
        error!("Failed to get ruby system: {err}");
        Error::new(runtime_error_class(), "Failed to get ruby system: {err}")
    })?;
    let hash = ruby.hash_new();

    if let Some(handle) = handle_guard.as_ref() {
        let runtime = handle.runtime_handle();
        let status = runtime
            .block_on(async { handle.status().await })
            .map_err(|err| {
                error!("Failed to get status from runtime: {err}");
                Error::new(
                    runtime_error_class(),
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
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or_else(|| Error::new(runtime_error_class(), "Worker system not running"))?;

    let runtime = handle.runtime_handle();
    runtime.block_on(async {
        let mut worker_core = handle.system_handle.worker_core.lock().await;
        worker_core.stop().await.map_err(|e| {
            error!("Failed to transition to graceful shutdown: {}", e);
            Error::new(
                runtime_error_class(),
                format!("Graceful shutdown failed: {}", e),
            )
        })
    })?;

    Ok("Worker system transitioned to graceful shutdown".to_string())
}
