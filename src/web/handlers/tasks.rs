//! # Task Management Handlers
//!
//! HTTP handlers for task creation, status retrieval, and management operations.
//! These are the core endpoints required for worker system integration (TAS-40).

use axum::extract::{Path, Query, State};
use axum::Json;
use bigdecimal::ToPrimitive;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info};
use uuid::Uuid;

#[cfg(feature = "web-api")]
// use utoipa::ToSchema;
use crate::database::sql_functions::{
    SqlFunctionExecutor, StepReadinessStatus, TaskExecutionContext,
};
use crate::models::core::task::PaginationInfo;
use crate::models::core::task_request::TaskRequest;
use crate::web::circuit_breaker::execute_with_circuit_breaker;
use crate::web::errors::{ApiError, ApiResult};
use crate::web::state::{AppState, DbOperationType};

#[cfg(feature = "web-api")]
use crate::web::openapi::{TaskListResponse as OpenApiTaskListResponse, TaskResponse as OpenApiTaskResponse, TaskStatus, PaginationMeta};

// Note: Using TaskRequest from src/models/core/task_request.rs instead of duplicate struct

/// Response for successful task creation
#[derive(Debug, Serialize)]
pub struct TaskCreationResponse {
    pub task_uuid: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub estimated_completion: Option<DateTime<Utc>>,
    /// Number of workflow steps created for this task
    pub step_count: usize,
    /// Mapping of step names to their workflow step UUIDs
    pub step_mapping: HashMap<String, String>,
    /// Handler configuration name used (if any)
    pub handler_config_name: Option<String>,
}

/// Task details response with execution context and step readiness information
#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub task_uuid: String,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub context: serde_json::Value,
    pub initiator: String,
    pub source_system: String,
    pub reason: String,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,

    // Execution context fields from TaskExecutionContext
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub completed_steps: i64,
    pub failed_steps: i64,
    pub ready_steps: i64,
    pub execution_status: String,
    pub recommended_action: String,
    pub completion_percentage: f64,
    pub health_status: String,

    // Step readiness information
    pub steps: Vec<StepReadinessStatus>,
}

/// Task list response with pagination
#[derive(Debug, Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskResponse>,
    pub pagination: PaginationInfo,
}

// Note: PaginationInfo is imported from crate::models::core::task

/// Query parameters for task listing
#[derive(Debug, Deserialize)]
pub struct TaskListQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    pub namespace: Option<String>,
    pub status: Option<String>,
    pub initiator: Option<String>,
    pub source_system: Option<String>,
}

fn default_page() -> u32 {
    1
}
fn default_per_page() -> u32 {
    25
}

/// Create a new task: POST /v1/tasks
///
/// This is the critical endpoint for TAS-40 worker integration.
/// Workers call this endpoint to create tasks and receive UUIDs for tracking.
#[cfg_attr(feature = "web-api", utoipa::path(
    post,
    path = "/v1/tasks",
    request_body = crate::web::openapi::TaskRequest,
    responses(
        (status = 201, description = "Task created successfully", body = crate::web::openapi::TaskResponse),
        (status = 400, description = "Invalid request", body = crate::web::openapi::ApiError),
        (status = 503, description = "Service unavailable", body = crate::web::openapi::ApiError)
    ),
    tag = "tasks"
))]
pub async fn create_task(
    State(state): State<AppState>,
    // Note: Authentication should be added via middleware when auth is enabled
    Json(request): Json<TaskRequest>,
) -> ApiResult<Json<TaskCreationResponse>> {
    info!(
        namespace = %request.namespace,
        task_name = %request.name,
        version = %request.version,
        initiator = %request.initiator,
        source_system = %request.source_system,
        "Creating new task via web API"
    );

    // Input validation
    if request.name.is_empty() {
        return Err(ApiError::bad_request("Task name cannot be empty"));
    }
    if request.namespace.is_empty() {
        return Err(ApiError::bad_request("Namespace cannot be empty"));
    }
    if request.version.is_empty() {
        return Err(ApiError::bad_request("Version cannot be empty"));
    }

    // Use circuit breaker wrapper for database operations
    let result = execute_with_circuit_breaker(&state, || async {
        // Set default values for any missing fields if needed
        let mut task_request = request.clone();

        // Ensure default status is set for web API requests
        if task_request.status.is_empty() {
            task_request.status = "pending".to_string();
        }

        // Set complete to false for new tasks
        task_request.complete = false;

        // Set requested_at to current time
        task_request.requested_at = chrono::Utc::now().naive_utc();

        // Initialize task using existing orchestration logic
        state
            .task_initializer
            .create_task_from_request(task_request)
            .await
    })
    .await;

    match result {
        Ok(task_result) => {
            // Convert step mapping UUIDs to strings for JSON serialization
            let step_mapping: HashMap<String, String> = task_result
                .step_mapping
                .into_iter()
                .map(|(step_name, uuid)| (step_name, uuid.to_string()))
                .collect();

            let response = TaskCreationResponse {
                task_uuid: task_result.task_uuid.to_string(),
                status: "created".to_string(),
                created_at: Utc::now(),
                estimated_completion: None, // Future: Could calculate from step count and historical data
                step_count: task_result.step_count,
                step_mapping,
                handler_config_name: task_result.handler_config_name.clone(),
            };

            info!(
                task_uuid = %task_result.task_uuid,
                step_count = task_result.step_count,
                handler_config = ?task_result.handler_config_name,
                "Task created successfully via web API"
            );

            Ok(Json(response))
        }
        Err(api_err) => {
            error!(
                namespace = %request.namespace,
                task_name = %request.name,
                error = %api_err,
                "Failed to create task via web API"
            );
            Err(api_err)
        }
    }
}

