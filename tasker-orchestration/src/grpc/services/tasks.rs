//! Task service gRPC implementation.
//!
//! Provides task management operations via gRPC.

use crate::grpc::conversions::{
    datetime_to_timestamp, json_to_struct, parse_uuid, proto_to_task_state,
    task_service_error_to_status, task_state_to_proto,
};
use crate::grpc::interceptors::AuthInterceptor;
use crate::grpc::state::GrpcState;
use crate::services::TaskServiceError;
use tasker_shared::models::core::task::TaskListQuery;
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::proto::v1::{
    self as proto, task_service_server::TaskService as TaskServiceTrait,
};
use tasker_shared::types::{Permission, SecurityContext};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};

/// gRPC Task service implementation.
#[derive(Debug)]
pub struct TaskServiceImpl {
    state: GrpcState,
    auth_interceptor: AuthInterceptor,
}

impl TaskServiceImpl {
    /// Create a new task service.
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
    ) -> Result<SecurityContext, Status> {
        let ctx = self.auth_interceptor.authenticate(request).await?;

        // Check permission
        if !ctx.has_permission(&required_permission) {
            return Err(Status::permission_denied(format!(
                "Permission denied: requires {:?}",
                required_permission
            )));
        }

        Ok(ctx)
    }

    /// Check backpressure and return error if active.
    fn check_backpressure(&self) -> Result<(), Status> {
        if let Some(reason) = self.state.check_backpressure() {
            let mut status = Status::unavailable(format!("Backpressure active: {reason}"));
            status
                .metadata_mut()
                .insert("retry-after", "5".parse().unwrap());
            return Err(status);
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl TaskServiceTrait for TaskServiceImpl {
    /// Create a new task.
    async fn create_task(
        &self,
        request: Request<proto::CreateTaskRequest>,
    ) -> Result<Response<proto::CreateTaskResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::TasksCreate)
            .await?;

        // Check backpressure
        self.check_backpressure()?;

        let req = request.into_inner();
        debug!(
            name = %req.name,
            namespace = %req.namespace,
            version = %req.version,
            "gRPC create task"
        );

        // Convert proto request to domain request using builder pattern
        let context = req
            .context
            .map(crate::grpc::conversions::struct_to_json)
            .unwrap_or_else(|| serde_json::json!({}));

        let task_request = TaskRequest::builder()
            .name(req.name)
            .namespace(req.namespace)
            .version(req.version)
            .context(context)
            .initiator(req.initiator.unwrap_or_else(|| "grpc".to_string()))
            .reason(
                req.reason
                    .unwrap_or_else(|| "Task created via gRPC".to_string()),
            )
            .tags(req.tags)
            .build();

        // Create task via service layer - returns full TaskResponse (same as GET)
        let result = self
            .state
            .services
            .task_service
            .create_task(task_request)
            .await;

        match result {
            Ok(response) => {
                info!(task_uuid = %response.task_uuid, "Task created via gRPC");
                Ok(Response::new(proto::CreateTaskResponse {
                    task: Some(proto::Task::from(&response)),
                    backpressure: None,
                }))
            }
            Err(e) => Err(task_service_error_to_status(&e)),
        }
    }

    /// Get a task by ID.
    async fn get_task(
        &self,
        request: Request<proto::GetTaskRequest>,
    ) -> Result<Response<proto::GetTaskResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::TasksRead)
            .await?;

        let req = request.into_inner();
        let task_id = parse_uuid(&req.task_uuid)?;

        debug!(task_id = %task_id, "gRPC get task");

        // Get task via service layer
        let result = self.state.services.task_service.get_task(task_id).await;

        match result {
            Ok(response) => {
                // Get context if requested
                let context = if req.include_context {
                    Some(proto::TaskContext {
                        inputs: json_to_struct(response.context.clone()),
                        outputs: json_to_struct(serde_json::json!({})),
                        merged: json_to_struct(response.context.clone()),
                    })
                } else {
                    None
                };

                Ok(Response::new(proto::GetTaskResponse {
                    task: Some(proto::Task::from(&response)),
                    steps: vec![], // TODO: Include steps if req.include_steps
                    context,
                }))
            }
            Err(e) => Err(task_service_error_to_status(&e)),
        }
    }

    /// List tasks with filtering and pagination.
    async fn list_tasks(
        &self,
        request: Request<proto::ListTasksRequest>,
    ) -> Result<Response<proto::ListTasksResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::TasksList)
            .await?;

        let req = request.into_inner();
        debug!("gRPC list tasks");

        // Build query parameters - TaskListQuery uses page/per_page
        let page = req
            .pagination
            .as_ref()
            .and_then(|p| p.offset)
            .map(|o| (o / req.pagination.as_ref().and_then(|p| p.limit).unwrap_or(50) + 1) as u32)
            .unwrap_or(1);
        let per_page = req
            .pagination
            .as_ref()
            .and_then(|p| p.limit)
            .map(|l| l as u32)
            .unwrap_or(50);

        // Convert proto states to domain state strings for filtering
        let status = if !req.states.is_empty() {
            // Take the first state for status filtering (API supports single status filter)
            req.states
                .first()
                .and_then(|s| proto::TaskState::try_from(*s).ok().map(proto_to_task_state))
        } else {
            None
        };

        let query = TaskListQuery {
            page,
            per_page,
            namespace: req.namespace,
            status,
            initiator: None,
            source_system: None,
        };

        // List tasks via service layer
        let result = self.state.services.task_service.list_tasks(query).await;

        match result {
            Ok(response) => {
                let tasks: Vec<proto::Task> =
                    response.tasks.iter().map(proto::Task::from).collect();
                let count = tasks.len() as i32;
                let total = response.pagination.total_count as i64;
                let offset = ((response.pagination.page - 1) * response.pagination.per_page) as i32;

                Ok(Response::new(proto::ListTasksResponse {
                    tasks,
                    pagination: Some(proto::PaginationResponse {
                        total,
                        count,
                        offset,
                        has_more: response.pagination.has_next,
                    }),
                }))
            }
            Err(e) => Err(task_service_error_to_status(&e)),
        }
    }

    /// Cancel a running task.
    async fn cancel_task(
        &self,
        request: Request<proto::CancelTaskRequest>,
    ) -> Result<Response<proto::CancelTaskResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::TasksCancel)
            .await?;

        let req = request.into_inner();
        let task_id = parse_uuid(&req.task_uuid)?;

        info!(task_id = %task_id, reason = ?req.reason, "gRPC cancel task");

        // Cancel task via service layer (reason is logged but not passed to service)
        let result = self.state.services.task_service.cancel_task(task_id).await;

        match result {
            Ok(response) => Ok(Response::new(proto::CancelTaskResponse {
                task: Some(proto::Task::from(&response)),
                success: true,
                message: None,
            })),
            Err(e) => {
                // Return a response indicating failure for expected errors
                if matches!(&e, TaskServiceError::CannotCancel(_)) {
                    Ok(Response::new(proto::CancelTaskResponse {
                        task: None,
                        success: false,
                        message: Some(e.to_string()),
                    }))
                } else {
                    Err(task_service_error_to_status(&e))
                }
            }
        }
    }

    /// Get task execution context.
    async fn get_task_context(
        &self,
        request: Request<proto::GetTaskContextRequest>,
    ) -> Result<Response<proto::GetTaskContextResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::TasksContextRead)
            .await?;

        let req = request.into_inner();
        let task_id = parse_uuid(&req.task_uuid)?;

        debug!(task_id = %task_id, "gRPC get task context");

        // Get task to get context
        let result = self.state.services.task_service.get_task(task_id).await;

        match result {
            Ok(response) => Ok(Response::new(proto::GetTaskContextResponse {
                context: Some(proto::TaskContext {
                    inputs: json_to_struct(response.context.clone()),
                    outputs: json_to_struct(serde_json::json!({})),
                    merged: json_to_struct(response.context),
                }),
            })),
            Err(e) => Err(task_service_error_to_status(&e)),
        }
    }

    /// Stream task status updates.
    type StreamTaskStatusStream = ReceiverStream<Result<proto::TaskStatusUpdate, Status>>;

    async fn stream_task_status(
        &self,
        request: Request<proto::StreamTaskStatusRequest>,
    ) -> Result<Response<Self::StreamTaskStatusStream>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::TasksRead)
            .await?;

        let req = request.into_inner();
        let task_id = parse_uuid(&req.task_uuid)?;

        info!(task_id = %task_id, "Starting task status stream");

        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let state = self.state.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            let mut last_state: Option<String> = None;

            loop {
                interval.tick().await;

                // Get current task state
                let result = state.services.task_service.get_task(task_id).await;

                match result {
                    Ok(response) => {
                        let current_state = response.status.clone();

                        // Only send update if state changed
                        if last_state.as_ref() != Some(&current_state) {
                            let proto_state = task_state_to_proto(&current_state);

                            let update = proto::TaskStatusUpdate {
                                update_type: proto::task_status_update::UpdateType::TaskStateChange
                                    as i32,
                                timestamp: Some(datetime_to_timestamp(chrono::Utc::now())),
                                task_state: Some(proto_state as i32),
                                step_update: None,
                                error_message: None,
                            };

                            if tx.send(Ok(update)).await.is_err() {
                                // Client disconnected
                                break;
                            }

                            last_state = Some(current_state.clone());

                            // Stop streaming if task is complete or error
                            if current_state == "complete"
                                || current_state == "error"
                                || current_state == "cancelled"
                            {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Error getting task for status stream");
                        let _ = tx.send(Err(task_service_error_to_status(&e))).await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
