// TAS-50: WorkerConfig - Language-agnostic worker configuration
//
// This configuration is used by all worker languages (Rust/Ruby/Python/WASM).
// It contains worker-specific components:
// - Worker system settings (web API, health monitoring, step processing)
// - Worker event systems (real-time coordination)
// - Worker MPSC channels (command processor, event listeners)
// - Template path with ENV override support (TASKER_TEMPLATE_PATH)
//
// Phase 1 implementation: Full implementation with conversion from TaskerConfig

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ConfigurationContext;
use crate::config::components::WorkerChannelsConfig;
use crate::config::error::ConfigurationError;
use crate::config::event_systems::WorkerEventSystemConfig;
use crate::config::worker::WorkerConfig as LegacyWorkerConfig;
use crate::config::TaskerConfig;

/// Language-agnostic worker configuration
///
/// Contains all configuration needed by worker systems regardless of language:
/// - Worker system behavior (web API, health monitoring)
/// - Event-driven coordination
/// - MPSC channel buffer sizing
/// - Template discovery path
///
/// This configuration is used by:
/// - Rust workers (via tasker-worker library directly)
/// - Ruby workers (via FFI to tasker-worker library)
/// - Python workers (future: via FFI to tasker-worker library)
/// - WASM workers (future: via FFI to tasker-worker library)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Worker system configuration (web API, health monitoring, step processing)
    pub worker_system: LegacyWorkerConfig,

    /// Worker event system configuration
    pub worker_events: WorkerEventSystemConfig,

    /// MPSC channels configuration for worker
    pub mpsc_channels: WorkerChannelsConfig,

    /// Template path for task handler discovery
    /// Can be overridden by TASKER_TEMPLATE_PATH environment variable
    pub template_path: Option<PathBuf>,

    /// Current environment (cached from common config)
    pub environment: String,
}

impl WorkerConfig {
    /// Get the template path with environment variable override support
    ///
    /// Priority:
    /// 1. TASKER_TEMPLATE_PATH environment variable (if set)
    /// 2. template_path from TOML configuration (if set)
    /// 3. None (no template path configured)
    ///
    /// This allows K8s deployments to override template paths without
    /// modifying TOML files, and enables testing flexibility.
    pub fn effective_template_path(&self) -> Option<PathBuf> {
        // Check for environment variable override first
        if let Ok(env_path) = std::env::var("TASKER_TEMPLATE_PATH") {
            if !env_path.is_empty() {
                return Some(PathBuf::from(env_path));
            }
        }

        // Fall back to TOML configuration
        self.template_path.clone()
    }
}

