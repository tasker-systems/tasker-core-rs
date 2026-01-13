//! # Client Configuration
//!
//! Configuration management for tasker-client library and CLI.
//! Supports environment variables, config files, and command-line overrides.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::error::{ClientError, ClientResult};

/// Client configuration for API connections and CLI behavior
///
/// # Examples
///
/// ```rust
/// use tasker_client::config::ClientConfig;
///
/// // Default configuration
/// let config = ClientConfig::default();
/// assert_eq!(config.orchestration.base_url, "http://localhost:8080");
/// assert_eq!(config.worker.base_url, "http://localhost:8081");
/// ```
///
/// ```rust,no_run
/// use tasker_client::config::ClientConfig;
///
/// // Load configuration from environment and config files
/// let config = ClientConfig::load().expect("Failed to load config");
///
/// // Access orchestration API settings
/// println!("Orchestration URL: {}", config.orchestration.base_url);
/// println!("Timeout: {}ms", config.orchestration.timeout_ms);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Orchestration API configuration
    pub orchestration: ApiEndpointConfig,
    /// Worker API configuration
    pub worker: ApiEndpointConfig,
    /// CLI-specific settings
    pub cli: CliConfig,
}

/// API endpoint configuration
///
/// Configuration for connecting to a single API endpoint (orchestration or worker).
///
/// # Examples
///
/// ```rust
/// use tasker_client::config::ApiEndpointConfig;
///
/// let config = ApiEndpointConfig {
///     base_url: "https://orchestration.example.com".to_string(),
///     timeout_ms: 60000,
///     max_retries: 5,
///     auth_token: Some("secret-token".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpointConfig {
    /// Base URL for the API (e.g., "<http://localhost:8080>")
    pub base_url: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retry attempts for failed requests
    pub max_retries: u32,
    /// API authentication token (if required)
    pub auth_token: Option<String>,
}

/// CLI-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Default output format (json, table, yaml)
    pub default_format: String,
    /// Enable colored output
    pub colored_output: bool,
    /// Verbose logging level
    pub verbose_level: u8,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            orchestration: ApiEndpointConfig {
                base_url: "http://localhost:8080".to_string(),
                timeout_ms: 30000,
                max_retries: 3,
                auth_token: None,
            },
            worker: ApiEndpointConfig {
                base_url: "http://localhost:8081".to_string(),
                timeout_ms: 30000,
                max_retries: 3,
                auth_token: None,
            },
            cli: CliConfig {
                default_format: "table".to_string(),
                colored_output: true,
                verbose_level: 0,
            },
        }
    }
}

impl ClientConfig {
    /// Load configuration from environment variables and config file
    ///
    /// Precedence (highest to lowest):
    /// 1. Environment variables
    /// 2. Config file (~/.tasker/config.toml)
    /// 3. Default values
    pub fn load() -> ClientResult<Self> {
        let mut config = Self::default();

        // Try to load from config file
        if let Some(config_path) = Self::find_config_file() {
            debug!("Loading config from: {}", config_path.display());
            match Self::load_from_file(&config_path) {
                Ok(file_config) => config = file_config,
                Err(e) => {
                    debug!("Failed to load config file: {}", e);
                    // Continue with defaults if config file fails
                }
            }
        }

        // Override with environment variables
        config.apply_env_overrides();

        debug!("Loaded client configuration: {:?}", config);
        Ok(config)
    }

    /// Load configuration from specific file
    pub fn load_from_file(path: &Path) -> ClientResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ClientError::config_error(format!("Failed to read config file: {}", e)))?;

        let config: Self = toml::from_str(&content).map_err(|e| {
            ClientError::config_error(format!("Failed to parse config file: {}", e))
        })?;

        Ok(config)
    }

    /// Find the config file in standard locations
    fn find_config_file() -> Option<PathBuf> {
        let possible_paths = [
            // Current directory
            Path::new("./tasker-client.toml"),
            Path::new("./config/tasker-client.toml"),
            // User home directory
            &dirs::home_dir()?.join(".tasker").join("config.toml"),
            &dirs::config_dir()?.join("tasker").join("client.toml"),
        ];

        for path in &possible_paths {
            if path.exists() && path.is_file() {
                return Some(path.to_path_buf());
            }
        }

        None
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // Orchestration API overrides
        if let Ok(url) = std::env::var("TASKER_ORCHESTRATION_URL") {
            self.orchestration.base_url = url;
        }
        if let Ok(timeout) = std::env::var("TASKER_ORCHESTRATION_TIMEOUT_MS") {
            if let Ok(timeout_ms) = timeout.parse() {
                self.orchestration.timeout_ms = timeout_ms;
            }
        }
        if let Ok(token) = std::env::var("TASKER_ORCHESTRATION_AUTH_TOKEN") {
            self.orchestration.auth_token = Some(token);
        }

        // Worker API overrides
        if let Ok(url) = std::env::var("TASKER_WORKER_URL") {
            self.worker.base_url = url;
        }
        if let Ok(timeout) = std::env::var("TASKER_WORKER_TIMEOUT_MS") {
            if let Ok(timeout_ms) = timeout.parse() {
                self.worker.timeout_ms = timeout_ms;
            }
        }
        if let Ok(token) = std::env::var("TASKER_WORKER_AUTH_TOKEN") {
            self.worker.auth_token = Some(token);
        }

        // CLI overrides
        if let Ok(format) = std::env::var("TASKER_CLI_FORMAT") {
            self.cli.default_format = format;
        }
        if let Ok(colored) = std::env::var("TASKER_CLI_COLORED") {
            self.cli.colored_output = colored.parse().unwrap_or(true);
        }
    }

    /// Save configuration to file
    pub fn save_to_file(&self, path: &Path) -> ClientResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ClientError::config_error(format!("Failed to create config directory: {}", e))
            })?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| ClientError::config_error(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(path, content).map_err(|e| {
            ClientError::config_error(format!("Failed to write config file: {}", e))
        })?;

        Ok(())
    }

    /// Get default config file path
    pub fn default_config_path() -> ClientResult<PathBuf> {
        let home_dir = dirs::home_dir()
            .ok_or_else(|| ClientError::config_error("Could not determine home directory"))?;

        Ok(home_dir.join(".tasker").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = ClientConfig::default();
        assert_eq!(config.orchestration.base_url, "http://localhost:8080");
        assert_eq!(config.worker.base_url, "http://localhost:8081");
        assert_eq!(config.cli.default_format, "table");
    }

    #[test]
    fn test_config_serialization() {
        let config = ClientConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: ClientConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(
            config.orchestration.base_url,
            deserialized.orchestration.base_url
        );
        assert_eq!(config.worker.base_url, deserialized.worker.base_url);
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test-config.toml");

        let original_config = ClientConfig::default();
        original_config.save_to_file(&config_path).unwrap();

        let loaded_config = ClientConfig::load_from_file(&config_path).unwrap();
        assert_eq!(
            original_config.orchestration.base_url,
            loaded_config.orchestration.base_url
        );
    }
}
