//! Unified TOML Configuration Loader
//!
//! Implements TAS-34 Unified Configuration Architecture with strict validation,
//! no silent defaults, and comprehensive error handling.
//!
//! This replaces the previous YAML-based component loader with a clean TOML-based
//! approach that follows fail-fast principles.

use super::error::{ConfigResult, ConfigurationError};
use dotenvy::dotenv;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use workspace_tools::workspace;

/// Unified configuration loader using TOML format exclusively
///
/// # Design Principles
/// - **Fail Fast**: No silent defaults - all missing configs cause explicit failures
/// - **Single Format**: TOML only, no YAML/mixed format support
/// - **Strict Validation**: All configurations validated before use
/// - **Environment Overrides**: Clean separation of base and environment-specific configs
/// - **Workspace Detection**: Uses workspace_tools for reliable project root detection
#[derive(Debug)]
pub struct UnifiedConfigLoader {
    /// Configuration root directory containing base/ and environments/
    root: PathBuf,
    /// Current environment (development, test, production, etc.)
    environment: String,
    /// Cache for loaded components to avoid re-parsing
    component_cache: HashMap<String, toml::Value>,
}

impl UnifiedConfigLoader {
    /// Create a new unified config loader with automatic workspace detection
    ///
    /// # Arguments
    /// * `environment` - Environment name (development, test, production, etc.)
    ///
    /// # Returns
    /// * `Result<Self, ConfigurationError>` - Loader instance or error
    ///
    /// # Errors
    /// * `ConfigurationError::ValidationError` - If workspace root cannot be found
    /// * `ConfigurationError::ConfigFileNotFound` - If config directory doesn't exist
    pub fn new(environment: &str) -> ConfigResult<Self> {
        // Use workspace_tools to find root - fail explicitly if it doesn't work
        let ws = workspace().map_err(|e| {
            ConfigurationError::validation_error(format!(
                "Failed to find workspace root using workspace_tools: {e}"
            ))
        })?;

        let root = ws.join("config").join("tasker");

        info!(
            "🔧 UNIFIED_CONFIG: Using workspace config root: {}",
            root.display()
        );

        Self::with_root(root, environment)
    }

    /// Create a unified config loader with explicit root path
    ///
    /// This is useful for embedded mode where Ruby/Python can pass explicit paths.
    ///
    /// # Arguments
    /// * `root` - Configuration root directory path
    /// * `environment` - Environment name
    ///
    /// # Returns
    /// * `Result<Self, ConfigurationError>` - Loader instance or error
    pub fn with_root(root: PathBuf, environment: &str) -> ConfigResult<Self> {
        // Validate that the root directory exists
        if !root.exists() {
            return Err(ConfigurationError::config_file_not_found(
                vec![root.clone()],
            ));
        }

        // Validate that base/ directory exists (root should already point to tasker config dir)
        let base_dir = root.join("base");
        if !base_dir.exists() {
            return Err(ConfigurationError::config_file_not_found(vec![
                base_dir.clone()
            ]));
        }

        info!(
            "🔧 UNIFIED_CONFIG: Initialized for environment '{}' with root: {}",
            environment,
            root.display()
        );

        Ok(Self {
            root,
            environment: environment.to_string(),
            component_cache: HashMap::new(),
        })
    }

