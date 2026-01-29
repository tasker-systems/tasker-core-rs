//! # Client Configuration
//!
//! Configuration management for tasker-client library and CLI.
//! Supports environment variables, config files, profiles, and command-line overrides.
//!
//! ## Profile System (TAS-177)
//!
//! Similar to nextest, the client supports named profiles in `.config/tasker-client.toml`:
//!
//! ```toml
//! [profile.default]
//! transport = "rest"
//!
//! [profile.default.orchestration]
//! base_url = "http://localhost:8080"
//!
//! [profile.grpc]
//! transport = "grpc"
//!
//! [profile.grpc.orchestration]
//! base_url = "http://localhost:9090"
//! ```
//!
//! Load a specific profile:
//! ```rust,ignore
//! let config = ClientConfig::load_profile("grpc")?;
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::error::{ClientError, ClientResult};

// =============================================================================
// Transport Selection (TAS-177)
// =============================================================================

/// Transport protocol for API communication.
///
/// Determines whether the client uses REST (HTTP/JSON) or gRPC for communication
/// with orchestration and worker services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// REST API using HTTP/JSON (default)
    #[default]
    Rest,
    /// gRPC API using Protocol Buffers
    Grpc,
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Transport::Rest => write!(f, "rest"),
            Transport::Grpc => write!(f, "grpc"),
        }
    }
}

impl std::str::FromStr for Transport {
    type Err = ClientError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rest" | "http" => Ok(Transport::Rest),
            "grpc" => Ok(Transport::Grpc),
            _ => Err(ClientError::config_error(format!(
                "Invalid transport '{}': expected 'rest' or 'grpc'",
                s
            ))),
        }
    }
}

// =============================================================================
// Profile Configuration (TAS-177)
// =============================================================================

/// Profile-based configuration file structure.
///
/// Similar to nextest.toml, this allows named profiles for different scenarios.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileConfigFile {
    /// Named profiles (e.g., "default", "grpc", "auth", "ci")
    #[serde(default)]
    pub profile: HashMap<String, ProfileConfig>,
}

/// A single named profile configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Transport protocol (rest or grpc)
    #[serde(default)]
    pub transport: Option<Transport>,
    /// Orchestration endpoint configuration
    #[serde(default)]
    pub orchestration: Option<ProfileEndpointConfig>,
    /// Worker endpoint configuration
    #[serde(default)]
    pub worker: Option<ProfileEndpointConfig>,
    /// CLI-specific settings
    #[serde(default)]
    pub cli: Option<ProfileCliConfig>,
}

/// Partial endpoint configuration for profiles (all fields optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileEndpointConfig {
    /// Base URL for the API
    pub base_url: Option<String>,
    /// Request timeout in milliseconds
    pub timeout_ms: Option<u64>,
    /// Maximum retry attempts
    pub max_retries: Option<u32>,
    /// Authentication configuration
    pub auth: Option<ClientAuthConfig>,
}

/// Partial CLI configuration for profiles (all fields optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileCliConfig {
    /// Default output format
    pub default_format: Option<String>,
    /// Enable colored output
    pub colored_output: Option<bool>,
    /// Verbose logging level
    pub verbose_level: Option<u8>,
}

// =============================================================================
// Main Client Configuration
// =============================================================================

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
///
/// ```rust,no_run
/// use tasker_client::config::ClientConfig;
///
/// // Load a specific profile (TAS-177)
/// let config = ClientConfig::load_profile("grpc").expect("Failed to load profile");
/// println!("Transport: {}", config.transport);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Transport protocol (rest or grpc) - TAS-177
    #[serde(default)]
    pub transport: Transport,
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
///     auth: None,
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
    /// API authentication token (legacy field, prefer `auth`)
    #[serde(default)]
    pub auth_token: Option<String>,
    /// Authentication configuration (TAS-150)
    #[serde(default)]
    pub auth: Option<ClientAuthConfig>,
}

/// Client-side authentication configuration (TAS-150)
///
/// Specifies how the client should authenticate when making API requests.
/// Supports Bearer token and API key authentication methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientAuthConfig {
    /// The authentication method and credentials
    pub method: ClientAuthMethod,
}

