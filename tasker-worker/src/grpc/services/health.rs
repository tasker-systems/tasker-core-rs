//! Health service gRPC implementation for Worker.
//!
//! Provides Kubernetes-compatible health check operations via gRPC.
//! Matches REST API health endpoints (GET /health, /live, /ready, /health/detailed).
//!
//! Uses worker-specific proto types that match the worker REST API exactly.
//! Conversions use `From`/`Into` traits defined in `tasker-shared/src/proto/conversions.rs`.

use crate::grpc::state::WorkerGrpcState;
use tasker_shared::proto::v1::{
    self as proto, worker_health_service_server::WorkerHealthService as WorkerHealthServiceTrait,
};
use tonic::{Request, Response, Status};
use tracing::debug;

/// gRPC Health service implementation for Worker.
///
/// Health endpoints are typically unauthenticated - they should work without auth
/// for Kubernetes probes and monitoring systems.
#[derive(Debug)]
pub struct WorkerHealthServiceImpl {
    state: WorkerGrpcState,
}

impl WorkerHealthServiceImpl {
    /// Create a new health service.
    pub fn new(state: WorkerGrpcState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl WorkerHealthServiceTrait for WorkerHealthServiceImpl {
    /// Basic health check - is the service running? (GET /health)
    async fn check_health(
        &self,
        _request: Request<proto::WorkerHealthRequest>,
    ) -> Result<Response<proto::WorkerBasicHealthResponse>, Status> {
        debug!("gRPC worker basic health check");

        let response = self.state.health_service().basic_health();
        Ok(Response::new(proto::WorkerBasicHealthResponse::from(
            response,
        )))
    }

    /// Liveness check - should the service be restarted? (GET /live)
    async fn check_liveness(
        &self,
        _request: Request<proto::WorkerLivenessRequest>,
    ) -> Result<Response<proto::WorkerBasicHealthResponse>, Status> {
        debug!("gRPC worker liveness check");

        let response = self.state.health_service().liveness();
        Ok(Response::new(proto::WorkerBasicHealthResponse::from(
            response,
        )))
    }

    /// Readiness check - is the service ready to accept traffic? (GET /ready)
    async fn check_readiness(
        &self,
        _request: Request<proto::WorkerReadinessRequest>,
    ) -> Result<Response<proto::WorkerReadinessResponse>, Status> {
        debug!("gRPC worker readiness check");

        // Readiness can return error status (503 = unavailable)
        let result = self.state.health_service().readiness().await;

        match result {
            Ok(response) => Ok(Response::new(proto::WorkerReadinessResponse::from(
                response,
            ))),
            Err(_) => {
                // Service is not ready - return unavailable status
                Err(Status::unavailable("Worker is not ready"))
            }
        }
    }

    /// Detailed health check with all subsystem status (GET /health/detailed)
    async fn check_detailed_health(
        &self,
        _request: Request<proto::WorkerDetailedHealthRequest>,
    ) -> Result<Response<proto::WorkerDetailedHealthResponse>, Status> {
        debug!("gRPC worker detailed health check");

        let response = self.state.health_service().detailed_health().await;
        Ok(Response::new(proto::WorkerDetailedHealthResponse::from(
            response,
        )))
    }
}
