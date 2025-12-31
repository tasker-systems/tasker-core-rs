//! Global bridge state and internal FFI implementation.
//!
//! This module manages the global worker state and provides internal
//! implementations for the C FFI functions in lib.rs.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

use tasker_shared::events::domain_events::{DomainEvent, DomainEventPublisher};
use tasker_worker::worker::{
    DomainEventCallback, FfiDispatchChannel, FfiDispatchChannelConfig, StepEventPublisherRegistry,
};
use tasker_worker::{WorkerBootstrap, WorkerSystemHandle};
use tokio::sync::broadcast;

use crate::conversions::{convert_ffi_dispatch_metrics_to_json, convert_ffi_step_event_to_json};
use crate::dto::FfiDomainEventDto;
use crate::error::TypeScriptFfiError;

/// Global worker system state.
///
/// This holds the runtime and all worker components. It's protected by a Mutex
/// because JavaScript is single-threaded and we need to ensure safe access.
pub static WORKER_SYSTEM: Mutex<Option<TypeScriptBridgeHandle>> = Mutex::new(None);

/// Handle containing all components needed for the TypeScript worker.
pub struct TypeScriptBridgeHandle {
    /// The main worker system handle
    pub system_handle: WorkerSystemHandle,

    /// FFI dispatch channel for poll/complete pattern
    pub ffi_dispatch_channel: Arc<FfiDispatchChannel>,

    /// Domain event publisher for fire-and-forget events
    pub domain_event_publisher: Arc<DomainEventPublisher>,

    /// Receiver for in-process domain events (fast path)
    pub in_process_event_receiver: Option<Arc<Mutex<broadcast::Receiver<DomainEvent>>>>,

    /// Tokio runtime - kept alive to ensure async tasks continue
    #[allow(dead_code)]
    pub runtime: tokio::runtime::Runtime,

    /// Worker ID for identification
    pub worker_id: String,
}

impl std::fmt::Debug for TypeScriptBridgeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeScriptBridgeHandle")
            .field("worker_id", &self.worker_id)
            .finish()
    }
}

