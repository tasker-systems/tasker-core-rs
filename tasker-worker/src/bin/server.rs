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
use tasker_worker::bootstrap::{WorkerBootstrap, WorkerBootstrapConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging first
    logging::init_structured_logging();

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

    let bootstrap_config = WorkerBootstrapConfig {
        worker_id: format!("server-worker-{}", uuid::Uuid::new_v4()),
        supported_namespaces: vec![
            "default".to_string(),
            "diamond_workflow".to_string(),
            "mixed_dag_workflow".to_string(),
            "order_fulfillment".to_string(),
            "linear_workflow".to_string(),
            "tree_workflow".to_string(),
        ],
        enable_web_api: true, // Always enable web API for server mode
        environment_override,
        ..Default::default()
    };

    let mut worker_handle = WorkerBootstrap::bootstrap(bootstrap_config)
        .await
        .map_err(|e| format!("Failed to bootstrap worker: {e}"))?;

    info!("🎉 Worker Server started successfully with TAS-43 Event-Driven Architecture!");

    if worker_handle.web_state.is_some() {
        info!("   Web API: Running");
    }
    info!(
        "   Environment: {}",
        worker_handle.config_manager.environment()
    );
    info!(
        "   Supported namespaces: {:?}",
        worker_handle.worker_config.supported_namespaces
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
