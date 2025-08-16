//! Worker Configuration Manager for FFI
//!
//! Provides configuration loading capabilities for Ruby, Python, and other language workers
//! using the unified TOML configuration system. This enables all workers to use the same
//! configuration source with consistent validation and error handling.
//!
//! Uses serde_magnus for direct Rust-to-Ruby conversion, avoiding inefficient JSON proxy patterns.

use crate::config::error::{ConfigResult, ConfigurationError};
use crate::config::unified_loader::UnifiedConfigLoader;
use crate::config::TaskerConfig;
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Worker Configuration Manager for FFI boundaries
///
/// This provides a simplified interface for language bindings to load and access
/// the unified TOML configuration system. Uses direct TaskerConfig for serde_magnus
/// conversion, avoiding inefficient JSON proxy patterns.
#[derive(Debug)]
pub struct WorkerConfigManager {
    /// Loaded and validated configuration as TaskerConfig
    config: TaskerConfig,
    /// Environment name
    environment: String,
    /// Configuration root path
    config_root: PathBuf,
}

impl WorkerConfigManager {
    /// Create a new WorkerConfigManager with automatic environment detection
    ///
    /// This uses the same environment detection as the Rust system:
    /// TASKER_ENV || RAILS_ENV || RACK_ENV || APP_ENV || 'development'
    ///
    /// # Returns
    /// * `ConfigResult<Self>` - Manager instance or configuration error
    pub fn new() -> ConfigResult<Self> {
        let environment = UnifiedConfigLoader::detect_environment();
        Self::new_with_environment(&environment)
    }

    /// Create a new WorkerConfigManager with explicit environment
    ///
    /// # Arguments
    /// * `environment` - Environment name (development, test, production, etc.)
    ///
    /// # Returns
    /// * `ConfigResult<Self>` - Manager instance or configuration error
    pub fn new_with_environment(environment: &str) -> ConfigResult<Self> {
        info!(
            "🔧 WORKER_CONFIG: Initializing for environment '{}'",
            environment
        );

        let mut loader = UnifiedConfigLoader::new(environment)?;
        let config = loader.load_tasker_config()?;
        let config_root = loader.root().to_path_buf();

        info!(
            "✅ WORKER_CONFIG: Configuration loaded successfully for '{}'",
            environment
        );

        Ok(Self {
            config,
            environment: environment.to_string(),
            config_root,
        })
    }

    /// Create a new WorkerConfigManager with explicit config root path
    ///
    /// This is useful for embedded applications where the config path is known.
    ///
    /// # Arguments
    /// * `config_root` - Path to config/tasker directory
    /// * `environment` - Environment name
    ///
    /// # Returns
    /// * `ConfigResult<Self>` - Manager instance or configuration error
    pub fn new_with_config_root<P: AsRef<Path>>(
        config_root: P,
        environment: &str,
    ) -> ConfigResult<Self> {
        let config_root = config_root.as_ref().to_path_buf();

        info!(
            "🔧 WORKER_CONFIG: Initializing with explicit root '{}' for environment '{}'",
            config_root.display(),
            environment
        );

        let mut loader = UnifiedConfigLoader::with_root(config_root.clone(), environment)?;
        let config = loader.load_tasker_config()?;

        info!("✅ WORKER_CONFIG: Configuration loaded successfully");

        Ok(Self {
            config,
            environment: environment.to_string(),
            config_root,
        })
    }

    /// Get the complete configuration as TaskerConfig for serde_magnus conversion
    ///
    /// This provides direct access to the TaskerConfig struct, which can be
    /// efficiently converted to Ruby using serde_magnus, avoiding JSON proxy patterns.
    ///
    /// # Returns
    /// * `&TaskerConfig` - Direct reference to TaskerConfig
    pub fn get_config(&self) -> &TaskerConfig {
        debug!("Providing direct access to TaskerConfig for serde_magnus conversion");
        &self.config
    }

    /// Get the complete configuration as JSON for legacy FFI consumption
    ///
    /// This method is kept for backward compatibility but should be avoided
    /// in favor of direct serde_magnus conversion using get_config().
    ///
    /// # Returns
    /// * `ConfigResult<JsonValue>` - Complete configuration as JSON
    pub fn get_config_as_json(&self) -> ConfigResult<JsonValue> {
        debug!("Converting configuration to JSON for legacy FFI consumption");

        // Convert TaskerConfig to JSON using serde_json
        serde_json::to_value(&self.config).map_err(|e| {
            ConfigurationError::json_serialization_error("TaskerConfig to JSON conversion", e)
        })
    }

