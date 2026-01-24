//! # Configuration Endpoint Handlers
//!
//! Runtime configuration observability endpoints for worker monitoring and debugging.
//! Provides a unified view of worker configuration (common + worker-specific)
//! with sensitive data redacted.
//!
//! TAS-77: Handlers now delegate to ConfigQueryService for actual config operations,
//! enabling the same functionality to be accessed via FFI.

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

/// Get complete worker configuration: GET /config
///
/// Returns the complete worker configuration including both common (shared) and
/// worker-specific settings with sensitive values redacted. This provides a unified
/// view of the deployed worker system configuration in a single response.
///
/// The response includes:
/// - `common`: Shared configuration (database, circuit breakers, telemetry, etc.)
/// - `worker`: Worker-specific configuration (template paths, handler discovery, etc.)
/// - `metadata`: Response metadata including which fields were redacted for transparency
///
/// This design makes it easy to compare configurations across systems and debug
/// deployment issues with a single curl command.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/config",
    responses(
        (status = 200, description = "Complete worker configuration (secrets redacted)", body = WorkerConfigResponse),
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
