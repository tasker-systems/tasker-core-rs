//! # Native Rust Worker Demonstration
//!
//! High-performance worker demonstrating tasker-worker foundation with pure Rust step handlers.
//! This validates that our worker architecture works seamlessly with native implementations
//! while achieving significant performance improvements over Ruby equivalents.
//!
//! ## TAS-65: Post-Execution Domain Event Publishing
//!
//! The bootstrap process sets up the `StepEventPublisherRegistry` which manages
//! custom domain event publishers. After each step executes, the appropriate
//! publisher is invoked to publish events based on YAML configuration.
//!
//! ## TAS-65: Dual-Path Domain Event Delivery
//!
//! Domain events can be routed to different delivery paths based on `delivery_mode`:
//! - `durable`: Published to PGMQ (external consumers, audit trails)
//! - `fast`: Dispatched to in-process bus (metrics, telemetry, notifications)

use crate::{
    event_handler::RustEventHandler,
    global_event_system::get_global_event_system,
    step_handlers::{
        registry::RustStepHandlerRegistryAdapter, DomainEventCallback, PaymentEventPublisher,
        StepEventPublisherRegistry,
    },
    RustStepHandlerRegistry, WorkerBootstrap,
};
use anyhow::Result;
use std::sync::Arc;
use tasker_worker::worker::{HandlerDispatchConfig, HandlerDispatchService, LoadSheddingConfig};
use tasker_worker::WorkerSystemHandle;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;

// Re-export config for external use
pub use tasker_worker::WorkerBootstrapConfig;

#[must_use]
pub fn default_config() -> WorkerBootstrapConfig {
    // Configure worker with all supported workflow namespaces
    WorkerBootstrapConfig {
        worker_id: "rust-worker-demo-001".to_string(),
        enable_web_api: true,
        event_driven_enabled: true,
        deployment_mode_hint: Some("Hybrid".to_string()),
        ..Default::default()
    }
}

#[must_use]
pub fn no_web_api_config() -> WorkerBootstrapConfig {
    WorkerBootstrapConfig {
        worker_id: "rust-worker-demo-001".to_string(),
        enable_web_api: false,
        event_driven_enabled: true,
        deployment_mode_hint: Some("Hybrid".to_string()),
        ..Default::default()
    }
}

#[must_use]
pub fn no_event_driven_config() -> WorkerBootstrapConfig {
    WorkerBootstrapConfig {
        worker_id: "rust-worker-demo-001".to_string(),
        enable_web_api: true,
        event_driven_enabled: false,
        deployment_mode_hint: Some("Hybrid".to_string()),
        ..Default::default()
    }
}

/// TAS-67: Bootstrap result containing all handles for the Rust worker
pub struct RustWorkerBootstrapResult {
    /// Worker system handle for lifecycle management
    pub worker_handle: WorkerSystemHandle,
    /// Event handler for bridging worker events to Rust handlers (legacy path)
    pub event_handler: RustEventHandler,
    /// TAS-67: Handler dispatch service task handle
    /// This spawns the non-blocking dispatch service that invokes Rust handlers
    pub dispatch_service_handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for RustWorkerBootstrapResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustWorkerBootstrapResult")
            .field("worker_handle", &self.worker_handle)
            .field("event_handler", &self.event_handler)
            .field(
                "dispatch_service_handle",
                &self
                    .dispatch_service_handle
                    .as_ref()
                    .map(|_| "JoinHandle<()>"),
            )
            .finish()
    }
}