    /// Load a single component configuration with environment overrides
    ///
    /// # Arguments
    /// * `component` - Component name (database, executor_pools, etc.)
    ///
    /// # Returns
    /// * `Result<toml::Value, ConfigurationError>` - Merged configuration or error
    ///
    /// # Process
    /// 1. Load base component configuration from base/{component}.toml
    /// 2. Apply environment overrides from environments/{env}/{component}.toml if present
    /// 3. Validate the merged configuration
    /// 4. Return the validated config
    pub fn load_component(&mut self, component: &str) -> ConfigResult<toml::Value> {
        debug!("Loading component configuration: {}", component);

        // Check cache first
        if let Some(cached_config) = self.component_cache.get(component) {
            debug!("Using cached configuration for component: {}", component);
            return Ok(cached_config.clone());
        }

        // 1. Load base component - this is REQUIRED, never optional
        let base_path = self.root.join("base").join(format!("{component}.toml"));
        let mut config = self.load_toml_with_env_substitution(&base_path)?;

        debug!(
            "Loaded base configuration for {}: {} bytes",
            component,
            config.to_string().len()
        );

        // 2. Apply environment overrides if they exist
        let env_path = self
            .root
            .join("environments")
            .join(&self.environment)
            .join(format!("{component}.toml"));

        if env_path.exists() {
            debug!(
                "Applying environment overrides from: {}",
                env_path.display()
            );
            let overrides = self.load_toml_with_env_substitution(&env_path)?;
            self.merge_toml(&mut config, overrides)?;

            info!(
                "Applied environment overrides for {} in {} environment",
                component, self.environment
            );
        } else {
            debug!(
                "No environment overrides found for {} in {} environment",
                component, self.environment
            );
        }

        // 3. Validate the merged configuration
        self.validate_component(component, &config)?;

        // 4. Cache the result and return
        self.component_cache
            .insert(component.to_string(), config.clone());

        info!(
            "✅ Successfully loaded and validated component: {}",
            component
        );
        Ok(config)
    }

    /// Load all available components
    ///
    /// # Returns
    /// * `Result<HashMap<String, toml::Value>, ConfigurationError>` - All component configs
    pub fn load_all_components(&mut self) -> ConfigResult<HashMap<String, toml::Value>> {
        let components = self.discover_available_components()?;
        let mut all_configs = HashMap::new();

        for component in components {
            let config = self.load_component(&component)?;
            all_configs.insert(component, config);
        }

        info!("✅ Loaded {} component configurations", all_configs.len());
        Ok(all_configs)
    }

    /// Discover all available components in the base/ directory
    fn discover_available_components(&self) -> ConfigResult<Vec<String>> {
        let base_dir = self.root.join("base");
        let mut components = Vec::new();

        let entries = std::fs::read_dir(&base_dir)
            .map_err(|e| ConfigurationError::file_read_error(base_dir.display().to_string(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ConfigurationError::file_read_error(base_dir.display().to_string(), e)
            })?;

            let path = entry.path();
            if let Some(extension) = path.extension() {
                if extension == "toml" {
                    if let Some(stem) = path.file_stem() {
                        if let Some(component_name) = stem.to_str() {
                            components.push(component_name.to_string());
                        }
                    }
                }
            }
        }

        components.sort();
        debug!("Discovered components: {:?}", components);
        Ok(components)
    }

    /// Load TOML file with environment variable substitution
    ///
    /// Supports ${VAR} and ${VAR:-default} syntax for environment variables
    fn load_toml_with_env_substitution(&self, path: &Path) -> ConfigResult<toml::Value> {
        // NO FALLBACKS - fail immediately if file doesn't exist
        if !path.exists() {
            return Err(ConfigurationError::config_file_not_found(vec![
                path.to_path_buf()
            ]));
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigurationError::file_read_error(path.display().to_string(), e))?;

        // Parse TOML first
        let mut value: toml::Value = toml::from_str(&content)
            .map_err(|e| ConfigurationError::invalid_toml(path.display().to_string(), e))?;

        // Then substitute environment variables in the parsed structure
        self.substitute_env_vars_in_value(&mut value)?;

        Ok(value)
    }

    /// Recursively substitute environment variables in a TOML value
    fn substitute_env_vars_in_value(&self, value: &mut toml::Value) -> ConfigResult<()> {
        match value {
            toml::Value::String(s) => {
                *s = self.expand_env_vars(s)?;
            }
            toml::Value::Table(table) => {
                for (_, v) in table.iter_mut() {
                    self.substitute_env_vars_in_value(v)?;
                }
            }
            toml::Value::Array(array) => {
                for v in array.iter_mut() {
                    self.substitute_env_vars_in_value(v)?;
                }
            }
            _ => {} // Numbers, booleans, dates don't need substitution
        }
        Ok(())
    }

