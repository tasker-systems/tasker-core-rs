//! # Configuration Endpoint Handlers
//!
//! Runtime configuration observability endpoints for monitoring and debugging.
//! Provides a unified view of orchestration configuration (common + orchestration-specific)
//! with sensitive data redacted.

use axum::extract::State;
use axum::Json;
use chrono::Utc;
use tracing::debug;

use crate::web::state::AppState;
use tasker_shared::types::api::orchestration::{
    redact_secrets, ConfigMetadata, OrchestrationConfigResponse,
};
use tasker_shared::types::web::ApiError;

/// Get complete orchestration configuration: GET /config
///
/// Returns the complete orchestration configuration including both common (shared) and
/// orchestration-specific settings with sensitive values redacted. This provides a unified
/// view of the deployed system configuration in a single response.
///
/// The response includes:
/// - `common`: Shared configuration (database, circuit breakers, telemetry, etc.)
/// - `orchestration`: Orchestration-specific configuration (web API, MPSC channels, etc.)
/// - `metadata`: Response metadata including which fields were redacted for transparency
///
/// This design makes it easy to compare configurations across systems and debug
/// deployment issues with a single curl command.
///
/// This is a system endpoint (like /health) and is at the root level, not under /v1/.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/config",
    responses(
        (status = 200, description = "Complete orchestration configuration (secrets redacted)", body = OrchestrationConfigResponse),
        (status = 500, description = "Failed to retrieve configuration", body = ApiError)
    ),
    tag = "config"
))]
pub async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<OrchestrationConfigResponse>, ApiError> {
    debug!("Retrieving complete orchestration configuration");

    let tasker_config = &state.orchestration_core.context.tasker_config;

    // Build common config JSON
    let common_json = serde_json::json!({
        "database": tasker_config.database,
        "circuit_breakers": tasker_config.circuit_breakers,
        "telemetry": tasker_config.telemetry,
        "system": tasker_config.system,
        "backoff": tasker_config.backoff,
        "task_templates": tasker_config.task_templates,
    });

    // Get orchestration-specific config
    let orchestration_json = serde_json::to_value(&tasker_config.orchestration).map_err(|e| {
        ApiError::internal_server_error(format!(
            "Failed to serialize orchestration configuration: {}",
            e
        ))
    })?;

    // Redact sensitive fields from both
    let (redacted_common, mut redacted_fields) = redact_secrets(common_json);
    let (redacted_orchestration, orchestration_fields) = redact_secrets(orchestration_json);
    redacted_fields.extend(orchestration_fields);

    let response = OrchestrationConfigResponse {
        environment: tasker_config.environment().to_string(),
        common: redacted_common,
        orchestration: redacted_orchestration,
        metadata: ConfigMetadata {
            timestamp: Utc::now(),
            source: "runtime".to_string(),
            redacted_fields,
        },
    };

    Ok(Json(response))
}
