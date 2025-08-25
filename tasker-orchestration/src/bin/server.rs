//! # Tasker Server
//!
//! Standalone server that runs both the orchestration system and web API.
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
//!
//! # Run with custom configuration
//! TASKER_PROJECT_ROOT=/path/to/config cargo run --bin tasker-server --features web-api
//! ```
//!
//! ## Architecture
//!
//! The server integrates:
//! - **OrchestrationBootstrap**: Unified bootstrap system with coordinator
//! - **Web API**: HTTP endpoints for task creation and monitoring
//! - **Health Monitoring**: Kubernetes-compatible health checks
//! - **Graceful Shutdown**: Clean shutdown with resource cleanup
//!
//! This prepares for TAS-40 Worker Foundations where workers will connect
//! to this orchestration service via HTTP API and message queues.

use std::env;
use tokio::signal;
use tracing::{error, info};

use tasker_orchestration::orchestration::bootstrap::{BootstrapConfig, OrchestrationBootstrap};
use tasker_orchestration::web::{
    create_app,
    state::{AppState, WebServerConfig},
};
use tasker_shared::logging;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging first
    logging::init_structured_logging();

    info!("🚀 Starting Tasker Server...");
    info!("   Version: {}", env!("CARGO_PKG_VERSION"));
    info!(
        "   Build Mode: {}",
        if cfg!(debug_assertions) {
            "Debug"
        } else {
            "Release"
        }
    );

    // Get environment override (only override if explicitly set)
    let environment_override = env::var("TASKER_ENV").ok();

    info!(
        "   Environment: {}",
        environment_override.as_deref().unwrap_or("auto-detect")
    );

    // Bootstrap orchestration system using unified bootstrap
    info!("🔧 Bootstrapping orchestration system...");
    let bootstrap_config = BootstrapConfig {
        namespaces: vec![], // Let configuration determine namespaces
        auto_start_processors: true,
        environment_override,
    };

    let orchestration_handle = OrchestrationBootstrap::bootstrap(bootstrap_config)
        .await
        .map_err(|e| format!("Failed to bootstrap orchestration: {e}"))?;

    info!("✅ Orchestration system bootstrapped successfully");

    // Get config manager from handle for web setup
    let config_manager = orchestration_handle.config_manager.clone();

    // Initialize web API
    info!("🌐 Initializing web API...");

    // Create WebServerConfig from configuration system
    let web_server_config = match WebServerConfig::from_config_manager(&config_manager)
        .map_err(|e| format!("Failed to load web server configuration: {e}"))?
    {
        Some(config) => config,
        None => {
            return Err("Web API is disabled in configuration - cannot start server mode".into());
        }
    };

    let app_state = AppState::from_orchestration_core(
        web_server_config.clone(),
        &orchestration_handle.orchestration_core,
        &config_manager,
    )
    .await
    .map_err(|e| format!("Failed to create app state: {e}"))?;

    let app = create_app(app_state);

    // Start web server
    let bind_address = &web_server_config.bind_address;
    info!("🌐 Starting web server on {}", bind_address);

    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .map_err(|e| format!("Failed to bind to {bind_address}: {e}"))?;

    info!("✅ Web server listening on http://{}", bind_address);
    info!("📖 API Documentation: http://{}/api-docs/ui", bind_address);
    info!("🏥 Health Check: http://{}/health", bind_address);

    // Run server with graceful shutdown
    let server_task = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
        {
            error!("Web server error: {}", e);
        }
    });

    info!("🎉 Tasker Server started successfully!");
    info!("   Orchestration: Running with OrchestrationLoopCoordinator");
    info!("   Web API: http://{}", bind_address);
    info!("   Environment: {}", config_manager.environment());
    info!("   Press Ctrl+C to shutdown gracefully");

    // Wait for shutdown signal
    shutdown_signal().await;

    info!("🛑 Shutdown signal received, initiating graceful shutdown...");

    // Note: The orchestration core will be stopped through the handle.stop() below
    // which properly manages the shutdown sequence

    // Stop web server (it should already be stopping due to graceful shutdown)
    server_task.abort();

    // Stop orchestration system using handle
    info!("🔧 Stopping orchestration system...");
    let mut handle = orchestration_handle;
    if let Err(e) = handle.stop() {
        error!("Failed to stop orchestration cleanly: {}", e);
    } else {
        info!("✅ Orchestration system stopped");
    }

    info!("👋 Tasker Server shutdown complete");

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