    /// Expand environment variables in a string
    ///
    /// Supports:
    /// - ${VAR} - replace with environment variable value, fail if not set
    /// - ${VAR:-default} - replace with environment variable value or default if not set
    fn expand_env_vars(&self, input: &str) -> ConfigResult<String> {
        use regex::Regex;
        use std::env;

        // Match ${VAR} or ${VAR:-default}
        let re = Regex::new(r"\$\{([^}:]+)(?::-([^}]*))?\}").unwrap();

        let mut result = String::new();
        let mut last_end = 0;

        for cap in re.captures_iter(input) {
            let whole_match = cap.get(0).unwrap();
            let var_name = cap.get(1).unwrap().as_str();
            let default_value = cap.get(2).map(|m| m.as_str());

            // Append text before the match
            result.push_str(&input[last_end..whole_match.start()]);

            // Get the environment variable value
            match env::var(var_name) {
                Ok(value) => {
                    result.push_str(&value);
                }
                Err(_) => {
                    if let Some(default) = default_value {
                        // Use the default value
                        result.push_str(default);
                    } else {
                        // Fail fast if no default provided and variable not found
                        return Err(ConfigurationError::validation_error(format!(
                            "Environment variable '{var_name}' not found and no default provided"
                        )));
                    }
                }
            }

            last_end = whole_match.end();
        }

        // Append any remaining text
        result.push_str(&input[last_end..]);

        Ok(result)
    }

    /// Merge TOML configurations with environment overrides taking precedence
    #[allow(clippy::only_used_in_recursion)]
    fn merge_toml(&self, base: &mut toml::Value, override_config: toml::Value) -> ConfigResult<()> {
        if let (toml::Value::Table(base_table), toml::Value::Table(override_table)) =
            (base, override_config)
        {
            for (key, value) in override_table {
                if let Some(base_value) = base_table.get_mut(&key) {
                    // Recursively merge nested tables
                    if let (toml::Value::Table(_), toml::Value::Table(_)) = (&*base_value, &value) {
                        self.merge_toml(base_value, value)?;
                    } else {
                        // Override scalar values
                        *base_value = value;
                    }
                } else {
                    // Add new keys from override
                    base_table.insert(key, value);
                }
            }
        }

        Ok(())
    }

    /// Validate component configuration
    ///
    /// This implements component-specific validation rules to catch configuration
    /// errors early and provide helpful error messages.
    fn validate_component(&self, name: &str, config: &toml::Value) -> ConfigResult<()> {
        match name {
            "database" => self.validate_database_config(config),
            "executor_pools" => self.validate_executor_pools_config(config),
            "pgmq" => self.validate_pgmq_config(config),
            "orchestration" => self.validate_orchestration_config(config),
            _ => {
                // For components without specific validation, just ensure it's a valid table
                if !config.is_table() {
                    return Err(ConfigurationError::validation_error(format!(
                        "Component '{name}' configuration must be a TOML table"
                    )));
                }
                Ok(())
            }
        }
    }

    /// Validate database configuration
    fn validate_database_config(&self, config: &toml::Value) -> ConfigResult<()> {
        let table = config.as_table().ok_or_else(|| {
            ConfigurationError::validation_error("Database configuration must be a TOML table")
        })?;

        // Check for required sections
        if !table.contains_key("database") {
            return Err(ConfigurationError::missing_required_field(
                "database",
                "database configuration",
            ));
        }

        // Validate pool configuration if present
        if let Some(db_section) = table.get("database") {
            if let Some(pool_section) = db_section.get("pool") {
                self.validate_pool_config(pool_section)?;
            }
        }

        Ok(())
    }

