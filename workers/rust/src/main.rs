//! # Native Rust Worker Demonstration
//!
//! High-performance worker demonstrating tasker-worker foundation with pure Rust step handlers.
//! This validates that our worker architecture works seamlessly with native implementations
//! while achieving significant performance improvements over Ruby equivalents.

use anyhow::Result;
use std::sync::Arc;
use tasker_worker_rust::{
    event_handler::RustEventHandler,
    global_event_system::get_global_event_system,
    RustStepHandlerRegistry, WorkerBootstrap, WorkerBootstrapConfig,
};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber for structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("tasker_worker_rust=info".parse()?)
                .add_directive("tasker_worker=info".parse()?)
                .add_directive("tasker_shared=warn".parse()?),
        )
        .init();

    info!("🚀 Starting Native Rust Worker Demonstration");
    info!("✅ TAS-41: Proving tasker-worker excellence for native Rust development");

    // Create registry with all Rust step handlers
    info!("📋 Creating native Rust step handler registry...");
    let registry = Arc::new(RustStepHandlerRegistry::new());
    info!("✅ Registry created with {} handlers", registry.handler_count());

    // Get global event system for connecting to worker events
    info!("🔗 Setting up event system connection...");
    let event_system = get_global_event_system();
    
    // Create and start the event handler to bridge worker events to Rust handlers
    let event_handler = RustEventHandler::new(
        registry.clone(),
        event_system.clone(),
        "rust-worker-demo-001".to_string(),
    );
    
    // Start the event handler in background
    let _event_handler_handle = tokio::spawn(async move {
        if let Err(e) = event_handler.start().await {
            warn!("Event handler stopped with error: {}", e);
        }
    });
    
    info!("✅ Event handler connected - ready to receive StepExecutionEvents");

    // Configure worker with all supported workflow namespaces
    let config = WorkerBootstrapConfig {
        worker_id: "rust-worker-demo-001".to_string(),
        supported_namespaces: vec![
            "linear_workflow".to_string(),
            "diamond_workflow".to_string(), 
            "tree_workflow".to_string(),
            "mixed_dag_workflow".to_string(),
            "order_fulfillment".to_string(),
        ],
        enable_web_api: true,
        event_driven_enabled: true, // TAS-43 real-time processing
        deployment_mode_hint: Some("Hybrid".to_string()),
        ..Default::default()
    };

    info!("🔧 Worker Configuration:");
    info!("   Worker ID: {}", config.worker_id);
    info!("   Supported Namespaces: {:?}", config.supported_namespaces);
    info!("   Event-Driven Processing: {:?}", config.event_driven_enabled);
    info!("   Web API: {}", config.enable_web_api);

    // Bootstrap the worker using tasker-worker foundation with our global event system
    info!("🏗️  Bootstrapping worker with tasker-worker foundation...");
    let mut worker_handle = WorkerBootstrap::bootstrap_with_event_system(config, Some(event_system)).await?;

    // Display successful startup information
    info!("🎉 Native Rust Worker is running!");
    info!("📡 Event-driven processing: PostgreSQL LISTEN/NOTIFY + fallback polling");
    info!("⚡ Command pattern: Tokio channels with WorkerProcessor");
    info!("🔬 Performance: Native Rust step processing for maximum throughput");
    info!("🛠️  Infrastructure: Same foundation as Ruby workers, proving architecture excellence");
    
    info!("💡 Ready to process tasks in all supported workflow patterns:");
    info!("   • Linear Workflow (mathematical_sequence)");
    info!("   • Diamond Workflow (parallel branches + convergence)");
    info!("   • Tree Workflow (hierarchical branching)");
    info!("   • Mixed DAG Workflow (complex dependencies)");
    info!("   • Order Fulfillment (real-world business process)");

    info!("🔄 Worker running... Press Ctrl+C to shutdown gracefully");

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("🛑 Received Ctrl+C, initiating graceful shutdown...");
        }
        result = wait_for_sigterm() => {
            match result {
                Ok(_) => info!("🛑 Received SIGTERM, initiating graceful shutdown..."),
                Err(e) => warn!("⚠️  Error setting up SIGTERM handler: {}", e),
            }
        }
    }

    // Graceful shutdown
    info!("🔄 Shutting down worker...");
    worker_handle.stop()?;
    info!("✅ Worker shutdown complete");

    Ok(())
}

/// Wait for SIGTERM signal (for container deployments)
#[cfg(unix)]
async fn wait_for_sigterm() -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate())?;
    sigterm.recv().await;
    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_sigterm() -> Result<()> {
    // On non-Unix systems, just wait indefinitely
    std::future::pending::<()>().await;
    Ok(())
}