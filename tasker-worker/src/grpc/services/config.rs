//! Config service gRPC implementation for Worker.
//!
//! Provides safe runtime configuration via gRPC.
//! Matches REST API config endpoint (GET /config).
//!
//! Uses worker-specific proto types that match the worker REST API exactly.
//! Conversions use `From`/`Into` traits defined in `tasker-shared/src/proto/conversions.rs`.
//!
//! NOTE: Only safe, whitelisted configuration is returned - no secrets,
//! credentials, keys, or database URLs.

use crate::grpc::state::WorkerGrpcState;
use crate::worker::services::ConfigQueryError;
use tasker_shared::proto::v1::{
    self as proto, worker_config_service_server::WorkerConfigService as WorkerConfigServiceTrait,
};
use tonic::{Request, Response, Status};
use tracing::debug;

/// gRPC Config service implementation for Worker.
#[derive(Debug)]
pub struct WorkerConfigServiceImpl {
    state: WorkerGrpcState,
}

impl WorkerConfigServiceImpl {
    /// Create a new config service.
    pub fn new(state: WorkerGrpcState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl WorkerConfigServiceTrait for WorkerConfigServiceImpl {
    /// Get current configuration (safe fields only).
    async fn get_config(
        &self,
        _request: Request<proto::WorkerGetConfigRequest>,
    ) -> Result<Response<proto::WorkerGetConfigResponse>, Status> {
        debug!("gRPC worker get config");

        // Get config from service
        let response = self
            .state
            .config_query_service()
            .runtime_config()
            .map_err(config_error_to_status)?;

        // Convert using From trait - no hardcoded values, just direct 1:1 mapping
        Ok(Response::new(proto::WorkerGetConfigResponse::from(
            response,
        )))
    }
}

/// Convert ConfigQueryError to gRPC Status.
fn config_error_to_status(error: ConfigQueryError) -> Status {
    match error {
        ConfigQueryError::WorkerConfigNotFound => {
            Status::not_found("Worker configuration not found")
        }
        ConfigQueryError::SerializationError(msg) => {
            Status::internal(format!("Failed to serialize configuration: {}", msg))
        }
    }
}