    /// Validate database pool configuration
    fn validate_pool_config(&self, pool_config: &toml::Value) -> ConfigResult<()> {
        let pool_table = pool_config.as_table().ok_or_else(|| {
            ConfigurationError::validation_error("Pool configuration must be a TOML table")
        })?;

        // Validate max_connections
        if let Some(max_conn) = pool_table.get("max_connections") {
            let max_val = max_conn.as_integer().ok_or_else(|| {
                ConfigurationError::invalid_value(
                    "max_connections",
                    max_conn.to_string(),
                    "must be an integer",
                )
            })?;

            if max_val <= 0 {
                return Err(ConfigurationError::invalid_value(
                    "max_connections",
                    max_val.to_string(),
                    "must be greater than 0",
                ));
            }

            if max_val > 1000 {
                warn!(
                    "Database max_connections ({}) is very high and may impact performance",
                    max_val
                );
            }
        }

        // Validate min_connections vs max_connections
        if let (Some(min_conn), Some(max_conn)) = (
            pool_table.get("min_connections"),
            pool_table.get("max_connections"),
        ) {
            let min_val = min_conn.as_integer().unwrap_or(0);
            let max_val = max_conn.as_integer().unwrap_or(0);

            if min_val > max_val {
                return Err(ConfigurationError::validation_error(format!(
                    "min_connections ({min_val}) cannot exceed max_connections ({max_val})"
                )));
            }
        }

        Ok(())
    }

