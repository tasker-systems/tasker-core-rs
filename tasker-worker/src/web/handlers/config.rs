//! # Configuration Endpoint Handlers
//!
//! Runtime configuration observability endpoints for worker monitoring and debugging.
//! Provides a unified view of worker configuration (common + worker-specific)
//! with sensitive data redacted.

use axum::extract::State;
use axum::Json;
use chrono::Utc;
use std::sync::Arc;
use tracing::debug;

use crate::web::state::WorkerWebState;
use tasker_shared::types::api::orchestration::{
    redact_secrets, ConfigMetadata, WorkerConfigResponse,
};
use tasker_shared::types::web::ApiError;

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
        (status = 500, description = "Failed to retrieve configuration", body = ApiError)
    ),
    tag = "config"
))]
pub async fn get_config(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<WorkerConfigResponse>, ApiError> {
    debug!("Retrieving complete worker configuration");

    let system_config = &state.system_config;

    // Build common config JSON
    let common_json = serde_json::json!({
        "database": system_config.database,
        "circuit_breakers": system_config.circuit_breakers,
        "telemetry": system_config.telemetry,
        "system": system_config.system,
        "backoff": system_config.backoff,
        "task_templates": system_config.task_templates,
    });

    // Get worker-specific configuration
    let worker_config = system_config.worker.as_ref().ok_or_else(|| {
        ApiError::internal_server_error("Worker configuration not found".to_string())
    })?;

    let worker_json = serde_json::to_value(worker_config).map_err(|e| {
        ApiError::internal_server_error(format!("Failed to serialize worker configuration: {}", e))
    })?;

    // Redact sensitive fields from both
    let (redacted_common, mut redacted_fields) = redact_secrets(common_json);
    let (redacted_worker, worker_fields) = redact_secrets(worker_json);
    redacted_fields.extend(worker_fields);

    let response = WorkerConfigResponse {
        environment: system_config.environment().to_string(),
        common: redacted_common,
        worker: redacted_worker,
        metadata: ConfigMetadata {
            timestamp: Utc::now(),
            source: "runtime".to_string(),
            redacted_fields,
        },
    };

    Ok(Json(response))
}
