//! Simple V2 Configuration Loader (TAS-61)
//!
//! Dead-simple config loader that:
//! 0. Automatically loads .env file if present (via dotenvy)
//! 1. Reads a single pre-merged TOML file from TASKER_CONFIG_PATH
//! 2. Deserializes to TaskerConfigV2
//! 3. Performs environment variable substitution
//! 4. Validates with validator library
//!
//! No component merging, no complex path resolution, no fallbacks.
//! The CLI generates merged configs, we just load them.

use super::error::{ConfigResult, ConfigurationError};
use super::tasker::TaskerConfigV2;
use std::path::PathBuf;
use validator::Validate;

/// Simple configuration loader for pre-merged V2 TOML files
///
/// This is a zero-state utility struct providing static configuration loading functions.
/// All methods are associated functions (no self parameter).
#[derive(Debug)]
pub struct ConfigLoader;

impl ConfigLoader {
    /// Detect environment from TASKER_ENV variable or default to "development"
    pub fn detect_environment() -> String {
        std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string())
    }

    /// Load configuration from TASKER_CONFIG_PATH environment variable
    ///
    /// Automatically loads .env file if present before reading configuration.
    ///
    /// # Returns
    /// TaskerConfigV2 (canonical configuration)
    ///
    /// # Errors
    /// - TASKER_CONFIG_PATH not set
    /// - File not found or cannot be read
    /// - TOML parse errors
    /// - Validation errors
    pub fn load_from_env() -> ConfigResult<TaskerConfigV2> {
        // Load .env file if present (silently ignore if not found)
        dotenvy::dotenv().ok();

        let environment = Self::detect_environment();
        let config_path = std::env::var("TASKER_CONFIG_PATH").map_err(|_| {
            ConfigurationError::validation_error(
                "TASKER_CONFIG_PATH environment variable not set. \
                 Set it to the path of your pre-merged configuration file.",
            )
        })?;

        tracing::info!(
            "Loading configuration from TASKER_CONFIG_PATH: {} (environment: {})",
            config_path,
            environment
        );

        Self::load_from_path(&PathBuf::from(config_path))
    }

    /// Load configuration from a specific file path
    ///
    /// # Arguments
    /// * `path` - Path to pre-merged TOML configuration file
    ///
    /// # Returns
    /// TaskerConfigV2 (canonical configuration)
    pub fn load_from_path(path: &PathBuf) -> ConfigResult<TaskerConfigV2> {
        // 1. Read file
        let contents = std::fs::read_to_string(path).map_err(|e| {
            ConfigurationError::validation_error(format!(
                "Failed to read config file {}: {}",
                path.display(),
                e
            ))
        })?;

        // 2. Perform environment variable substitution
        let contents_with_env = Self::substitute_env_vars(&contents);

        // 3. Parse TOML
        let toml_value: toml::Value = toml::from_str(&contents_with_env)
            .map_err(|e| ConfigurationError::json_serialization_error("TOML parsing", e))?;

        // 4. Deserialize to TaskerConfigV2
        let config_v2: TaskerConfigV2 = toml_value.try_into().map_err(|e| {
            ConfigurationError::json_serialization_error(
                "TOML to TaskerConfigV2 deserialization",
                e,
            )
        })?;

        // 5. Validate
        config_v2.validate().map_err(|errors| {
            ConfigurationError::validation_error(format!(
                "Configuration validation failed: {:?}",
                errors
            ))
        })?;

        tracing::debug!("Successfully loaded and validated TaskerConfigV2");
        tracing::info!("Configuration loaded successfully from {}", path.display());

        Ok(config_v2)
    }

    /// Substitute environment variables in configuration content
    ///
    /// Replaces ${VAR_NAME} patterns with environment variable values
    fn substitute_env_vars(content: &str) -> String {
        let mut result = content.to_string();

        // Simple regex-free approach: find ${...} patterns
        while let Some(start) = result.find("${") {
            if let Some(end) = result[start..].find('}') {
                let var_name = &result[start + 2..start + end];
                if let Ok(var_value) = std::env::var(var_name) {
                    let pattern = format!("${{{}}}", var_name);
                    result = result.replace(&pattern, &var_value);
                } else {
                    tracing::warn!(
                        "Environment variable {} not found, leaving placeholder",
                        var_name
                    );
                    break; // Avoid infinite loop
                }
            } else {
                break; // Malformed ${...}
            }
        }

        result
    }
}