/// Get task details: GET /v1/tasks/{uuid}
///
/// Returns comprehensive task information including execution context and step readiness.
/// Used by workers for task correlation and status tracking.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/tasks/{uuid}",
    params(
        ("uuid" = String, Path, description = "Task UUID")
    ),
    responses(
        (status = 200, description = "Task details", body = crate::web::openapi::TaskResponse),
        (status = 404, description = "Task not found", body = crate::web::openapi::ApiError),
        (status = 503, description = "Service unavailable", body = crate::web::openapi::ApiError)
    ),
    tag = "tasks"
))]
pub async fn get_task(
    State(state): State<AppState>,
    Path(task_uuid): Path<String>,
) -> ApiResult<Json<TaskResponse>> {
    debug!(task_uuid = %task_uuid, "Retrieving task details with execution context");

    // Parse and validate UUID
    let uuid = Uuid::parse_str(&task_uuid).map_err(|_| ApiError::invalid_uuid(task_uuid))?;

    // Use read-only database pool for this operation
    let pool = state.select_db_pool(DbOperationType::ReadOnly);
    let sql_executor = SqlFunctionExecutor::new(pool.clone());

    let result = execute_with_circuit_breaker(&state, || async {
        // Get basic task information using existing Task model
        let task = crate::models::Task::find_by_id(pool, uuid)
            .await
            .map_err(std::io::Error::other)?;

        if task.is_none() {
            return Ok::<
                Option<(
                    crate::models::Task,
                    TaskExecutionContext,
                    Vec<StepReadinessStatus>,
                )>,
                std::io::Error,
            >(None);
        }

        let task = task.unwrap();

        // Get task execution context using SQL function
        let execution_context = sql_executor
            .get_task_execution_context(uuid)
            .await
            .map_err(std::io::Error::other)?
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Task execution context not found",
                )
            })?;

        // Get step readiness status using SQL function
        let steps = sql_executor
            .get_step_readiness_status(uuid, None)
            .await
            .map_err(std::io::Error::other)?;

        Ok(Some((task, execution_context, steps)))
    })
    .await;

    match result {
        Ok(Some((task, execution_context, steps))) => {
            // Get task metadata using existing Task model methods
            let pool_for_metadata = pool.clone();
            let task_name = task.name(&pool_for_metadata).await.map_err(|e| {
                error!(error = %e, "Failed to get task name");
                ApiError::database_error("Failed to retrieve task metadata")
            })?;

            let namespace = task.namespace_name(&pool_for_metadata).await.map_err(|e| {
                error!(error = %e, "Failed to get namespace name");
                ApiError::database_error("Failed to retrieve task metadata")
            })?;

            let version = task.version(&pool_for_metadata).await.map_err(|e| {
                error!(error = %e, "Failed to get task version");
                ApiError::database_error("Failed to retrieve task metadata")
            })?;

            // Convert tags from JSONB to Vec<String> if it's an array
            let tags = match &task.tags {
                Some(serde_json::Value::Array(arr)) => Some(
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect(),
                ),
                _ => None,
            };

            // Convert BigDecimal to f64 for JSON serialization
            let completion_percentage = execution_context
                .completion_percentage
                .to_f64()
                .unwrap_or(0.0);

            // Determine status based on task completion
            let status = if task.complete {
                "completed".to_string()
            } else {
                "pending".to_string()
            };

            let response = TaskResponse {
                task_uuid: task.task_uuid.to_string(),
                name: task_name,
                namespace,
                version,
                status,
                created_at: DateTime::from_naive_utc_and_offset(task.created_at, Utc),
                updated_at: DateTime::from_naive_utc_and_offset(task.updated_at, Utc),
                completed_at: None, // Task model doesn't have completed_at timestamp - would come from state transitions
                context: task.context.unwrap_or_else(|| serde_json::json!({})),
                initiator: task.initiator.unwrap_or_else(|| "unknown".to_string()),
                source_system: task.source_system.unwrap_or_else(|| "unknown".to_string()),
                reason: task.reason.unwrap_or_else(|| "unknown".to_string()),
                priority: Some(task.priority),
                tags,

                // Execution context fields
                total_steps: execution_context.total_steps,
                pending_steps: execution_context.pending_steps,
                in_progress_steps: execution_context.in_progress_steps,
                completed_steps: execution_context.completed_steps,
                failed_steps: execution_context.failed_steps,
                ready_steps: execution_context.ready_steps,
                execution_status: execution_context.execution_status,
                recommended_action: execution_context.recommended_action,
                completion_percentage,
                health_status: execution_context.health_status,

                // Step readiness information
                steps,
            };

            Ok(Json(response))
        }
        Ok(None) => Err(ApiError::NotFound),
        Err(e) => {
            error!(task_uuid = %uuid, error = %e, "Failed to retrieve task with execution context");
            Err(ApiError::database_error("Failed to retrieve task"))
        }
    }
}

