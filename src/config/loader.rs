//! Configuration Loader
//!
//! Environment-aware configuration loading system that mirrors Ruby's approach.
//! Handles YAML file discovery, environment detection, and configuration merging.

use super::error::{ConfigResult, ConfigurationError};
use super::TaskerConfig;
use serde_yaml::Value as YamlValue;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::{debug, warn};

/// Global configuration manager singleton
pub struct ConfigManager {
    config: TaskerConfig,
    environment: String,
    config_directory: PathBuf,
    /// Project root directory - the absolute root of the tasker-core-rs project
    project_root: PathBuf,
}

impl ConfigManager {
    /// Load configuration with environment auto-detection
    pub fn load() -> ConfigResult<Arc<ConfigManager>> {
        Self::load_from_directory(None)
    }

    /// Load configuration from a specific directory
    pub fn load_from_directory(config_dir: Option<PathBuf>) -> ConfigResult<Arc<ConfigManager>> {
        let environment = Self::detect_environment();
        Self::load_from_directory_with_env(config_dir, &environment)
    }

    /// Load configuration from a specific directory with explicit environment
    /// This is useful for testing without modifying global environment variables
    pub fn load_from_directory_with_env(
        config_dir: Option<PathBuf>,
        environment: &str,
    ) -> ConfigResult<Arc<ConfigManager>> {
        let config_directory = config_dir.unwrap_or_else(Self::default_config_directory);

        debug!(
            "Loading configuration for environment '{}' from directory: {}",
            environment,
            config_directory.display()
        );

        let config = Self::load_and_merge_config(&config_directory, environment)?;

        // Validate the loaded configuration
        config.validate()?;

        // Use sanitized configuration for logging to avoid exposing sensitive information
        let sanitized_config = Self::sanitize_config_for_logging(&config);
        debug!(
            "Configuration loaded successfully: {}",
            serde_json::to_string_pretty(&sanitized_config)
                .unwrap_or_else(|_| "[serialization error]".to_string())
        );

        crate::log_config!(info, "Configuration loaded successfully",
            environment: environment,
            database_host: config.database.host.clone(),
            pool_size: config.database.pool
        );

        // Determine project root from config directory
        let project_root = Self::determine_project_root(&config_directory)?;

        debug!("✅ Project root established: {}", project_root.display());

        Ok(Arc::new(ConfigManager {
            config,
            environment: environment.to_string(),
            config_directory,
            project_root,
        }))
    }

    /// Get the loaded configuration
    pub fn config(&self) -> &TaskerConfig {
        &self.config
    }

    /// Get sanitized configuration for debugging/logging that masks sensitive fields
    ///
    /// This method provides a safe way to log or debug configuration values without
    /// exposing sensitive information like passwords or API keys.
    ///
    /// # Returns
    ///
    /// A JSON representation of the configuration with sensitive fields masked
    pub fn debug_config(&self) -> serde_json::Value {
        Self::sanitize_config_for_logging(&self.config)
    }

    /// Create an emergency fallback configuration with safe defaults
    /// Used when configuration loading fails to prevent application crashes
    fn emergency_fallback() -> ConfigManager {
        warn!("Creating emergency fallback configuration with minimal safe defaults");

        // Use basic defaults that are guaranteed to work
        let fallback_config = TaskerConfig::default();

        ConfigManager {
            config: fallback_config,
            environment: Self::detect_environment(),
            config_directory: PathBuf::from("config"),
            project_root: PathBuf::from("."),
        }
    }