/// Authentication method for client API requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ClientAuthMethod {
    /// Bearer token authentication (JWT or opaque token)
    BearerToken(String),
    /// API key authentication with custom header
    ApiKey {
        /// The API key value
        key: String,
        /// The header name to use (default: X-API-Key)
        #[serde(default = "default_api_key_header")]
        header_name: String,
    },
}

fn default_api_key_header() -> String {
    "X-API-Key".to_string()
}

impl ApiEndpointConfig {
    /// Resolve the effective authentication configuration as a `WebAuthConfig`.
    ///
    /// Prefers the structured `auth` field over the legacy `auth_token` field.
    /// Returns `None` if no authentication is configured.
    pub fn resolve_web_auth_config(&self) -> Option<tasker_shared::config::WebAuthConfig> {
        if let Some(ref auth) = self.auth {
            Some(auth.to_web_auth_config())
        } else {
            self.auth_token
                .as_ref()
                .map(|token| tasker_shared::config::WebAuthConfig {
                    enabled: true,
                    bearer_token: token.clone(),
                    ..Default::default()
                })
        }
    }
}

impl ClientAuthConfig {
    /// Convert to a `WebAuthConfig` for use with API clients.
    pub fn to_web_auth_config(&self) -> tasker_shared::config::WebAuthConfig {
        match &self.method {
            ClientAuthMethod::BearerToken(token) => tasker_shared::config::WebAuthConfig {
                enabled: true,
                bearer_token: token.clone(),
                ..Default::default()
            },
            ClientAuthMethod::ApiKey { key, header_name } => tasker_shared::config::WebAuthConfig {
                enabled: true,
                api_key: key.clone(),
                api_key_header: header_name.clone(),
                ..Default::default()
            },
        }
    }
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
            transport: Transport::Rest,
            orchestration: ApiEndpointConfig {
                base_url: "http://localhost:8080".to_string(),
                timeout_ms: 30000,
                max_retries: 3,
                auth_token: None,
                auth: None,
            },
            worker: ApiEndpointConfig {
                base_url: "http://localhost:8081".to_string(),
                timeout_ms: 30000,
                max_retries: 3,
                auth_token: None,
                auth: None,
            },
            cli: CliConfig {
                default_format: "table".to_string(),
                colored_output: true,
                verbose_level: 0,
            },
        }
    }
}

/// Default gRPC endpoints for when transport is gRPC
impl ClientConfig {
    /// Get default gRPC orchestration URL
    pub const DEFAULT_GRPC_ORCHESTRATION_URL: &'static str = "http://localhost:9090";
    /// Get default gRPC worker URL
    pub const DEFAULT_GRPC_WORKER_URL: &'static str = "http://localhost:9100";
}

impl ClientConfig {
    /// Load configuration from environment variables and config file
    ///
    /// Precedence (highest to lowest):
    /// 1. Environment variables
    /// 2. Config file (~/.tasker/config.toml)
    /// 3. Default values
    pub fn load() -> ClientResult<Self> {
        // Check if a profile is specified via environment
        if let Ok(profile_name) = std::env::var("TASKER_CLIENT_PROFILE") {
            return Self::load_profile(&profile_name);
        }

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

        debug!(
            transport = %config.transport,
            orchestration_url = %config.orchestration.base_url,
            worker_url = %config.worker.base_url,
            "Client configuration loaded"
        );
        Ok(config)
    }