/// List tasks with pagination and filtering: GET /v1/tasks
///
/// Returns a paginated list of tasks with execution context using batch SQL functions.
/// Used by workers and administrators for task monitoring and management.
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
        (status = 200, description = "List of tasks", body = crate::web::openapi::TaskListResponse),
        (status = 400, description = "Invalid query parameters", body = crate::web::openapi::ApiError),
        (status = 503, description = "Service unavailable", body = crate::web::openapi::ApiError)
    ),
    tag = "tasks"
))]
pub async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> ApiResult<Json<TaskListResponse>> {
    debug!(?query, "Listing tasks with execution context and filters");

    // Validate pagination parameters
    if query.per_page == 0 || query.per_page > 100 {
        return Err(ApiError::bad_request("per_page must be between 1 and 100"));
    }
    if query.page == 0 {
        return Err(ApiError::bad_request("page must be >= 1"));
    }

    let _offset = (query.page - 1) * query.per_page;
    let pool = state.select_db_pool(DbOperationType::ReadOnly);
    let sql_executor = SqlFunctionExecutor::new(pool.clone());

    let result = execute_with_circuit_breaker(&state, || async {
        // Step 1: Convert web query to domain model query
        let task_query = crate::models::core::task::TaskListQuery {
            page: query.page,
            per_page: query.per_page,
            namespace: query.namespace.clone(),
            status: query.status.clone(),
            initiator: query.initiator.clone(),
            source_system: query.source_system.clone(),
        };

        // Step 2: Get paginated tasks using domain model
        let paginated_result = crate::models::Task::list_with_pagination(pool, &task_query)
            .await
            .map_err(std::io::Error::other)?;

        // Step 3: Extract task UUIDs for batch execution context lookup
        let task_uuids: Vec<Uuid> = paginated_result
            .tasks
            .iter()
            .map(|task_with_metadata| task_with_metadata.task.task_uuid)
            .collect();

        // Step 4: Get execution contexts using batch SQL function
        let execution_contexts = sql_executor
            .get_task_execution_contexts_batch(task_uuids.clone())
            .await
            .map_err(std::io::Error::other)?;

        // Step 5: Create a lookup map for execution contexts
        let execution_context_map: HashMap<Uuid, TaskExecutionContext> = execution_contexts
            .into_iter()
            .map(|ctx| (ctx.task_uuid, ctx))
            .collect();

        // Step 6: Convert domain model tasks to API responses with execution context
        let tasks: Vec<TaskResponse> = paginated_result
            .tasks
            .into_iter()
            .map(|task_with_metadata| {
                let task = &task_with_metadata.task;

                let tags = match &task.tags {
                    Some(serde_json::Value::Array(arr)) => Some(
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect(),
                    ),
                    _ => None,
                };

                // Get execution context from lookup map
                let execution_context = execution_context_map
                    .get(&task.task_uuid)
                    .cloned()
                    .unwrap_or_else(|| {
                        // Fallback execution context if not found
                        TaskExecutionContext {
                            task_uuid: task.task_uuid,
                            named_task_uuid: task.named_task_uuid,
                            status: "unknown".to_string(),
                            total_steps: 0,
                            pending_steps: 0,
                            in_progress_steps: 0,
                            completed_steps: 0,
                            failed_steps: 0,
                            ready_steps: 0,
                            execution_status: "unknown".to_string(),
                            recommended_action: "investigate".to_string(),
                            completion_percentage: sqlx::types::BigDecimal::from(0),
                            health_status: "unknown".to_string(),
                        }
                    });

                let completion_percentage = execution_context
                    .completion_percentage
                    .to_f64()
                    .unwrap_or(0.0);

                TaskResponse {
                    task_uuid: task.task_uuid.to_string(),
                    name: task_with_metadata.task_name,
                    namespace: task_with_metadata.namespace_name,
                    version: task_with_metadata.task_version,
                    status: task_with_metadata.status,
                    created_at: DateTime::from_naive_utc_and_offset(task.created_at, Utc),
                    updated_at: DateTime::from_naive_utc_and_offset(task.updated_at, Utc),
                    completed_at: None, // Task model doesn't have completed_at timestamp
                    context: task
                        .context
                        .clone()
                        .unwrap_or_else(|| serde_json::json!({})),
                    initiator: task
                        .initiator
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    source_system: task
                        .source_system
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason: task.reason.clone().unwrap_or_else(|| "unknown".to_string()),
                    priority: Some(task.priority),
                    tags,

                    // Execution context fields from batch SQL function
                    total_steps: execution_context.total_steps,
                    pending_steps: execution_context.pending_steps,
                    in_progress_steps: execution_context.in_progress_steps,
                    completed_steps: execution_context.completed_steps,
                    failed_steps: execution_context.failed_steps,
                    ready_steps: execution_context.ready_steps,
                    execution_status: execution_context.execution_status,
                    recommended_action: execution_context.recommended_action,
                    completion_percentage,
                    health_status: execution_context.health_status,

                    // Note: For list operations, we don't include detailed step information
                    // to avoid N+1 queries. Individual task details can be retrieved via get_task
                    steps: Vec::new(),
                }
            })
            .collect();

        Ok::<(Vec<TaskResponse>, PaginationInfo), std::io::Error>((
            tasks,
            paginated_result.pagination,
        ))
    })
    .await;

    let (tasks, pagination) = match result {
        Ok(data) => data,
        Err(e) => {
            error!(error = %e, "Failed to list tasks with execution context");
            return Err(ApiError::database_error("Failed to retrieve task list"));
        }
    };

    let response = TaskListResponse { tasks, pagination };

    Ok(Json(response))
}

