//! # Dead Letter Queue (DLQ) Investigation Handlers
//!
//! HTTP handlers for DLQ investigation tracking and management (TAS-49).
//!
//! ## Architecture
//!
//! DLQ is an **investigation tracking system**, NOT a task manipulation layer:
//! - Tracks "why task is stuck" and "who investigated"
//! - Resolution happens at step level via existing step APIs
//! - No task-level "requeue" - fix the problem steps instead
//!
//! ## Resolution Workflow
//!
//! 1. Operator: `GET /v1/dlq/task/{task_uuid}` → review task_snapshot
//! 2. Operator: `PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}` → fix problem steps
//! 3. Task state machine: Automatically progresses when steps fixed
//! 4. Operator: `PATCH /v1/dlq/entry/{dlq_entry_uuid}` → update investigation status
//!
//! ## Available Endpoints
//!
//! - `GET /v1/dlq` - List DLQ entries with optional filtering
//! - `GET /v1/dlq/task/{task_uuid}` - Get DLQ entry with full task snapshot
//! - `PATCH /v1/dlq/entry/{dlq_entry_uuid}` - Update investigation status
//! - `GET /v1/dlq/stats` - DLQ statistics by reason
//! - `GET /v1/dlq/investigation-queue` - Prioritized investigation queue for triage
//! - `GET /v1/dlq/staleness` - Proactive staleness monitoring

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::web::state::AppState;
use tasker_shared::models::orchestration::dlq::{
    DlqEntry, DlqInvestigationQueueEntry, DlqInvestigationUpdate,
    DlqListParams as ModelDlqListParams, DlqResolutionStatus, DlqStats, StalenessMonitoring,
};
use tasker_shared::types::web::{ApiError, ApiResult};

// ============================================================================
// Request Types (Web Layer)
// ============================================================================

/// Query parameters for listing DLQ entries
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::IntoParams))]
pub struct DlqListQueryParams {
    /// Filter by resolution status (optional)
    pub resolution_status: Option<DlqResolutionStatus>,
    /// Maximum number of entries to return (default: 50)
    pub limit: Option<i64>,
    /// Offset for pagination (default: 0)
    pub offset: Option<i64>,
}

impl From<DlqListQueryParams> for ModelDlqListParams {
    fn from(params: DlqListQueryParams) -> Self {
        Self {
            resolution_status: params.resolution_status,
            limit: params.limit.unwrap_or(50),
            offset: params.offset.unwrap_or(0),
        }
    }
}

/// Request body for updating DLQ investigation status
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct UpdateInvestigationRequest {
    /// New resolution status (optional)
    pub resolution_status: Option<DlqResolutionStatus>,
    /// Investigation notes (optional)
    pub resolution_notes: Option<String>,
    /// Who resolved the investigation (optional)
    pub resolved_by: Option<String>,
    /// Additional metadata (optional)
    pub metadata: Option<serde_json::Value>,
}

impl From<UpdateInvestigationRequest> for DlqInvestigationUpdate {
    fn from(req: UpdateInvestigationRequest) -> Self {
        Self {
            resolution_status: req.resolution_status,
            resolution_notes: req.resolution_notes,
            resolved_by: req.resolved_by,
            metadata: req.metadata,
        }
    }
}

/// Query parameters for investigation queue endpoint
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::IntoParams))]
pub struct InvestigationQueueParams {
    /// Maximum number of entries to return (default: 100)
    pub limit: Option<i64>,
}

/// Query parameters for staleness monitoring endpoint
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::IntoParams))]
pub struct StalenessMonitoringParams {
    /// Maximum number of tasks to return (default: 100)
    pub limit: Option<i64>,
}

// ============================================================================
// Response Types (Web Layer - Re-exports from Model Layer)
// ============================================================================

