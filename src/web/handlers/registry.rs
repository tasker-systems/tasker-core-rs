//! # Handler Registry Endpoints
//!
//! Read-only endpoints for discovering available task handlers and namespaces.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use tracing::info;

use crate::web::circuit_breaker::execute_with_circuit_breaker;
use crate::web::errors::{ApiError, ApiResult};
use crate::web::state::AppState;

#[cfg(feature = "web-api")]
// use utoipa::ToSchema;

/// Namespace information
#[derive(Debug, Serialize)]
pub struct NamespaceInfo {
    pub name: String,
    pub description: Option<String>,
    pub handler_count: u32,
}

/// Handler information
#[derive(Debug, Serialize)]
pub struct HandlerInfo {
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub description: Option<String>,
    pub step_templates: Vec<String>,
}

/// List available namespaces: GET /v1/handlers
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/handlers",
    responses(
        (status = 200, description = "List of available namespaces", body = crate::web::openapi::NamespaceListResponse),
        (status = 503, description = "Service unavailable", body = crate::web::openapi::ApiError)
    ),
    tag = "handlers"
))]
pub async fn list_namespaces(State(state): State<AppState>) -> ApiResult<Json<Vec<NamespaceInfo>>> {
    info!("Listing available handler namespaces");

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(crate::web::state::DbOperationType::ReadOnly);

        let namespaces = sqlx::query!(
            r#"
            SELECT
                tn.name,
                tn.description,
                COUNT(nt.named_task_uuid) as handler_count
            FROM tasker_task_namespaces tn
            LEFT JOIN tasker_named_tasks nt ON tn.task_namespace_uuid = nt.task_namespace_uuid
            GROUP BY tn.task_namespace_uuid, tn.name, tn.description
            ORDER BY tn.name
            "#
        )
        .fetch_all(db_pool)
        .await?;

        let result: Vec<NamespaceInfo> = namespaces
            .into_iter()
            .map(|row| NamespaceInfo {
                name: row.name,
                description: row.description,
                handler_count: row.handler_count.unwrap_or(0) as u32,
            })
            .collect();

        Ok::<axum::Json<Vec<NamespaceInfo>>, sqlx::Error>(Json(result))
    })
    .await
}

/// List handlers in a namespace: GET /v1/handlers/{namespace}
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/handlers/{namespace}",
    params(
        ("namespace" = String, Path, description = "Namespace name")
    ),
    responses(
        (status = 200, description = "List of handlers in namespace", body = crate::web::openapi::HandlerListResponse),
        (status = 503, description = "Service unavailable", body = crate::web::openapi::ApiError)
    ),
    tag = "handlers"
))]
pub async fn list_namespace_handlers(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> ApiResult<Json<Vec<HandlerInfo>>> {
    info!(namespace = %namespace, "Listing handlers in namespace");

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(crate::web::state::DbOperationType::ReadOnly);

        let handlers = sqlx::query!(
            r#"
            SELECT
                nt.name,
                tn.name as namespace_name,
                nt.version,
                nt.description
            FROM tasker_named_tasks nt
            JOIN tasker_task_namespaces tn ON nt.task_namespace_uuid = tn.task_namespace_uuid
            WHERE tn.name = $1
            ORDER BY nt.name, nt.version
            "#,
            namespace
        )
        .fetch_all(db_pool)
        .await?;

        let result: Vec<HandlerInfo> = handlers
            .into_iter()
            .map(|row| HandlerInfo {
                name: row.name,
                namespace: row.namespace_name,
                version: row.version,
                description: row.description,
                step_templates: Vec::new(), // Would need additional query to fetch step templates
            })
            .collect();

        Ok::<axum::Json<Vec<HandlerInfo>>, sqlx::Error>(Json(result))
    })
    .await
}

/// Get handler information: GET /v1/handlers/{namespace}/{name}
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/handlers/{namespace}/{name}",
    params(
        ("namespace" = String, Path, description = "Namespace name"),
        ("name" = String, Path, description = "Handler name")
    ),
    responses(
        (status = 200, description = "Handler information", body = crate::web::openapi::HandlerInfoResponse),
        (status = 404, description = "Handler not found", body = crate::web::openapi::ApiError),
        (status = 503, description = "Service unavailable", body = crate::web::openapi::ApiError)
    ),
    tag = "handlers"
))]
pub async fn get_handler_info(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<HandlerInfo>> {
    info!(namespace = %namespace, name = %name, "Getting handler information");

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(crate::web::state::DbOperationType::ReadOnly);

        let handler = sqlx::query!(
            r#"
            SELECT
                nt.name,
                tn.name as namespace_name,
                nt.version,
                nt.description
            FROM tasker_named_tasks nt
            JOIN tasker_task_namespaces tn ON nt.task_namespace_uuid = tn.task_namespace_uuid
            WHERE tn.name = $1 AND nt.name = $2
            ORDER BY nt.version DESC
            LIMIT 1
            "#,
            namespace,
            name
        )
        .fetch_optional(db_pool)
        .await?;

        match handler {
            Some(row) => Ok(Json(HandlerInfo {
                name: row.name,
                namespace: row.namespace_name,
                version: row.version,
                description: row.description,
                step_templates: Vec::new(), // Would need additional query to fetch step templates
            })),
            None => Err(ApiError::NotFound),
        }
    })
    .await
}
