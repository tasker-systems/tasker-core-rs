//! gRPC server setup and configuration.
//!
//! This module provides the main entry point for starting the gRPC server,
//! including service registration, interceptors, and server configuration.

use crate::grpc::services::{
    AnalyticsServiceImpl, ConfigServiceImpl, DlqServiceImpl, HealthServiceImpl, StepServiceImpl,
    TaskServiceImpl, TemplateServiceImpl,
};
use crate::grpc::state::GrpcState;
use std::net::SocketAddr;
use std::time::Duration;
use tasker_shared::config::GrpcConfig;
use tasker_shared::proto::v1::{
    analytics_service_server::AnalyticsServiceServer, config_service_server::ConfigServiceServer,
    dlq_service_server::DlqServiceServer, health_service_server::HealthServiceServer,
    step_service_server::StepServiceServer, task_service_server::TaskServiceServer,
    template_service_server::TemplateServiceServer, FILE_DESCRIPTOR_SET,
};
use tonic::transport::Server;
use tracing::{error, info};

/// gRPC server wrapper.
///
/// Manages the lifecycle of the gRPC server, including service registration,
/// reflection, and health checking.
#[derive(Debug)]
pub struct GrpcServer {
    config: GrpcConfig,
    state: GrpcState,
}

impl GrpcServer {
    /// Create a new gRPC server.
    pub fn new(config: GrpcConfig, state: GrpcState) -> Self {
        Self { config, state }
    }

    /// Start the gRPC server.
    ///
    /// This method blocks until the server is shut down.
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr: SocketAddr = self.config.bind_address.parse().map_err(|e| {
            format!(
                "Invalid gRPC bind address '{}': {}",
                self.config.bind_address, e
            )
        })?;

        info!(
            address = %addr,
            reflection = self.config.enable_reflection,
            health = self.config.enable_health_service,
            "Starting gRPC server"
        );

        // Create service implementations
        let health_service = HealthServiceImpl::new(self.state.clone());
        let task_service = TaskServiceImpl::new(self.state.clone());
        let step_service = StepServiceImpl::new(self.state.clone());
        let config_service = ConfigServiceImpl::new(self.state.clone());
        let analytics_service = AnalyticsServiceImpl::new(self.state.clone());
        let dlq_service = DlqServiceImpl::new(self.state.clone());
        let template_service = TemplateServiceImpl::new(self.state.clone());

        // Build server
        let mut server = Server::builder()
            // HTTP/2 settings
            .http2_keepalive_interval(Some(Duration::from_secs(
                self.config.keepalive_interval_seconds as u64,
            )))
            .http2_keepalive_timeout(Some(Duration::from_secs(
                self.config.keepalive_timeout_seconds as u64,
            )))
            .max_concurrent_streams(Some(self.config.max_concurrent_streams))
            .max_frame_size(Some(self.config.max_frame_size));

        // Build router by adding services one by one
        // In tonic 0.13+, add_service returns a Router
        let mut router = server
            .add_service(HealthServiceServer::new(health_service))
            .add_service(TaskServiceServer::new(task_service))
            .add_service(StepServiceServer::new(step_service))
            .add_service(ConfigServiceServer::new(config_service))
            .add_service(AnalyticsServiceServer::new(analytics_service))
            .add_service(DlqServiceServer::new(dlq_service))
            .add_service(TemplateServiceServer::new(template_service));

        // Add reflection service if enabled
        if self.config.enable_reflection {
            let reflection_service = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
                .build_v1()
                .map_err(|e| format!("Failed to build reflection service: {}", e))?;

            router = router.add_service(reflection_service);
            info!("gRPC reflection service enabled");
        }

        // Add standard gRPC health service if enabled (using tonic-health)
        if self.config.enable_health_service {
            let (health_reporter, grpc_health_service) = tonic_health::server::health_reporter();

            // Set service statuses
            health_reporter
                .set_serving::<TaskServiceServer<TaskServiceImpl>>()
                .await;
            health_reporter
                .set_serving::<StepServiceServer<StepServiceImpl>>()
                .await;
            health_reporter
                .set_serving::<HealthServiceServer<HealthServiceImpl>>()
                .await;
            health_reporter
                .set_serving::<ConfigServiceServer<ConfigServiceImpl>>()
                .await;
            health_reporter
                .set_serving::<AnalyticsServiceServer<AnalyticsServiceImpl>>()
                .await;
            health_reporter
                .set_serving::<DlqServiceServer<DlqServiceImpl>>()
                .await;
            health_reporter
                .set_serving::<TemplateServiceServer<TemplateServiceImpl>>()
                .await;

            router = router.add_service(grpc_health_service);
            info!("gRPC health service (grpc.health.v1) enabled");
        }

        // Serve
        router.serve(addr).await.map_err(|e| {
            error!(error = %e, "gRPC server error");
            e
        })?;

        Ok(())
    }

    /// Start the gRPC server in the background.
    ///
    /// Returns a handle that can be used to stop the server.
    pub fn spawn(self) -> GrpcServerHandle {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let addr = self.config.bind_address.clone();

        let handle = tokio::spawn(async move {
            tokio::select! {
                result = self.serve() => {
                    if let Err(e) = result {
                        error!(error = %e, "gRPC server error");
                    }
                }
                _ = shutdown_rx => {
                    info!("gRPC server shutting down");
                }
            }
        });

        GrpcServerHandle {
            shutdown_tx: Some(shutdown_tx),
            handle,
            bind_address: addr,
        }
    }
}

/// Handle for a running gRPC server.
#[derive(Debug)]
pub struct GrpcServerHandle {
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    handle: tokio::task::JoinHandle<()>,
    bind_address: String,
}

impl GrpcServerHandle {
    /// Get the bind address.
    pub fn bind_address(&self) -> &str {
        &self.bind_address
    }

    /// Stop the gRPC server.
    pub async fn stop(mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.handle.await?;
        Ok(())
    }
}

/// Start the gRPC server with the given configuration.
///
/// This is a convenience function that creates and starts a gRPC server.
pub async fn start_grpc_server(
    config: GrpcConfig,
    state: GrpcState,
) -> Result<GrpcServerHandle, Box<dyn std::error::Error + Send + Sync>> {
    if !config.enabled {
        return Err("gRPC server is disabled in configuration".into());
    }

    let server = GrpcServer::new(config, state);
    Ok(server.spawn())
}