/// Response for update investigation endpoint
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct UpdateInvestigationResponse {
    pub success: bool,
    pub message: String,
    pub dlq_entry_uuid: Uuid,
}

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// List DLQ entries: GET /v1/dlq
///
/// Returns a list of DLQ entries with optional filtering by resolution status.
/// Supports pagination via limit/offset parameters.
///
/// # Query Parameters
///
/// - `resolution_status` - Filter by resolution status (optional)
/// - `limit` - Maximum entries to return (default: 50)
/// - `offset` - Pagination offset (default: 0)
///
/// # Example
///
/// ```text
/// GET /v1/dlq?resolution_status=pending&limit=20
/// ```
///
/// **Required Permission:** `dlq:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/dlq",
    params(DlqListQueryParams),
    responses(
        (status = 200, description = "List of DLQ entries", body = Vec<DlqEntry>),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 500, description = "Database error", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("dlq:read"))
    ),
    tag = "dlq"
))]
pub async fn list_dlq_entries(
    State(state): State<AppState>,
    Query(params): Query<DlqListQueryParams>,
) -> ApiResult<Json<Vec<DlqEntry>>> {
    debug!(
        resolution_status = ?params.resolution_status,
        limit = params.limit,
        offset = params.offset,
        "Listing DLQ entries"
    );

    // Delegate to model layer
    let entries = DlqEntry::list(state.orchestration_db_pool(), params.into())
        .await
        .map_err(|e| {
            error!("Failed to fetch DLQ entries: {}", e);
            ApiError::database_error(format!("Failed to fetch DLQ entries: {}", e))
        })?;

    info!(count = entries.len(), "Successfully fetched DLQ entries");
    Ok(Json(entries))
}

/// Get DLQ entry with full task snapshot: GET /v1/dlq/task/{task_uuid}
///
/// Returns the most recent DLQ entry for a task, including the complete
/// task snapshot (task + all steps state) for investigation.
///
/// # Path Parameters
///
/// - `task_uuid` - UUID of the task
///
/// # Example
///
/// ```text
/// GET /v1/dlq/task/550e8400-e29b-41d4-a716-446655440000
/// ```
///
/// **Required Permission:** `dlq:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/dlq/task/{task_uuid}",
    params(
        ("task_uuid" = Uuid, Path, description = "Task UUID")
    ),
    responses(
        (status = 200, description = "DLQ entry with full snapshot", body = DlqEntry),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 404, description = "DLQ entry not found", body = ApiError),
        (status = 500, description = "Database error", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("dlq:read"))
    ),
    tag = "dlq"
))]
pub async fn get_dlq_entry(
    State(state): State<AppState>,
    Path(task_uuid): Path<Uuid>,
) -> ApiResult<Json<DlqEntry>> {
    debug!(task_uuid = %task_uuid, "Fetching DLQ entry with task snapshot");

    // Delegate to model layer
    let entry = DlqEntry::find_by_task(state.orchestration_db_pool(), task_uuid)
        .await
        .map_err(|e| {
            error!("Failed to fetch DLQ entry for task {}: {}", task_uuid, e);
            ApiError::database_error(format!("Failed to fetch DLQ entry: {}", e))
        })?
        .ok_or_else(|| {
            debug!(task_uuid = %task_uuid, "DLQ entry not found");
            ApiError::not_found(format!("DLQ entry not found for task {}", task_uuid))
        })?;

    info!(
        dlq_entry_uuid = %entry.dlq_entry_uuid,
        task_uuid = %task_uuid,
        dlq_reason = ?entry.dlq_reason,
        "Successfully fetched DLQ entry with snapshot"
    );

    Ok(Json(entry))
}

