//! # Handler Registry Endpoints
//!
//! Read-only endpoints for discovering available task handlers and namespaces.

use axum::extract::{Path, State};
use axum::Json;
use tracing::info;

use crate::web::circuit_breaker::execute_with_circuit_breaker;
use crate::web::state::AppState;
use tasker_shared::models::{NamedTask, TaskNamespace};
use tasker_shared::types::api::orchestration::{HandlerInfo, NamespaceInfo};
use tasker_shared::types::web::{ApiError, ApiResult, DbOperationType};

/// List available namespaces: GET /v1/handlers
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/handlers",
    responses(
        (status = 200, description = "List of available namespaces", body = Vec<NamespaceInfo>),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    tag = "handlers"
))]
pub async fn list_namespaces(State(state): State<AppState>) -> ApiResult<Json<Vec<NamespaceInfo>>> {
    info!("Listing available handler namespaces");

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::ReadOnly);

        let namespace_infos = TaskNamespace::get_namespace_info_with_handler_count(db_pool).await?;

        let result: Vec<NamespaceInfo> = namespace_infos
            .into_iter()
            .map(|(namespace, handler_count)| NamespaceInfo {
                name: namespace.name,
                description: namespace.description,
                handler_count: handler_count as u32,
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
        (status = 200, description = "List of handlers in namespace", body = Vec<HandlerInfo>),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    tag = "handlers"
))]
pub async fn list_namespace_handlers(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> ApiResult<Json<Vec<HandlerInfo>>> {
    info!(namespace = %namespace, "Listing handlers in namespace");

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::ReadOnly);

        let tasks = NamedTask::list_by_namespace_name(db_pool, &namespace).await?;

        let result: Vec<HandlerInfo> = tasks
            .into_iter()
            .map(|task| HandlerInfo {
                name: task.name,
                namespace: namespace.clone(),
                version: task.version,
                description: task.description,
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
        (status = 200, description = "Handler information", body = HandlerInfo),
        (status = 404, description = "Handler not found", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    tag = "handlers"
))]
pub async fn get_handler_info(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<HandlerInfo>> {
    info!(namespace = %namespace, name = %name, "Getting handler information");

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::ReadOnly);

        let task =
            NamedTask::find_latest_by_name_and_namespace_name(db_pool, &name, &namespace).await?;

        match task {
            Some(task) => Ok(Json(HandlerInfo {
                name: task.name,
                namespace,
                version: task.version,
                description: task.description,
                step_templates: Vec::new(), // Would need additional query to fetch step templates
            })),
            None => Err(ApiError::NotFound),
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_info_creation() {
        let namespace_info = NamespaceInfo {
            name: "test_namespace".to_string(),
            description: Some("Test description".to_string()),
            handler_count: 5,
        };

        assert_eq!(namespace_info.name, "test_namespace");
        assert_eq!(
            namespace_info.description,
            Some("Test description".to_string())
        );
        assert_eq!(namespace_info.handler_count, 5);
    }

    #[test]
    fn test_handler_info_creation() {
        let handler_info = HandlerInfo {
            name: "test_handler".to_string(),
            namespace: "test_namespace".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test handler description".to_string()),
            step_templates: vec!["step1".to_string(), "step2".to_string()],
        };

        assert_eq!(handler_info.name, "test_handler");
        assert_eq!(handler_info.namespace, "test_namespace");
        assert_eq!(handler_info.version, "1.0.0");
        assert_eq!(handler_info.step_templates.len(), 2);
    }
}