/// Internal implementation of bootstrap_worker.
pub fn bootstrap_worker_internal(config_json: Option<&str>) -> Result<String> {
    let mut guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    // Check if already running
    if guard.is_some() {
        return Ok(json!({
            "success": true,
            "status": "already_running",
            "message": "Worker is already running"
        })
        .to_string());
    }

    // Parse config if provided
    let _config: Option<serde_json::Value> = if let Some(json_str) = config_json {
        Some(serde_json::from_str(json_str).map_err(|e| {
            TypeScriptFfiError::InvalidArgument(format!("Invalid config JSON: {}", e))
        })?)
    } else {
        None
    };

    // Generate worker ID
    let worker_id = Uuid::new_v4();
    let worker_id_str = format!("typescript-worker-{}", worker_id);

    // Create Tokio runtime
    let runtime = tokio::runtime::Runtime::new().map_err(|e| {
        error!("Failed to create tokio runtime: {}", e);
        TypeScriptFfiError::RuntimeError(format!("Runtime creation failed: {}", e))
    })?;

    // Initialize tracing in Tokio runtime context
    runtime.block_on(async {
        tasker_shared::logging::init_tracing();
    });

    // Bootstrap the worker using tasker-worker foundation
    let mut system_handle = runtime
        .block_on(async { WorkerBootstrap::bootstrap().await })
        .map_err(|e| {
            error!("Failed to bootstrap worker system: {}", e);
            TypeScriptFfiError::BootstrapFailed(e.to_string())
        })?;

    info!("Worker system bootstrapped successfully");

    // Create domain event callback for step completion
    info!("Setting up step event publisher registry for domain events...");
    let (domain_event_publisher, domain_event_callback) = runtime.block_on(async {
        let worker_core = system_handle.worker_core.lock().await;

        // Get the message client for durable events
        let message_client = worker_core.context.message_client.clone();
        let publisher = Arc::new(DomainEventPublisher::new(message_client));

        // Get EventRouter from WorkerCore for stats tracking
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
    info!("Domain event callback created with EventRouter for stats tracking");

    // Take dispatch handles and create FfiDispatchChannel with callback
    let ffi_dispatch_channel = if let Some(dispatch_handles) = system_handle.take_dispatch_handles()
    {
        info!("Creating FfiDispatchChannel from dispatch handles...");

        // Create config with runtime handle for executing async callbacks from FFI threads
        let config = FfiDispatchChannelConfig::new(runtime.handle().clone())
            .with_service_id(worker_id_str.clone())
            .with_completion_timeout(std::time::Duration::from_secs(30));

        let channel = FfiDispatchChannel::new(
            dispatch_handles.dispatch_receiver,
            dispatch_handles.completion_sender,
            config,
            domain_event_callback,
        );

        info!("FfiDispatchChannel created with domain event callback for TypeScript step dispatch");
        Arc::new(channel)
    } else {
        error!("Failed to get dispatch handles from WorkerSystemHandle");
        return Err(TypeScriptFfiError::BootstrapFailed(
            "Dispatch handles not available".to_string(),
        )
        .into());
    };

    // Get in-process event receiver from WorkerCore's event bus
    info!("Subscribing to WorkerCore's in-process event bus for fast domain events...");
    let in_process_event_receiver = runtime.block_on(async {
        let worker_core = system_handle.worker_core.lock().await;
        let bus = worker_core.in_process_event_bus();
        let bus_guard = bus.write().await;
        bus_guard.subscribe_ffi()
    });
    info!("Subscribed to WorkerCore's in-process event bus for FFI domain events");

    // Store the bridge handle with FfiDispatchChannel
    *guard = Some(TypeScriptBridgeHandle {
        system_handle,
        ffi_dispatch_channel,
        domain_event_publisher,
        in_process_event_receiver: Some(Arc::new(Mutex::new(in_process_event_receiver))),
        runtime,
        worker_id: worker_id_str.clone(),
    });

    Ok(json!({
        "success": true,
        "status": "started",
        "message": "TypeScript worker system started successfully",
        "worker_id": worker_id_str
    })
    .to_string())
}

/// Internal implementation of get_worker_status.
pub fn get_worker_status_internal() -> Result<String> {
    let guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    match &*guard {
        Some(handle) => {
            let runtime = &handle.runtime;
            let status = runtime
                .block_on(async { handle.system_handle.status().await })
                .map_err(|e| {
                    error!("Failed to get status from runtime: {}", e);
                    TypeScriptFfiError::RuntimeError(format!("Failed to get status: {}", e))
                })?;

            Ok(json!({
                "success": true,
                "running": status.running,
                "worker_id": handle.worker_id,
                "environment": status.environment,
                "worker_core_status": format!("{:?}", status.worker_core_status),
                "web_api_enabled": status.web_api_enabled,
                "supported_namespaces": status.supported_namespaces,
                "database_pool_size": status.database_pool_size,
                "database_pool_idle": status.database_pool_idle
            })
            .to_string())
        }
        None => Ok(json!({
            "success": true,
            "running": false,
            "status": "stopped"
        })
        .to_string()),
    }
}

/// Internal implementation of stop_worker.
pub fn stop_worker_internal() -> Result<String> {
    let mut guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    match guard.as_mut() {
        Some(handle) => {
            let worker_id = handle.worker_id.clone();

            handle.system_handle.stop().map_err(|e| {
                error!("Failed to stop worker system: {}", e);
                TypeScriptFfiError::RuntimeError(e.to_string())
            })?;

            *guard = None;

            Ok(json!({
                "success": true,
                "status": "stopped",
                "message": "Worker stopped successfully",
                "worker_id": worker_id
            })
            .to_string())
        }
        None => Ok(json!({
            "success": true,
            "status": "not_running",
            "message": "Worker was not running"
        })
        .to_string()),
    }
}

/// Internal implementation of transition_to_graceful_shutdown.
pub fn transition_to_graceful_shutdown_internal() -> Result<String> {
    let guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    match &*guard {
        Some(handle) => {
            let runtime = &handle.runtime;
            runtime.block_on(async {
                let mut worker_core = handle.system_handle.worker_core.lock().await;
                worker_core.stop().await.map_err(|e| {
                    error!("Failed to transition to graceful shutdown: {}", e);
                    TypeScriptFfiError::RuntimeError(format!("Graceful shutdown failed: {}", e))
                })
            })?;

            Ok(json!({
                "success": true,
                "status": "transitioning",
                "message": "Transitioning to graceful shutdown"
            })
            .to_string())
        }
        None => Err(TypeScriptFfiError::WorkerNotInitialized.into()),
    }
}

/// Internal implementation of poll_step_events.
pub fn poll_step_events_internal() -> Result<Option<String>> {
    let guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    let handle = guard
        .as_ref()
        .ok_or(TypeScriptFfiError::WorkerNotInitialized)?;

    match handle.ffi_dispatch_channel.poll() {
        Some(event) => {
            let json_str = convert_ffi_step_event_to_json(&event)?;
            Ok(Some(json_str))
        }
        None => Ok(None),
    }
}

/// Internal implementation of complete_step_event.
pub fn complete_step_event_internal(event_id_str: &str, result_json: &str) -> Result<bool> {
    let guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    let handle = guard
        .as_ref()
        .ok_or(TypeScriptFfiError::WorkerNotInitialized)?;

    // Parse event ID
    let event_id = Uuid::parse_str(event_id_str)
        .map_err(|e| TypeScriptFfiError::InvalidArgument(format!("Invalid event ID: {}", e)))?;

    // Parse result
    let result: tasker_shared::messaging::StepExecutionResult = serde_json::from_str(result_json)
        .map_err(|e| {
        TypeScriptFfiError::ConversionError(format!("Invalid result JSON: {}", e))
    })?;

    // Complete the event
    Ok(handle.ffi_dispatch_channel.complete(event_id, result))
}

/// Internal implementation of get_ffi_dispatch_metrics.
pub fn get_ffi_dispatch_metrics_internal() -> Result<String> {
    let guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    let handle = guard
        .as_ref()
        .ok_or(TypeScriptFfiError::WorkerNotInitialized)?;

    let metrics = handle.ffi_dispatch_channel.metrics();
    convert_ffi_dispatch_metrics_to_json(&metrics)
}

/// Internal implementation of check_starvation_warnings.
pub fn check_starvation_warnings_internal() -> Result<()> {
    let guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    let handle = guard
        .as_ref()
        .ok_or(TypeScriptFfiError::WorkerNotInitialized)?;

    handle.ffi_dispatch_channel.check_starvation_warnings();
    Ok(())
}

/// Internal implementation of cleanup_timeouts.
pub fn cleanup_timeouts_internal() -> Result<()> {
    let guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    let handle = guard
        .as_ref()
        .ok_or(TypeScriptFfiError::WorkerNotInitialized)?;

    handle.ffi_dispatch_channel.cleanup_timeouts();
    Ok(())
}

/// Internal implementation of poll_in_process_events.
///
/// Polls for in-process domain events (fast path) from the broadcast channel.
/// Returns a JSON string representing the DomainEvent, or None if no events available.
pub fn poll_in_process_events_internal() -> Result<Option<String>> {
    let guard = WORKER_SYSTEM
        .lock()
        .map_err(|_| TypeScriptFfiError::LockError)?;

    let handle = guard
        .as_ref()
        .ok_or(TypeScriptFfiError::WorkerNotInitialized)?;

    // Check if we have an in-process event receiver
    let receiver = match &handle.in_process_event_receiver {
        Some(r) => r,
        None => {
            tracing::debug!("No in-process event receiver configured");
            return Ok(None);
        }
    };

    // Try to receive an event (non-blocking)
    let mut receiver_guard = receiver.lock().map_err(|_| TypeScriptFfiError::LockError)?;

    // Use try_recv for non-blocking receive
    match receiver_guard.try_recv() {
        Ok(event) => {
            tracing::debug!(event_name = %event.event_name, "Received in-process domain event");

            // Convert to JSON using type-safe DTO (TAS-112)
            let dto = FfiDomainEventDto::from(&event);
            let json_string = dto
                .to_json_string()
                .map_err(|e| TypeScriptFfiError::SerializationError(e.to_string()))?;

            Ok(Some(json_string))
        }
        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
            // No events available
            Ok(None)
        }
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(count)) => {
            tracing::warn!(
                count = count,
                "In-process event receiver lagged, some events dropped"
            );
            // Still return None, next call will get new events
            Ok(None)
        }
        Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
            tracing::warn!("In-process event channel closed");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_not_initialized() {
        // Reset state
        *WORKER_SYSTEM.lock().unwrap() = None;

        let result = get_worker_status_internal().unwrap();
        assert!(result.contains("\"running\":false"));
    }
}
