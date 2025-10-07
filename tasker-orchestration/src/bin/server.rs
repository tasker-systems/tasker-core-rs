//! # Tasker Orchestration Server
//!
//! Thin wrapper binary for running the orchestration system as a standalone server.
//! This is the production deployment target for the Tasker orchestration service.
//!
//! ## Usage
//!
//! ```bash
//! # Run with default configuration
//! cargo run --bin tasker-server --features web-api
//!
//! # Run with specific environment
//! TASKER_ENV=production cargo run --bin tasker-server --features web-api
//! ```

use std::env;
use tokio::signal;
use tracing::{error, info};

use tasker_orchestration::orchestration::bootstrap::OrchestrationBootstrap;
use tasker_shared::logging;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging first
    logging::init_tracing();

    info!("🚀 Starting Tasker Orchestration Server...");
    info!("   Version: {}", env!("CARGO_PKG_VERSION"));
    info!(
        "   Build Mode: {}",
        if cfg!(debug_assertions) {
            "Debug"
        } else {
            "Release"
        }
    );

    let mut orchestration_handle = OrchestrationBootstrap::bootstrap()
        .await
        .map_err(|e| format!("Failed to bootstrap orchestration: {e}"))?;

    info!("🎉 Orchestration Server started successfully!");

    if orchestration_handle.web_state.is_some() {
        info!("   Web API: Running");
    }
    info!(
        "   Environment: {}",
        orchestration_handle.tasker_config.environment()
    );
    info!("   Press Ctrl+C to shutdown gracefully");

    // Wait for shutdown signal
    shutdown_signal().await;

    info!("🛑 Shutdown signal received, initiating graceful shutdown...");

    // Stop orchestration system using handle (includes web server)
    info!("🔧 Stopping orchestration system...");
    if let Err(e) = orchestration_handle.stop().await {
        error!("Failed to stop orchestration cleanly: {}", e);
    } else {
        info!("✅ Orchestration system stopped");
    }

    info!("👋 Orchestration Server shutdown complete");

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
