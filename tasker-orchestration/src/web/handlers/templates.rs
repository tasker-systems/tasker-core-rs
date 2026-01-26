//! # Task Template Discovery Handlers
//!
//! TAS-76: Read-only endpoints for discovering available task templates.
//! Replaces the legacy /v1/handlers endpoints with a cleaner /v1/templates API
//! that better reflects the orchestration system's view of templates.
//!
//! Unlike workers, orchestration does not execute templates - it only stores
//! and manages template definitions. These endpoints provide visibility into
//! what templates are available for task creation.
//!
//! ## Pattern
//!
//! These handlers delegate to `TemplateQueryService` following the TAS-76
//! service layer pattern for gRPC reusability:
//!
//! ```text
//! Handler -> TemplateQueryService -> TaskHandlerRegistry (cached)
//!                                 -> Database (for listings)
//! ```

use axum::extract::{Path, Query, State};
use axum::Json;
use tracing::debug;

use crate::services::TemplateQueryError;
use crate::web::state::AppState;
use tasker_shared::types::api::templates::{
    TemplateDetail, TemplateListResponse, TemplatePathParams, TemplateQueryParams,
};
use tasker_shared::types::web::{ApiError, ApiResult};

/// List available templates
///
/// GET /v1/templates
///
/// Returns a list of all registered task templates, optionally filtered by namespace.
/// Includes namespace summaries and template details.
///
/// **Required Permission:** `templates:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/templates",
    params(TemplateQueryParams),
    responses(
        (status = 200, description = "List of available templates", body = TemplateListResponse),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("templates:read"))
    ),
    tag = "templates"
))]
pub async fn list_templates(
    State(state): State<AppState>,
    Query(params): Query<TemplateQueryParams>,
) -> ApiResult<Json<TemplateListResponse>> {
    debug!(namespace = ?params.namespace, "Listing available templates");

    // TAS-76: Delegate to service layer
    let response = state
        .template_query_service
        .list_templates(params.namespace.as_deref())
        .await
        .map_err(template_error_to_api_error)?;

    Ok(Json(response))
}

/// Get a specific template by namespace, name, and version
///
/// GET /v1/templates/{namespace}/{name}/{version}
///
/// Returns detailed information about a specific template including its step definitions.
///
/// **Required Permission:** `templates:read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/templates/{namespace}/{name}/{version}",
    params(
        ("namespace" = String, Path, description = "Template namespace"),
        ("name" = String, Path, description = "Template name"),
        ("version" = String, Path, description = "Template version")
    ),
    responses(
        (status = 200, description = "Template details", body = TemplateDetail),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 404, description = "Template not found", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("templates:read"))
    ),
    tag = "templates"
))]
pub async fn get_template(
    State(state): State<AppState>,
    Path(params): Path<TemplatePathParams>,
) -> ApiResult<Json<TemplateDetail>> {
    debug!(
        namespace = %params.namespace,
        name = %params.name,
        version = %params.version,
        "Getting template details"
    );

    // TAS-76: Delegate to service layer
    let response = state
        .template_query_service
        .get_template(&params.namespace, &params.name, &params.version)
        .await
        .map_err(template_error_to_api_error)?;

    Ok(Json(response))
}

/// Convert TemplateQueryError to ApiError
fn template_error_to_api_error(error: TemplateQueryError) -> ApiError {
    match error {
        TemplateQueryError::NamespaceNotFound(ns) => {
            ApiError::not_found(format!("Namespace not found: {ns}"))
        }
        TemplateQueryError::TemplateNotFound {
            namespace,
            name,
            version,
        } => ApiError::not_found(format!("Template not found: {namespace}/{name}/{version}")),
        TemplateQueryError::DatabaseError(e) => ApiError::database_error(e.to_string()),
        TemplateQueryError::Internal(msg) => ApiError::internal_server_error(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::types::api::templates::{NamespaceSummary, TemplateSummary};

    #[test]
    fn test_template_summary_creation() {
        let summary = TemplateSummary {
            name: "test_template".to_string(),
            namespace: "test_namespace".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test description".to_string()),
            step_count: 3,
        };

        assert_eq!(summary.name, "test_template");
        assert_eq!(summary.namespace, "test_namespace");
        assert_eq!(summary.version, "1.0.0");
        assert_eq!(summary.step_count, 3);
    }

    #[test]
    fn test_namespace_summary_creation() {
        let summary = NamespaceSummary {
            name: "test_namespace".to_string(),
            description: Some("Test namespace".to_string()),
            template_count: 5,
        };

        assert_eq!(summary.name, "test_namespace");
        assert_eq!(summary.template_count, 5);
    }

    #[test]
    fn test_template_error_conversion_not_found() {
        let error = TemplateQueryError::TemplateNotFound {
            namespace: "payments".to_string(),
            name: "process".to_string(),
            version: "1.0.0".to_string(),
        };

        let api_error = template_error_to_api_error(error);
        assert!(matches!(api_error, ApiError::NotFound { .. }));
    }

    #[test]
    fn test_template_error_conversion_namespace_not_found() {
        let error = TemplateQueryError::NamespaceNotFound("unknown".to_string());

        let api_error = template_error_to_api_error(error);
        assert!(matches!(api_error, ApiError::NotFound { .. }));
    }

    #[test]
    fn test_template_error_conversion_internal() {
        let error = TemplateQueryError::Internal("Something went wrong".to_string());

        let api_error = template_error_to_api_error(error);
        assert!(matches!(api_error, ApiError::Internal));
    }
}
