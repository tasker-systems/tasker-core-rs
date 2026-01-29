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

use crate::grpc::interceptors::AuthInterceptor;
use crate::grpc::state::WorkerGrpcState;
use crate::worker::services::ConfigQueryError;
use tasker_shared::proto::v1::{
    self as proto, worker_config_service_server::WorkerConfigService as WorkerConfigServiceTrait,
};
use tasker_shared::types::Permission;
use tonic::{Request, Response, Status};
use tracing::debug;

/// gRPC Config service implementation for Worker.
#[derive(Debug)]
pub struct WorkerConfigServiceImpl {
    state: WorkerGrpcState,
    auth_interceptor: AuthInterceptor,
}

impl WorkerConfigServiceImpl {
    /// Create a new config service.
    pub fn new(state: WorkerGrpcState) -> Self {
        let auth_interceptor = AuthInterceptor::new(state.security_service().cloned());
        Self {
            state,
            auth_interceptor,
        }
    }

    /// Authenticate the request and check permissions.
    async fn authenticate_and_authorize<T>(
        &self,
        request: &Request<T>,
        required_permission: Permission,
    ) -> Result<(), Status> {
        let ctx = self.auth_interceptor.authenticate(request).await?;

        // Check permission
        if !ctx.has_permission(&required_permission) {
            return Err(Status::permission_denied(
                "Insufficient permissions for this operation",
            ));
        }

        Ok(())
    }
}

#[tonic::async_trait]
impl WorkerConfigServiceTrait for WorkerConfigServiceImpl {
    /// Get current configuration (safe fields only).
    async fn get_config(
        &self,
        request: Request<proto::WorkerGetConfigRequest>,
    ) -> Result<Response<proto::WorkerGetConfigResponse>, Status> {
        // Authenticate and authorize
        self.authenticate_and_authorize(&request, Permission::WorkerConfigRead)
            .await?;

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