/// Cancel a task: DELETE /v1/tasks/{uuid}
///
/// Cancels a task if it's in a cancellable state and triggers orchestration events.
/// Returns the updated task with execution context and step information.
#[cfg_attr(feature = "web-api", utoipa::path(
    delete,
    path = "/v1/tasks/{uuid}",
    params(
        ("uuid" = String, Path, description = "Task UUID")
    ),
    responses(
        (status = 200, description = "Task cancelled successfully", body = crate::web::openapi::TaskResponse),
        (status = 400, description = "Task cannot be cancelled", body = crate::web::openapi::ApiError),
        (status = 404, description = "Task not found", body = crate::web::openapi::ApiError),
        (status = 503, description = "Service unavailable", body = crate::web::openapi::ApiError)
    ),
    tag = "tasks"
))]
pub async fn cancel_task(
    State(state): State<AppState>,
    Path(task_uuid): Path<String>,
    // Note: Authentication should be added via middleware when auth is enabled
) -> ApiResult<Json<TaskResponse>> {
    info!(task_uuid = %task_uuid, "Cancelling task via web API");

    // Parse and validate UUID
    let uuid = Uuid::parse_str(&task_uuid).map_err(|_| ApiError::invalid_uuid(task_uuid))?;

    // Use dedicated web pool for write operations
    let pool = state.select_db_pool(DbOperationType::WebWrite);
    let sql_executor = SqlFunctionExecutor::new(pool.clone());

    let result = execute_with_circuit_breaker(&state, || async {
        // Step 1: Find the task using existing Task model
        let task = crate::models::Task::find_by_id(pool, uuid)
            .await
            .map_err(std::io::Error::other)?;

        let task = match task {
            Some(t) => t,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Task not found",
                ));
            }
        };

        // Step 2: Check if task can be cancelled using domain model
        let can_cancel = task
            .can_be_cancelled(pool)
            .await
            .map_err(std::io::Error::other)?;

        if !can_cancel {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot cancel a completed task",
            ));
        }

        // Step 3: Cancel the task using domain model
        let mut task = task; // Make mutable for cancellation
        task.cancel_task(pool)
            .await
            .map_err(std::io::Error::other)?;

        // Step 4: Get task metadata using domain model
        let (name, namespace, version, status) = task
            .get_task_metadata(pool)
            .await
            .map_err(std::io::Error::other)?;

        // Step 5: Get execution context using SQL function
        let execution_context = sql_executor
            .get_task_execution_context(uuid)
            .await
            .map_err(std::io::Error::other)?
            .unwrap_or_else(|| {
                // Fallback execution context if not found
                TaskExecutionContext {
                    task_uuid: uuid,
                    named_task_uuid: task.named_task_uuid,
                    status: "cancelled".to_string(),
                    total_steps: 0,
                    pending_steps: 0,
                    in_progress_steps: 0,
                    completed_steps: 0,
                    failed_steps: 0,
                    ready_steps: 0,
                    execution_status: "cancelled".to_string(),
                    recommended_action: "none".to_string(),
                    completion_percentage: sqlx::types::BigDecimal::from(100),
                    health_status: "cancelled".to_string(),
                }
            });

        // Step 6: Get step readiness status using SQL function
        let steps = sql_executor
            .get_step_readiness_status(uuid, None)
            .await
            .map_err(std::io::Error::other)?;

        // Step 7: Convert tags from JSONB to Vec<String> if it's an array
        let tags = match &task.tags {
            Some(serde_json::Value::Array(arr)) => Some(
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
            ),
            _ => None,
        };

        let completion_percentage = execution_context
            .completion_percentage
            .to_f64()
            .unwrap_or(100.0);

        // Step 8: Build TaskResponse using domain model data
        let task_response = TaskResponse {
            task_uuid: task.task_uuid.to_string(),
            name,
            namespace,
            version,
            status: if status == "completed" {
                "cancelled".to_string()
            } else {
                status
            },
            created_at: DateTime::from_naive_utc_and_offset(task.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(task.updated_at, Utc),
            completed_at: None, // Task model doesn't have completed_at timestamp
            context: task.context.unwrap_or_else(|| serde_json::json!({})),
            initiator: task.initiator.unwrap_or_else(|| "unknown".to_string()),
            source_system: task.source_system.unwrap_or_else(|| "unknown".to_string()),
            reason: task.reason.unwrap_or_else(|| "unknown".to_string()),
            priority: Some(task.priority),
            tags,

            // Execution context fields
            total_steps: execution_context.total_steps,
            pending_steps: execution_context.pending_steps,
            in_progress_steps: execution_context.in_progress_steps,
            completed_steps: execution_context.completed_steps,
            failed_steps: execution_context.failed_steps,
            ready_steps: execution_context.ready_steps,
            execution_status: execution_context.execution_status,
            recommended_action: execution_context.recommended_action,
            completion_percentage,
            health_status: execution_context.health_status,

            // Step readiness information
            steps,
        };

        Ok::<TaskResponse, std::io::Error>(task_response)
    })
    .await;

    match result {
        Ok(task_response) => {
            info!(task_uuid = %uuid, "Task cancelled successfully via web API");
            Ok(Json(task_response))
        }
        Err(api_err) => {
            error!(task_uuid = %uuid, error = %api_err, "Failed to cancel task via web API");

            // Convert specific error types to appropriate API errors
            let error_str = api_err.to_string();
            if error_str.contains("Task not found") {
                Err(ApiError::NotFound)
            } else if error_str.contains("Cannot cancel a completed task") {
                Err(ApiError::bad_request("Cannot cancel a completed task"))
            } else {
                Err(ApiError::database_error("Failed to cancel task"))
            }
        }
    }
}

// Helper structures for database queries
// (TaskRow removed - now using crate::models::Task directly)