    /// Safely read a configuration file with resource management and size limits
    fn read_config_file_safely(path: &Path) -> ConfigResult<String> {
        const MAX_CONFIG_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB limit

        // Check file metadata first
        let metadata = std::fs::metadata(path)
            .map_err(|e| ConfigurationError::file_read_error(path.display().to_string(), e))?;

        // Check file size limit
        if metadata.len() > MAX_CONFIG_FILE_SIZE {
            return Err(ConfigurationError::invalid_value(
                "file_size",
                metadata.len().to_string(),
                format!(
                    "Configuration file too large ({}MB > {}MB limit)",
                    metadata.len() / (1024 * 1024),
                    MAX_CONFIG_FILE_SIZE / (1024 * 1024)
                ),
            ));
        }

        // Check if it's a regular file
        if !metadata.is_file() {
            return Err(ConfigurationError::invalid_value(
                "file_type",
                "directory or special file".to_string(),
                "Configuration path must point to a regular file",
            ));
        }

        // Read the file with proper error context
        std::fs::read_to_string(path)
            .map_err(|e| ConfigurationError::file_read_error(path.display().to_string(), e))
    }

    /// Sanitize configuration for safe logging by removing or masking sensitive fields
    ///
    /// This method ensures sensitive information like passwords, API keys, and connection
    /// strings are not exposed in log output while preserving debug information.
    fn sanitize_config_for_logging(config: &TaskerConfig) -> serde_json::Value {
        use serde_json::json;

        // Start with the full config as JSON for manipulation
        let mut config_json = json!(config);

        // Define sensitive field patterns to sanitize
        let sensitive_patterns = ["password", "secret", "key", "token", "credential", "auth"];

        // Recursively sanitize sensitive fields
        Self::sanitize_json_recursive(&mut config_json, &sensitive_patterns);

        config_json
    }