/// Configuration Manager with V2 configuration only
///
/// ## TAS-61 Phase 6C/6D: V2 Only
///
/// Holds only V2 configuration as the source of truth:
/// - `config_v2`: TaskerConfigV2 (canonical configuration)
///
/// Legacy config access has been removed.
#[derive(Debug, Clone)]
pub struct ConfigManager {
    /// V2 configuration (source of truth and only configuration)
    config_v2: TaskerConfigV2,
    environment: String,
}

impl ConfigManager {
    /// Load configuration from TASKER_CONFIG_PATH environment variable
    ///
    /// ## TAS-61 Phase 6C/6D: V2 Only
    ///
    /// Loads V2 config only - no legacy conversion
    pub fn load_from_env(environment: &str) -> ConfigResult<std::sync::Arc<ConfigManager>> {
        // Load .env file if present (silently ignore if not found)
        dotenvy::dotenv().ok();

        let config_path = std::env::var("TASKER_CONFIG_PATH").map_err(|_| {
            ConfigurationError::validation_error(
                "TASKER_CONFIG_PATH environment variable not set. \
                 Set it to the path of your pre-merged configuration file.",
            )
        })?;

        tracing::info!(
            "Loading configuration from TASKER_CONFIG_PATH: {} (environment: {}) [TAS-61 Phase 6C/6D: V2 only]",
            config_path,
            environment
        );

        // TAS-61 Phase 6C/6D: Load V2 config (source of truth and only config)
        let config_v2 = Self::load_v2_from_path(&PathBuf::from(&config_path))?;

        Ok(std::sync::Arc::new(ConfigManager {
            config_v2,
            environment: environment.to_string(),
        }))
    }

    /// Load TaskerConfigV2 from file path (internal helper)
    ///
    /// This is the V2 loading logic extracted for reuse.
    fn load_v2_from_path(path: &PathBuf) -> ConfigResult<TaskerConfigV2> {
        // 1. Read file
        let contents = std::fs::read_to_string(path).map_err(|e| {
            ConfigurationError::validation_error(format!(
                "Failed to read config file {}: {}",
                path.display(),
                e
            ))
        })?;

        // 2. Perform environment variable substitution
        let contents_with_env = ConfigLoader::substitute_env_vars(&contents);

        // 3. Parse TOML
        let toml_value: toml::Value = toml::from_str(&contents_with_env)
            .map_err(|e| ConfigurationError::json_serialization_error("TOML parsing", e))?;

        // 4. Deserialize to TaskerConfigV2
        let config_v2: TaskerConfigV2 = toml_value.try_into().map_err(|e| {
            ConfigurationError::json_serialization_error(
                "TOML to TaskerConfigV2 deserialization",
                e,
            )
        })?;

        // 5. Validate
        config_v2.validate().map_err(|errors| {
            ConfigurationError::validation_error(format!(
                "Configuration validation failed: {:?}",
                errors
            ))
        })?;

        tracing::debug!("Successfully loaded and validated TaskerConfigV2");

        Ok(config_v2)
    }

    /// Get reference to the V2 configuration
    ///
    /// TAS-61 Phase 6C/6D: This is the only configuration method.
    /// Returns the canonical TaskerConfigV2.
    pub fn config_v2(&self) -> &TaskerConfigV2 {
        &self.config_v2
    }

    /// Get the environment name
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Create ConfigManager from existing TaskerConfigV2
    ///
    /// TAS-61 Phase 6C/6D: Direct V2 construction for tests
    ///
    /// This is primarily for test compatibility where you have a V2 config
    /// constructed directly. For production use, prefer `load_from_env()`.
    pub fn from_config_v2(config_v2: TaskerConfigV2, environment: String) -> Self {
        Self {
            config_v2,
            environment,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_substitution() {
        std::env::set_var("TEST_VAR", "test_value");

        let input = "database_url = \"${TEST_VAR}\"";
        let output = ConfigLoader::substitute_env_vars(input);

        assert_eq!(output, "database_url = \"test_value\"");
    }

    #[test]
    fn test_detect_environment() {
        std::env::set_var("TASKER_ENV", "production");
        assert_eq!(ConfigLoader::detect_environment(), "production");

        std::env::remove_var("TASKER_ENV");
        assert_eq!(ConfigLoader::detect_environment(), "development");
    }
}
