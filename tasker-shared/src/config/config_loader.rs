//! Simple V2 Configuration Loader (TAS-61)
//!
//! Dead-simple config loader that:
//! 0. Automatically loads .env file if present (via dotenvy)
//! 1. Reads a single pre-merged TOML file from TASKER_CONFIG_PATH
//! 2. Deserializes to TaskerConfig
//! 3. Performs environment variable substitution
//! 4. Validates with validator library
//!
//! No component merging, no complex path resolution, no fallbacks.
//! The CLI generates merged configs, we just load them.

use super::error::{ConfigResult, ConfigurationError};
use super::tasker::TaskerConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use validator::Validate;

/// Environment variable validation rule
#[derive(Debug, Clone)]
struct EnvVarRule {
    /// Variable name
    name: &'static str,
    /// Description for error messages
    description: &'static str,
    /// Validation regex pattern
    pattern: &'static str,
}

/// Get the allowlist of supported environment variables with validation rules
///
/// This provides defense-in-depth security by:
/// 1. Allowlist enforcement - only these env vars can be substituted
/// 2. Format validation - each must match its expected pattern
/// 3. Fail-fast - invalid values cause immediate errors
fn get_env_var_allowlist() -> Vec<EnvVarRule> {
    vec![
        EnvVarRule {
            name: "DATABASE_URL",
            description: "PostgreSQL connection URL",
            // PostgreSQL URL: postgresql://[user[:password]@]host[:port]/dbname[?param=value]
            // Supports both authenticated (user:pass@host) and unauthenticated (host) formats
            pattern: r"^postgresql://([a-zA-Z0-9_-]+:[^@]+@)?[a-zA-Z0-9._-]+(:[0-9]+)?/[a-zA-Z0-9_-]+(\?.*)?$",
        },
        EnvVarRule {
            name: "PGMQ_DATABASE_URL",
            description:
                "PostgreSQL connection URL for PGMQ (optional, falls back to DATABASE_URL)",
            // Same pattern as DATABASE_URL but allows empty string for fallback behavior
            pattern: r"^(postgresql://([a-zA-Z0-9_-]+:[^@]+@)?[a-zA-Z0-9._-]+(:[0-9]+)?/[a-zA-Z0-9_-]+(\?.*)?)?$",
        },
        EnvVarRule {
            name: "TASKER_ENV",
            description: "Environment name (test, development, production)",
            pattern: r"^(test|development|production)$",
        },
        EnvVarRule {
            name: "TASKER_CONFIG_PATH",
            description: "Path to configuration file",
            // Unix/Windows path (absolute or relative)
            pattern: r"^[/\\]?[a-zA-Z0-9._/-]+\.toml$",
        },
        EnvVarRule {
            name: "TASKER_WEB_BIND_ADDRESS",
            description: "Web server bind address (IP:port)",
            // IPv4:port or hostname:port
            pattern: r"^([0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}|[a-zA-Z0-9._-]+):[0-9]{1,5}$",
        },
        EnvVarRule {
            name: "TASKER_TEMPLATE_PATH",
            description: "Path to task templates directory",
            // Unix/Windows path
            pattern: r"^[/\\]?[a-zA-Z0-9._/-]+$",
        },
        EnvVarRule {
            name: "RABBITMQ_URL",
            description: "RabbitMQ connection URL (amqp://user:pass@host:port/vhost)",
            // AMQP URL: amqp://[user[:password]@]host[:port][/vhost]
            pattern: r"^amqp://([a-zA-Z0-9_-]+:[^@]+@)?[a-zA-Z0-9._-]+(:[0-9]+)?(/[a-zA-Z0-9_%.-]*)?$",
        },
        EnvVarRule {
            name: "TASKER_MESSAGING_BACKEND",
            description: "Messaging backend selection (pgmq or rabbitmq)",
            // TAS-133: Valid backends for messaging provider selection
            pattern: r"^(pgmq|rabbitmq)$",
        },
        // TAS-73: Multi-instance deployment support
        EnvVarRule {
            name: "TASKER_INSTANCE_ID",
            description: "Instance identifier for multi-instance deployments (e.g., orchestration-1, worker-rust-2)",
            // Alphanumeric with hyphens, 1-64 chars
            pattern: r"^[a-zA-Z0-9][a-zA-Z0-9\-]{0,63}$",
        },
        EnvVarRule {
            name: "TASKER_INSTANCE_PORT",
            description: "Override port for this instance (e.g., 8080, 8100)",
            // Port number 1-65535
            pattern: r"^[0-9]{1,5}$",
        },
        EnvVarRule {
            name: "TASKER_WORKER_ID",
            description: "Worker identifier for logging and metrics",
            // Alphanumeric with hyphens, 1-64 chars
            pattern: r"^[a-zA-Z0-9][a-zA-Z0-9\-]{0,63}$",
        },
        // TAS-150: JWT authentication support
        EnvVarRule {
            name: "TASKER_JWT_PUBLIC_KEY",
            description: "JWT public key for token verification (PEM format, RSA)",
            // PEM format: begins with -----BEGIN, ends with -----END, base64 content
            pattern: r"(?s)^-----BEGIN [A-Z ]+KEY-----.*-----END [A-Z ]+KEY-----\s*$",
        },
        EnvVarRule {
            name: "TASKER_JWT_PUBLIC_KEY_PATH",
            description: "Path to JWT public key file (PEM format)",
            // Unix/Windows file path
            pattern: r"^[/\\]?[a-zA-Z0-9._/ \\-]+\.pem$",
        },
    ]
}

