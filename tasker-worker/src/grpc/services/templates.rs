//! Template service gRPC implementation for Worker.
//!
//! Provides template discovery operations via gRPC.
//! Matches REST API template endpoints.
//!
//! Uses worker-specific proto types that match the worker REST API exactly.
//! Conversions use `From`/`Into` traits defined in `tasker-shared/src/proto/conversions.rs`.

use crate::grpc::state::WorkerGrpcState;
use crate::worker::services::TemplateQueryError;
use tasker_shared::proto::v1::{
    self as proto,
    worker_template_service_server::WorkerTemplateService as WorkerTemplateServiceTrait,
};
use tonic::{Request, Response, Status};
use tracing::debug;

/// gRPC Template service implementation for Worker.
#[derive(Debug)]
pub struct WorkerTemplateServiceImpl {
    state: WorkerGrpcState,
}

impl WorkerTemplateServiceImpl {
    /// Create a new template service.
    pub fn new(state: WorkerGrpcState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl WorkerTemplateServiceTrait for WorkerTemplateServiceImpl {
    /// List all available templates.
    async fn list_templates(
        &self,
        request: Request<proto::WorkerListTemplatesRequest>,
    ) -> Result<Response<proto::WorkerTemplateListResponse>, Status> {
        debug!("gRPC worker list templates");

        let req = request.into_inner();

        // Get templates from service - include cache stats as requested
        let response = self
            .state
            .template_query_service()
            .list_templates(req.include_cache_stats)
            .await;

        // Convert using From trait - direct 1:1 mapping
        Ok(Response::new(proto::WorkerTemplateListResponse::from(
            response,
        )))
    }

    /// Get a specific template by namespace/name/version.
    async fn get_template(
        &self,
        request: Request<proto::WorkerGetTemplateRequest>,
    ) -> Result<Response<proto::WorkerTemplateResponse>, Status> {
        let req = request.into_inner();
        debug!(
            namespace = %req.namespace,
            name = %req.name,
            version = %req.version,
            "gRPC worker get template"
        );

        // Get template from service
        let response = self
            .state
            .template_query_service()
            .get_template(&req.namespace, &req.name, &req.version)
            .await
            .map_err(template_error_to_status)?;

        // Convert using From trait - direct 1:1 mapping
        Ok(Response::new(proto::WorkerTemplateResponse::from(response)))
    }
}

/// Convert TemplateQueryError to gRPC Status.
fn template_error_to_status(error: TemplateQueryError) -> Status {
    match error {
        TemplateQueryError::NotFound {
            namespace,
            name,
            version,
        } => Status::not_found(format!(
            "Template not found: {}/{}/{}",
            namespace, name, version
        )),
        TemplateQueryError::HandlerMetadataNotFound {
            namespace,
            name,
            version,
        } => Status::not_found(format!(
            "Handler metadata not found: {}/{}/{}",
            namespace, name, version
        )),
        TemplateQueryError::Internal(msg) => Status::internal(msg),
    }
}
