//! # Native Rust Worker Demonstration
//!
//! High-performance worker demonstrating tasker-worker foundation with pure Rust step handlers.
//! This validates that our worker architecture works seamlessly with native implementations
//! while achieving significant performance improvements over Ruby equivalents.

use anyhow::Result;
use tasker_worker_rust::bootstrap::bootstrap;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Use tasker-shared's structured logging initialization
    tasker_shared::logging::init_tracing();

    // TAS-67: Bootstrap returns RustWorkerBootstrapResult with dispatch service handle
    let mut bootstrap_result = bootstrap().await?;

    // Start event handler in background (legacy path - kept for compatibility)
    tokio::spawn(async move {
        if let Err(e) = bootstrap_result.event_handler.start().await {
            warn!("Event handler stopped with error: {}", e);
        }
    });

    // TAS-67: The HandlerDispatchService is already spawned by bootstrap
    // It will handle step execution via the dispatch channel
    if bootstrap_result.dispatch_service_handle.is_some() {
        info!("🚀 TAS-67: HandlerDispatchService active for non-blocking handler dispatch");
    }

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
    bootstrap_result.worker_handle.stop()?;
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
