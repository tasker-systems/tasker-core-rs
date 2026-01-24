//! # Workflow Step Handlers
//!
//! HTTP handlers for workflow step operations including status retrieval
//! and manual resolution for failed or stuck steps.
//!
//! This module follows domain-driven design principles by delegating business logic
//! to the WorkflowStep domain model and StepStateMachine instead of using direct SQL.

use axum::extract::{Path, State};
use axum::Json;
use std::collections::HashMap;
use tracing::{error, info};
use uuid::Uuid;

use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::messaging::execution_types::StepExecutionResult;
use tasker_shared::models::core::workflow_step::WorkflowStep;

// StepReadinessStatus is used through SqlFunctionExecutor, removing unused direct import
use crate::web::circuit_breaker::execute_with_circuit_breaker;
use crate::web::middleware::permission::require_permission;
use crate::web::state::AppState;
use tasker_shared::models::core::workflow_step_result_audit::WorkflowStepResultAudit;
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::state_machine::StateMachineError;
use tasker_shared::types::api::orchestration::{StepAuditResponse, StepManualAction, StepResponse};
use tasker_shared::types::permissions::Permission;
use tasker_shared::types::security::SecurityContext;
use tasker_shared::types::web::{ApiError, ApiResult, DbOperationType};

/// List workflow steps for a task: GET /v1/tasks/{uuid}/workflow_steps
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
    tag = "workflow_steps"
))]
pub async fn list_task_steps(
    State(state): State<AppState>,
    security: SecurityContext,
    Path(task_uuid): Path<String>,
) -> ApiResult<Json<Vec<StepResponse>>> {
    require_permission(&security, Permission::StepsRead)?;

    info!(task_uuid = %task_uuid, "Listing workflow steps for task");

    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::bad_request("Invalid task UUID format"))?;

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::ReadOnly);

        // Use WorkflowStep domain model to get step data
        let workflow_steps = WorkflowStep::for_task(db_pool, task_uuid).await?;

        if workflow_steps.is_empty() {
            return Ok::<Json<Vec<StepResponse>>, sqlx::Error>(Json(vec![]));
        }

        // Get step readiness status using SQL function integration
        let sql_executor = SqlFunctionExecutor::new(db_pool.clone());
        let readiness_statuses = sql_executor
            .get_step_readiness_status(task_uuid, None)
            .await?;

        // Build comprehensive response with step data and readiness information
        let mut step_responses = Vec::new();

        for step in workflow_steps {
            // Find matching readiness status
            let readiness = readiness_statuses
                .iter()
                .find(|r| r.workflow_step_uuid == step.workflow_step_uuid)
                .cloned();

            let step_response = if let Some(readiness) = readiness {
                StepResponse {
                    step_uuid: step.workflow_step_uuid.to_string(),
                    task_uuid: step.task_uuid.to_string(),
                    name: readiness.name.clone(),
                    created_at: step.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                    updated_at: step.updated_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                    completed_at: step
                        .processed_at
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                    results: step.results,

                    // StepReadinessStatus fields
                    current_state: readiness.current_state,
                    dependencies_satisfied: readiness.dependencies_satisfied,
                    retry_eligible: readiness.retry_eligible,
                    ready_for_execution: readiness.ready_for_execution,
                    total_parents: readiness.total_parents,
                    completed_parents: readiness.completed_parents,
                    attempts: readiness.attempts,
                    max_attempts: readiness.max_attempts,
                    last_failure_at: readiness
                        .last_failure_at
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                    next_retry_at: readiness
                        .next_retry_at
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                    last_attempted_at: readiness
                        .last_attempted_at
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                }
            } else {
                // Fallback when readiness data is not available
                StepResponse {
                    step_uuid: step.workflow_step_uuid.to_string(),
                    task_uuid: step.task_uuid.to_string(),
                    name: format!("step_{}", step.workflow_step_uuid),
                    created_at: step.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                    updated_at: step.updated_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                    completed_at: step
                        .processed_at
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                    results: step.results,

                    // Default readiness fields when status unavailable
                    current_state: "unknown".to_string(),
                    dependencies_satisfied: false,
                    retry_eligible: false,
                    ready_for_execution: false,
                    total_parents: 0,
                    completed_parents: 0,
                    attempts: step.attempts.unwrap_or(0),
                    max_attempts: step.max_attempts.unwrap_or(3),
                    last_failure_at: None,
                    next_retry_at: None,
                    last_attempted_at: step
                        .last_attempted_at
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                }
            };

            step_responses.push(step_response);
        }

        // Sort by creation order
        step_responses.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok::<Json<Vec<StepResponse>>, sqlx::Error>(Json(step_responses))
    })
    .await
}

