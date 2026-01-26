//! # Workflow Step Handlers
//!
//! TAS-76: HTTP handlers for workflow step operations including status retrieval
//! and manual resolution for failed or stuck steps.
//! These handlers delegate business logic to StepService, keeping handlers thin.

use axum::extract::{Path, State};
use axum::Json;
use tracing::info;
use uuid::Uuid;

use crate::services::StepServiceError;
use crate::web::state::AppState;
use tasker_shared::types::api::orchestration::{StepAuditResponse, StepManualAction, StepResponse};
use tasker_shared::types::web::{ApiError, ApiResult};

/// List workflow steps for a task: GET /v1/tasks/{uuid}/workflow_steps
///
/// **Required Permission:** `steps:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/tasks/{uuid}/workflow_steps",
    params(
        ("uuid" = String, Path, description = "Task UUID")
    ),
    responses(
        (status = 200, description = "List of workflow steps", body = Vec<StepResponse>),
        (status = 400, description = "Invalid task UUID", body = ApiError),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("steps:read"))
    ),
    tag = "workflow_steps"
))]
pub async fn list_task_steps(
    State(state): State<AppState>,
    Path(task_uuid): Path<String>,
) -> ApiResult<Json<Vec<StepResponse>>> {
    info!(task_uuid = %task_uuid, "Listing workflow steps for task");

    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::bad_request("Invalid task UUID format"))?;

    state
        .step_service
        .list_task_steps(task_uuid)
        .await
        .map(Json)
        .map_err(step_service_error_to_api_error)
}

/// Get workflow step details: GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}
///
/// **Required Permission:** `steps:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/tasks/{uuid}/workflow_steps/{step_uuid}",
    params(
        ("uuid" = String, Path, description = "Task UUID"),
        ("step_uuid" = String, Path, description = "Step UUID")
    ),
    responses(
        (status = 200, description = "Workflow step details", body = StepResponse),
        (status = 400, description = "Invalid UUID", body = ApiError),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 404, description = "Step not found", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("steps:read"))
    ),
    tag = "workflow_steps"
))]
pub async fn get_step(
    State(state): State<AppState>,
    Path((task_uuid, step_uuid)): Path<(String, String)>,
) -> ApiResult<Json<StepResponse>> {
    info!(task_uuid = %task_uuid, step_uuid = %step_uuid, "Getting workflow step details");

    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::bad_request("Invalid task UUID format"))?;
    let step_uuid = Uuid::parse_str(&step_uuid)
        .map_err(|_| ApiError::bad_request("Invalid step UUID format"))?;

    state
        .step_service
        .get_step(task_uuid, step_uuid)
        .await
        .map(Json)
        .map_err(step_service_error_to_api_error)
}