/// Update DLQ investigation status: PATCH /v1/dlq/entry/{dlq_entry_uuid}
///
/// Updates the investigation status and notes for a DLQ entry.
/// This endpoint tracks the INVESTIGATION workflow, not task resolution.
///
/// **Note**: Task resolution happens via step APIs:
/// `PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}`
///
/// # Path Parameters
///
/// - `dlq_entry_uuid` - UUID of the DLQ entry
///
/// # Request Body
///
/// ```json
/// {
///   "resolution_status": "manually_resolved",
///   "resolution_notes": "Fixed blocked step by recreating upstream dependency",
///   "resolved_by": "operator@example.com"
/// }
/// ```
///
/// **Required Permission:** `dlq:update`
#[cfg_attr(feature = "web-api", utoipa::path(
    patch,
    path = "/v1/dlq/entry/{dlq_entry_uuid}",
    params(
        ("dlq_entry_uuid" = Uuid, Path, description = "DLQ entry UUID")
    ),
    request_body = UpdateInvestigationRequest,
    responses(
        (status = 200, description = "Investigation updated successfully", body = UpdateInvestigationResponse),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 404, description = "DLQ entry not found", body = ApiError),
        (status = 500, description = "Database error", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("dlq:update"))
    ),
    tag = "dlq"
))]
pub async fn update_dlq_investigation(
    State(state): State<AppState>,
    Path(dlq_entry_uuid): Path<Uuid>,
    Json(payload): Json<UpdateInvestigationRequest>,
) -> ApiResult<Json<UpdateInvestigationResponse>> {
    debug!(
        dlq_entry_uuid = %dlq_entry_uuid,
        resolution_status = ?payload.resolution_status,
        "Updating DLQ investigation status"
    );

    // Delegate to model layer
    let updated = DlqEntry::update_investigation(
        state.orchestration_db_pool(),
        dlq_entry_uuid,
        payload.into(),
    )
    .await
    .map_err(|e| {
        error!("Failed to update DLQ entry {}: {}", dlq_entry_uuid, e);
        ApiError::database_error(format!("Failed to update DLQ entry: {}", e))
    })?;

    if !updated {
        debug!(dlq_entry_uuid = %dlq_entry_uuid, "DLQ entry not found");
        return Err(ApiError::not_found(format!(
            "DLQ entry not found: {}",
            dlq_entry_uuid
        )));
    }

    info!(
        dlq_entry_uuid = %dlq_entry_uuid,
        "Successfully updated DLQ investigation"
    );

    Ok(Json(UpdateInvestigationResponse {
        success: true,
        message: "Investigation status updated successfully".to_string(),
        dlq_entry_uuid,
    }))
}

/// Get DLQ statistics: GET /v1/dlq/stats
///
/// Returns aggregated statistics for DLQ entries grouped by reason.
/// Useful for identifying systemic issues and patterns.
///
/// # Response
///
/// Returns statistics including:
/// - Total entries per reason
/// - Count by resolution status (pending, resolved, failed)
/// - Oldest and newest entry timestamps
///
/// # Example
///
/// ```text
/// GET /v1/dlq/stats
/// ```
///
/// **Required Permission:** `dlq:stats`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/dlq/stats",
    responses(
        (status = 200, description = "DLQ statistics", body = Vec<DlqStats>),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 500, description = "Database error", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("dlq:stats"))
    ),
    tag = "dlq"
))]
pub async fn get_dlq_stats(State(state): State<AppState>) -> ApiResult<Json<Vec<DlqStats>>> {
    debug!("Fetching DLQ statistics");

    // Delegate to model layer
    let stats = DlqEntry::get_stats(state.orchestration_db_pool())
        .await
        .map_err(|e| {
            error!("Failed to fetch DLQ statistics: {}", e);
            ApiError::database_error(format!("Failed to fetch DLQ statistics: {}", e))
        })?;

    info!(
        stats_count = stats.len(),
        "Successfully fetched DLQ statistics"
    );
    Ok(Json(stats))
}

