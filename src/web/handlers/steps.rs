//! # Workflow Step Handlers
//!
//! HTTP handlers for workflow step operations including status retrieval
//! and manual resolution for failed or stuck steps.
//!
//! This module follows domain-driven design principles by delegating business logic
//! to the WorkflowStep domain model and StepStateMachine instead of using direct SQL.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::events::publisher::EventPublisher;
use crate::models::core::workflow_step::WorkflowStep;
#[cfg(feature = "web-api")]
use utoipa::ToSchema;
// StepReadinessStatus is used through SqlFunctionExecutor, removing unused direct import
use crate::state_machine::events::StepEvent;
use crate::state_machine::step_state_machine::StepStateMachine;
use crate::web::circuit_breaker::execute_with_circuit_breaker;
use crate::web::response_types::{ApiError, ApiResult};
use crate::web::state::{AppState, DbOperationType};

/// Manual step resolution request
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ManualResolutionRequest {
    pub resolution_data: serde_json::Value,
    pub resolved_by: String,
    pub reason: String,
}

/// Step details response with readiness information
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct StepResponse {
    pub step_uuid: String,
    pub task_uuid: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub results: Option<serde_json::Value>,

    // StepReadinessStatus fields
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub attempts: i32,
    pub retry_limit: i32,
    pub last_failure_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub last_attempted_at: Option<String>,
}

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
        (status = 503, description = "Service unavailable", body = ApiError)
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
                    retry_limit: readiness.retry_limit,
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
                    retry_limit: step.retry_limit.unwrap_or(3),
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
        (status = 404, description = "Step not found", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
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
                    return Err(ApiError::NotFound);
                }
                step
            }
            None => {
                return Err(ApiError::NotFound);
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
                retry_limit: readiness.retry_limit,
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
                retry_limit: step.retry_limit.unwrap_or(3),
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
            ApiError::NotFound
        } else {
            e
        }
    })
}

/// Manually resolve a workflow step: PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}
#[cfg_attr(feature = "web-api", utoipa::path(
    patch,
    path = "/v1/tasks/{uuid}/workflow_steps/{step_uuid}",
    params(
        ("uuid" = String, Path, description = "Task UUID"),
        ("step_uuid" = String, Path, description = "Step UUID")
    ),
    request_body = ManualResolutionRequest,
    responses(
        (status = 200, description = "Step resolved successfully", body = StepResponse),
        (status = 400, description = "Invalid request or step cannot be resolved", body = ApiError),
        (status = 404, description = "Step not found", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    tag = "workflow_steps"
))]
pub async fn resolve_step_manually(
    State(state): State<AppState>,
    Path((task_uuid, step_uuid)): Path<(String, String)>,
    Json(request): Json<ManualResolutionRequest>,
) -> ApiResult<Json<StepResponse>> {
    info!(
        task_uuid = %task_uuid,
        step_uuid = %step_uuid,
        resolved_by = %request.resolved_by,
        reason = %request.reason,
        "Manually resolving workflow step"
    );

    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::bad_request("Invalid task UUID format"))?;
    let step_uuid = Uuid::parse_str(&step_uuid)
        .map_err(|_| ApiError::bad_request("Invalid step UUID format"))?;

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::WebWrite);

        // Use WorkflowStep domain model to find the step
        let workflow_step = WorkflowStep::find_by_id(db_pool, step_uuid)
            .await
            .map_err(ApiError::from)?;

        let step = match workflow_step {
            Some(step) => {
                // Verify the step belongs to the specified task
                if step.task_uuid != task_uuid {
                    return Err(ApiError::NotFound);
                }
                step
            }
            None => {
                return Err(ApiError::NotFound);
            }
        };

        // Create event publisher for state machine
        let event_publisher = EventPublisher::new();

        // Initialize StepStateMachine for proper state transition
        let mut step_state_machine =
            StepStateMachine::new(step.clone(), db_pool.clone(), event_publisher);

        // Attempt manual resolution using state machine
        match step_state_machine
            .transition(StepEvent::ResolveManually)
            .await
        {
            Ok(new_state) => {
                info!(
                    step_uuid = %step_uuid,
                    new_state = %new_state,
                    resolved_by = %request.resolved_by,
                    "Step manually resolved successfully"
                );

                // Get updated step data after state transition
                let updated_step = WorkflowStep::find_by_id(db_pool, step_uuid)
                    .await
                    .map_err(ApiError::from)?
                    .ok_or(ApiError::NotFound)?;

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
                        retry_limit: readiness.retry_limit,
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
                        retry_limit: updated_step.retry_limit.unwrap_or(3),
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
                    crate::state_machine::errors::StateMachineError::InvalidTransition {
                        from,
                        to,
                    } => {
                        format!(
                            "Cannot manually resolve step: invalid transition from {} to {to}",
                            from.unwrap_or("unknown".to_string())
                        )
                    }
                    crate::state_machine::errors::StateMachineError::GuardFailed { reason } => {
                        format!("Cannot manually resolve step: {reason}")
                    }
                    crate::state_machine::errors::StateMachineError::Database(db_error) => {
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
            ApiError::NotFound
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