    /// Recursively sanitize sensitive fields in JSON configuration
    fn sanitize_json_recursive(value: &mut serde_json::Value, sensitive_patterns: &[&str]) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, val) in map.iter_mut() {
                    let key_lower = key.to_lowercase();

                    // Check if this field name contains sensitive patterns
                    let is_sensitive = sensitive_patterns
                        .iter()
                        .any(|pattern| key_lower.contains(pattern));

                    if is_sensitive {
                        match val {
                            serde_json::Value::String(s) => {
                                if s.is_empty() {
                                    *val = serde_json::Value::String("[EMPTY]".to_string());
                                } else {
                                    // Show only first 2 and last 2 characters for debugging
                                    let masked = if s.len() > 4 {
                                        format!("{}***{}", &s[..2], &s[s.len() - 2..])
                                    } else {
                                        "***".to_string()
                                    };
                                    *val = serde_json::Value::String(format!("[MASKED: {masked}]"));
                                }
                            }
                            serde_json::Value::Number(n) => {
                                *val = serde_json::Value::String(format!("[MASKED: {n}]"));
                            }
                            _ => {
                                *val = serde_json::Value::String("[MASKED]".to_string());
                            }
                        }
                    } else {
                        // Recursively process nested objects
                        Self::sanitize_json_recursive(val, sensitive_patterns);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr.iter_mut() {
                    Self::sanitize_json_recursive(item, sensitive_patterns);
                }
            }
            _ => {} // Primitive values don't need recursive processing
        }
    }

    /// Get the current environment
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Get the configuration directory
    pub fn config_directory(&self) -> &Path {
        &self.config_directory
    }

    /// Get the project root directory
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Detect current environment from environment variables
    /// Matches Ruby side detection: RAILS_ENV || RACK_ENV || APP_ENV || TASKER_ENV || 'development'
    fn detect_environment() -> String {
        env::var("TASKER_ENV")
            .or_else(|_| env::var("RAILS_ENV"))
            .or_else(|_| env::var("RACK_ENV"))
            .or_else(|_| env::var("APP_ENV"))
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
    }

    /// Get default configuration directory (matches Ruby DEFAULT_CONFIG_DIR)
    /// Ruby: File.expand_path('../../../../config', __dir__)
    /// From bindings/ruby/lib/tasker_core -> ../../../../config
    fn default_config_directory() -> PathBuf {
        // Use project root detection for cleaner path resolution
        if let Ok(project_root) = Self::find_project_root() {
            return project_root.join("config");
        }

        // Fallback to legacy detection method
        let possible_dirs = vec![
            // From cargo run or tests - look for config in project root
            PathBuf::from("config"),
            PathBuf::from("../../config"),    // From bindings/ruby
            PathBuf::from("../../../config"), // From deeper nesting
        ];

        // Try each possible directory
        for dir in possible_dirs {
            let config_path = dir.join("tasker-config.yaml");
            if config_path.exists() {
                debug!("Found config directory: {}", dir.display());
                return dir;
            }
        }

        // Fallback to ./config
        PathBuf::from("config")
    }

    /// Find project root by looking for characteristic files
    fn find_project_root() -> ConfigResult<PathBuf> {
        // Start from current directory and walk up
        let mut current_dir = std::env::current_dir()
            .map_err(|e| ConfigurationError::file_read_error("current_dir", e))?;

        // Project markers to look for (in order of preference)
        let markers = [
            "Cargo.toml",         // Rust project root
            ".git",               // Git repository root
            "tasker-config.yaml", // Our config file
            "README.md",          // Common project root indicator
        ];

        // Walk up directories looking for project markers
        loop {
            // Check for any of our markers
            for marker in &markers {
                let marker_path = current_dir.join(marker);
                if marker_path.exists() {
                    // For Cargo.toml, verify it's the right project
                    if marker == &"Cargo.toml" {
                        if let Ok(cargo_content) = std::fs::read_to_string(&marker_path) {
                            if cargo_content.contains("name = \"tasker-core-rs\"")
                                || cargo_content.contains("tasker")
                            {
                                debug!(
                                    "Project root found via Cargo.toml: {}",
                                    current_dir.display()
                                );
                                return Ok(current_dir);
                            }
                        }
                    } else {
                        debug!(
                            "Project root found via {}: {}",
                            marker,
                            current_dir.display()
                        );
                        return Ok(current_dir);
                    }
                }
            }

            // Move to parent directory
            if let Some(parent) = current_dir.parent() {
                current_dir = parent.to_path_buf();
            } else {
                break;
            }
        }

        Err(ConfigurationError::config_file_not_found(vec![
            PathBuf::from("project root not found"),
        ]))
    }

    /// Determine project root from config directory
    fn determine_project_root(config_directory: &Path) -> ConfigResult<PathBuf> {
        // First try environment variable override
        if let Ok(root) = std::env::var("TASKER_PROJECT_ROOT") {
            let root_path = PathBuf::from(root);
            if root_path.exists() {
                debug!("Using TASKER_PROJECT_ROOT: {}", root_path.display());
                return Ok(root_path);
            }
        }

        // Try CARGO_MANIFEST_DIR (set by Cargo during development/testing)
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let root_path = PathBuf::from(manifest_dir);
            debug!("Using CARGO_MANIFEST_DIR: {}", root_path.display());
            return Ok(root_path);
        }

        // Derive from config directory (config directory should be <project_root>/config)
        if config_directory.file_name().and_then(|n| n.to_str()) == Some("config") {
            if let Some(parent) = config_directory.parent() {
                debug!(
                    "Project root derived from config directory: {}",
                    parent.display()
                );
                return Ok(parent.to_path_buf());
            }
        }

        // Use our project root finder as last resort
        Self::find_project_root()
    }

    /// Find the configuration file
    fn find_config_file(config_directory: &Path) -> ConfigResult<PathBuf> {
        let possible_names = vec!["tasker-config.yaml", "tasker-config.yml"];
        let mut searched_paths = Vec::new();

        for name in possible_names {
            let config_path = config_directory.join(name);
            searched_paths.push(config_path.clone());

            if config_path.exists() {
                debug!("Found configuration file: {}", config_path.display());
                return Ok(config_path);
            }
        }

        Err(ConfigurationError::config_file_not_found(searched_paths))
    }

    /// Load and merge configuration with environment-specific overrides
    fn load_and_merge_config(
        config_directory: &Path,
        environment: &str,
    ) -> ConfigResult<TaskerConfig> {
        let config_file = Self::find_config_file(config_directory)?;

        // Read the YAML file safely with resource management
        let yaml_content = Self::read_config_file_safely(&config_file)?;

        // Parse YAML as a generic value for manipulation
        let mut yaml_data: YamlValue = serde_yaml::from_str(&yaml_content)
            .map_err(|e| ConfigurationError::invalid_yaml(config_file.display().to_string(), e))?;

        // Apply environment-specific overrides
        if let Some(env_overrides) = yaml_data
            .get(YamlValue::String(environment.to_string()))
            .cloned()
        {
            debug!(
                "Applying environment-specific overrides for: {}",
                environment
            );
            Self::merge_yaml_values(&mut yaml_data, env_overrides)?;
        }

        // Remove environment sections to avoid confusion
        if let YamlValue::Mapping(ref mut map) = yaml_data {
            map.remove(YamlValue::String("development".to_string()));
            map.remove(YamlValue::String("test".to_string()));
            map.remove(YamlValue::String("production".to_string()));
        }

        // Convert to our config struct
        let mut config: TaskerConfig = serde_yaml::from_value(yaml_data).map_err(|e| {
            ConfigurationError::invalid_yaml(
                config_file.display().to_string(),
                format!("Failed to deserialize configuration: {e}"),
            )
        })?;

        // Ensure environment is set correctly
        config.execution.environment = environment.to_string();

        Ok(config)
    }

    /// Recursively merge YAML values (environment overrides into base config)
    fn merge_yaml_values(base: &mut YamlValue, override_value: YamlValue) -> ConfigResult<()> {
        match (&mut *base, override_value) {
            (YamlValue::Mapping(base_map), YamlValue::Mapping(override_map)) => {
                for (key, value) in override_map {
                    if let Some(existing_value) = base_map.get_mut(&key) {
                        // Recursively merge nested objects
                        Self::merge_yaml_values(existing_value, value)?;
                    } else {
                        // Add new key-value pair
                        base_map.insert(key, value);
                    }
                }
            }
            (base_ref, override_val) => {
                // For non-mapping values, override completely
                *base_ref = override_val;
            }
        }
        Ok(())
    }

    /// Expand environment variables in configuration values
    #[allow(dead_code)]
    fn expand_environment_variables(&self, config: &mut TaskerConfig) -> ConfigResult<()> {
        // Handle database URL expansion
        if let Some(ref mut url) = config.database.url {
            if url.starts_with("${") && url.ends_with("}") {
                let var_name = &url[2..url.len() - 1];
                match env::var(var_name) {
                    Ok(env_value) => {
                        debug!(
                            "Expanding environment variable {} in database URL",
                            var_name
                        );
                        *url = env_value;
                    }
                    Err(_) => {
                        warn!(
                            "Environment variable {} not found, keeping original value",
                            var_name
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolve a relative path from project root
    pub fn resolve_path<P: AsRef<Path>>(&self, relative_path: P) -> PathBuf {
        self.project_root.join(relative_path)
    }

    /// Resolve a configuration file path (relative to config directory)
    pub fn resolve_config_path<P: AsRef<Path>>(&self, relative_path: P) -> PathBuf {
        self.config_directory.join(relative_path)
    }

    /// Resolve a task configuration file path
    pub fn resolve_task_config_path<P: AsRef<Path>>(&self, relative_path: P) -> PathBuf {
        self.resolve_config_path("tasks").join(relative_path)
    }

    /// Get task templates search paths resolved to absolute paths
    pub fn task_template_search_paths(&self) -> Vec<PathBuf> {
        self.config
            .task_templates
            .search_paths
            .iter()
            .map(|path| self.resolve_path(path))
            .collect()
    }

    /// Get custom events directories resolved to absolute paths
    pub fn custom_events_directories(&self) -> Vec<PathBuf> {
        self.config
            .engine
            .custom_events_directories
            .iter()
            .map(|path| self.resolve_path(path))
            .collect()
    }

    /// Get task handler directory resolved to absolute path
    pub fn task_handler_directory(&self) -> PathBuf {
        self.resolve_path(&self.config.engine.task_handler_directory)
    }

    /// Get task config directory resolved to absolute path
    pub fn task_config_directory(&self) -> PathBuf {
        self.resolve_path(&self.config.engine.task_config_directory)
    }
}

/// Global configuration singleton for easy access throughout the application
static GLOBAL_CONFIG: OnceLock<Arc<ConfigManager>> = OnceLock::new();
static CONFIG_LOCK: Mutex<()> = Mutex::new(());

impl ConfigManager {
    /// Get or initialize the global configuration instance
    pub fn global() -> Arc<ConfigManager> {
        GLOBAL_CONFIG
            .get_or_init(|| {
                let _lock = CONFIG_LOCK.lock().unwrap();
                ConfigManager::load().unwrap_or_else(|e| {
                    eprintln!("Failed to load configuration: {e}");
                    eprintln!("Using emergency fallback configuration");
                    warn!("Configuration loading failed, using fallback: {e}");
                    Arc::new(ConfigManager::emergency_fallback())
                })
            })
            .clone()
    }

    /// Initialize global configuration with a specific directory (for testing)
    pub fn initialize_global(config_dir: Option<PathBuf>) -> ConfigResult<Arc<ConfigManager>> {
        let _lock = CONFIG_LOCK.lock().unwrap();

        let config_manager = ConfigManager::load_from_directory(config_dir)?;

        // This will only succeed once, but that's what we want for a singleton
        let _ = GLOBAL_CONFIG.set(config_manager.clone());

        Ok(config_manager)
    }

    /// Reset global configuration (for testing only)
    #[cfg(test)]
    pub fn reset_global_for_testing() {
        // Note: This is a testing-only function and doesn't actually reset the OnceLock
        // In real tests, you should use initialize_global with test directories
        // This function exists for API compatibility but does nothing
        // The OnceLock pattern means the first initialization wins
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_config_yaml() -> &'static str {
        r#"
# Test configuration
auth:
  authentication_enabled: false
  strategy: "none"
  current_user_method: "current_user"
  authenticate_user_method: "authenticate_user!"
  authorization_enabled: false
  authorization_coordinator_class: "Tasker::Authorization::BaseCoordinator"

database:
  enable_secondary_database: false
  url: "${DATABASE_URL}"
  adapter: "postgresql"
  encoding: "unicode"
  host: "localhost"
  username: "test_user"
  password: "test_password"
  pool: 10
  variables:
    statement_timeout: 5000
  checkout_timeout: 10
  reaping_frequency: 10

telemetry:
  enabled: false
  service_name: "tasker-core-rs"
  sample_rate: 1.0

engine:
  task_handler_directory: "tasks"
  task_config_directory: "tasker/tasks"
  identity_strategy: "default"
  custom_events_directories:
    - "config/tasker/events"

task_templates:
  search_paths:
    - "config/task_templates/*.{yml,yaml}"

health:
  enabled: true
  check_interval_seconds: 60
  alert_thresholds:
    error_rate: 0.05
    queue_depth: 1000.0

dependency_graph:
  max_depth: 50
  cycle_detection_enabled: true
  optimization_enabled: true
circuit_breakers:
  enabled: false
  global_settings:
    max_circuit_breakers: 50
    metrics_collection_interval_seconds: 30
    auto_create_enabled: true
    min_state_transition_interval_seconds: 1.0
  default_config:
    failure_threshold: 3
    timeout_seconds: 5
    success_threshold: 2
  component_configs: {}

system:
  default_dependent_system: "default"
  default_queue_name: "default"
  version: "1.0.0"
  max_recursion_depth: 100

backoff:
  default_backoff_seconds: [1, 2, 4, 8, 16, 32]
  max_backoff_seconds: 300
  backoff_multiplier: 2.0
  jitter_enabled: true
  jitter_max_percentage: 0.1
  reenqueue_delays:
    has_ready_steps: 0
    waiting_for_dependencies: 45
    processing: 10
  default_reenqueue_delay: 30
  buffer_seconds: 5

execution:
  processing_mode: "pgmq"
  max_concurrent_tasks: 100
  max_concurrent_steps: 1000
  default_timeout_seconds: 3600
  step_execution_timeout_seconds: 300
  environment: "development"
  max_discovery_attempts: 3
  step_batch_size: 10
  max_retries: 3
  max_workflow_steps: 1000
  connection_timeout_seconds: 30

reenqueue:
  has_ready_steps: 1
  waiting_for_dependencies: 5
  processing: 2

events:
  batch_size: 100
  enabled: true
  batch_timeout_ms: 1000

cache:
  enabled: true
  ttl_seconds: 3600
  max_size: 10000

query_cache:
  enabled: true
  active_workers:
    ttl_seconds: 30
    max_entries: 1000
  worker_health:
    ttl_seconds: 10
    max_entries: 500
  task_metadata:
    ttl_seconds: 300
    max_entries: 2000
  handler_metadata:
    ttl_seconds: 600
    max_entries: 100
  cleanup_interval_seconds: 300
  memory_pressure_threshold: 0.8

pgmq:
  poll_interval_ms: 250
  visibility_timeout_seconds: 30
  batch_size: 10
  max_retries: 3
  default_namespaces:
    - "default"
    - "fulfillment"
  queue_naming_pattern: "{namespace}_queue"
  max_batch_size: 100
  shutdown_timeout_seconds: 30

orchestration:
  mode: "distributed"
  task_requests_queue_name: "task_requests_queue"
  tasks_per_cycle: 5
  cycle_interval_ms: 250
  task_request_polling_interval_ms: 250
  task_request_visibility_timeout_seconds: 300
  task_request_batch_size: 10
  active_namespaces:
    - "fulfillment"
    - "inventory"
  max_concurrent_orchestrators: 3
  enable_performance_logging: false
  default_claim_timeout_seconds: 300
  queues:
    task_requests: "task_requests_queue"
    task_processing: "task_processing_queue"
    batch_results: "batch_results_queue"
    step_results: "orchestration_step_results"
    worker_queues:
      default: "default_queue"
      fulfillment: "fulfillment_queue"
    settings:
      visibility_timeout_seconds: 30
      message_retention_seconds: 604800
      dead_letter_queue_enabled: true
      max_receive_count: 3
  embedded_orchestrator:
    auto_start: false
    namespaces:
      - "default"
      - "fulfillment"
    shutdown_timeout_seconds: 30
  enable_heartbeat: true
  heartbeat_interval_ms: 5000

# Environment-specific overrides
test:
  database:
    database: "tasker_rust_test"
  execution:
    environment: "test"

development:
  database:
    database: "tasker_rust_development"
  execution:
    environment: "development"

production:
  database:
    database: "tasker_production"
  execution:
    environment: "production"
  telemetry:
    enabled: true
    sample_rate: 0.1
"#
    }

    fn setup_test_config_dir() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().to_path_buf();
        let config_file = config_dir.join("tasker-config.yaml");

        fs::write(&config_file, create_test_config_yaml()).unwrap();

        (temp_dir, config_dir)
    }

    #[test]
    fn test_environment_detection() {
        // Test RAILS_ENV takes precedence
        env::set_var("RAILS_ENV", "test");
        let env = ConfigManager::detect_environment();
        assert_eq!(env, "test");
        env::remove_var("RAILS_ENV");

        // Test TASKER_ENV
        env::set_var("TASKER_ENV", "production");
        let env = ConfigManager::detect_environment();
        assert_eq!(env, "production");
        env::remove_var("TASKER_ENV");
        // reset
        env::set_var("RAILS_ENV", "test");
        env::set_var("TASKER_ENV", "test");
        env::set_var("APP_ENV", "test");
    }

    #[test]
    fn test_config_file_discovery() {
        let (_temp_dir, config_dir) = setup_test_config_dir();

        let config_file = ConfigManager::find_config_file(&config_dir).unwrap();
        assert!(config_file.exists());
        assert_eq!(config_file.file_name().unwrap(), "tasker-config.yaml");
    }

    #[test]
    fn test_config_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let empty_dir = temp_dir.path();

        let result = ConfigManager::find_config_file(empty_dir);
        assert!(result.is_err());

        if let Err(ConfigurationError::ConfigFileNotFound { searched_paths }) = result {
            assert!(!searched_paths.is_empty());
        } else {
            panic!("Expected ConfigFileNotFound error");
        }
    }

    #[test]
    fn test_basic_config_loading() {
        let (_temp_dir, config_dir) = setup_test_config_dir();

        env::set_var("TASKER_ENV", "test");
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();
        env::remove_var("TASKER_ENV");

        let config = config_manager.config();

        // Test basic configuration
        assert_eq!(config.database.host, "localhost");
        assert_eq!(config.database.username, "test_user");
        assert_eq!(config.database.pool, 10);

        // Test environment override was applied
        assert_eq!(config.execution.environment, "test");
        assert_eq!(
            config.database.database,
            Some("tasker_rust_test".to_string())
        );

        // Test project root is available
        assert!(config_manager.project_root().exists());
    }

    #[test]
    fn test_environment_specific_overrides() {
        let (_temp_dir, config_dir) = setup_test_config_dir();

        // Test production environment without modifying global state
        let config_manager =
            ConfigManager::load_from_directory_with_env(Some(config_dir.clone()), "production")
                .unwrap();

        let config = config_manager.config();
        assert_eq!(config.execution.environment, "production");
        assert_eq!(
            config.database.database,
            Some("tasker_production".to_string())
        );
        assert!(config.telemetry.enabled);
        assert_eq!(config.telemetry.sample_rate, 0.1);

        // Test development environment without modifying global state
        let config_manager =
            ConfigManager::load_from_directory_with_env(Some(config_dir.clone()), "development")
                .unwrap();

        let config = config_manager.config();
        assert_eq!(config.execution.environment, "development");
        assert_eq!(
            config.database.database,
            Some("tasker_rust_development".to_string())
        );
        assert!(!config.telemetry.enabled); // Base config value

        // Test test environment
        let config_manager =
            ConfigManager::load_from_directory_with_env(Some(config_dir), "test").unwrap();

        let config = config_manager.config();
        assert_eq!(config.execution.environment, "test");
        assert_eq!(
            config.database.database,
            Some("tasker_rust_test".to_string())
        );
    }

    #[test]
    fn test_database_url_generation() {
        let (_temp_dir, config_dir) = setup_test_config_dir();

        // Clear DATABASE_URL to ensure we test the component-based URL generation
        let original_database_url = env::var("DATABASE_URL").ok();
        env::remove_var("DATABASE_URL");

        env::set_var("TASKER_ENV", "test");
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();
        env::remove_var("TASKER_ENV");

        let database_url = config_manager.config().database_url();

        assert!(database_url.contains("test_user:test_password"));
        assert!(database_url.contains("localhost:5432"));
        assert!(database_url.contains("tasker_rust_test"));

        // Restore original DATABASE_URL if it was set
        if let Some(original_url) = original_database_url {
            env::set_var("DATABASE_URL", original_url);
        }
    }

    #[test]
    fn test_duration_helpers() {
        let (_temp_dir, config_dir) = setup_test_config_dir();
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();
        let config = config_manager.config();

        assert_eq!(
            config.execution.step_execution_timeout(),
            Duration::from_secs(300)
        );

        assert_eq!(config.pgmq.poll_interval(), Duration::from_millis(250));

        assert_eq!(
            config.orchestration.cycle_interval(),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn test_pgmq_queue_name_generation() {
        let (_temp_dir, config_dir) = setup_test_config_dir();
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();
        let config = config_manager.config();

        assert_eq!(
            config.pgmq.queue_name_for_namespace("fulfillment"),
            "fulfillment_queue"
        );

        assert_eq!(
            config.pgmq.queue_name_for_namespace("inventory"),
            "inventory_queue"
        );
    }

    #[test]
    fn test_config_validation() {
        let (_temp_dir, config_dir) = setup_test_config_dir();
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();

        // Config should be valid
        assert!(config_manager.config().validate().is_ok());
    }

    #[test]
    fn test_config_validation_errors() {
        let (_temp_dir, config_dir) = setup_test_config_dir();
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();

        // Create a modified config for validation testing
        let mut modified_config = config_manager.config().clone();
        modified_config.database.host = "".to_string();

        let result = modified_config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_path_resolution() {
        let (_temp_dir, config_dir) = setup_test_config_dir();
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();

        // Test basic path resolution
        let resolved = config_manager.resolve_path("tasks/example.yaml");
        assert!(resolved.to_string_lossy().ends_with("tasks/example.yaml"));

        // Test config path resolution
        let config_path = config_manager.resolve_config_path("custom.yaml");
        assert!(config_path.to_string_lossy().ends_with("custom.yaml"));

        // Test task config path resolution
        let task_config = config_manager.resolve_task_config_path("order_processing.yaml");
        assert!(task_config.to_string_lossy().contains("tasks"));
        assert!(task_config
            .to_string_lossy()
            .ends_with("order_processing.yaml"));
    }

    #[test]
    fn test_project_root_detection() {
        // This test depends on being run from the project directory
        if let Ok(project_root) = ConfigManager::find_project_root() {
            assert!(project_root.join("Cargo.toml").exists());
            assert!(project_root.join("src").exists());
        }
    }

    #[test]
    fn test_search_paths() {
        let (_temp_dir, config_dir) = setup_test_config_dir();
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();

        // Test task template search paths
        let search_paths = config_manager.task_template_search_paths();
        assert!(!search_paths.is_empty());

        // Test custom events directories
        let events_dirs = config_manager.custom_events_directories();
        assert!(!events_dirs.is_empty());
    }

    #[test]
    fn test_config_sanitization() {
        let (_temp_dir, config_dir) = setup_test_config_dir();
        let config_manager = ConfigManager::load_from_directory(Some(config_dir)).unwrap();

        // Get the sanitized configuration
        let sanitized = config_manager.debug_config();

        // Verify that the password field is masked
        let password_value = sanitized
            .get("database")
            .and_then(|db| db.get("password"))
            .and_then(|p| p.as_str());

        assert!(password_value.is_some(), "Password field should be present");
        let password_str = password_value.unwrap();
        assert!(
            password_str.contains("[MASKED:"),
            "Password should be masked, got: {password_str}"
        );
        assert!(
            password_str.contains("te***rd"),
            "Password should show partial content, got: {password_str}"
        );

        // Verify that non-sensitive fields are not masked
        let host_value = sanitized
            .get("database")
            .and_then(|db| db.get("host"))
            .and_then(|h| h.as_str());

        assert_eq!(host_value, Some("localhost"), "Host should not be masked");

        // Verify that auth section is completely masked (due to sensitive section name)
        let auth_section = sanitized.get("auth");
        assert!(auth_section.is_some(), "Auth section should be present");

        // Auth section should be completely masked since "auth" is in sensitive patterns
        let auth_str = auth_section.unwrap().as_str();
        assert!(
            auth_str.is_some(),
            "Auth section should be masked as string"
        );
        assert_eq!(
            auth_str.unwrap(),
            "[MASKED]",
            "Auth section should be completely masked"
        );

        // Verify that the original configuration is not modified
        let original_password = &config_manager.config().database.password;
        assert_eq!(
            original_password, "test_password",
            "Original config should not be modified"
        );
    }

    #[test]
    #[ignore] // This test modifies global state
    fn test_global_config_singleton() {
        ConfigManager::reset_global_for_testing();

        let (_temp_dir, config_dir) = setup_test_config_dir();

        let config1 = ConfigManager::initialize_global(Some(config_dir)).unwrap();
        let config2 = ConfigManager::global();

        // Should be the same instance
        assert!(Arc::ptr_eq(&config1, &config2));
    }
}
