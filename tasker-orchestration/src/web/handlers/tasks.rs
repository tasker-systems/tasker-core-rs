//! # Task Management Handlers
//!
//! TAS-76: HTTP handlers for task creation, status retrieval, and management operations.
//! These handlers delegate business logic to TaskService, keeping handlers thin.

use axum::extract::{Path, Query, State};
use axum::Json;
use tracing::{debug, info};
use uuid::Uuid;

use crate::services::TaskServiceError;
use crate::web::circuit_breaker::record_backpressure_rejection;
use crate::web::state::AppState;
use tasker_shared::models::core::task::TaskListQuery;
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::types::api::orchestration::{TaskListResponse, TaskResponse};
use tasker_shared::types::web::{ApiError, ApiResult};

/// Create a new task: POST /v1/tasks
///
/// This is the critical endpoint for TAS-40 worker integration.
/// Workers call this endpoint to create tasks and receive UUIDs for tracking.
///
/// Returns the same `TaskResponse` shape as GET /v1/tasks/{uuid}, following REST best
/// practices where create operations return the same representation as read operations.
///
/// **Required Permission:** `tasks:create`
#[cfg_attr(feature = "web-api", utoipa::path(
    post,
    path = "/v1/tasks",
    request_body = TaskRequest,
    responses(
        (status = 201, description = "Task created successfully", body = TaskResponse),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("tasks:create"))
    ),
    tag = "tasks"
))]
pub async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<TaskRequest>,
) -> ApiResult<Json<TaskResponse>> {
    info!(
        namespace = %request.namespace,
        task_name = %request.name,
        version = %request.version,
        initiator = %request.initiator,
        source_system = %request.source_system,
        "Creating new task via web API"
    );

    // TAS-75: Check comprehensive backpressure status before attempting operation
    if let Some(backpressure_error) = state.check_backpressure_status() {
        record_backpressure_rejection("/v1/tasks", &backpressure_error);
        return Err(backpressure_error);
    }

    // Delegate to TaskService
    state
        .task_service
        .create_task(request)
        .await
        .map(|response| {
            state.record_database_success();
            Json(response)
        })
        .map_err(|e| {
            // Only record failure for server errors
            if !e.is_client_error() {
                state.record_database_failure();
            }
            task_service_error_to_api_error(e)
        })
}

/// Get task details: GET /v1/tasks/{uuid}
///
/// Returns comprehensive task information including execution context and step readiness.
/// Used by workers for task correlation and status tracking.
///
/// **Required Permission:** `tasks:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/tasks/{uuid}",
    params(
        ("uuid" = String, Path, description = "Task UUID")
    ),
    responses(
        (status = 200, description = "Task details", body = TaskResponse),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 404, description = "Task not found", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("tasks:read"))
    ),
    tag = "tasks"
))]
pub async fn get_task(
    State(state): State<AppState>,
    Path(task_uuid): Path<String>,
) -> ApiResult<Json<TaskResponse>> {
    debug!(task_uuid = %task_uuid, "Retrieving task details with execution context");

    let uuid = Uuid::parse_str(&task_uuid).map_err(|_| ApiError::invalid_uuid(task_uuid))?;

    state
        .task_service
        .get_task(uuid)
        .await
        .map(Json)
        .map_err(task_service_error_to_api_error)
}

/// List tasks with pagination and filtering: GET /v1/tasks
///
/// Returns a paginated list of tasks with execution context using batch SQL functions.
/// Used by workers and administrators for task monitoring and management.
///
/// **Required Permission:** `tasks:list`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/tasks",
    params(
        ("page" = Option<u32>, Query, description = "Page number (default: 1)"),
        ("per_page" = Option<u32>, Query, description = "Items per page (default: 25, max: 100)"),
        ("namespace" = Option<String>, Query, description = "Filter by namespace"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("initiator" = Option<String>, Query, description = "Filter by initiator"),
        ("source_system" = Option<String>, Query, description = "Filter by source system")
    ),
    responses(
        (status = 200, description = "List of tasks", body = TaskListResponse),
        (status = 400, description = "Invalid query parameters", body = ApiError),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("tasks:list"))
    ),
    tag = "tasks"
))]
pub async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> ApiResult<Json<TaskListResponse>> {
    debug!(?query, "Listing tasks with execution context and filters");

    state
        .task_service
        .list_tasks(query)
        .await
        .map(Json)
        .map_err(task_service_error_to_api_error)
}

/// Cancel a task: DELETE /v1/tasks/{uuid}
///
/// Cancels a task if it's in a cancellable state and triggers orchestration events.
/// Returns the updated task with execution context and step information.
///
/// **Required Permission:** `tasks:cancel`
#[cfg_attr(feature = "web-api", utoipa::path(
    delete,
    path = "/v1/tasks/{uuid}",
    params(
        ("uuid" = String, Path, description = "Task UUID")
    ),
    responses(
        (status = 200, description = "Task cancelled successfully", body = TaskResponse),
        (status = 400, description = "Task cannot be cancelled", body = ApiError),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 404, description = "Task not found", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("tasks:cancel"))
    ),
    tag = "tasks"
))]
pub async fn cancel_task(
    State(state): State<AppState>,
    Path(task_uuid): Path<String>,
) -> ApiResult<Json<TaskResponse>> {
    info!(task_uuid = %task_uuid, "Cancelling task via web API");

    let uuid = Uuid::parse_str(&task_uuid).map_err(|_| ApiError::invalid_uuid(task_uuid))?;

    state
        .task_service
        .cancel_task(uuid)
        .await
        .map(Json)
        .map_err(task_service_error_to_api_error)
}

/// Convert TaskServiceError to ApiError.
fn task_service_error_to_api_error(err: TaskServiceError) -> ApiError {
    match err {
        TaskServiceError::Validation(msg) => ApiError::bad_request(msg),
        TaskServiceError::NotFound(_) => ApiError::not_found("Resource not found"),
        TaskServiceError::TemplateNotFound(msg) => {
            ApiError::not_found(format!("Task template not found: {}", msg))
        }
        TaskServiceError::InvalidConfiguration(msg) => {
            ApiError::bad_request(format!("Invalid task configuration: {}", msg))
        }
        TaskServiceError::DuplicateTask(msg) => ApiError::conflict(msg),
        TaskServiceError::CannotCancel(msg) => ApiError::bad_request(msg),
        TaskServiceError::Backpressure {
            reason,
            retry_after_seconds,
        } => ApiError::backpressure(reason, retry_after_seconds),
        TaskServiceError::CircuitBreakerOpen => ApiError::ServiceUnavailable,
        TaskServiceError::Database(msg) => ApiError::database_error(msg),
        TaskServiceError::Internal(msg) => ApiError::internal_server_error(msg),
    }
}
