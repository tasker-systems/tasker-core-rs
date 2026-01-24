//! # Config Query Service
//!
//! TAS-77/TAS-150: Configuration query logic with whitelist-only exposure.
//! Only explicitly chosen operational metadata is returned — no secrets,
//! keys, credentials, or database URLs can leak through this service.

use chrono::Utc;
use thiserror::Error;
use tracing::debug;

use tasker_shared::config::tasker::TaskerConfig;
use tasker_shared::types::api::orchestration::{
    ConfigMetadata, SafeAuthConfig, SafeMessagingConfig, WorkerConfigResponse,
};

/// Errors that can occur during config query operations
#[derive(Error, Debug)]
pub enum ConfigQueryError {
    /// Worker configuration not found
    #[error("Worker configuration not found")]
    WorkerConfigNotFound,

    /// Serialization error
    #[error("Failed to serialize configuration: {0}")]
    SerializationError(String),
}

/// Config Query Service
///
/// TAS-77: Provides configuration query functionality independent of the HTTP layer.
///
/// This service can be used by:
/// - Web API handlers (via `WorkerWebState`)
/// - FFI consumers (Ruby, Python, etc.)
/// - Internal systems
pub struct ConfigQueryService {
    /// System configuration reference
    system_config: TaskerConfig,
}

impl std::fmt::Debug for ConfigQueryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigQueryService")
            .field(
                "environment",
                &self.system_config.common.execution.environment,
            )
            .finish()
    }
}

impl ConfigQueryService {
    /// Create a new ConfigQueryService
    pub fn new(system_config: TaskerConfig) -> Self {
        Self { system_config }
    }

    /// Get the environment name
    pub fn environment(&self) -> &str {
        &self.system_config.common.execution.environment
    }

    /// Get access to the underlying config (not redacted)
    ///
    /// Use with caution - this returns the raw config with secrets intact.
    /// For external-facing responses, use `runtime_config()` instead.
    pub fn raw_config(&self) -> &TaskerConfig {
        &self.system_config
    }

    // =========================================================================
    // Configuration Query Methods
    // =========================================================================

    /// Get worker configuration (safe fields only): GET /config
    ///
    /// Returns only whitelisted operational metadata. No secrets, keys,
    /// credentials, or database URLs are included.
    pub fn runtime_config(&self) -> Result<WorkerConfigResponse, ConfigQueryError> {
        debug!("Retrieving worker configuration (safe fields only)");

        let worker_config = self
            .system_config
            .worker
            .as_ref()
            .ok_or(ConfigQueryError::WorkerConfigNotFound)?;

        let auth = worker_config.web.as_ref().and_then(|w| w.auth.as_ref());
        let queues = &self.system_config.common.queues;

        // Worker namespace queues use the naming pattern
        let worker_namespace = &queues.worker_namespace;
        let worker_queues = vec![
            queues
                .naming_pattern
                .replace("{namespace}", worker_namespace)
                .replace("{name}", "dispatch"),
            queues
                .naming_pattern
                .replace("{namespace}", worker_namespace)
                .replace("{name}", "completion"),
        ];

        Ok(WorkerConfigResponse {
            metadata: ConfigMetadata {
                timestamp: Utc::now(),
                environment: self.system_config.common.execution.environment.clone(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            worker_id: worker_config.worker_id.clone(),
            worker_type: worker_config.worker_type.clone(),
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
            messaging: SafeMessagingConfig {
                backend: queues.backend.clone(),
                queues: worker_queues,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_query_error_display() {
        let error = ConfigQueryError::WorkerConfigNotFound;
        let display = format!("{}", error);
        assert!(display.contains("Worker configuration not found"));
    }

    #[test]
    fn test_serialization_error_display() {
        let error = ConfigQueryError::SerializationError("test error".to_string());
        let display = format!("{}", error);
        assert!(display.contains("test error"));
    }
}
