//! # Configuration Endpoint Handlers
//!
//! TAS-150: Runtime configuration observability with whitelist-only exposure.
//! Only explicitly chosen operational metadata is returned — no secrets, keys,
//! credentials, or database URLs can leak through this endpoint.

use axum::extract::State;
use axum::Json;
use chrono::Utc;
use tracing::debug;

use crate::web::middleware::permission::require_permission;
use crate::web::state::AppState;
use tasker_shared::types::api::orchestration::{
    ConfigMetadata, OrchestrationConfigResponse, SafeAuthConfig, SafeCircuitBreakerConfig,
    SafeDatabasePoolConfig, SafeMessagingConfig,
};
use tasker_shared::types::permissions::Permission;
use tasker_shared::types::security::SecurityContext;
use tasker_shared::types::web::ApiError;

/// Get orchestration configuration: GET /config
///
/// Returns operational configuration metadata safe for external consumption.
/// Only whitelisted fields are included — no secrets, keys, or credentials.
///
/// **Required Permission:** `system:config_read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/config",
    responses(
        (status = 200, description = "Orchestration configuration (safe fields only)", body = OrchestrationConfigResponse),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("system:config_read"))
    ),
    tag = "config"
))]
pub async fn get_config(
    State(state): State<AppState>,
    security: SecurityContext,
) -> Result<Json<OrchestrationConfigResponse>, ApiError> {
    require_permission(&security, Permission::SystemConfigRead)?;

    debug!("Retrieving orchestration configuration (safe fields only)");

    let tasker_config = &state.orchestration_core.context.tasker_config;
    let web_config = tasker_config
        .orchestration
        .as_ref()
        .and_then(|o| o.web.as_ref());
    let auth = web_config.and_then(|w| w.auth.as_ref());
    let queues = &tasker_config.common.queues;
    let cb = &tasker_config.common.circuit_breakers;

    let deployment_mode = tasker_config
        .orchestration
        .as_ref()
        .map(|o| format!("{:?}", o.event_systems.orchestration.deployment_mode))
        .unwrap_or_else(|| "Unknown".to_string());

    let db_pools = web_config
        .map(|w| &w.database_pools)
        .map(|p| SafeDatabasePoolConfig {
            web_api_pool_size: p.web_api_pool_size,
            web_api_max_connections: p.web_api_max_connections,
        })
        .unwrap_or(SafeDatabasePoolConfig {
            web_api_pool_size: 0,
            web_api_max_connections: 0,
        });

    let response = OrchestrationConfigResponse {
        metadata: ConfigMetadata {
            timestamp: Utc::now(),
            environment: tasker_config.common.execution.environment.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        auth: SafeAuthConfig {
            enabled: auth.map(|a| a.enabled).unwrap_or(false),
            verification_method: auth
                .map(|a| a.jwt_verification_method.clone())
                .unwrap_or_else(|| "none".to_string()),
            jwt_issuer: auth.map(|a| a.jwt_issuer.clone()).unwrap_or_default(),
            jwt_audience: auth.map(|a| a.jwt_audience.clone()).unwrap_or_default(),
            api_key_header: auth
                .map(|a| a.api_key_header.clone())
                .unwrap_or_else(|| "X-API-Key".to_string()),
            api_key_count: auth.map(|a| a.api_keys.len()).unwrap_or(0),
            strict_validation: auth.map(|a| a.strict_validation).unwrap_or(true),
            allowed_algorithms: auth
                .map(|a| a.jwt_allowed_algorithms.clone())
                .unwrap_or_else(|| vec!["RS256".to_string()]),
        },
        circuit_breakers: SafeCircuitBreakerConfig {
            enabled: cb.enabled,
            failure_threshold: cb.default_config.failure_threshold,
            timeout_seconds: cb.default_config.timeout_seconds,
            success_threshold: cb.default_config.success_threshold,
        },
        database_pools: db_pools,
        deployment_mode,
        messaging: SafeMessagingConfig {
            backend: queues.backend.clone(),
            queues: vec![
                queues.orchestration_queues.task_requests.clone(),
                queues.orchestration_queues.task_finalizations.clone(),
                queues.orchestration_queues.step_results.clone(),
            ],
        },
    };

    Ok(Json(response))
}
