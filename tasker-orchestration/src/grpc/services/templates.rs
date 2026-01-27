//! Template service gRPC implementation.
//!
//! Provides template discovery operations via gRPC.

use crate::grpc::conversions::json_to_struct;
use crate::grpc::interceptors::AuthInterceptor;
use crate::grpc::state::GrpcState;
use crate::services::TemplateQueryError;
use tasker_shared::proto::v1::{
    self as proto, template_service_server::TemplateService as TemplateServiceTrait,
};
use tasker_shared::types::Permission;
use tonic::{Request, Response, Status};
use tracing::debug;

/// gRPC Template service implementation.
#[derive(Debug)]
pub struct TemplateServiceImpl {
    state: GrpcState,
    auth_interceptor: AuthInterceptor,
}

impl TemplateServiceImpl {
    /// Create a new template service.
    pub fn new(state: GrpcState) -> Self {
        let auth_interceptor = AuthInterceptor::new(state.security_service.clone());
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
            return Err(Status::permission_denied(format!(
                "Permission denied: requires {:?}",
                required_permission
            )));
        }

        Ok(())
    }
}

#[tonic::async_trait]
impl TemplateServiceTrait for TemplateServiceImpl {
    /// List all available templates.
    async fn list_templates(
        &self,
        request: Request<proto::ListTemplatesRequest>,
    ) -> Result<Response<proto::ListTemplatesResponse>, Status> {
        // Authenticate and authorize
        self.authenticate_and_authorize(&request, Permission::TemplatesRead)
            .await?;

        let req = request.into_inner();
        debug!(namespace = ?req.namespace, "gRPC list templates");

        // List templates via service layer
        let result = self
            .state
            .template_query_service
            .list_templates(req.namespace.as_deref())
            .await;

        match result {
            Ok(response) => {
                // Convert namespaces
                let namespaces: Vec<proto::NamespaceSummary> = response
                    .namespaces
                    .into_iter()
                    .map(|ns| proto::NamespaceSummary {
                        name: ns.name,
                        description: ns.description,
                        template_count: ns.template_count as i64,
                    })
                    .collect();

                // Convert templates
                let templates: Vec<proto::TemplateSummary> = response
                    .templates
                    .into_iter()
                    .map(|t| proto::TemplateSummary {
                        name: t.name,
                        namespace: t.namespace,
                        version: t.version,
                        description: t.description,
                        step_count: t.step_count as i64,
                    })
                    .collect();

                Ok(Response::new(proto::ListTemplatesResponse {
                    namespaces,
                    templates,
                    total_count: response.total_count as i64,
                }))
            }
            Err(e) => Err(template_error_to_status(&e)),
        }
    }

    /// Get a specific template by namespace/name/version.
    async fn get_template(
        &self,
        request: Request<proto::GetTemplateRequest>,
    ) -> Result<Response<proto::GetTemplateResponse>, Status> {
        // Authenticate and authorize
        self.authenticate_and_authorize(&request, Permission::TemplatesRead)
            .await?;

        let req = request.into_inner();
        debug!(
            namespace = %req.namespace,
            name = %req.name,
            version = %req.version,
            "gRPC get template"
        );

        // Get template via service layer
        let result = self
            .state
            .template_query_service
            .get_template(&req.namespace, &req.name, &req.version)
            .await;

        match result {
            Ok(detail) => {
                // Convert steps
                let steps: Vec<proto::StepDefinition> = detail
                    .steps
                    .into_iter()
                    .map(|step| proto::StepDefinition {
                        name: step.name,
                        description: step.description,
                        default_retryable: step.default_retryable,
                        default_max_attempts: step.default_max_attempts,
                    })
                    .collect();

                // Convert configuration if present
                let configuration = detail.configuration.and_then(json_to_struct);

                Ok(Response::new(proto::GetTemplateResponse {
                    template: Some(proto::TemplateDetail {
                        name: detail.name,
                        namespace: detail.namespace,
                        version: detail.version,
                        description: detail.description,
                        configuration,
                        steps,
                    }),
                }))
            }
            Err(e) => Err(template_error_to_status(&e)),
        }
    }
}

/// Convert a TemplateQueryError to a gRPC Status.
fn template_error_to_status(error: &TemplateQueryError) -> Status {
    match error {
        TemplateQueryError::NamespaceNotFound(_) | TemplateQueryError::TemplateNotFound { .. } => {
            Status::not_found(error.to_string())
        }
        TemplateQueryError::DatabaseError(_) | TemplateQueryError::Internal(_) => {
            Status::internal(error.to_string())
        }
    }
}
