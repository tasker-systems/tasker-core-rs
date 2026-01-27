//! Health service gRPC implementation.
//!
//! Provides Kubernetes-compatible health check operations via gRPC.
//! Matches REST API health endpoints (GET /health, /live, /ready, /health/detailed).

use crate::grpc::state::GrpcState;
use tasker_shared::proto::v1::{
    self as proto, health_service_server::HealthService as HealthServiceTrait,
};
use tonic::{Request, Response, Status};
use tracing::debug;

/// gRPC Health service implementation.
///
/// Health endpoints are typically unauthenticated - they should work without auth
/// for Kubernetes probes and monitoring systems.
#[derive(Debug)]
pub struct HealthServiceImpl {
    state: GrpcState,
}

impl HealthServiceImpl {
    /// Create a new health service.
    pub fn new(state: GrpcState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl HealthServiceTrait for HealthServiceImpl {
    /// Basic health check - is the service running? (GET /health)
    async fn check_health(
        &self,
        _request: Request<proto::HealthRequest>,
    ) -> Result<Response<proto::HealthResponse>, Status> {
        debug!("gRPC basic health check");

        let response = self.state.health_service.basic_health();
        Ok(Response::new(proto::HealthResponse::from(&response)))
    }

    /// Liveness check - should the service be restarted? (GET /live)
    async fn check_liveness(
        &self,
        _request: Request<proto::LivenessRequest>,
    ) -> Result<Response<proto::HealthResponse>, Status> {
        debug!("gRPC liveness check");

        let response = self.state.health_service.liveness().await;
        Ok(Response::new(proto::HealthResponse::from(&response)))
    }

    /// Readiness check - is the service ready to accept traffic? (GET /ready)
    async fn check_readiness(
        &self,
        _request: Request<proto::ReadinessRequest>,
    ) -> Result<Response<proto::ReadinessResponse>, Status> {
        debug!("gRPC readiness check");

        // Readiness can return error status (503 = unavailable)
        let result = self.state.health_service.readiness().await;

        match result {
            Ok(response) => Ok(Response::new(proto::ReadinessResponse::from(&response))),
            Err(_) => {
                // Service is not ready - return unavailable status
                Err(Status::unavailable("Service is not ready"))
            }
        }
    }

    /// Detailed health check with all subsystem status (GET /health/detailed)
    async fn check_detailed_health(
        &self,
        _request: Request<proto::DetailedHealthRequest>,
    ) -> Result<Response<proto::DetailedHealthResponse>, Status> {
        debug!("gRPC detailed health check");

        let response = self.state.health_service.detailed_health().await;
        Ok(Response::new(proto::DetailedHealthResponse::from(
            &response,
        )))
    }
}