    /// Get a specific component configuration as JSON
    ///
    /// # Arguments
    /// * `component_name` - Name of the component (database, orchestration, etc.)
    ///
    /// # Returns
    /// * `ConfigResult<Option<JsonValue>>` - Component configuration or None if not found
    pub fn get_component_as_json(&self, component_name: &str) -> ConfigResult<Option<JsonValue>> {
        debug!(
            "Extracting component '{}' from TaskerConfig",
            component_name
        );

        // Extract component from TaskerConfig based on component name
        let component_result = match component_name {
            "database" => serde_json::to_value(&self.config.database).map_err(|e| {
                ConfigurationError::json_serialization_error("database component serialization", e)
            }),
            "orchestration" => serde_json::to_value(&self.config.orchestration).map_err(|e| {
                ConfigurationError::json_serialization_error(
                    "orchestration component serialization",
                    e,
                )
            }),
            "executor_pools" => serde_json::to_value(&self.config.executor_pools).map_err(|e| {
                ConfigurationError::json_serialization_error(
                    "executor_pools component serialization",
                    e,
                )
            }),
            "pgmq" => serde_json::to_value(&self.config.pgmq).map_err(|e| {
                ConfigurationError::json_serialization_error("pgmq component serialization", e)
            }),
            "auth" => serde_json::to_value(&self.config.auth).map_err(|e| {
                ConfigurationError::json_serialization_error("auth component serialization", e)
            }),
            "telemetry" => serde_json::to_value(&self.config.telemetry).map_err(|e| {
                ConfigurationError::json_serialization_error("telemetry component serialization", e)
            }),
            "engine" => serde_json::to_value(&self.config.engine).map_err(|e| {
                ConfigurationError::json_serialization_error("engine component serialization", e)
            }),
            "system" => serde_json::to_value(&self.config.system).map_err(|e| {
                ConfigurationError::json_serialization_error("system component serialization", e)
            }),
            "circuit_breakers" => {
                serde_json::to_value(&self.config.circuit_breakers).map_err(|e| {
                    ConfigurationError::json_serialization_error(
                        "circuit_breakers component serialization",
                        e,
                    )
                })
            }
            "query_cache" => serde_json::to_value(&self.config.query_cache).map_err(|e| {
                ConfigurationError::json_serialization_error(
                    "query_cache component serialization",
                    e,
                )
            }),
            _ => {
                warn!("Component '{}' not found in TaskerConfig", component_name);
                return Ok(None);
            }
        };

        component_result.map(Some)
    }

    /// Get available component names
    ///
    /// # Returns
    /// * `Vec<String>` - List of available component names
    pub fn get_component_names(&self) -> Vec<String> {
        vec![
            "database".to_string(),
            "orchestration".to_string(),
            "executor_pools".to_string(),
            "pgmq".to_string(),
            "auth".to_string(),
            "telemetry".to_string(),
            "engine".to_string(),
            "system".to_string(),
            "circuit_breakers".to_string(),
            "query_cache".to_string(),
        ]
    }

    /// Get the current environment name
    ///
    /// # Returns
    /// * `&str` - Environment name
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Get the configuration root path
    ///
    /// # Returns
    /// * `&Path` - Configuration root path
    pub fn config_root(&self) -> &Path {
        &self.config_root
    }

    /// Check if unified TOML configuration is available
    ///
    /// This can be used by workers to determine if they should use the new
    /// unified configuration or fall back to legacy YAML configuration.
    ///
    /// # Arguments
    /// * `config_dir` - Base configuration directory to check
    ///
    /// # Returns
    /// * `bool` - True if unified TOML structure is available
    pub fn is_unified_config_available<P: AsRef<Path>>(config_dir: P) -> bool {
        let tasker_root = config_dir.as_ref().join("tasker");
        let base_dir = tasker_root.join("base");
        let environments_dir = tasker_root.join("environments");

        let has_structure = base_dir.exists()
            && base_dir.join("database.toml").exists()
            && environments_dir.exists();

        debug!(
            "Unified config availability check: {} (path: {})",
            has_structure,
            tasker_root.display()
        );

        has_structure
    }

    /// Get configuration summary for debugging
    ///
    /// Returns a summary of the loaded configuration that's safe for logging
    /// (no sensitive information exposed).
    ///
    /// # Returns
    /// * `ConfigResult<JsonValue>` - Configuration summary
    pub fn get_config_summary(&self) -> ConfigResult<JsonValue> {
        let component_names = self.get_component_names();
        let component_count = component_names.len();

        let summary = serde_json::json!({
            "environment": self.environment,
            "config_root": self.config_root.display().to_string(),
            "component_count": component_count,
            "components": component_names
        });

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_unified_config_detection() {
        // Test unified config detection logic without actual file I/O dependency
        let temp_dir = TempDir::new().unwrap();
        let config_root = temp_dir.path();

        // Empty directory should not have unified config
        assert!(!WorkerConfigManager::is_unified_config_available(
            config_root
        ));

        // Create the expected directory structure: config_root/tasker/base/
        let tasker_dir = config_root.join("tasker");
        let base_dir = tasker_dir.join("base");
        let environments_dir = tasker_dir.join("environments");
        fs::create_dir_all(&base_dir).unwrap();
        fs::create_dir_all(&environments_dir).unwrap();

        // Just directories without TOML files should still not be detected
        assert!(!WorkerConfigManager::is_unified_config_available(
            config_root
        ));

        // Add required TOML file to make it a valid unified config
        fs::write(
            base_dir.join("database.toml"),
            "[database]\nhost = 'localhost'",
        )
        .unwrap();

        // Now it should be detected
        assert!(WorkerConfigManager::is_unified_config_available(
            config_root
        ));
    }

    // Note: Other FFI tests removed as they were testing file I/O
    // rather than FFI-specific functionality. The actual FFI behavior
    // is tested by Ruby integration tests and example usage.
}
