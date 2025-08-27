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

use tasker_orchestration::orchestration::bootstrap::{BootstrapConfig, OrchestrationBootstrap};
use tasker_shared::logging;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging first
    logging::init_structured_logging();

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

    // Get environment override from env var
    let environment_override = env::var("TASKER_ENV").ok();

    info!(
        "   Environment: {}",
        environment_override.as_deref().unwrap_or("auto-detect")
    );

    // Bootstrap orchestration system with web API enabled
    info!("🔧 Bootstrapping orchestration system...");

    let bootstrap_config = BootstrapConfig {
        namespaces: vec![], // Let configuration determine namespaces
        auto_start_processors: true,
        environment_override,
        enable_web_api: true, // Always enable web API for server mode
        web_config: None,     // Will be loaded from config manager
        enable_event_driven_coordination: true, // Enable TAS-43 event-driven coordination
        event_driven_config: None, // Use default configuration
    };

    let mut orchestration_handle = OrchestrationBootstrap::bootstrap(bootstrap_config)
        .await
        .map_err(|e| format!("Failed to bootstrap orchestration: {e}"))?;

    info!("🎉 Orchestration Server started successfully!");

    if orchestration_handle.web_state.is_some() {
        info!("   Web API: Running");
    }
    info!(
        "   Environment: {}",
        orchestration_handle.config_manager.environment()
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