/// Simple configuration loader for pre-merged V2 TOML files
///
/// This is a zero-state utility struct providing static configuration loading functions.
/// All methods are associated functions (no self parameter).
#[derive(Debug)]
pub struct ConfigLoader;

impl ConfigLoader {
    /// Validate an environment variable value against its allowlist rule
    ///
    /// # Returns
    /// Ok(()) if valid, Err if invalid or not in allowlist
    fn validate_env_var(var_name: &str, value: &str) -> ConfigResult<()> {
        let allowlist = get_env_var_allowlist();
        let rules: HashMap<&str, &EnvVarRule> =
            allowlist.iter().map(|rule| (rule.name, rule)).collect();

        // Check if variable is in allowlist
        let rule = rules.get(var_name).ok_or_else(|| {
            ConfigurationError::validation_error(format!(
                "Environment variable '{}' is not in the allowlist. \
                 Allowed variables: {}",
                var_name,
                allowlist
                    .iter()
                    .map(|r| r.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;

        // Validate against regex pattern
        let regex = regex::Regex::new(rule.pattern).map_err(|e| {
            ConfigurationError::validation_error(format!(
                "Invalid regex pattern for {}: {}",
                var_name, e
            ))
        })?;

        if !regex.is_match(value) {
            return Err(ConfigurationError::validation_error(format!(
                "Invalid value for environment variable '{}' ({}): '{}'\n\
                 Expected format matching: {}",
                var_name, rule.description, value, rule.pattern
            )));
        }

        Ok(())
    }
    /// Detect environment from TASKER_ENV variable or default to "development"
    pub fn detect_environment() -> String {
        std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string())
    }

    /// Load configuration from TASKER_CONFIG_PATH environment variable
    ///
    /// Automatically loads .env file if present before reading configuration.
    ///
    /// # Returns
    /// TaskerConfig (canonical configuration)
    ///
    /// # Errors
    /// - TASKER_CONFIG_PATH not set
    /// - File not found or cannot be read
    /// - TOML parse errors
    /// - Validation errors
    pub fn load_from_env() -> ConfigResult<TaskerConfig> {
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
    /// TaskerConfig (canonical configuration)
    pub fn load_from_path(path: &PathBuf) -> ConfigResult<TaskerConfig> {
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

        // 4. Deserialize to TaskerConfig
        let config: TaskerConfig = toml_value.try_into().map_err(|e| {
            ConfigurationError::json_serialization_error("TOML to TaskerConfig deserialization", e)
        })?;

        // 5. Validate
        config.validate().map_err(|errors| {
            ConfigurationError::validation_error(format!(
                "Configuration validation failed: {:?}",
                errors
            ))
        })?;

        tracing::debug!("Successfully loaded and validated TaskerConfig");
        tracing::info!("Configuration loaded successfully from {}", path.display());

        Ok(config)
    }

    /// Escape TOML special characters to prevent injection attacks
    ///
    /// Escapes characters that have special meaning in TOML strings:
    /// - Backslash (\) → \\
    /// - Quote (") → \"
    /// - Newline (\n) → \\n
    /// - Carriage return (\r) → \\r
    /// - Tab (\t) → \\t
    ///
    /// ## Security Note
    /// This prevents TOML injection attacks where malicious environment
    /// variables could break out of string literals to inject new config sections.
    fn escape_toml_string(value: &str) -> String {
        value
            .replace('\\', "\\\\") // Must be first to avoid double-escaping
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }

    /// Substitute environment variables in configuration content
    ///
    /// Replaces ${VAR_NAME} patterns with environment variable values.
    /// Supports both ${VAR} and ${VAR:-default} syntax.
    ///
    /// ## Security
    /// 1. **Allowlist enforcement**: Only pre-approved env vars can be substituted
    /// 2. **Format validation**: Each env var must match its expected regex pattern
    /// 3. **TOML injection prevention**: Values are escaped to prevent injection attacks
    /// 4. **Fail-fast**: Invalid values cause immediate configuration load failure
    ///
    /// ## Panics
    /// Panics if an environment variable fails allowlist or validation checks.
    /// This is intentional - we want to fail fast during config loading.
    fn substitute_env_vars(content: &str) -> String {
        let mut result = content.to_string();

        // Simple regex-free approach: find ${...} patterns
        // Supports both ${VAR} and ${VAR:-default} syntax
        while let Some(start) = result.find("${") {
            if let Some(end) = result[start..].find('}') {
                let full_expr = &result[start + 2..start + end];

                // Check for ${VAR:-default} syntax
                let (var_name, default_value) = if let Some(sep_pos) = full_expr.find(":-") {
                    (&full_expr[..sep_pos], Some(&full_expr[sep_pos + 2..]))
                } else {
                    (full_expr, None)
                };

                // Try to get the environment variable
                let replacement = match std::env::var(var_name) {
                    Ok(var_value) => {
                        // Debug: Log environment variable substitution
                        tracing::info!(
                            "Config loader: Substituting {}={} (found in environment)",
                            var_name,
                            var_value
                        );

                        // Security: Validate against allowlist and format rules
                        Self::validate_env_var(var_name, &var_value).unwrap_or_else(|e| {
                            panic!("Environment variable validation failed: {}", e);
                        });

                        // Security: Escape TOML special characters to prevent injection
                        Self::escape_toml_string(&var_value)
                    }
                    Err(_) => {
                        if let Some(default) = default_value {
                            // Debug: Log default value usage
                            tracing::info!(
                                "Config loader: Using default for {}={} (not in environment)",
                                var_name,
                                default
                            );

                            // Security: Default values from config are trusted, don't escape
                            // (they're part of the config file, not user input)
                            default.to_string()
                        } else {
                            // No default, warn and leave placeholder
                            tracing::warn!(
                                "Environment variable {} not found, leaving placeholder",
                                var_name
                            );
                            break; // Avoid infinite loop
                        }
                    }
                };

                let pattern = format!("${{{}}}", full_expr);
                result = result.replace(&pattern, &replacement);
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
/// - `config`: TaskerConfig (canonical configuration)
///
/// Legacy config access has been removed.
#[derive(Debug, Clone)]
pub struct ConfigManager {
    /// V2 configuration (source of truth and only configuration)
    config: TaskerConfig,
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
            "Loading configuration from TASKER_CONFIG_PATH: {} (environment: {})",
            config_path,
            environment
        );

        // TAS-61 Phase 6C/6D: Load V2 config (source of truth and only config)
        let config = Self::load_from_path(&PathBuf::from(&config_path))?;

        Ok(std::sync::Arc::new(ConfigManager {
            config,
            environment: environment.to_string(),
        }))
    }

    /// Load TaskerConfig from file path (internal helper)
    ///
    /// This is the V2 loading logic extracted for reuse.
    fn load_from_path(path: &PathBuf) -> ConfigResult<TaskerConfig> {
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

        // 4. Deserialize to TaskerConfig
        let config: TaskerConfig = toml_value.try_into().map_err(|e| {
            ConfigurationError::json_serialization_error("TOML to TaskerConfig deserialization", e)
        })?;

        // 5. Validate
        config.validate().map_err(|errors| {
            ConfigurationError::validation_error(format!(
                "Configuration validation failed: {:?}",
                errors
            ))
        })?;

        tracing::debug!("Successfully loaded and validated TaskerConfig");

        Ok(config)
    }

    /// Get reference to the V2 configuration
    ///
    /// TAS-61 Phase 6C/6D: This is the only configuration method.
    /// Returns the canonical TaskerConfig.
    pub fn config(&self) -> &TaskerConfig {
        &self.config
    }

    /// Get the environment name
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Create ConfigManager from existing TaskerConfig
    ///
    /// TAS-61 Phase 6C/6D: Direct V2 construction for tests
    ///
    /// This is primarily for test compatibility where you have a V2 config
    /// constructed directly. For production use, prefer `load_from_env()`.
    pub fn from_tasker_config(config: TaskerConfig, environment: String) -> Self {
        Self {
            config,
            environment,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_env_var_substitution() {
        // Clean up any existing DATABASE_URL
        std::env::remove_var("DATABASE_URL");
        // Use allowlisted DATABASE_URL variable
        std::env::set_var(
            "DATABASE_URL",
            "postgresql://tasker:password@localhost:5432/test_db",
        );

        let input = "database_url = \"${DATABASE_URL}\"";
        let output = ConfigLoader::substitute_env_vars(input);

        assert_eq!(
            output,
            "database_url = \"postgresql://tasker:password@localhost:5432/test_db\""
        );

        // Clean up
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    #[serial]
    fn test_env_var_substitution_with_default() {
        // Test case 1: Environment variable not set, use default
        std::env::remove_var("DATABASE_URL");
        let input = "url = \"${DATABASE_URL:-postgresql://localhost:5432/test}\"";
        let output = ConfigLoader::substitute_env_vars(input);
        assert_eq!(output, "url = \"postgresql://localhost:5432/test\"");

        // Test case 2: Environment variable set, use it instead of default
        std::env::set_var(
            "DATABASE_URL",
            "postgresql://postgres:password@postgres:5432/production",
        );
        let input = "url = \"${DATABASE_URL:-postgresql://localhost:5432/test}\"";
        let output = ConfigLoader::substitute_env_vars(input);
        assert_eq!(
            output,
            "url = \"postgresql://postgres:password@postgres:5432/production\""
        );

        // Clean up
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn test_detect_environment() {
        std::env::set_var("TASKER_ENV", "production");
        assert_eq!(ConfigLoader::detect_environment(), "production");

        std::env::remove_var("TASKER_ENV");
        assert_eq!(ConfigLoader::detect_environment(), "development");
    }

    #[test]
    #[serial]
    fn test_toml_injection_prevention() {
        // Clean up any existing DATABASE_URL
        std::env::remove_var("DATABASE_URL");

        // Test case 1: Password with backslashes (valid URL, but needs escaping)
        // Passwords can contain backslashes, which need escaping in TOML
        std::env::set_var(
            "DATABASE_URL",
            "postgresql://user:p\\ass\\word@host:5432/testdb",
        );
        let input = "url = \"${DATABASE_URL}\"";
        let output = ConfigLoader::substitute_env_vars(input);

        // Should escape backslashes
        assert!(output.contains("\\\\"));
        assert_eq!(
            output,
            "url = \"postgresql://user:p\\\\ass\\\\word@host:5432/testdb\""
        );

        // Verify it parses as valid TOML
        let parsed: toml::Value = toml::from_str(&output).expect("Should parse as valid TOML");
        if let Some(url) = parsed.get("url") {
            // After TOML parsing, the value should have the original backslashes
            assert_eq!(
                url.as_str().unwrap(),
                "postgresql://user:p\\ass\\word@host:5432/testdb"
            );
        } else {
            panic!("url field not found in parsed TOML");
        }

        // Test case 2: Tab character in password (needs escaping)
        std::env::set_var(
            "DATABASE_URL",
            "postgresql://user:pass\tword@host:5432/testdb",
        );
        let input = "url = \"${DATABASE_URL}\"";
        let output = ConfigLoader::substitute_env_vars(input);

        // Should escape tab
        assert!(output.contains("\\t"));
        assert!(!output.contains('\t')); // No literal tabs

        // Test case 3: Carriage return in password (needs escaping)
        std::env::set_var(
            "DATABASE_URL",
            "postgresql://user:pass\rword@host:5432/testdb",
        );
        let input = "url = \"${DATABASE_URL}\"";
        let output = ConfigLoader::substitute_env_vars(input);

        // Should escape carriage return
        assert!(output.contains("\\r"));
        assert!(!output.contains('\r')); // No literal carriage returns

        // Clean up
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn test_escape_toml_string() {
        // Test all special character escaping
        assert_eq!(ConfigLoader::escape_toml_string("foo\"bar"), "foo\\\"bar");
        assert_eq!(ConfigLoader::escape_toml_string("foo\\bar"), "foo\\\\bar");
        assert_eq!(ConfigLoader::escape_toml_string("foo\nbar"), "foo\\nbar");
        assert_eq!(ConfigLoader::escape_toml_string("foo\rbar"), "foo\\rbar");
        assert_eq!(ConfigLoader::escape_toml_string("foo\tbar"), "foo\\tbar");

        // Test combined escaping (backslash must be escaped first)
        assert_eq!(
            ConfigLoader::escape_toml_string("foo\\\"bar\n"),
            "foo\\\\\\\"bar\\n"
        );
    }

    #[test]
    fn test_env_var_allowlist_validation_success() {
        // Valid DATABASE_URL
        assert!(ConfigLoader::validate_env_var(
            "DATABASE_URL",
            "postgresql://tasker:password@localhost:5432/tasker_db"
        )
        .is_ok());

        assert!(ConfigLoader::validate_env_var(
            "DATABASE_URL",
            "postgresql://user:pass@postgres:5432/db_name?sslmode=require"
        )
        .is_ok());

        // Valid TASKER_ENV
        assert!(ConfigLoader::validate_env_var("TASKER_ENV", "test").is_ok());
        assert!(ConfigLoader::validate_env_var("TASKER_ENV", "development").is_ok());
        assert!(ConfigLoader::validate_env_var("TASKER_ENV", "production").is_ok());

        // Valid TASKER_CONFIG_PATH
        assert!(
            ConfigLoader::validate_env_var("TASKER_CONFIG_PATH", "/etc/tasker/config.toml").is_ok()
        );
        assert!(ConfigLoader::validate_env_var("TASKER_CONFIG_PATH", "config/tasker.toml").is_ok());

        // Valid TASKER_WEB_BIND_ADDRESS
        assert!(ConfigLoader::validate_env_var("TASKER_WEB_BIND_ADDRESS", "0.0.0.0:8080").is_ok());
        assert!(
            ConfigLoader::validate_env_var("TASKER_WEB_BIND_ADDRESS", "localhost:3000").is_ok()
        );

        // Valid TASKER_TEMPLATE_PATH
        assert!(ConfigLoader::validate_env_var("TASKER_TEMPLATE_PATH", "/opt/templates").is_ok());
    }

    #[test]
    fn test_env_var_allowlist_validation_failure() {
        // Invalid DATABASE_URL (not postgresql://)
        assert!(ConfigLoader::validate_env_var("DATABASE_URL", "mysql://localhost/db").is_err());

        // Invalid TASKER_ENV (not in allowed list)
        assert!(ConfigLoader::validate_env_var("TASKER_ENV", "staging").is_err());

        // Invalid TASKER_CONFIG_PATH (not .toml)
        assert!(
            ConfigLoader::validate_env_var("TASKER_CONFIG_PATH", "/etc/tasker/config.yaml")
                .is_err()
        );

        // Invalid TASKER_WEB_BIND_ADDRESS (missing port)
        assert!(ConfigLoader::validate_env_var("TASKER_WEB_BIND_ADDRESS", "localhost").is_err());
    }

    #[test]
    fn test_env_var_not_in_allowlist() {
        // Variable not in allowlist should fail
        let result = ConfigLoader::validate_env_var("RANDOM_VAR", "some_value");

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("not in the allowlist"));
        assert!(err_msg.contains("DATABASE_URL"));
        assert!(err_msg.contains("TASKER_ENV"));
    }

    #[test]
    #[serial]
    #[should_panic(expected = "Environment variable validation failed")]
    fn test_substitute_env_vars_with_invalid_value() {
        // Clean up any existing DATABASE_URL
        std::env::remove_var("DATABASE_URL");

        // Set an invalid DATABASE_URL that doesn't match the pattern
        std::env::set_var("DATABASE_URL", "invalid://not-postgresql");

        let input = "url = \"${DATABASE_URL}\"";
        let _output = ConfigLoader::substitute_env_vars(input);

        // Should panic due to validation failure
    }

    #[test]
    #[serial]
    fn test_substitute_env_vars_with_valid_allowlisted_var() {
        // Clean up any existing DATABASE_URL
        std::env::remove_var("DATABASE_URL");

        // Set a valid DATABASE_URL
        std::env::set_var("DATABASE_URL", "postgresql://tasker:pass@localhost:5432/db");

        let input = "url = \"${DATABASE_URL}\"";
        let output = ConfigLoader::substitute_env_vars(input);

        assert_eq!(
            output,
            "url = \"postgresql://tasker:pass@localhost:5432/db\""
        );

        // Clean up
        std::env::remove_var("DATABASE_URL");
    }
}