    /// Validate executor pools configuration
    fn validate_executor_pools_config(&self, config: &toml::Value) -> ConfigResult<()> {
        let table = config.as_table().ok_or_else(|| {
            ConfigurationError::validation_error(
                "Executor pools configuration must be a TOML table",
            )
        })?;

        // Check for required executor_pools section
        if !table.contains_key("executor_pools") {
            return Err(ConfigurationError::missing_required_field(
                "executor_pools",
                "executor pools configuration",
            ));
        }

        // Validate individual executor pool configurations
        if let Some(pools_section) = table.get("executor_pools") {
            if let Some(pools_table) = pools_section.as_table() {
                for (pool_name, pool_config) in pools_table {
                    if pool_name != "coordinator" {
                        // coordinator has different schema
                        self.validate_single_executor_pool(pool_name, pool_config)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate a single executor pool configuration
    fn validate_single_executor_pool(&self, name: &str, config: &toml::Value) -> ConfigResult<()> {
        let pool_table = config.as_table().ok_or_else(|| {
            ConfigurationError::validation_error(format!(
                "Executor pool '{name}' configuration must be a TOML table"
            ))
        })?;

        // Validate min/max executors
        if let (Some(min_exec), Some(max_exec)) = (
            pool_table.get("min_executors"),
            pool_table.get("max_executors"),
        ) {
            let min_val = min_exec.as_integer().ok_or_else(|| {
                ConfigurationError::invalid_value(
                    "min_executors",
                    min_exec.to_string(),
                    "must be an integer",
                )
            })?;

            let max_val = max_exec.as_integer().ok_or_else(|| {
                ConfigurationError::invalid_value(
                    "max_executors",
                    max_exec.to_string(),
                    "must be an integer",
                )
            })?;

            if min_val < 0 || max_val < 0 {
                return Err(ConfigurationError::validation_error(format!(
                    "Executor pool '{name}' min/max executors must be non-negative"
                )));
            }

            if min_val > max_val {
                return Err(ConfigurationError::validation_error(format!(
                    "Executor pool '{name}' min_executors ({min_val}) cannot exceed max_executors ({max_val})"
                )));
            }

            if max_val > 100 {
                warn!(
                    "Executor pool '{}' max_executors ({}) is very high",
                    name, max_val
                );
            }
        }

        Ok(())
    }

    /// Validate PGMQ configuration
    fn validate_pgmq_config(&self, config: &toml::Value) -> ConfigResult<()> {
        let table = config.as_table().ok_or_else(|| {
            ConfigurationError::validation_error("PGMQ configuration must be a TOML table")
        })?;

        if !table.contains_key("pgmq") {
            return Err(ConfigurationError::missing_required_field(
                "pgmq",
                "PGMQ configuration",
            ));
        }

        Ok(())
    }

    /// Validate orchestration configuration
    fn validate_orchestration_config(&self, config: &toml::Value) -> ConfigResult<()> {
        let table = config.as_table().ok_or_else(|| {
            ConfigurationError::validation_error("Orchestration configuration must be a TOML table")
        })?;

        if !table.contains_key("orchestration") {
            return Err(ConfigurationError::missing_required_field(
                "orchestration",
                "orchestration configuration",
            ));
        }

        Ok(())
    }

    /// Get the current environment
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Get the configuration root path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Clear the component cache (useful for testing)
    pub fn clear_cache(&mut self) {
        self.component_cache.clear();
        debug!("Configuration cache cleared");
    }
}

/// Environment detection utilities
impl UnifiedConfigLoader {
    /// Detect current environment from environment variables
    ///
    /// Matches the detection order from the original system:
    /// TASKER_ENV || RAILS_ENV || RACK_ENV || APP_ENV || 'development'
    ///
    /// Note: dotenv() should be called earlier in the chain (e.g., in embedded_bridge.rs)
    /// to ensure environment variables are loaded before configuration detection.
    pub fn detect_environment() -> String {
        dotenv().ok();
        env::var("TASKER_ENV")
            .or_else(|_| env::var("RAILS_ENV"))
            .or_else(|_| env::var("RACK_ENV"))
            .or_else(|_| env::var("APP_ENV"))
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
    }

    /// Create loader with automatic environment detection
    pub fn new_from_env() -> ConfigResult<Self> {
        let environment = Self::detect_environment();
        Self::new(&environment)
    }

    /// Load configuration with resource constraint validation
    ///
    /// This integrates with the resource validation system to ensure that
    /// executor pool configurations don't exceed system limits.
    pub fn load_with_resource_validation(&mut self) -> ConfigResult<ValidatedConfig> {
        let all_configs = self.load_all_components()?;

        // Extract relevant configurations for validation
        let database_config = all_configs.get("database").ok_or_else(|| {
            ConfigurationError::missing_required_field("database", "system configuration")
        })?;

        let executor_pools_config = all_configs.get("executor_pools").ok_or_else(|| {
            ConfigurationError::missing_required_field("executor_pools", "system configuration")
        })?;

        // Validate resource constraints
        self.validate_resource_constraints(database_config, executor_pools_config)?;

        Ok(ValidatedConfig {
            configs: all_configs,
        })
    }

    /// Load configuration directly as TaskerConfig with resource validation
    ///
    /// This is the preferred method for loading TOML configuration as it avoids
    /// unnecessary JSON conversion and directly produces the target structure.
    pub fn load_tasker_config(&mut self) -> ConfigResult<super::TaskerConfig> {
        let validated_config = self.load_with_resource_validation()?;
        validated_config.to_tasker_config()
    }

    /// Validate resource constraints between database and executor configurations
    fn validate_resource_constraints(
        &self,
        database_config: &toml::Value,
        executor_pools_config: &toml::Value,
    ) -> ConfigResult<()> {
        // Extract database max connections
        let max_connections = if let Some(db_section) = database_config.get("database") {
            if let Some(pool_section) = db_section.get("pool") {
                pool_section
                    .get("max_connections")
                    .and_then(|v| v.as_integer())
                    .unwrap_or(30) // Default fallback
            } else {
                30
            }
        } else {
            30
        };

        // Calculate total executor requirements
        let mut total_max_executors = 0i64;
        if let Some(pools_section) = executor_pools_config.get("executor_pools") {
            if let Some(pools_table) = pools_section.as_table() {
                for (pool_name, pool_config) in pools_table {
                    if pool_name != "coordinator" {
                        // Skip coordinator section
                        if let Some(max_exec) = pool_config.get("max_executors") {
                            if let Some(max_val) = max_exec.as_integer() {
                                total_max_executors += max_val;
                            }
                        }
                    }
                }
            }
        }

        // Validate that executor requirements don't exceed database connections
        let connection_utilization = total_max_executors as f64 / max_connections as f64;

        if total_max_executors > max_connections {
            return Err(ConfigurationError::validation_error(format!(
                "Resource constraint violation: requested {total_max_executors} executors but only {max_connections} database connections available. \
                 Increase database pool size or reduce executor limits."
            )));
        }

        if connection_utilization > 0.85 {
            warn!(
                "High resource utilization: {} executors will use {:.1}% of {} database connections",
                total_max_executors,
                connection_utilization * 100.0,
                max_connections
            );
        }

        info!(
            "✅ Resource validation passed: {} executors, {} database connections ({:.1}% utilization)",
            total_max_executors,
            max_connections,
            connection_utilization * 100.0
        );

        Ok(())
    }
}

/// Validated configuration container
#[derive(Debug)]
pub struct ValidatedConfig {
    /// All loaded and validated component configurations
    pub configs: HashMap<String, toml::Value>,
}

impl ValidatedConfig {
    /// Get a specific component configuration
    pub fn get_component(&self, name: &str) -> Option<&toml::Value> {
        self.configs.get(name)
    }

    /// Get all component names
    pub fn component_names(&self) -> Vec<&String> {
        self.configs.keys().collect()
    }

    /// Convert directly to TaskerConfig without JSON proxy
    ///
    /// This is the preferred method for converting TOML configuration to TaskerConfig.
    /// It directly deserializes TOML components into the TaskerConfig structure.
    pub fn to_tasker_config(&self) -> ConfigResult<super::TaskerConfig> {
        use super::TaskerConfig;

        // Create a combined TOML document with all components
        let mut combined_config = toml::Table::new();

        // Flatten all component configurations into a single TOML table
        for (component_name, component_toml) in &self.configs {
            if let toml::Value::Table(component_table) = component_toml {
                // Insert component sections directly into combined config
                for (key, value) in component_table {
                    combined_config.insert(key.clone(), value.clone());
                }
            } else {
                return Err(ConfigurationError::invalid_toml(
                    format!("component {component_name}"),
                    "Component configuration must be a TOML table",
                ));
            }
        }

        // Deserialize directly from TOML to TaskerConfig
        let tasker_config: TaskerConfig =
            toml::Value::Table(combined_config)
                .try_into()
                .map_err(|e| {
                    ConfigurationError::json_serialization_error(
                        "TOML to TaskerConfig deserialization",
                        e,
                    )
                })?;

        Ok(tasker_config)
    }

    /// Convert to legacy format for backward compatibility
    ///
    /// This allows the new unified loader to work with existing configuration
    /// structures during the migration period.
    pub fn to_legacy_format(&self) -> ConfigResult<serde_json::Value> {
        // Convert TOML values to JSON for easier manipulation
        let mut legacy_config = serde_json::Map::new();

        for (component_name, toml_config) in &self.configs {
            // Convert TOML to JSON
            let json_str = toml::to_string(toml_config).map_err(|e| {
                ConfigurationError::json_serialization_error(
                    format!("component {component_name}"),
                    e,
                )
            })?;

            let component_json: toml::Value = toml::from_str(&json_str).map_err(|e| {
                ConfigurationError::json_serialization_error(
                    format!("component {component_name}"),
                    e,
                )
            })?;

            // Convert to serde_json::Value
            let json_value = toml_to_json(component_json)?;

            // Flatten component sections into top-level
            if let serde_json::Value::Object(component_map) = json_value {
                for (key, value) in component_map {
                    legacy_config.insert(key, value);
                }
            }
        }

        Ok(serde_json::Value::Object(legacy_config))
    }
}

/// Convert TOML value to JSON value
fn toml_to_json(toml_value: toml::Value) -> ConfigResult<serde_json::Value> {
    let toml_str = toml::to_string(&toml_value).map_err(|e| {
        ConfigurationError::json_serialization_error("TOML to string conversion", e)
    })?;

    let json_value: serde_json::Value = serde_json::from_str(&toml_str)
        .map_err(|e| ConfigurationError::json_serialization_error("JSON parsing", e))?;

    Ok(json_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        let loader = UnifiedConfigLoader {
            root: PathBuf::from("/test"),
            environment: "test".to_string(),
            component_cache: HashMap::new(),
        };

        // Set test env var
        std::env::set_var("TEST_VAR", "test_value");
        std::env::set_var("TEST_NUMBER", "42");

        // Test simple substitution
        let result = loader.expand_env_vars("${TEST_VAR}").unwrap();
        assert_eq!(result, "test_value");

        // Test substitution in string
        let result = loader.expand_env_vars("prefix_${TEST_VAR}_suffix").unwrap();
        assert_eq!(result, "prefix_test_value_suffix");

        // Test default value when var exists
        let result = loader.expand_env_vars("${TEST_VAR:-default}").unwrap();
        assert_eq!(result, "test_value");

        // Test default value when var doesn't exist
        let result = loader
            .expand_env_vars("${NONEXISTENT:-default_val}")
            .unwrap();
        assert_eq!(result, "default_val");

        // Test error when var doesn't exist and no default
        let result = loader.expand_env_vars("${NONEXISTENT}");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Environment variable 'NONEXISTENT' not found"));

        // Clean up
        std::env::remove_var("TEST_VAR");
        std::env::remove_var("TEST_NUMBER");
    }

    #[test]
    fn test_merge_toml_logic() {
        let loader = UnifiedConfigLoader {
            root: PathBuf::from("/test"),
            environment: "test".to_string(),
            component_cache: HashMap::new(),
        };

        // Create base config
        let mut base = toml::Value::Table(toml::Table::new());
        if let toml::Value::Table(ref mut base_table) = base {
            let mut database = toml::Table::new();
            database.insert(
                "host".to_string(),
                toml::Value::String("localhost".to_string()),
            );
            database.insert("port".to_string(), toml::Value::Integer(5432));

            let mut pool = toml::Table::new();
            pool.insert("max_connections".to_string(), toml::Value::Integer(25));
            pool.insert("min_connections".to_string(), toml::Value::Integer(5));
            database.insert("pool".to_string(), toml::Value::Table(pool));

            base_table.insert("database".to_string(), toml::Value::Table(database));
        }

        // Create override config
        let mut override_config = toml::Table::new();
        let mut database = toml::Table::new();
        database.insert(
            "database".to_string(),
            toml::Value::String("test_db".to_string()),
        );

        let mut pool = toml::Table::new();
        pool.insert("max_connections".to_string(), toml::Value::Integer(10));
        database.insert("pool".to_string(), toml::Value::Table(pool));

        override_config.insert("database".to_string(), toml::Value::Table(database));
        let override_value = toml::Value::Table(override_config);

        // Merge
        loader.merge_toml(&mut base, override_value).unwrap();

        // Verify merge results
        if let toml::Value::Table(ref base_table) = base {
            if let Some(toml::Value::Table(ref db_table)) = base_table.get("database") {
                // Original host should remain
                assert_eq!(db_table.get("host").unwrap().as_str(), Some("localhost"));
                assert_eq!(db_table.get("port").unwrap().as_integer(), Some(5432));

                // New database name should be added
                assert_eq!(db_table.get("database").unwrap().as_str(), Some("test_db"));

                // Pool should be merged
                if let Some(toml::Value::Table(ref pool_table)) = db_table.get("pool") {
                    assert_eq!(
                        pool_table.get("max_connections").unwrap().as_integer(),
                        Some(10)
                    ); // overridden
                    assert_eq!(
                        pool_table.get("min_connections").unwrap().as_integer(),
                        Some(5)
                    ); // preserved
                }
            }
        }
    }

    #[test]
    fn test_pool_validation_logic() {
        let loader = UnifiedConfigLoader {
            root: PathBuf::from("/test"),
            environment: "test".to_string(),
            component_cache: HashMap::new(),
        };

        // Test valid pool config
        let mut valid_pool = toml::Table::new();
        valid_pool.insert("max_connections".to_string(), toml::Value::Integer(10));
        valid_pool.insert("min_connections".to_string(), toml::Value::Integer(2));
        let valid_config = toml::Value::Table(valid_pool);

        assert!(loader.validate_pool_config(&valid_config).is_ok());

        // Test invalid pool config (min > max)
        let mut invalid_pool = toml::Table::new();
        invalid_pool.insert("max_connections".to_string(), toml::Value::Integer(5));
        invalid_pool.insert("min_connections".to_string(), toml::Value::Integer(10));
        let invalid_config = toml::Value::Table(invalid_pool);

        let result = loader.validate_pool_config(&invalid_config);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("min_connections") && error_msg.contains("max_connections"));

        // Test invalid pool config (negative values)
        let mut negative_pool = toml::Table::new();
        negative_pool.insert("max_connections".to_string(), toml::Value::Integer(-5));
        let negative_config = toml::Value::Table(negative_pool);

        let result = loader.validate_pool_config(&negative_config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be greater than 0"));
    }
}