    /// Load configuration for a specific named profile (TAS-177).
    ///
    /// Profiles are defined in `.config/tasker-client.toml` (like nextest):
    ///
    /// ```toml
    /// [profile.default]
    /// transport = "rest"
    ///
    /// [profile.grpc]
    /// transport = "grpc"
    /// [profile.grpc.orchestration]
    /// base_url = "http://localhost:9090"
    /// ```
    ///
    /// # Precedence
    /// 1. Environment variables (highest)
    /// 2. Selected profile settings
    /// 3. `[profile.default]` settings
    /// 4. Hardcoded defaults (lowest)
    pub fn load_profile(profile_name: &str) -> ClientResult<Self> {
        debug!(profile = %profile_name, "Loading client profile");

        // Start with defaults
        let mut config = Self::default();

        // Find and load profile config file
        if let Some(profile_path) = Self::find_profile_config_file() {
            debug!("Loading profiles from: {}", profile_path.display());
            let profile_file = Self::load_profile_file(&profile_path)?;

            // Apply [profile.default] first if it exists
            if let Some(default_profile) = profile_file.profile.get("default") {
                config.apply_profile(default_profile);
            }

            // Apply the requested profile on top
            if profile_name != "default" {
                if let Some(profile) = profile_file.profile.get(profile_name) {
                    config.apply_profile(profile);
                } else {
                    return Err(ClientError::config_error(format!(
                        "Profile '{}' not found in {}. Available profiles: {}",
                        profile_name,
                        profile_path.display(),
                        profile_file
                            .profile
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
        } else if profile_name != "default" {
            // Non-default profile requested but no profile file found
            return Err(ClientError::config_error(format!(
                "Profile '{}' requested but no profile config file found. \
                 Create .config/tasker-client.toml with [profile.{}] section.",
                profile_name, profile_name
            )));
        }

        // Override with environment variables
        config.apply_env_overrides();

        debug!(
            profile = %profile_name,
            transport = %config.transport,
            orchestration_url = %config.orchestration.base_url,
            worker_url = %config.worker.base_url,
            "Profile configuration loaded"
        );

        Ok(config)
    }

    /// Apply a profile's settings to this config.
    fn apply_profile(&mut self, profile: &ProfileConfig) {
        if let Some(transport) = profile.transport {
            self.transport = transport;
        }

        if let Some(ref orch) = profile.orchestration {
            if let Some(ref url) = orch.base_url {
                self.orchestration.base_url = url.clone();
            }
            if let Some(timeout) = orch.timeout_ms {
                self.orchestration.timeout_ms = timeout;
            }
            if let Some(retries) = orch.max_retries {
                self.orchestration.max_retries = retries;
            }
            if let Some(ref auth) = orch.auth {
                self.orchestration.auth = Some(auth.clone());
            }
        }

        if let Some(ref worker) = profile.worker {
            if let Some(ref url) = worker.base_url {
                self.worker.base_url = url.clone();
            }
            if let Some(timeout) = worker.timeout_ms {
                self.worker.timeout_ms = timeout;
            }
            if let Some(retries) = worker.max_retries {
                self.worker.max_retries = retries;
            }
            if let Some(ref auth) = worker.auth {
                self.worker.auth = Some(auth.clone());
            }
        }

        if let Some(ref cli) = profile.cli {
            if let Some(ref format) = cli.default_format {
                self.cli.default_format = format.clone();
            }
            if let Some(colored) = cli.colored_output {
                self.cli.colored_output = colored;
            }
            if let Some(verbose) = cli.verbose_level {
                self.cli.verbose_level = verbose;
            }
        }
    }

    /// Load configuration from specific file (legacy format)
    pub fn load_from_file(path: &Path) -> ClientResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ClientError::config_error(format!("Failed to read config file: {}", e)))?;

        let config: Self = toml::from_str(&content).map_err(|e| {
            ClientError::config_error(format!("Failed to parse config file: {}", e))
        })?;

        Ok(config)
    }

    /// Load profile configuration file
    fn load_profile_file(path: &Path) -> ClientResult<ProfileConfigFile> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ClientError::config_error(format!("Failed to read profile file: {}", e))
        })?;

        let profile_file: ProfileConfigFile = toml::from_str(&content).map_err(|e| {
            ClientError::config_error(format!("Failed to parse profile file: {}", e))
        })?;

        Ok(profile_file)
    }

    /// Find the config file in standard locations (legacy format)
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

    /// Find the profile config file (TAS-177, like nextest)
    ///
    /// Search order:
    /// 1. `.config/tasker-client.toml` (project-local, like nextest)
    /// 2. `./tasker-client.toml` (current directory)
    /// 3. `~/.config/tasker/client.toml` (user config)
    pub fn find_profile_config_file() -> Option<PathBuf> {
        let possible_paths: Vec<PathBuf> = vec![
            // Project-local (like nextest's .config/nextest.toml)
            PathBuf::from(".config/tasker-client.toml"),
            // Current directory
            PathBuf::from("./tasker-client.toml"),
            // User config directory
            dirs::config_dir()
                .map(|d| d.join("tasker").join("client.toml"))
                .unwrap_or_default(),
            // Home directory
            dirs::home_dir()
                .map(|d| d.join(".tasker").join("client.toml"))
                .unwrap_or_default(),
        ];

        possible_paths
            .into_iter()
            .find(|path| path.exists() && path.is_file())
    }