/// Manually resolve or complete a workflow step: PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}
///
/// This endpoint supports two resolution modes:
///
/// 1. **Simple Resolution** (`ResolveManually`): Marks the step as `resolved_manually` without providing execution results.
///    Use this when you want to acknowledge a step issue without continuing workflow execution.
///
/// 2. **Complete with Results** (`CompleteManually`): Provides execution results and transitions the step to `complete` state.
///    - Operators provide only `result` (business data) and optional `metadata`
///    - System automatically enforces `success: true` and `status: "completed"`
///    - Prevents operators from incorrectly setting `status` or `error` fields
///    - Dependent steps can consume the provided results, allowing workflow continuation
///
/// You can optionally reset the attempt counter when resolving a step that has exceeded retry limits.
///
/// **Required Permission:** `steps:resolve`
#[cfg_attr(feature = "web-api", utoipa::path(
    patch,
    path = "/v1/tasks/{uuid}/workflow_steps/{step_uuid}",
    params(
        ("uuid" = String, Path, description = "Task UUID"),
        ("step_uuid" = String, Path, description = "Step UUID")
    ),
    request_body = StepManualAction,
    responses(
        (status = 200, description = "Step resolved successfully. The response indicates whether the step was marked as 'resolved_manually' (simple resolution) or 'complete' (with execution results).", body = StepResponse),
        (status = 400, description = "Invalid request or step cannot be resolved", body = ApiError),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 404, description = "Step not found", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("steps:resolve"))
    ),
    tag = "workflow_steps"
))]
pub async fn resolve_step_manually(
    State(state): State<AppState>,
    Path((task_uuid, step_uuid)): Path<(String, String)>,
    Json(action): Json<StepManualAction>,
) -> ApiResult<Json<StepResponse>> {
    // Log the action with operator details
    match &action {
        StepManualAction::ResetForRetry { reset_by, reason } => {
            info!(
                task_uuid = %task_uuid,
                step_uuid = %step_uuid,
                reset_by = %reset_by,
                reason = %reason,
                "Resetting step for retry"
            );
        }
        StepManualAction::ResolveManually {
            resolved_by,
            reason,
        } => {
            info!(
                task_uuid = %task_uuid,
                step_uuid = %step_uuid,
                resolved_by = %resolved_by,
                reason = %reason,
                "Manually resolving step"
            );
        }
        StepManualAction::CompleteManually {
            completed_by,
            reason,
            ..
        } => {
            info!(
                task_uuid = %task_uuid,
                step_uuid = %step_uuid,
                completed_by = %completed_by,
                reason = %reason,
                "Manually completing step with results"
            );
        }
    }

    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::bad_request("Invalid task UUID format"))?;
    let step_uuid = Uuid::parse_str(&step_uuid)
        .map_err(|_| ApiError::bad_request("Invalid step UUID format"))?;

    state
        .step_service
        .resolve_step_manually(task_uuid, step_uuid, action)
        .await
        .map(Json)
        .map_err(step_service_error_to_api_error)
}

/// Get audit history for a workflow step: GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}/audit
///
/// Returns SOC2-compliant audit trail for step execution results (TAS-62).
/// Each audit record includes:
/// - Worker attribution (worker_uuid, correlation_id)
/// - Execution summary (success, execution_time_ms)
/// - Full execution result via JOIN to transition metadata
///
/// ## Response
///
/// Returns an array of audit records ordered by recorded_at DESC (most recent first).
/// Each record links to the transition that captured the execution result.
///
/// **Required Permission:** `steps:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/tasks/{uuid}/workflow_steps/{step_uuid}/audit",
    params(
        ("uuid" = String, Path, description = "Task UUID"),
        ("step_uuid" = String, Path, description = "Step UUID")
    ),
    responses(
        (status = 200, description = "Audit history for the step", body = Vec<StepAuditResponse>),
        (status = 400, description = "Invalid UUID format", body = ApiError),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 404, description = "Step not found or does not belong to task", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("steps:read"))
    ),
    tag = "workflow_steps"
))]
pub async fn get_step_audit(
    State(state): State<AppState>,
    Path((task_uuid, step_uuid)): Path<(String, String)>,
) -> ApiResult<Json<Vec<StepAuditResponse>>> {
    info!(
        task_uuid = %task_uuid,
        step_uuid = %step_uuid,
        "Getting audit history for workflow step"
    );

    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::bad_request("Invalid task UUID format"))?;
    let step_uuid = Uuid::parse_str(&step_uuid)
        .map_err(|_| ApiError::bad_request("Invalid step UUID format"))?;

    state
        .step_service
        .get_step_audit(task_uuid, step_uuid)
        .await
        .map(Json)
        .map_err(step_service_error_to_api_error)
}

/// Convert StepServiceError to ApiError.
fn step_service_error_to_api_error(err: StepServiceError) -> ApiError {
    match err {
        StepServiceError::Validation(msg) => ApiError::bad_request(msg),
        StepServiceError::NotFound(_) => ApiError::not_found("Resource not found"),
        StepServiceError::OwnershipMismatch => {
            ApiError::not_found("Step does not belong to the specified task")
        }
        StepServiceError::InvalidTransition(msg) => ApiError::bad_request(msg),
        StepServiceError::Database(msg) => ApiError::database_error(msg),
        StepServiceError::Internal(msg) => ApiError::internal_server_error(msg),
    }
}