impl ConfigurationContext for WorkerConfig {
    fn validate(&self) -> Result<(), Vec<ConfigurationError>> {
        let mut errors = Vec::new();

        // Environment validation
        if self.environment.is_empty() {
            errors.push(ConfigurationError::MissingRequiredField {
                field: "environment".to_string(),
                context: "WorkerConfig".to_string(),
            });
        }

        // MPSC channel validation
        if self.mpsc_channels.command_processor.command_buffer_size == 0 {
            errors.push(ConfigurationError::InvalidValue {
                field: "mpsc_channels.command_processor.command_buffer_size".to_string(),
                value: "0".to_string(),
                context: "command_buffer_size must be greater than 0".to_string(),
            });
        }

        // Worker system validation
        if self.worker_system.web.enabled && self.worker_system.web.bind_address.is_empty() {
            errors.push(ConfigurationError::InvalidValue {
                field: "worker_system.web.bind_address".to_string(),
                value: "".to_string(),
                context: "bind_address must not be empty when web API is enabled".to_string(),
            });
        }

        // Template path validation (if set)
        if let Some(path) = &self.template_path {
            if path.as_os_str().is_empty() {
                errors.push(ConfigurationError::InvalidValue {
                    field: "template_path".to_string(),
                    value: "".to_string(),
                    context: "template_path must not be empty if specified".to_string(),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn environment(&self) -> &str {
        &self.environment
    }

    fn summary(&self) -> String {
        let template_path_str = self
            .effective_template_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not set".to_string());

        format!(
            "WorkerConfig: environment={}, web_enabled={}, event_mode={:?}, template_path={}",
            self.environment,
            self.worker_system.web.enabled,
            self.worker_events.deployment_mode,
            template_path_str
        )
    }
}

impl From<&TaskerConfig> for WorkerConfig {
    /// Convert legacy TaskerConfig to WorkerConfig
    ///
    /// This conversion extracts only the worker-specific configuration fields
    /// from the monolithic TaskerConfig structure.
    ///
    /// Note: template_path is initially None and can be set via environment
    /// variable TASKER_TEMPLATE_PATH at runtime.
    fn from(config: &TaskerConfig) -> Self {
        Self {
            worker_system: config.worker.clone().unwrap_or_default(),
            worker_events: config.event_systems.worker.clone(),
            mpsc_channels: config.mpsc_channels.worker.clone(),
            template_path: None, // Set via TASKER_TEMPLATE_PATH env var or future TOML config
            environment: config.execution.environment.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_worker_config_from_tasker_config() {
        // Create a test TaskerConfig
        let tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };

        // Convert to WorkerConfig
        let worker_config = WorkerConfig::from(&tasker_config);

        // Verify fields were correctly extracted
        assert_eq!(
            worker_config.environment,
            tasker_config.execution.environment
        );
        assert_eq!(
            worker_config.worker_system.web.enabled,
            tasker_config.worker.as_ref().unwrap().web.enabled
        );
    }

    #[test]
    fn test_worker_config_validation_success() {
        let tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };

        let worker_config = WorkerConfig::from(&tasker_config);

        // Should pass validation
        assert!(worker_config.validate().is_ok());
    }

    #[test]
    fn test_worker_config_validation_zero_command_buffer() {
        let mut tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };
        tasker_config
            .mpsc_channels
            .worker
            .command_processor
            .command_buffer_size = 0;

        let worker_config = WorkerConfig::from(&tasker_config);

        // Should fail validation
        let result = worker_config.validate();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(
            e,
            ConfigurationError::InvalidValue { field, .. }
                if field == "mpsc_channels.command_processor.command_buffer_size"
        )));
    }

    #[test]
    #[serial]
    fn test_worker_config_effective_template_path_from_env() {
        // Clean up first to ensure no interference
        std::env::remove_var("TASKER_TEMPLATE_PATH");

        // Set environment variable
        std::env::set_var("TASKER_TEMPLATE_PATH", "/custom/path");

        let tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };

        let worker_config = WorkerConfig::from(&tasker_config);

        // Should use environment variable override
        let effective_path = worker_config.effective_template_path();
        assert_eq!(effective_path, Some(PathBuf::from("/custom/path")));

        // Clean up
        std::env::remove_var("TASKER_TEMPLATE_PATH");
    }

    #[test]
    #[serial]
    fn test_worker_config_effective_template_path_from_toml() {
        // Ensure no environment variable
        std::env::remove_var("TASKER_TEMPLATE_PATH");

        let tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };

        let mut worker_config = WorkerConfig::from(&tasker_config);
        worker_config.template_path = Some(PathBuf::from("/toml/path"));

        // Should use TOML configuration
        let effective_path = worker_config.effective_template_path();
        assert_eq!(effective_path, Some(PathBuf::from("/toml/path")));
    }

    #[test]
    #[serial]
    fn test_worker_config_effective_template_path_env_overrides_toml() {
        // Clean up first to ensure no interference
        std::env::remove_var("TASKER_TEMPLATE_PATH");

        // Set environment variable
        std::env::set_var("TASKER_TEMPLATE_PATH", "/env/path");

        let tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };

        let mut worker_config = WorkerConfig::from(&tasker_config);
        worker_config.template_path = Some(PathBuf::from("/toml/path"));

        // Environment variable should override TOML
        let effective_path = worker_config.effective_template_path();
        assert_eq!(effective_path, Some(PathBuf::from("/env/path")));

        // Clean up
        std::env::remove_var("TASKER_TEMPLATE_PATH");
    }

    #[test]
    #[serial]
    fn test_worker_config_effective_template_path_none() {
        // Ensure no environment variable
        std::env::remove_var("TASKER_TEMPLATE_PATH");

        let tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };

        let worker_config = WorkerConfig::from(&tasker_config);

        // Should return None when neither is set
        let effective_path = worker_config.effective_template_path();
        assert_eq!(effective_path, None);
    }

    #[test]
    fn test_worker_config_summary() {
        let tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };

        let worker_config = WorkerConfig::from(&tasker_config);

        let summary = worker_config.summary();
        assert!(summary.contains("WorkerConfig"));
        assert!(summary.contains(&worker_config.environment));
    }

    #[test]
    fn test_worker_config_validation_empty_template_path() {
        let tasker_config = TaskerConfig {
            worker: Some(LegacyWorkerConfig::default()),
            ..Default::default()
        };

        let mut worker_config = WorkerConfig::from(&tasker_config);
        worker_config.template_path = Some(PathBuf::from(""));

        // Should fail validation
        let result = worker_config.validate();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigurationError::InvalidValue { field, .. } if field == "template_path")));
    }
}
