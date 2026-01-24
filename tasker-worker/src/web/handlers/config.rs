//! # Configuration Endpoint Handlers
//!
//! TAS-77/TAS-150: Runtime configuration observability with whitelist-only exposure.
//! Only explicitly chosen operational metadata is returned.
//! Handlers delegate to ConfigQueryService for actual config operations.

use axum::extract::State;
use axum::Json;
use std::sync::Arc;

use crate::web::state::WorkerWebState;
use crate::worker::services::ConfigQueryError;
use tasker_shared::types::api::orchestration::WorkerConfigResponse;
use tasker_shared::types::permissions::Permission;
use tasker_shared::types::security::SecurityContext;
use tasker_shared::types::web::ApiError;

/// Convert ConfigQueryError to HTTP API error
fn config_error_to_api_error(error: ConfigQueryError) -> ApiError {
    match error {
        ConfigQueryError::WorkerConfigNotFound => {
            ApiError::internal_server_error("Worker configuration not found".to_string())
        }
        ConfigQueryError::SerializationError(msg) => {
            ApiError::internal_server_error(format!("Failed to serialize configuration: {}", msg))
        }
    }
}

/// Get worker configuration: GET /config
///
/// Returns operational configuration metadata safe for external consumption.
/// Only whitelisted fields are included — no secrets, keys, or credentials.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/config",
    responses(
        (status = 200, description = "Worker configuration (safe fields only)", body = WorkerConfigResponse),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 500, description = "Failed to retrieve configuration", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "config"
))]
pub async fn get_config(
    State(state): State<Arc<WorkerWebState>>,
    security: SecurityContext,
) -> Result<Json<WorkerConfigResponse>, ApiError> {
    crate::web::middleware::auth::require_permission(&security, Permission::WorkerConfigRead)?;

    state
        .config_query_service()
        .runtime_config()
        .map(Json)
        .map_err(config_error_to_api_error)
}
