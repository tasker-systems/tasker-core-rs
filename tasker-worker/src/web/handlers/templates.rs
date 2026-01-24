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
use tasker_shared::types::permissions::Permission;
use tasker_shared::types::security::SecurityContext;
use tasker_shared::types::web::ErrorResponse;

/// Check permission and convert to handler error format.
fn check_permission(
    ctx: &SecurityContext,
    perm: Permission,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    crate::web::middleware::auth::require_permission(ctx, perm).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(error_response(
                "FORBIDDEN".to_string(),
                format!("Missing required permission: {perm}"),
            )),
        )
    })
}

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
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/templates/{namespace}/{name}/{version}",
    params(
        ("namespace" = String, Path, description = "Template namespace"),
        ("name" = String, Path, description = "Template name"),
        ("version" = String, Path, description = "Template version")
    ),
    responses(
        (status = 200, description = "Template details", body = TemplateResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Template not found")
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "templates"
))]
pub async fn get_template(
    State(state): State<Arc<WorkerWebState>>,
    security: SecurityContext,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<TemplateResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_permission(&security, Permission::WorkerTemplatesRead)?;

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
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/templates",
    params(
        ("namespace" = Option<String>, Query, description = "Filter by namespace"),
        ("include_cache_stats" = Option<bool>, Query, description = "Include cache statistics")
    ),
    responses(
        (status = 200, description = "List of templates", body = TemplateListResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Insufficient permissions")
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "templates"
))]
pub async fn list_templates(
    State(state): State<Arc<WorkerWebState>>,
    security: SecurityContext,
    Query(params): Query<TemplateQueryParams>,
) -> Result<Json<TemplateListResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_permission(&security, Permission::WorkerTemplatesRead)?;

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
#[cfg_attr(feature = "web-api", utoipa::path(
    post,
    path = "/templates/{namespace}/{name}/{version}/validate",
    params(
        ("namespace" = String, Path, description = "Template namespace"),
        ("name" = String, Path, description = "Template name"),
        ("version" = String, Path, description = "Template version")
    ),
    responses(
        (status = 200, description = "Validation result", body = TemplateValidationResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Template not found")
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "templates"
))]
pub async fn validate_template(
    State(state): State<Arc<WorkerWebState>>,
    security: SecurityContext,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<TemplateValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_permission(&security, Permission::TemplatesValidate)?;

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
#[cfg_attr(feature = "web-api", utoipa::path(
    delete,
    path = "/templates/cache",
    responses(
        (status = 200, description = "Cache cleared", body = CacheOperationResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Insufficient permissions")
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "templates"
))]
pub async fn clear_cache(
    State(state): State<Arc<WorkerWebState>>,
    security: SecurityContext,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_permission(&security, Permission::WorkerTemplatesRead)?;

    Ok(Json(state.template_query_service().clear_cache().await))
}

/// Refresh specific template in cache
///
/// POST /templates/{namespace}/{name}/{version}/refresh
#[cfg_attr(feature = "web-api", utoipa::path(
    post,
    path = "/templates/{namespace}/{name}/{version}/refresh",
    params(
        ("namespace" = String, Path, description = "Template namespace"),
        ("name" = String, Path, description = "Template name"),
        ("version" = String, Path, description = "Template version")
    ),
    responses(
        (status = 200, description = "Template refreshed", body = CacheOperationResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Insufficient permissions"),
        (status = 500, description = "Refresh failed")
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "templates"
))]
pub async fn refresh_template(
    State(state): State<Arc<WorkerWebState>>,
    security: SecurityContext,
    Path(params): Path<TemplatePathParams>,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_permission(&security, Permission::WorkerTemplatesRead)?;

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
#[cfg_attr(feature = "web-api", utoipa::path(
    post,
    path = "/templates/cache/maintain",
    responses(
        (status = 200, description = "Maintenance completed", body = CacheOperationResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Insufficient permissions")
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "templates"
))]
pub async fn maintain_cache(
    State(state): State<Arc<WorkerWebState>>,
    security: SecurityContext,
) -> Result<Json<CacheOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_permission(&security, Permission::WorkerTemplatesRead)?;

    Ok(Json(state.template_query_service().maintain_cache().await))
}

/// Get cache statistics
///
/// GET /templates/cache/stats
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/templates/cache/stats",
    responses(
        (status = 200, description = "Cache statistics", body = CacheStats),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Insufficient permissions")
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "templates"
))]
pub async fn get_cache_stats(
    State(state): State<Arc<WorkerWebState>>,
    security: SecurityContext,
) -> Result<Json<CacheStats>, (StatusCode, Json<ErrorResponse>)> {
    check_permission(&security, Permission::WorkerTemplatesRead)?;

    Ok(Json(state.template_query_service().cache_stats().await))
}
