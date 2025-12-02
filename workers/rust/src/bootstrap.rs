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
    step_handlers::{PaymentEventPublisher, StepEventPublisherRegistry},
    RustStepHandlerRegistry, WorkerBootstrap,
};
use anyhow::Result;
use std::sync::Arc;
use tasker_worker::worker::{EventRouter, InProcessEventBus, InProcessEventBusConfig};
use tasker_worker::WorkerSystemHandle;
use tokio::sync::RwLock;
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

pub async fn bootstrap() -> Result<(WorkerSystemHandle, RustEventHandler)> {
    info!("📋 Creating native Rust step handler registry...");
    let registry = Arc::new(RustStepHandlerRegistry::new());
    info!(
        "✅ Registry created with {} handlers",
        registry.handler_count()
    );

    // Get global event system for connecting to worker events
    info!("🔗 Setting up event system connection...");
    let event_system = get_global_event_system();

    // Bootstrap the worker using tasker-worker foundation with our global event system
    // We need to bootstrap first to get access to the message client
    info!("🏗️  Bootstrapping worker with tasker-worker foundation...");
    let worker_handle =
        WorkerBootstrap::bootstrap_with_event_system(Some(event_system.clone())).await?;

    // TAS-65: Create step event publisher registry with domain event publisher
    // Access the domain_event_publisher from WorkerCore
    info!("🔔 Setting up step event publisher registry...");
    let domain_event_publisher = {
        let worker_core = worker_handle.worker_core.lock().await;
        worker_core.domain_event_publisher()
    };

    // TAS-65 Dual-Path: Create in-process event bus for fast event delivery
    info!("⚡ Creating in-process event bus for fast domain events...");
    let in_process_bus = Arc::new(RwLock::new(InProcessEventBus::new(
        InProcessEventBusConfig::default(),
    )));

    // TAS-65 Dual-Path: Create event router for dual-path delivery
    info!("🔀 Creating event router for dual-path delivery...");
    let event_router = Arc::new(RwLock::new(EventRouter::new(
        domain_event_publisher.clone(),
        in_process_bus.clone(),
    )));

    // Create registry with EventRouter for dual-path delivery
    let mut step_event_registry =
        StepEventPublisherRegistry::with_event_router(domain_event_publisher.clone(), event_router);
    info!("✅ Step event registry created with dual-path routing support");

    // Register custom publishers
    step_event_registry.register(PaymentEventPublisher::new(domain_event_publisher.clone()));
    info!("✅ Registered PaymentEventPublisher for payment-related steps");

    let step_event_registry = Arc::new(RwLock::new(step_event_registry));

    // Create and start the event handler to bridge worker events to Rust handlers
    let event_handler = RustEventHandler::new(
        registry.clone(),
        event_system.clone(),
        step_event_registry,
        "rust-worker-demo-001".to_string(),
    );

    info!("✅ Event handler connected - ready to receive StepExecutionEvents");
    info!("✅ Dual-path domain event delivery enabled (durable/fast)");

    Ok((worker_handle, event_handler))
}
