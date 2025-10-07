//! # Task Template Management Handlers
//!
//! HTTP handlers for task template operations including template retrieval,
//! cache management, and namespace validation.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use std::sync::Arc;
use tracing::{debug, error, warn};

use crate::{
    web::state::WorkerWebState, worker::task_template_manager::WorkerTaskTemplateOperations,
};
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

/// Get a specific task template
///
/// GET /templates/{namespace}/{name}/{version}
pub async fn get_template(
    State(state): State<Arc<WorkerWebState>>,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<TemplateResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(
        namespace = %params.namespace,
        name = %params.name,
        version = %params.version,
        "Getting task template"
    );

    // Get the template from the task template manager
    match state
        .task_template_manager
        .get_task_template(&params.namespace, &params.name, &params.version)
        .await
    {
        Ok(template) => {
            // Get handler metadata as well
            match state
                .task_template_manager
                .get_handler_metadata(&params.namespace, &params.name, &params.version)
                .await
            {
                Ok(handler_metadata) => {
                    // Check if it was cached
                    let cache_stats = state.task_template_manager.cache_stats().await;
                    let cached = cache_stats.total_cached > 0;

                    Ok(Json(TemplateResponse {
                        template,
                        handler_metadata,
                        cached,
                        cache_age_seconds: None, // TODO: Calculate from cached entry
                        access_count: None,      // TODO: Get from cached entry
                    }))
                }
                Err(e) => {
                    error!(
                        namespace = %params.namespace,
                        name = %params.name,
                        version = %params.version,
                        error = %e,
                        "Failed to get handler metadata for template"
                    );
                    Err((
                        StatusCode::NOT_FOUND,
                        Json(error_response(
                            "handler_metadata_not_found".to_string(),
                            format!("Handler metadata not found: {}", e),
                        )),
                    ))
                }
            }
        }
        Err(e) => {
            warn!(
                namespace = %params.namespace,
                name = %params.name,
                version = %params.version,
                error = %e,
                "Template not found or access error"
            );
            Err((
                StatusCode::NOT_FOUND,
                Json(error_response(
                    "template_not_found".to_string(),
                    format!("Template not found: {}", e),
                )),
            ))
        }
    }
}

/// List supported templates and namespaces
///
/// GET /templates
pub async fn list_templates(
    State(state): State<Arc<WorkerWebState>>,
    Query(params): Query<TemplateQueryParams>,
) -> Result<Json<TemplateListResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Listing supported templates and namespaces");

    let supported_namespaces = state.supported_namespaces().await;

    // Get cache stats if requested
    let cache_stats = if params.include_cache_stats.unwrap_or(false) {
        Some(state.task_template_manager.cache_stats().await)
    } else {
        None
    };

    let template_count = state.task_template_manager.cache_stats().await.total_cached;

    // Worker capabilities (static for now)
    let worker_capabilities = vec![
        "command_processing".to_string(),
        "database_access".to_string(),
        "message_queues".to_string(),
        "ffi_integration".to_string(),
    ];

    Ok(Json(TemplateListResponse {
        supported_namespaces,
        template_count,
        cache_stats,
        worker_capabilities,
    }))
}

/// Validate a template for worker execution
///
/// POST /templates/{namespace}/{name}/{version}/validate
pub async fn validate_template(
    State(state): State<Arc<WorkerWebState>>,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<TemplateValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(
        namespace = %params.namespace,
        name = %params.name,
        version = %params.version,
        "Validating template for worker execution"
    );

    // Get the template
    match state
        .task_template_manager
        .get_task_template(&params.namespace, &params.name, &params.version)
        .await
    {
        Ok(template) => {
            let mut errors = Vec::new();

            // Validate template for worker execution
            if let Err(validation_error) = state
                .task_template_manager
                .validate_for_worker(&template)
                .await
            {
                errors.push(validation_error.to_string());
            }

            // Extract required capabilities and step handlers
            let required_capabilities =
                state.task_template_manager.requires_capabilities(&template);
            let step_handlers = state.task_template_manager.extract_step_handlers(&template);

            Ok(Json(TemplateValidationResponse {
                valid: errors.is_empty(),
                errors,
                required_capabilities,
                step_handlers,
            }))
        }
        Err(e) => {
            warn!(
                namespace = %params.namespace,
                name = %params.name,
                version = %params.version,
                error = %e,
                "Template not found for validation"
            );
            Err((
                StatusCode::NOT_FOUND,
                Json(error_response(
                    "template_not_found".to_string(),
                    format!("Template not found: {}", e),
                )),
            ))
        }
    }
}

/// Clear template cache
///
/// DELETE /templates/cache
pub async fn clear_cache(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Clearing template cache");

    state.task_template_manager.clear_cache().await;
    let cache_stats = state.task_template_manager.cache_stats().await;

    Ok(Json(CacheOperationResponse {
        operation: "clear".to_string(),
        success: true,
        cache_stats,
    }))
}

/// Refresh specific template in cache
///
/// POST /templates/{namespace}/{name}/{version}/refresh
pub async fn refresh_template(
    State(state): State<Arc<WorkerWebState>>,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(
        namespace = %params.namespace,
        name = %params.name,
        version = %params.version,
        "Refreshing template in cache"
    );

    match state
        .task_template_manager
        .refresh_template(&params.namespace, &params.name, &params.version)
        .await
    {
        Ok(()) => {
            let cache_stats = state.task_template_manager.cache_stats().await;
            Ok(Json(CacheOperationResponse {
                operation: "refresh".to_string(),
                success: true,
                cache_stats,
            }))
        }
        Err(e) => {
            error!(
                namespace = %params.namespace,
                name = %params.name,
                version = %params.version,
                error = %e,
                "Failed to refresh template"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(
                    "refresh_failed".to_string(),
                    format!("Failed to refresh template: {}", e),
                )),
            ))
        }
    }
}

/// Perform cache maintenance
///
/// POST /templates/cache/maintain
pub async fn maintain_cache(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Performing template cache maintenance");

    state.maintain_template_cache().await;
    let cache_stats = state.task_template_manager.cache_stats().await;

    Ok(Json(CacheOperationResponse {
        operation: "maintain".to_string(),
        success: true,
        cache_stats,
    }))
}

/// Get cache statistics
///
/// GET /templates/cache/stats
pub async fn get_cache_stats(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<CacheStats>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Getting template cache statistics");

    let cache_stats = state.task_template_manager.cache_stats().await;
    Ok(Json(cache_stats))
}
