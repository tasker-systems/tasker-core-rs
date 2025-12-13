//! # Task Template Management Handlers
//!
//! HTTP handlers for task template operations including template retrieval,
//! cache management, and namespace validation.
//!
//! TAS-77: Handlers now delegate to TemplateQueryService for actual template operations,
//! enabling the same functionality to be accessed via FFI.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use std::sync::Arc;

use crate::web::state::WorkerWebState;
use crate::worker::services::TemplateQueryError;
use tasker_shared::types::api::worker::{
    CacheOperationResponse, TemplateListResponse, TemplatePathParams, TemplateQueryParams,
    TemplateResponse, TemplateValidationResponse,
};
use tasker_shared::types::base::CacheStats;
use tasker_shared::types::web::ErrorResponse;

/// Helper function to create standardized error responses
fn error_response(error: String, message: String) -> ErrorResponse {
    ErrorResponse {
        error,
        message,
        timestamp: Utc::now(),
        request_id: None,
    }
}

/// Convert TemplateQueryError to HTTP response
fn template_error_to_response(error: TemplateQueryError) -> (StatusCode, Json<ErrorResponse>) {
    match error {
        TemplateQueryError::NotFound {
            namespace,
            name,
            version,
        } => (
            StatusCode::NOT_FOUND,
            Json(error_response(
                "template_not_found".to_string(),
                format!("Template not found: {}/{}/{}", namespace, name, version),
            )),
        ),
        TemplateQueryError::HandlerMetadataNotFound {
            namespace,
            name,
            version,
        } => (
            StatusCode::NOT_FOUND,
            Json(error_response(
                "handler_metadata_not_found".to_string(),
                format!(
                    "Handler metadata not found: {}/{}/{}",
                    namespace, name, version
                ),
            )),
        ),
        TemplateQueryError::RefreshFailed { message } => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response(
                "refresh_failed".to_string(),
                format!("Failed to refresh template: {}", message),
            )),
        ),
        TemplateQueryError::Internal(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("internal_error".to_string(), message)),
        ),
    }
}

/// Get a specific task template
///
/// GET /templates/{namespace}/{name}/{version}
pub async fn get_template(
    State(state): State<Arc<WorkerWebState>>,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<TemplateResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .template_query_service()
        .get_template(&params.namespace, &params.name, &params.version)
        .await
        .map(Json)
        .map_err(template_error_to_response)
}

/// List supported templates and namespaces
///
/// GET /templates
pub async fn list_templates(
    State(state): State<Arc<WorkerWebState>>,
    Query(params): Query<TemplateQueryParams>,
) -> Result<Json<TemplateListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let include_cache_stats = params.include_cache_stats.unwrap_or(false);
    Ok(Json(
        state
            .template_query_service()
            .list_templates(include_cache_stats)
            .await,
    ))
}

/// Validate a template for worker execution
///
/// POST /templates/{namespace}/{name}/{version}/validate
pub async fn validate_template(
    State(state): State<Arc<WorkerWebState>>,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<TemplateValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .template_query_service()
        .validate_template(&params.namespace, &params.name, &params.version)
        .await
        .map(Json)
        .map_err(template_error_to_response)
}

/// Clear template cache
///
/// DELETE /templates/cache
pub async fn clear_cache(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(state.template_query_service().clear_cache().await))
}

/// Refresh specific template in cache
///
/// POST /templates/{namespace}/{name}/{version}/refresh
pub async fn refresh_template(
    State(state): State<Arc<WorkerWebState>>,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .template_query_service()
        .refresh_template(&params.namespace, &params.name, &params.version)
        .await
        .map(Json)
        .map_err(template_error_to_response)
}

/// Perform cache maintenance
///
/// POST /templates/cache/maintain
pub async fn maintain_cache(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(state.template_query_service().maintain_cache().await))
}

/// Get cache statistics
///
/// GET /templates/cache/stats
pub async fn get_cache_stats(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<CacheStats>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(state.template_query_service().cache_stats().await))
}
