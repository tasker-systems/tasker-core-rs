//! # Native Rust Worker Demonstration
//!
//! High-performance worker demonstrating tasker-worker foundation with pure Rust step handlers.
//! This validates that our worker architecture works seamlessly with native implementations
//! while achieving significant performance improvements over Ruby equivalents.

use crate::{
    event_handler::RustEventHandler, global_event_system::get_global_event_system,
    RustStepHandlerRegistry, WorkerBootstrap, WorkerBootstrapConfig,
};
use anyhow::Result;
use std::sync::Arc;
use tasker_worker::WorkerSystemHandle;
use tracing::{info, warn};

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

pub fn no_web_api_config() -> WorkerBootstrapConfig {
    WorkerBootstrapConfig {
        worker_id: "rust-worker-demo-001".to_string(),
        enable_web_api: false,
        event_driven_enabled: true,
        deployment_mode_hint: Some("Hybrid".to_string()),
        ..Default::default()
    }
}

pub fn no_event_driven_config() -> WorkerBootstrapConfig {
    WorkerBootstrapConfig {
        worker_id: "rust-worker-demo-001".to_string(),
        enable_web_api: true,
        event_driven_enabled: false,
        deployment_mode_hint: Some("Hybrid".to_string()),
        ..Default::default()
    }
}

pub async fn bootstrap(
    config: WorkerBootstrapConfig,
) -> Result<(WorkerSystemHandle, RustEventHandler)> {
    info!("📋 Creating native Rust step handler registry...");
    let registry = Arc::new(RustStepHandlerRegistry::new());
    info!(
        "✅ Registry created with {} handlers",
        registry.handler_count()
    );

    // Get global event system for connecting to worker events
    info!("🔗 Setting up event system connection...");
    let event_system = get_global_event_system();

    // Create and start the event handler to bridge worker events to Rust handlers
    let event_handler = RustEventHandler::new(
        registry.clone(),
        event_system.clone(),
        "rust-worker-demo-001".to_string(),
    );

    info!("✅ Event handler connected - ready to receive StepExecutionEvents");

    // Bootstrap the worker using tasker-worker foundation with our global event system
    info!("🏗️  Bootstrapping worker with tasker-worker foundation...");
    let worker_handle =
        WorkerBootstrap::bootstrap_with_event_system(config, Some(event_system)).await?;

    Ok((worker_handle, event_handler))
}
