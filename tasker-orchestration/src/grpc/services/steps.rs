//! Step service gRPC implementation.
//!
//! Provides workflow step operations via gRPC.

use crate::grpc::conversions::{parse_uuid, step_service_error_to_status};
use crate::grpc::interceptors::AuthInterceptor;
use crate::grpc::state::GrpcState;
use tasker_shared::proto::v1::{
    self as proto, step_service_server::StepService as StepServiceTrait,
};
use tasker_shared::types::api::orchestration::{ManualCompletionData, StepManualAction};
use tasker_shared::types::Permission;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

/// gRPC Step service implementation.
#[derive(Debug)]
pub struct StepServiceImpl {
    state: GrpcState,
    auth_interceptor: AuthInterceptor,
}

impl StepServiceImpl {
    /// Create a new step service.
    pub fn new(state: GrpcState) -> Self {
        let auth_interceptor = AuthInterceptor::new(state.services.security_service.clone());
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
impl StepServiceTrait for StepServiceImpl {
    /// Get a step by ID.
    async fn get_step(
        &self,
        request: Request<proto::GetStepRequest>,
    ) -> Result<Response<proto::GetStepResponse>, Status> {
        // Authenticate and authorize
        self.authenticate_and_authorize(&request, Permission::StepsRead)
            .await?;

        let req = request.into_inner();
        let task_uuid = parse_uuid(&req.task_uuid)?;
        let step_uuid = parse_uuid(&req.step_uuid)?;

        debug!(task_uuid = %task_uuid, step_uuid = %step_uuid, "gRPC get step");

        // Get step via service layer
        let result = self
            .state
            .services
            .step_service
            .get_step(task_uuid, step_uuid)
            .await;

        match result {
            Ok(response) => Ok(Response::new(proto::GetStepResponse {
                step: Some(proto::Step::from(&response)),
            })),
            Err(e) => Err(step_service_error_to_status(&e)),
        }
    }

    /// List steps for a task.
    async fn list_steps(
        &self,
        request: Request<proto::ListStepsRequest>,
    ) -> Result<Response<proto::ListStepsResponse>, Status> {
        // Authenticate and authorize
        self.authenticate_and_authorize(&request, Permission::StepsRead)
            .await?;

        let req = request.into_inner();
        let task_uuid = parse_uuid(&req.task_uuid)?;

        debug!(task_uuid = %task_uuid, "gRPC list steps");

        // List steps via service layer
        let result = self
            .state
            .services
            .step_service
            .list_task_steps(task_uuid)
            .await;

        match result {
            Ok(response) => {
                let steps: Vec<proto::Step> = response.iter().map(proto::Step::from).collect();
                let count = steps.len() as i32;

                Ok(Response::new(proto::ListStepsResponse {
                    steps,
                    pagination: Some(proto::PaginationResponse {
                        total: count as i64,
                        count,
                        offset: 0,
                        has_more: false,
                    }),
                }))
            }
            Err(e) => Err(step_service_error_to_status(&e)),
        }
    }

    /// Manually resolve a step.
    async fn resolve_step(
        &self,
        request: Request<proto::ResolveStepRequest>,
    ) -> Result<Response<proto::ResolveStepResponse>, Status> {
        // Authenticate and authorize
        self.authenticate_and_authorize(&request, Permission::StepsResolve)
            .await?;

        let req = request.into_inner();
        let task_uuid = parse_uuid(&req.task_uuid)?;
        let step_uuid = parse_uuid(&req.step_uuid)?;

        // Convert proto action type to domain StepManualAction
        let action_type = proto::StepManualActionType::try_from(req.action_type)
            .map_err(|_| Status::invalid_argument("Invalid action type"))?;

        let action = match action_type {
            proto::StepManualActionType::ResetForRetry => {
                info!(
                    task_uuid = %task_uuid,
                    step_uuid = %step_uuid,
                    reset_by = %req.performed_by,
                    reason = %req.reason,
                    "gRPC reset step for retry"
                );
                StepManualAction::ResetForRetry {
                    reason: req.reason,
                    reset_by: req.performed_by,
                }
            }
            proto::StepManualActionType::ResolveManually => {
                info!(
                    task_uuid = %task_uuid,
                    step_uuid = %step_uuid,
                    resolved_by = %req.performed_by,
                    reason = %req.reason,
                    "gRPC resolve step manually"
                );
                StepManualAction::ResolveManually {
                    reason: req.reason,
                    resolved_by: req.performed_by,
                }
            }
            proto::StepManualActionType::CompleteManually => {
                let result = req.result.ok_or_else(|| {
                    Status::invalid_argument("result is required for CompleteManually")
                })?;

                info!(
                    task_uuid = %task_uuid,
                    step_uuid = %step_uuid,
                    completed_by = %req.performed_by,
                    reason = %req.reason,
                    "gRPC complete step manually"
                );

                StepManualAction::CompleteManually {
                    completion_data: ManualCompletionData {
                        result: crate::grpc::conversions::struct_to_json(result),
                        metadata: req.metadata.map(crate::grpc::conversions::struct_to_json),
                    },
                    reason: req.reason,
                    completed_by: req.performed_by,
                }
            }
            proto::StepManualActionType::Unspecified => {
                return Err(Status::invalid_argument("Action type must be specified"));
            }
        };

        // Resolve step via service layer
        let result = self
            .state
            .services
            .step_service
            .resolve_step_manually(task_uuid, step_uuid, action)
            .await;

        match result {
            Ok(response) => Ok(Response::new(proto::ResolveStepResponse {
                step: Some(proto::Step::from(&response)),
            })),
            Err(e) => Err(step_service_error_to_status(&e)),
        }
    }

    /// Get step audit history.
    async fn get_step_audit(
        &self,
        request: Request<proto::GetStepAuditRequest>,
    ) -> Result<Response<proto::GetStepAuditResponse>, Status> {
        // Authenticate and authorize
        self.authenticate_and_authorize(&request, Permission::StepsRead)
            .await?;

        let req = request.into_inner();
        let task_uuid = parse_uuid(&req.task_uuid)?;
        let step_uuid = parse_uuid(&req.step_uuid)?;

        debug!(
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            "gRPC get step audit"
        );

        // Get audit history via service layer
        let result = self
            .state
            .services
            .step_service
            .get_step_audit(task_uuid, step_uuid)
            .await;

        match result {
            Ok(response) => {
                let audit_records: Vec<proto::StepAuditRecord> =
                    response.iter().map(proto::StepAuditRecord::from).collect();

                Ok(Response::new(proto::GetStepAuditResponse { audit_records }))
            }
            Err(e) => Err(step_service_error_to_status(&e)),
        }
    }
}