/// Get DLQ investigation queue: GET /v1/dlq/investigation-queue
///
/// Returns a prioritized queue of pending DLQ entries for operator triage.
/// Entries are ordered by priority score (higher = more urgent) combining
/// base reason priority with age factor.
///
/// # Query Parameters
///
/// - `limit` - Maximum entries to return (default: 100)
///
/// # Response
///
/// Returns investigation queue entries including:
/// - DLQ entry and task UUIDs
/// - Original state and DLQ reason
/// - Task metadata (namespace, name, current state)
/// - Time in DLQ (minutes)
/// - Priority score for triage ordering
///
/// # Example
///
/// ```text
/// GET /v1/dlq/investigation-queue?limit=50
/// ```
///
/// **Required Permission:** `dlq:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/dlq/investigation-queue",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum entries to return (default: 100)")
    ),
    responses(
        (status = 200, description = "Prioritized investigation queue", body = Vec<DlqInvestigationQueueEntry>),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 500, description = "Database error", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("dlq:read"))
    ),
    tag = "dlq"
))]
pub async fn get_investigation_queue(
    State(state): State<AppState>,
    Query(params): Query<InvestigationQueueParams>,
) -> ApiResult<Json<Vec<DlqInvestigationQueueEntry>>> {
    debug!(limit = params.limit, "Fetching DLQ investigation queue");

    // Delegate to model layer
    let queue = DlqEntry::list_investigation_queue(state.orchestration_db_pool(), params.limit)
        .await
        .map_err(|e| {
            error!("Failed to fetch investigation queue: {}", e);
            ApiError::database_error(format!("Failed to fetch investigation queue: {}", e))
        })?;

    info!(
        queue_size = queue.len(),
        "Successfully fetched investigation queue"
    );
    Ok(Json(queue))
}

/// Get task staleness monitoring: GET /v1/dlq/staleness
///
/// Returns real-time staleness monitoring for active tasks in waiting states.
/// Provides proactive visibility into tasks approaching staleness thresholds,
/// enabling prevention of DLQ entries through early intervention.
///
/// # Query Parameters
///
/// - `limit` - Maximum tasks to return (default: 100)
///
/// # Response
///
/// Returns staleness monitoring entries with:
/// - Task UUID and metadata (namespace, name)
/// - Current state and time in state
/// - Staleness threshold and health status classification
/// - Health status: `healthy` (< 80%), `warning` (80-99%), `stale` (≥ 100%)
///
/// Results are ordered by health status (stale first) then time in state.
///
/// # Use Cases
///
/// - **Alerting**: Monitor tasks at 80%+ threshold (warning status)
/// - **Prevention**: Investigate tasks before they hit DLQ
/// - **Capacity Planning**: Identify systemic staleness patterns
///
/// # Example
///
/// ```text
/// GET /v1/dlq/staleness?limit=50
/// ```
///
/// **Required Permission:** `dlq:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/dlq/staleness",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum tasks to return (default: 100)")
    ),
    responses(
        (status = 200, description = "Task staleness monitoring data", body = Vec<StalenessMonitoring>),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 500, description = "Database error", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("dlq:read"))
    ),
    tag = "dlq"
))]
pub async fn get_staleness_monitoring(
    State(state): State<AppState>,
    Query(params): Query<StalenessMonitoringParams>,
) -> ApiResult<Json<Vec<StalenessMonitoring>>> {
    debug!(limit = params.limit, "Fetching staleness monitoring data");

    // Delegate to model layer
    let monitoring =
        DlqEntry::get_staleness_monitoring(state.orchestration_db_pool(), params.limit)
            .await
            .map_err(|e| {
                error!("Failed to fetch staleness monitoring: {}", e);
                ApiError::database_error(format!("Failed to fetch staleness monitoring: {}", e))
            })?;

    info!(
        monitoring_count = monitoring.len(),
        stale_count = monitoring
            .iter()
            .filter(|m| m.health_status.is_stale())
            .count(),
        warning_count = monitoring
            .iter()
            .filter(|m| m.health_status
                == tasker_shared::models::orchestration::StalenessHealthStatus::Warning)
            .count(),
        "Successfully fetched staleness monitoring data"
    );
    Ok(Json(monitoring))
}