pub async fn bootstrap() -> Result<RustWorkerBootstrapResult> {
    info!("📋 Creating native Rust step handler registry...");
    // TAS-67: Create registry for legacy event handler path
    let registry = Arc::new(RustStepHandlerRegistry::new());
    info!(
        "✅ Registry created with {} handlers",
        registry.handler_count()
    );
    // TAS-67: Create adapter that implements StepHandlerRegistry trait for HandlerDispatchService
    // This is a separate registry instance since the adapter takes ownership
    let registry_adapter = Arc::new(RustStepHandlerRegistryAdapter::with_default_handlers());

    // Get global event system for connecting to worker events
    info!("🔗 Setting up event system connection...");
    let event_system = get_global_event_system();

    // Bootstrap the worker using tasker-worker foundation with our global event system
    // We need to bootstrap first to get access to the message client
    info!("🏗️  Bootstrapping worker with tasker-worker foundation...");
    let mut worker_handle =
        WorkerBootstrap::bootstrap_with_event_system(Some(event_system.clone())).await?;

    // TAS-65: Create step event publisher registry with domain event publisher and event router
    // Access both from WorkerCore to ensure stats are tracked in the same instances
    // NOTE: Must be created BEFORE HandlerDispatchService to pass to callback
    info!("🔔 Setting up step event publisher registry...");
    let (domain_event_publisher, event_router) = {
        let worker_core = worker_handle.worker_core.lock().await;
        let publisher = worker_core.domain_event_publisher();
        // TAS-67: Use the SAME EventRouter from WorkerCore to ensure stats are shared
        // This is critical for the /debug/events endpoint to show correct counts
        let router = worker_core
            .event_router()
            .expect("EventRouter should be available from WorkerCore");
        (publisher, router)
    };
    info!("✅ Using EventRouter from WorkerCore for shared stats tracking");

    // Create registry with EventRouter for dual-path delivery
    let mut step_event_registry = StepEventPublisherRegistry::with_event_router(
        domain_event_publisher.clone(),
        event_router.clone(),
    );
    info!("✅ Step event registry created with dual-path routing support");

    // Register custom publishers with EventRouter for stats tracking
    // TAS-67: Use with_event_router() to enable dual-path routing with stats
    #[allow(deprecated)]
    step_event_registry.register(PaymentEventPublisher::with_event_router(
        domain_event_publisher.clone(),
        event_router,
    ));
    info!("✅ Registered PaymentEventPublisher with EventRouter for stats tracking");

    let step_event_registry = Arc::new(RwLock::new(step_event_registry));

    // TAS-67: Take dispatch handles and spawn HandlerDispatchService with domain event callback
    // This is the non-blocking dispatch path that replaces direct handler invocation
    let dispatch_service_handle = if let Some(dispatch_handles) =
        worker_handle.take_dispatch_handles()
    {
        info!("🚀 Setting up TAS-67 HandlerDispatchService for non-blocking handler dispatch...");

        let config = HandlerDispatchConfig {
            max_concurrent_handlers: 10,
            handler_timeout: std::time::Duration::from_secs(30),
            service_id: "rust-handler-dispatch".to_string(),
            load_shedding: LoadSheddingConfig::default(),
        };

        // TAS-67: Create domain event callback for post-handler event publishing
        let domain_event_callback = Arc::new(DomainEventCallback::new(step_event_registry.clone()));
        info!("✅ Domain event callback created for dispatch service");

        // Use with_callback to enable domain event publishing through dispatch path
        // TAS-75: Returns (service, capacity_checker) tuple - capacity_checker for future load shedding integration
        let (dispatch_service, _capacity_checker) = HandlerDispatchService::with_callback(
            dispatch_handles.dispatch_receiver,
            dispatch_handles.completion_sender,
            registry_adapter.clone(),
            config,
            domain_event_callback,
        );

        let handle = tokio::spawn(async move {
            dispatch_service.run().await;
        });

        info!("✅ HandlerDispatchService spawned with domain event callback - non-blocking handler dispatch active");
        Some(handle)
    } else {
        info!("⚠️  No dispatch handles available - HandlerDispatchService not started");
        None
    };

    // Create and start the event handler to bridge worker events to Rust handlers
    // NOTE: This is the legacy path. TAS-67 dispatch service is now the primary path.
    let event_handler = RustEventHandler::new(
        registry.clone(),
        event_system.clone(),
        step_event_registry,
        "rust-worker-demo-001".to_string(),
    );

    info!("✅ Event handler connected - ready to receive StepExecutionEvents");
    info!("✅ Dual-path domain event delivery enabled (durable/fast)");
    info!("✅ TAS-67: Non-blocking handler dispatch enabled via HandlerDispatchService");

    Ok(RustWorkerBootstrapResult {
        worker_handle,
        event_handler,
        dispatch_service_handle,
    })
}