/// Get workflow step details: GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}
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
    tag = "workflow_steps"
))]
pub async fn get_step(
    State(state): State<AppState>,
    security: SecurityContext,
    Path((task_uuid, step_uuid)): Path<(String, String)>,
) -> ApiResult<Json<StepResponse>> {
    require_permission(&security, Permission::StepsRead)?;

    info!(task_uuid = %task_uuid, step_uuid = %step_uuid, "Getting workflow step details");

    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::bad_request("Invalid task UUID format"))?;
    let step_uuid = Uuid::parse_str(&step_uuid)
        .map_err(|_| ApiError::bad_request("Invalid step UUID format"))?;

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::ReadOnly);

        // Use WorkflowStep domain model to find the step
        let workflow_step = WorkflowStep::find_by_id(db_pool, step_uuid)
            .await
            .map_err(ApiError::from)?;

        let step = match workflow_step {
            Some(step) => {
                // Verify the step belongs to the specified task
                if step.task_uuid != task_uuid {
                    return Err(ApiError::not_found("Resource not found"));
                }
                step
            }
            None => {
                return Err(ApiError::not_found("Resource not found"));
            }
        };

        // Get step readiness status using SQL function integration
        let sql_executor = SqlFunctionExecutor::new(db_pool.clone());
        let readiness_statuses = sql_executor
            .get_step_readiness_status(task_uuid, Some(vec![step_uuid]))
            .await
            .map_err(ApiError::from)?;

        // Build comprehensive response with step data and readiness information
        let step_response = if let Some(readiness) = readiness_statuses.first() {
            StepResponse {
                step_uuid: step.workflow_step_uuid.to_string(),
                task_uuid: step.task_uuid.to_string(),
                name: readiness.name.clone(),
                created_at: step.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                updated_at: step.updated_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                completed_at: step
                    .processed_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                results: step.results,

                // StepReadinessStatus fields
                current_state: readiness.current_state.clone(),
                dependencies_satisfied: readiness.dependencies_satisfied,
                retry_eligible: readiness.retry_eligible,
                ready_for_execution: readiness.ready_for_execution,
                total_parents: readiness.total_parents,
                completed_parents: readiness.completed_parents,
                attempts: readiness.attempts,
                max_attempts: readiness.max_attempts,
                last_failure_at: readiness
                    .last_failure_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                next_retry_at: readiness
                    .next_retry_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                last_attempted_at: readiness
                    .last_attempted_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            }
        } else {
            // Fallback when readiness data is not available
            StepResponse {
                step_uuid: step.workflow_step_uuid.to_string(),
                task_uuid: step.task_uuid.to_string(),
                name: format!("step_{}", step.workflow_step_uuid),
                created_at: step.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                updated_at: step.updated_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                completed_at: step
                    .processed_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                results: step.results,

                // Default readiness fields when status unavailable
                current_state: "unknown".to_string(),
                dependencies_satisfied: false,
                retry_eligible: false,
                ready_for_execution: false,
                total_parents: 0,
                completed_parents: 0,
                attempts: step.attempts.unwrap_or(0),
                max_attempts: step.max_attempts.unwrap_or(3),
                last_failure_at: None,
                next_retry_at: None,
                last_attempted_at: step
                    .last_attempted_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            }
        };

        Ok::<Json<StepResponse>, ApiError>(Json(step_response))
    })
    .await
    .map_err(|e| {
        let error_str = e.to_string();
        if error_str.contains("Step not found") {
            ApiError::not_found("Resource not found")
        } else {
            e
        }
    })
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
    tag = "workflow_steps"
))]
pub async fn resolve_step_manually(
    State(state): State<AppState>,
    security: SecurityContext,
    Path((task_uuid, step_uuid)): Path<(String, String)>,
    Json(action): Json<StepManualAction>,
) -> ApiResult<Json<StepResponse>> {
    require_permission(&security, Permission::StepsResolve)?;

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

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::WebWrite);

        // Find the step and verify it belongs to the task
        let workflow_step = WorkflowStep::find_by_id(db_pool, step_uuid)
            .await
            .map_err(ApiError::from)?;

        let step = match workflow_step {
            Some(step) if step.task_uuid == task_uuid => step,
            Some(_) => return Err(ApiError::not_found("Resource not found")),
            None => return Err(ApiError::not_found("Resource not found")),
        };

        // Initialize StepStateMachine for proper state transition
        let mut step_state_machine =
            StepStateMachine::new(step.clone(), state.orchestration_core.context.clone());

        // Route to the appropriate event based on action type
        let event = match action {
            StepManualAction::ResetForRetry { .. } => {
                info!(step_uuid = %step_uuid, "Using ResetForRetry event");
                StepEvent::ResetForRetry
            }
            StepManualAction::ResolveManually { .. } => {
                info!(step_uuid = %step_uuid, "Using ResolveManually event");
                StepEvent::ResolveManually
            }
            StepManualAction::CompleteManually { completion_data, reason, completed_by } => {
                info!(step_uuid = %step_uuid, "Using CompleteManually event with execution results");

                // Build custom metadata from operator-provided metadata or defaults
                let mut custom_metadata = if let Some(metadata_value) = completion_data.metadata {
                    // If operator provided metadata as JSON object, convert to HashMap
                    if let Some(metadata_obj) = metadata_value.as_object() {
                        metadata_obj
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect()
                    } else {
                        // If metadata is not an object, create a wrapper
                        let mut map = HashMap::new();
                        map.insert("operator_metadata".to_string(), metadata_value);
                        map
                    }
                } else {
                    HashMap::new()
                };

                // Always add manual completion tracking
                custom_metadata.insert(
                    "manually_completed".to_string(),
                    serde_json::json!(true),
                );
                custom_metadata.insert(
                    "completed_by".to_string(),
                    serde_json::json!(completed_by),
                );
                custom_metadata.insert(
                    "completion_reason".to_string(),
                    serde_json::json!(reason),
                );

                // Construct proper StepExecutionResult using typed constructor
                // System enforces success=true and status="completed"
                let execution_result = StepExecutionResult::success(
                    step_uuid,
                    completion_data.result,
                    0, // execution_time_ms - manual completion has no measured execution time
                    Some(custom_metadata),
                );

                // Serialize to JSON for the event
                let execution_result_json = serde_json::to_value(&execution_result)
                    .map_err(|e| ApiError::internal_server_error(format!("Failed to serialize execution result: {}", e)))?;

                StepEvent::CompleteManually(Some(execution_result_json))
            }
        };

        // Attempt transition using state machine
        match step_state_machine.transition(event).await {
            Ok(new_state) => {
                info!(
                    step_uuid = %step_uuid,
                    new_state = %new_state,
                    "Step action completed successfully"
                );

                // Get updated step data after state transition
                let updated_step = WorkflowStep::find_by_id(db_pool, step_uuid)
                    .await
                    .map_err(ApiError::from)?
                    .ok_or(ApiError::not_found("Resource not found"))?;

                // Get step readiness status after resolution
                let sql_executor = SqlFunctionExecutor::new(db_pool.clone());
                let readiness_statuses = sql_executor
                    .get_step_readiness_status(task_uuid, Some(vec![step_uuid]))
                    .await
                    .map_err(ApiError::from)?;

                // Build response with updated step data
                let step_response = if let Some(readiness) = readiness_statuses.first() {
                    StepResponse {
                        step_uuid: updated_step.workflow_step_uuid.to_string(),
                        task_uuid: updated_step.task_uuid.to_string(),
                        name: readiness.name.clone(),
                        created_at: updated_step
                            .created_at
                            .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                            .to_string(),
                        updated_at: updated_step
                            .updated_at
                            .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                            .to_string(),
                        completed_at: updated_step
                            .processed_at
                            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                        results: updated_step.results,

                        // StepReadinessStatus fields
                        current_state: readiness.current_state.clone(),
                        dependencies_satisfied: readiness.dependencies_satisfied,
                        retry_eligible: readiness.retry_eligible,
                        ready_for_execution: readiness.ready_for_execution,
                        total_parents: readiness.total_parents,
                        completed_parents: readiness.completed_parents,
                        attempts: readiness.attempts,
                        max_attempts: readiness.max_attempts,
                        last_failure_at: readiness
                            .last_failure_at
                            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                        next_retry_at: readiness
                            .next_retry_at
                            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                        last_attempted_at: readiness
                            .last_attempted_at
                            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                    }
                } else {
                    // Fallback response
                    StepResponse {
                        step_uuid: updated_step.workflow_step_uuid.to_string(),
                        task_uuid: updated_step.task_uuid.to_string(),
                        name: format!("step_{}", updated_step.workflow_step_uuid),
                        created_at: updated_step
                            .created_at
                            .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                            .to_string(),
                        updated_at: updated_step
                            .updated_at
                            .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                            .to_string(),
                        completed_at: updated_step
                            .processed_at
                            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                        results: updated_step.results,

                        // Default readiness fields
                        current_state: new_state.to_string(),
                        dependencies_satisfied: true, // Manual resolution satisfies dependencies
                        retry_eligible: false,        // Manually resolved steps don't need retry
                        ready_for_execution: false,   // Already resolved
                        total_parents: 0,
                        completed_parents: 0,
                        attempts: updated_step.attempts.unwrap_or(0),
                        max_attempts: updated_step.max_attempts.unwrap_or(3),
                        last_failure_at: None,
                        next_retry_at: None,
                        last_attempted_at: updated_step
                            .last_attempted_at
                            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                    }
                };

                Ok::<Json<StepResponse>, ApiError>(Json(step_response))
            }
            Err(state_machine_error) => {
                error!(
                    error = %state_machine_error,
                    step_uuid = %step_uuid,
                    "Failed to manually resolve step"
                );

                let error_message = match state_machine_error {
                    StateMachineError::InvalidTransition { from, to } => {
                        format!(
                            "Cannot manually resolve step: invalid transition from {} to {to}",
                            from.unwrap_or("unknown".to_string())
                        )
                    }
                    StateMachineError::GuardFailed { reason } => {
                        format!("Cannot manually resolve step: {reason}")
                    }
                    StateMachineError::Database(db_error) => {
                        format!("Database error during manual resolution: {db_error}")
                    }
                    _ => format!("Manual resolution failed: {state_machine_error}"),
                };

                Err(ApiError::bad_request(error_message))
            }
        }
    })
    .await
    .map_err(|e| {
        let error_str = e.to_string();
        if error_str.contains("Step not found") {
            ApiError::not_found("Resource not found")
        } else if error_str.contains("Cannot manually resolve")
            || error_str.contains("Manual resolution failed")
        {
            ApiError::bad_request(error_str)
        } else {
            error!(error = %error_str, "Unexpected error during manual step resolution");
            ApiError::internal_server_error(format!("Manual resolution failed: {error_str}"))
        }
    })
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
    tag = "workflow_steps"
))]
pub async fn get_step_audit(
    State(state): State<AppState>,
    security: SecurityContext,
    Path((task_uuid, step_uuid)): Path<(String, String)>,
) -> ApiResult<Json<Vec<StepAuditResponse>>> {
    require_permission(&security, Permission::StepsRead)?;

    info!(
        task_uuid = %task_uuid,
        step_uuid = %step_uuid,
        "Getting audit history for workflow step"
    );

    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::bad_request("Invalid task UUID format"))?;
    let step_uuid = Uuid::parse_str(&step_uuid)
        .map_err(|_| ApiError::bad_request("Invalid step UUID format"))?;

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::ReadOnly);

        // Verify the step exists and belongs to the task
        let workflow_step = WorkflowStep::find_by_id(db_pool, step_uuid)
            .await
            .map_err(ApiError::from)?;

        match workflow_step {
            Some(step) if step.task_uuid == task_uuid => {
                // Step exists and belongs to task, continue
            }
            Some(_) => {
                return Err(ApiError::not_found(
                    "Step does not belong to the specified task",
                ));
            }
            None => {
                return Err(ApiError::not_found("Step not found"));
            }
        }

        // Get audit history with full transition details via JOIN
        let audit_records = WorkflowStepResultAudit::get_audit_history(db_pool, step_uuid)
            .await
            .map_err(ApiError::from)?;

        // Transform to API response format
        let response: Vec<StepAuditResponse> = audit_records
            .iter()
            .map(StepAuditResponse::from_audit_with_transition)
            .collect();

        info!(
            step_uuid = %step_uuid,
            record_count = response.len(),
            "Retrieved audit history for step"
        );

        Ok::<Json<Vec<StepAuditResponse>>, ApiError>(Json(response))
    })
    .await
}