    /// List available profiles from the config file
    pub fn list_profiles() -> ClientResult<Vec<String>> {
        if let Some(profile_path) = Self::find_profile_config_file() {
            let profile_file = Self::load_profile_file(&profile_path)?;
            Ok(profile_file.profile.keys().cloned().collect())
        } else {
            Ok(vec!["default".to_string()])
        }
    }

    /// Apply environment variable overrides
    ///
    /// Transport priority (highest to lowest):
    /// 1. `TASKER_CLIENT_TRANSPORT` or `TASKER_TEST_TRANSPORT`
    /// 2. Config file / profile value
    /// 3. Default (rest)
    ///
    /// Auth resolution priority (highest to lowest):
    /// 1. Endpoint-specific token: `TASKER_ORCHESTRATION_AUTH_TOKEN` / `TASKER_WORKER_AUTH_TOKEN`
    /// 2. Global token: `TASKER_AUTH_TOKEN`
    /// 3. API key: `TASKER_API_KEY` (with optional `TASKER_API_KEY_HEADER`)
    /// 4. Config file values
    fn apply_env_overrides(&mut self) {
        // Transport override (TAS-177)
        // Check both TASKER_CLIENT_TRANSPORT and TASKER_TEST_TRANSPORT for flexibility
        if let Some(transport_str) = std::env::var("TASKER_CLIENT_TRANSPORT")
            .ok()
            .or_else(|| std::env::var("TASKER_TEST_TRANSPORT").ok())
        {
            if let Ok(transport) = transport_str.parse() {
                self.transport = transport;
            }
        }

        // Orchestration API overrides
        // For gRPC, check gRPC-specific env var first, then fall back to generic
        if self.transport == Transport::Grpc {
            if let Ok(url) = std::env::var("TASKER_ORCHESTRATION_GRPC_URL")
                .or_else(|_| std::env::var("TASKER_TEST_ORCHESTRATION_GRPC_URL"))
            {
                self.orchestration.base_url = url;
            }
        } else if let Ok(url) = std::env::var("TASKER_ORCHESTRATION_URL") {
            self.orchestration.base_url = url;
        }

        if let Ok(timeout) = std::env::var("TASKER_ORCHESTRATION_TIMEOUT_MS") {
            if let Ok(timeout_ms) = timeout.parse() {
                self.orchestration.timeout_ms = timeout_ms;
            }
        }

        // Worker API overrides
        if self.transport == Transport::Grpc {
            if let Ok(url) = std::env::var("TASKER_WORKER_GRPC_URL")
                .or_else(|_| std::env::var("TASKER_TEST_WORKER_GRPC_URL"))
            {
                self.worker.base_url = url;
            }
        } else if let Ok(url) = std::env::var("TASKER_WORKER_URL") {
            self.worker.base_url = url;
        }

        if let Ok(timeout) = std::env::var("TASKER_WORKER_TIMEOUT_MS") {
            if let Ok(timeout_ms) = timeout.parse() {
                self.worker.timeout_ms = timeout_ms;
            }
        }

        // Auth resolution: endpoint-specific token > global token > API key
        let global_token = std::env::var("TASKER_AUTH_TOKEN").ok();
        let api_key = std::env::var("TASKER_API_KEY").ok();
        let api_key_header =
            std::env::var("TASKER_API_KEY_HEADER").unwrap_or_else(|_| "X-API-Key".to_string());

        // Orchestration auth
        let orch_token = std::env::var("TASKER_ORCHESTRATION_AUTH_TOKEN").ok();
        if let Some(token) = orch_token.or_else(|| global_token.clone()) {
            self.orchestration.auth_token = Some(token.clone());
            self.orchestration.auth = Some(ClientAuthConfig {
                method: ClientAuthMethod::BearerToken(token),
            });
        } else if let Some(ref key) = api_key {
            self.orchestration.auth = Some(ClientAuthConfig {
                method: ClientAuthMethod::ApiKey {
                    key: key.clone(),
                    header_name: api_key_header.clone(),
                },
            });
        }

        // Worker auth
        let worker_token = std::env::var("TASKER_WORKER_AUTH_TOKEN").ok();
        if let Some(token) = worker_token.or(global_token) {
            self.worker.auth_token = Some(token.clone());
            self.worker.auth = Some(ClientAuthConfig {
                method: ClientAuthMethod::BearerToken(token),
            });
        } else if let Some(ref key) = api_key {
            self.worker.auth = Some(ClientAuthConfig {
                method: ClientAuthMethod::ApiKey {
                    key: key.clone(),
                    header_name: api_key_header,
                },
            });
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
        assert!(config.orchestration.auth.is_none());
        assert!(config.worker.auth.is_none());
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
    fn test_config_serialization_with_bearer_auth() {
        let mut config = ClientConfig::default();
        config.orchestration.auth = Some(ClientAuthConfig {
            method: ClientAuthMethod::BearerToken("test-token-123".to_string()),
        });

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: ClientConfig = toml::from_str(&toml_str).unwrap();

        let auth = deserialized.orchestration.auth.unwrap();
        match auth.method {
            ClientAuthMethod::BearerToken(token) => assert_eq!(token, "test-token-123"),
            _ => panic!("Expected BearerToken"),
        }
    }

    #[test]
    fn test_config_serialization_with_api_key_auth() {
        let mut config = ClientConfig::default();
        config.worker.auth = Some(ClientAuthConfig {
            method: ClientAuthMethod::ApiKey {
                key: "my-api-key".to_string(),
                header_name: "X-Custom-Key".to_string(),
            },
        });

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: ClientConfig = toml::from_str(&toml_str).unwrap();

        let auth = deserialized.worker.auth.unwrap();
        match auth.method {
            ClientAuthMethod::ApiKey { key, header_name } => {
                assert_eq!(key, "my-api-key");
                assert_eq!(header_name, "X-Custom-Key");
            }
            _ => panic!("Expected ApiKey"),
        }
    }

    #[test]
    fn test_resolve_web_auth_config_bearer_token() {
        let endpoint = ApiEndpointConfig {
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth_token: None,
            auth: Some(ClientAuthConfig {
                method: ClientAuthMethod::BearerToken("jwt-token".to_string()),
            }),
        };

        let web_auth = endpoint.resolve_web_auth_config().unwrap();
        assert!(web_auth.enabled);
        assert_eq!(web_auth.bearer_token, "jwt-token");
        assert!(web_auth.api_key.is_empty());
    }

    #[test]
    fn test_resolve_web_auth_config_api_key() {
        let endpoint = ApiEndpointConfig {
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth_token: None,
            auth: Some(ClientAuthConfig {
                method: ClientAuthMethod::ApiKey {
                    key: "secret-key".to_string(),
                    header_name: "X-API-Key".to_string(),
                },
            }),
        };

        let web_auth = endpoint.resolve_web_auth_config().unwrap();
        assert!(web_auth.enabled);
        assert_eq!(web_auth.api_key, "secret-key");
        assert_eq!(web_auth.api_key_header, "X-API-Key");
        assert!(web_auth.bearer_token.is_empty());
    }

    #[test]
    fn test_resolve_web_auth_config_legacy_auth_token() {
        let endpoint = ApiEndpointConfig {
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth_token: Some("legacy-token".to_string()),
            auth: None,
        };

        let web_auth = endpoint.resolve_web_auth_config().unwrap();
        assert!(web_auth.enabled);
        assert_eq!(web_auth.bearer_token, "legacy-token");
    }

    #[test]
    fn test_resolve_web_auth_config_none() {
        let endpoint = ApiEndpointConfig {
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth_token: None,
            auth: None,
        };

        assert!(endpoint.resolve_web_auth_config().is_none());
    }

    #[test]
    fn test_resolve_web_auth_config_prefers_auth_over_legacy() {
        let endpoint = ApiEndpointConfig {
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth_token: Some("legacy-token".to_string()),
            auth: Some(ClientAuthConfig {
                method: ClientAuthMethod::BearerToken("new-token".to_string()),
            }),
        };

        let web_auth = endpoint.resolve_web_auth_config().unwrap();
        assert_eq!(web_auth.bearer_token, "new-token");
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

    // ==========================================================================
    // Transport Tests (TAS-177)
    // ==========================================================================

    #[test]
    fn test_transport_default() {
        let config = ClientConfig::default();
        assert_eq!(config.transport, Transport::Rest);
    }

    #[test]
    fn test_transport_parsing() {
        assert_eq!("rest".parse::<Transport>().unwrap(), Transport::Rest);
        assert_eq!("grpc".parse::<Transport>().unwrap(), Transport::Grpc);
        assert_eq!("http".parse::<Transport>().unwrap(), Transport::Rest);
        assert_eq!("REST".parse::<Transport>().unwrap(), Transport::Rest);
        assert_eq!("GRPC".parse::<Transport>().unwrap(), Transport::Grpc);
        assert!("invalid".parse::<Transport>().is_err());
    }

    #[test]
    fn test_transport_display() {
        assert_eq!(Transport::Rest.to_string(), "rest");
        assert_eq!(Transport::Grpc.to_string(), "grpc");
    }

    #[test]
    fn test_config_with_transport_serialization() {
        let mut config = ClientConfig::default();
        config.transport = Transport::Grpc;
        config.orchestration.base_url = "http://localhost:9090".to_string();

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("transport = \"grpc\""));

        let deserialized: ClientConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(deserialized.transport, Transport::Grpc);
        assert_eq!(deserialized.orchestration.base_url, "http://localhost:9090");
    }

    // ==========================================================================
    // Profile Tests (TAS-177)
    // ==========================================================================

    #[test]
    fn test_profile_config_parsing() {
        let toml_content = r#"
[profile.default]
transport = "rest"

[profile.default.orchestration]
base_url = "http://localhost:8080"
timeout_ms = 30000

[profile.grpc]
transport = "grpc"

[profile.grpc.orchestration]
base_url = "http://localhost:9090"
timeout_ms = 60000

[profile.auth]
transport = "rest"

[profile.auth.orchestration]
base_url = "http://localhost:8080"
"#;

        let profile_file: ProfileConfigFile = toml::from_str(toml_content).unwrap();

        assert!(profile_file.profile.contains_key("default"));
        assert!(profile_file.profile.contains_key("grpc"));
        assert!(profile_file.profile.contains_key("auth"));

        let grpc_profile = profile_file.profile.get("grpc").unwrap();
        assert_eq!(grpc_profile.transport, Some(Transport::Grpc));
        assert_eq!(
            grpc_profile.orchestration.as_ref().unwrap().base_url,
            Some("http://localhost:9090".to_string())
        );
    }

    #[test]
    fn test_apply_profile() {
        let profile = ProfileConfig {
            transport: Some(Transport::Grpc),
            orchestration: Some(ProfileEndpointConfig {
                base_url: Some("http://localhost:9090".to_string()),
                timeout_ms: Some(60000),
                max_retries: None,
                auth: None,
            }),
            worker: Some(ProfileEndpointConfig {
                base_url: Some("http://localhost:9100".to_string()),
                timeout_ms: None,
                max_retries: None,
                auth: None,
            }),
            cli: None,
        };

        let mut config = ClientConfig::default();
        config.apply_profile(&profile);

        assert_eq!(config.transport, Transport::Grpc);
        assert_eq!(config.orchestration.base_url, "http://localhost:9090");
        assert_eq!(config.orchestration.timeout_ms, 60000);
        // max_retries should remain default since not specified in profile
        assert_eq!(config.orchestration.max_retries, 3);
        assert_eq!(config.worker.base_url, "http://localhost:9100");
        // worker timeout should remain default
        assert_eq!(config.worker.timeout_ms, 30000);
    }

    #[test]
    fn test_profile_partial_override() {
        // Test that profiles can partially override settings
        let profile = ProfileConfig {
            transport: None, // Don't override transport
            orchestration: Some(ProfileEndpointConfig {
                base_url: Some("http://custom:8080".to_string()),
                timeout_ms: None, // Don't override timeout
                max_retries: None,
                auth: None,
            }),
            worker: None, // Don't override worker at all
            cli: None,
        };

        let mut config = ClientConfig::default();
        config.apply_profile(&profile);

        // Transport should remain default
        assert_eq!(config.transport, Transport::Rest);
        // URL should be overridden
        assert_eq!(config.orchestration.base_url, "http://custom:8080");
        // Timeout should remain default
        assert_eq!(config.orchestration.timeout_ms, 30000);
        // Worker should be unchanged
        assert_eq!(config.worker.base_url, "http://localhost:8081");
    }
}
