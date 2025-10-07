//! # Tasker Worker Server
//!
//! Thin wrapper binary for running the worker system as a standalone server.
//! This is the production deployment target for the Tasker worker service.
//!
//! ## Usage
//!
//! ```bash
//! # Run with default configuration
//! cargo run --bin tasker-worker --features web-api
//!
//! # Run with specific environment
//! TASKER_ENV=production cargo run --bin tasker-worker --features web-api
//! ```

use std::env;
use tokio::signal;
use tracing::{error, info};

use tasker_shared::logging;
use tasker_worker::bootstrap::WorkerBootstrap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for console logging
    logging::init_tracing();

    info!("🚀 Starting Tasker Worker Server with TAS-43 Event-Driven Architecture...");
    info!("   Version: {}", env!("CARGO_PKG_VERSION"));
    info!(
        "   Build Mode: {}",
        if cfg!(debug_assertions) {
            "Debug"
        } else {
            "Release"
        }
    );
    info!("   Architecture: TAS-43 Event-Driven Processing with Hybrid Deployment Mode");

    // Get environment override from env var
    let environment_override = env::var("TASKER_ENV").ok();

    info!(
        "   Environment: {}",
        environment_override.as_deref().unwrap_or("auto-detect")
    );

    // Bootstrap worker system with web API enabled
    info!("🔧 Bootstrapping worker system...");

    let mut worker_handle = WorkerBootstrap::bootstrap()
        .await
        .map_err(|e| format!("Failed to bootstrap worker: {e}"))?;

    info!("🎉 Worker Server started successfully with TAS-43 Event-Driven Architecture!");

    if worker_handle.web_state.is_some() {
        info!("   Web API: Running");
    }
    info!(
        "   Supported namespaces: {:?}",
        worker_handle.supported_namespaces().await
    );
    info!(
        "   Event-driven processing: {}",
        if worker_handle.worker_config.event_driven_enabled {
            "Enabled"
        } else {
            "Disabled"
        }
    );
    info!(
        "   Deployment mode: {}",
        worker_handle
            .worker_config
            .deployment_mode_hint
            .as_deref()
            .unwrap_or("Default")
    );
    info!("   Press Ctrl+C to shutdown gracefully");

    // Wait for shutdown signal
    shutdown_signal().await;

    info!("🛑 Shutdown signal received, initiating graceful shutdown...");

    // Stop worker system using handle (includes web server)
    info!("🔧 Stopping worker system...");
    if let Err(e) = worker_handle.stop() {
        error!("Failed to stop worker cleanly: {}", e);
    } else {
        info!("✅ Worker system stopped");
    }

    info!("👋 Worker Server shutdown complete");

    Ok(())
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C");
        },
        _ = terminate => {
            info!("Received SIGTERM");
        },
    }
}
