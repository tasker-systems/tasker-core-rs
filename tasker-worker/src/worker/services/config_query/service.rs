//! # Config Query Service
//!
//! TAS-77: Configuration query logic extracted from web/handlers/config.rs.
//!
//! This service encapsulates all configuration query functionality, making it available
//! to both the HTTP API and FFI consumers without code duplication.

use chrono::Utc;
use thiserror::Error;
use tracing::debug;

use tasker_shared::config::tasker::TaskerConfig;
use tasker_shared::types::api::orchestration::{
    redact_secrets, ConfigMetadata, WorkerConfigResponse,
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
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_worker::worker::services::config_query::ConfigQueryService;
/// use tasker_shared::config::tasker::TaskerConfig;
///
/// fn example(system_config: TaskerConfig) -> Result<(), Box<dyn std::error::Error>> {
///     let service = ConfigQueryService::new(system_config);
///
///     // Get runtime configuration (with sensitive fields redacted)
///     let config = service.runtime_config()?;
///
///     // Access configuration components
///     println!("Environment: {}", config.environment);
///     Ok(())
/// }
/// ```
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

    /// Get complete worker configuration: GET /config
    ///
    /// Returns the complete worker configuration including both common (shared) and
    /// worker-specific settings with sensitive values redacted.
    ///
    /// The response includes:
    /// - `common`: Shared configuration (database, circuit breakers, telemetry, etc.)
    /// - `worker`: Worker-specific configuration (template paths, handler discovery, etc.)
    /// - `metadata`: Response metadata including which fields were redacted
    pub fn runtime_config(&self) -> Result<WorkerConfigResponse, ConfigQueryError> {
        debug!("Retrieving complete worker configuration");

        // Build common config JSON (TAS-61 Phase 6D: V2 config structure)
        let common_json = serde_json::json!({
            "database": self.system_config.common.database,
            "circuit_breakers": self.system_config.common.circuit_breakers,
            "telemetry": self.system_config.common.telemetry,
            "system": self.system_config.common.system,
            "backoff": self.system_config.common.backoff,
            "task_templates": self.system_config.common.task_templates,
        });

        // Get worker-specific configuration
        let worker_config = self
            .system_config
            .worker
            .as_ref()
            .ok_or(ConfigQueryError::WorkerConfigNotFound)?;

        let worker_json = serde_json::to_value(worker_config)
            .map_err(|e| ConfigQueryError::SerializationError(e.to_string()))?;

        // Redact sensitive fields from both
        let (redacted_common, mut redacted_fields) = redact_secrets(common_json);
        let (redacted_worker, worker_fields) = redact_secrets(worker_json);
        redacted_fields.extend(worker_fields);

        Ok(WorkerConfigResponse {
            environment: self.system_config.common.execution.environment.clone(),
            common: redacted_common,
            worker: redacted_worker,
            metadata: ConfigMetadata {
                timestamp: Utc::now(),
                source: "runtime".to_string(),
                redacted_fields,
            },
        })
    }

    /// Get common configuration only (redacted)
    ///
    /// Returns only the common (shared) configuration with sensitive fields redacted.
    pub fn common_config(&self) -> (serde_json::Value, Vec<String>) {
        let common_json = serde_json::json!({
            "database": self.system_config.common.database,
            "circuit_breakers": self.system_config.common.circuit_breakers,
            "telemetry": self.system_config.common.telemetry,
            "system": self.system_config.common.system,
            "backoff": self.system_config.common.backoff,
            "task_templates": self.system_config.common.task_templates,
        });

        redact_secrets(common_json)
    }

    /// Get worker configuration only (redacted)
    ///
    /// Returns only the worker-specific configuration with sensitive fields redacted.
    pub fn worker_config(&self) -> Result<(serde_json::Value, Vec<String>), ConfigQueryError> {
        let worker_config = self
            .system_config
            .worker
            .as_ref()
            .ok_or(ConfigQueryError::WorkerConfigNotFound)?;

        let worker_json = serde_json::to_value(worker_config)
            .map_err(|e| ConfigQueryError::SerializationError(e.to_string()))?;

        Ok(redact_secrets(worker_json))
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
