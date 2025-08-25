//! # Task Template Management Handlers
//!
//! HTTP handlers for task template operations including template retrieval,
//! cache management, and namespace validation.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, warn};
use chrono::Utc;

use crate::{
    task_template_manager::{CacheStats, WorkerTaskTemplateOperations},
    web::{response_types::ErrorResponse, state::WorkerWebState},
};
use tasker_shared::{
    models::core::task_template::ResolvedTaskTemplate,
    types::HandlerMetadata,
};

/// Helper function to create standardized error responses
fn error_response(error: String, message: String) -> ErrorResponse {
    ErrorResponse {
        error,
        message,
        timestamp: Utc::now(),
        request_id: None,
    }
}

/// Query parameters for template listing
#[derive(Debug, Deserialize)]
pub struct TemplateQueryParams {
    /// Filter by namespace
    pub namespace: Option<String>,
    /// Include cache statistics
    pub include_cache_stats: Option<bool>,
}

/// Path parameters for template operations
#[derive(Debug, Deserialize)]
pub struct TemplatePathParams {
    pub namespace: String,
    pub name: String,
    pub version: String,
}

/// Response for template retrieval
#[derive(Debug, Serialize)]
pub struct TemplateResponse {
    pub template: ResolvedTaskTemplate,
    pub handler_metadata: HandlerMetadata,
    pub cached: bool,
    pub cache_age_seconds: Option<u64>,
    pub access_count: Option<u64>,
}

/// Response for template listing
#[derive(Debug, Serialize)]
pub struct TemplateListResponse {
    pub supported_namespaces: Vec<String>,
    pub template_count: usize,
    pub cache_stats: Option<CacheStats>,
    pub worker_capabilities: Vec<String>,
}

/// Response for cache operations
#[derive(Debug, Serialize)]
pub struct CacheOperationResponse {
    pub operation: String,
    pub success: bool,
    pub cache_stats: CacheStats,
}

/// Response for template validation
#[derive(Debug, Serialize)]
pub struct TemplateValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub step_handlers: Vec<String>,
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
                    let cache_stats = state.task_template_manager.cache_stats();
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

    let supported_namespaces = state.supported_namespaces();
    
    // Filter by namespace if requested
    let filtered_namespaces = if let Some(filter_namespace) = &params.namespace {
        if state.is_namespace_supported(filter_namespace) {
            vec![filter_namespace.clone()]
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(error_response(
                    "namespace_not_supported".to_string(),
                    format!("Namespace '{}' is not supported by this worker", filter_namespace),
                )),
            ));
        }
    } else {
        supported_namespaces.clone()
    };

    // Get cache stats if requested
    let cache_stats = if params.include_cache_stats.unwrap_or(false) {
        Some(state.task_template_manager.cache_stats())
    } else {
        None
    };

    // TODO: Get actual template count from registry
    let template_count = 0;

    // Worker capabilities (static for now)
    let worker_capabilities = vec![
        "command_processing".to_string(),
        "database_access".to_string(),
        "message_queues".to_string(),
        "ffi_integration".to_string(),
    ];

    Ok(Json(TemplateListResponse {
        supported_namespaces: filtered_namespaces,
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
            if let Err(validation_error) = state.task_template_manager.validate_for_worker(&template) {
                errors.push(validation_error.to_string());
            }

            // Extract required capabilities and step handlers
            let required_capabilities = state.task_template_manager.requires_capabilities(&template);
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

    state.task_template_manager.clear_cache();
    let cache_stats = state.task_template_manager.cache_stats();

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
            let cache_stats = state.task_template_manager.cache_stats();
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

    state.maintain_template_cache();
    let cache_stats = state.task_template_manager.cache_stats();

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

    let cache_stats = state.task_template_manager.cache_stats();
    Ok(Json(cache_stats))
}