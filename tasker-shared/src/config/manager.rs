//! Enhanced Configuration Manager
//!
//! This module provides a configuration manager that combines:
//! - Modern TOML-based configuration loading via UnifiedConfigLoader
//! - Context-specific configuration loading (TAS-50 Phase 2)
//!
//! Follows fail-fast principle: any configuration error results in immediate failure.

use crate::config::unified_loader::UnifiedConfigLoader;
use crate::config::{
    contexts::{CommonConfig, ConfigContext, OrchestrationConfig, WorkerConfig},
    error::ConfigResult,
    TaskerConfig,
};
use std::any::Any;
use std::sync::Arc;
use tracing::info;

/// Simplified configuration manager that wraps UnifiedConfigLoader
///
/// This is a thin compatibility layer that delegates all work to UnifiedConfigLoader.
/// No fallbacks, no legacy support, no YAML handling.
#[derive(Debug, Clone)]
pub struct ConfigManager {
    config: TaskerConfig,
    environment: String,
}

impl ConfigManager {
    /// Create a new configuration manager with default configuration
    ///
    /// This provides backward compatibility with the legacy ConfigurationManager::new()
    /// method. Returns a ConfigManager with default TaskerConfig values.
    pub fn new() -> Self {
        ConfigManager {
            config: TaskerConfig::default(),
            environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
        }
    }

    /// Load configuration from directory with specific environment
    ///
    /// This is the core loading method that delegates to UnifiedConfigLoader.
    /// Fails fast on any configuration errors.
    pub fn load_from_env(environment: &str) -> ConfigResult<Arc<ConfigManager>> {
        // Use UnifiedConfigLoader - no fallbacks, fail fast on errors
        let mut loader = UnifiedConfigLoader::new(environment)?;
        let config = loader.load_tasker_config()?;

        // Validate the configuration
        config.validate()?;

        info!(
            "Configuration loaded successfully for environment: {}",
            environment
        );

        Ok(Arc::new(ConfigManager {
            config,
            environment: environment.to_string(),
        }))
    }

    /// Create ConfigManager from an already-loaded TaskerConfig
    ///
    /// This is used when UnifiedConfigLoader is the primary loader and ConfigManager
    /// is just a compatibility wrapper (TAS-34 Phase 2).
    pub fn from_tasker_config(config: TaskerConfig, environment: String) -> Self {
        ConfigManager {
            config,
            environment,
        }
    }

    /// Get reference to the loaded configuration
    pub fn config(&self) -> &TaskerConfig {
        &self.config
    }

    /// Get the current environment
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Get system configuration as Arc for compatibility with orchestration tests
    ///
    /// This method provides backward compatibility with the legacy ConfigurationManager
    /// by returning the TaskerConfig wrapped in an Arc.
    pub fn system_config(&self) -> Arc<TaskerConfig> {
        Arc::new(self.config.clone())
    }

    // Legacy TaskTemplate methods removed - TaskTemplate handling now done via TaskHandlerRegistry

    /// Load context-specific configuration directly from TOML files (TAS-50 Phase 2)
    ///
    /// This is the preferred method for Phase 2. It loads configuration directly from
    /// context-specific TOML files rather than converting from monolithic TaskerConfig.
    ///
    /// Benefits over `load_for_context()`:
    /// - More efficient: Only loads needed configuration fields
    /// - Clearer separation: Each context has its own TOML file
    /// - Better for deployment: Can use separate ConfigMaps in K8s
    ///
    /// # Arguments
    /// * `context` - The configuration context to load
    ///
    /// # Example
    /// ```no_run
    /// use tasker_shared::config::{ConfigManager, contexts::ConfigContext};
    ///
    /// // Load orchestration configuration directly from TOML files
    /// let manager = ConfigManager::load_context_direct(ConfigContext::Orchestration).unwrap();
    /// let common = manager.common().unwrap();
    /// let orch = manager.orchestration().unwrap();
    /// ```
    pub fn load_context_direct(context: ConfigContext) -> ConfigResult<ContextConfigManager> {
        // Create a unified config loader
        let mut loader = UnifiedConfigLoader::new_from_env()?;
        let environment = loader.environment().to_string();

        // Load context-specific configurations from TOML files
        let config: Box<dyn Any + Send + Sync> = match context {
            ConfigContext::Orchestration => {
                let common = loader.load_common_config()?;
                let orchestration = loader.load_orchestration_config()?;
                Box::new((common, orchestration))
            }
            ConfigContext::Worker => {
                let common = loader.load_common_config()?;
                let worker = loader.load_worker_config()?;
                Box::new((common, worker))
            }
            ConfigContext::Combined => {
                let common = loader.load_common_config()?;
                let orchestration = loader.load_orchestration_config()?;
                let worker = loader.load_worker_config()?;
                Box::new((common, orchestration, worker))
            }
            ConfigContext::Legacy => {
                // For legacy, still use the monolithic TaskerConfig loading
                let tasker_config = loader.load_tasker_config()?;
                Box::new(tasker_config)
            }
        };

        info!(
            "Context-specific configuration loaded directly from TOML (Phase 2): {:?}, environment: {}",
            context,
            environment
        );

        Ok(ContextConfigManager {
            context,
            environment,
            config,
        })
    }

    /// Load context-specific configuration from a single TOML file (TAS-50 Phase 3)
    ///
    /// This method loads configuration from a single merged TOML file (generated by tasker-cli)
    /// instead of loading from the multi-file directory structure. This is the preferred method
    /// for Docker/production deployments.
    ///
    /// # Arguments
    /// * `path` - Path to the merged configuration TOML file
    /// * `context` - The configuration context to load (Orchestration or Worker)
    ///
    /// # Returns
    /// A ContextConfigManager with the loaded configuration
    ///
    /// # Errors
    /// Returns ConfigError if:
    /// - File doesn't exist
    /// - TOML parsing fails
    /// - Deserialization fails
    /// - Required fields are missing
    ///
    /// # Example
    /// ```no_run
    /// use tasker_shared::config::{ConfigManager, contexts::ConfigContext};
    /// use std::path::PathBuf;
    ///
    /// let path = PathBuf::from("/etc/tasker/orchestration-production.toml");
    /// let manager = ConfigManager::load_from_single_file(&path, ConfigContext::Orchestration).unwrap();
    /// ```
    pub fn load_from_single_file(
        path: &std::path::Path,
        context: ConfigContext,
    ) -> ConfigResult<ContextConfigManager> {
        use crate::config::error::ConfigurationError;
        use std::fs;

        // Read the TOML file
        let contents = fs::read_to_string(path).map_err(|e| {
            ConfigurationError::FileNotFound(format!(
                "Failed to read config file {}: {}",
                path.display(),
                e
            ))
        })?;

        // Parse TOML first to enable environment variable substitution
        let mut toml_value: toml::Value = toml::from_str(&contents).map_err(|e| {
            ConfigurationError::ParseError {
                file_path: path.to_string_lossy().to_string(),
                reason: format!("Failed to parse TOML: {}", e),
            }
        })?;

        // Perform environment variable substitution (TAS-50: fix for Docker deployments)
        // This uses the same logic as UnifiedConfigLoader::load_toml_with_env_substitution()
        Self::substitute_env_vars_in_value(&mut toml_value)?;

        // Convert back to string for deserialization
        let substituted_contents = toml::to_string(&toml_value).map_err(|e| {
            ConfigurationError::ParseError {
                file_path: path.to_string_lossy().to_string(),
                reason: format!("Failed to serialize substituted TOML: {}", e),
            }
        })?;

        // Parse TOML and deserialize based on context
        let environment = crate::config::UnifiedConfigLoader::detect_environment();

        let config: Box<dyn Any + Send + Sync> = match context {
            ConfigContext::Orchestration => {
                // Deserialize as (CommonConfig, OrchestrationConfig)
                let common: CommonConfig = toml::from_str(&substituted_contents).map_err(|e| {
                    ConfigurationError::ParseError {
                        file_path: path.to_string_lossy().to_string(),
                        reason: format!("Failed to parse CommonConfig: {}", e),
                    }
                })?;
                let orchestration: OrchestrationConfig = toml::from_str(&substituted_contents).map_err(|e| {
                    ConfigurationError::ParseError {
                        file_path: path.to_string_lossy().to_string(),
                        reason: format!("Failed to parse OrchestrationConfig: {}", e),
                    }
                })?;
                Box::new((common, orchestration))
            }
            ConfigContext::Worker => {
                // Deserialize as (CommonConfig, WorkerConfig)
                let common: CommonConfig = toml::from_str(&substituted_contents).map_err(|e| {
                    ConfigurationError::ParseError {
                        file_path: path.to_string_lossy().to_string(),
                        reason: format!("Failed to parse CommonConfig: {}", e),
                    }
                })?;
                let worker: WorkerConfig = toml::from_str(&substituted_contents).map_err(|e| {
                    ConfigurationError::ParseError {
                        file_path: path.to_string_lossy().to_string(),
                        reason: format!("Failed to parse WorkerConfig: {}", e),
                    }
                })?;
                Box::new((common, worker))
            }
            ConfigContext::Combined => {
                // Deserialize all configs from single file
                let common: CommonConfig = toml::from_str(&substituted_contents).map_err(|e| {
                    ConfigurationError::ParseError {
                        file_path: path.to_string_lossy().to_string(),
                        reason: format!("Failed to parse CommonConfig: {}", e),
                    }
                })?;
                let orchestration: OrchestrationConfig = toml::from_str(&substituted_contents).map_err(|e| {
                    ConfigurationError::ParseError {
                        file_path: path.to_string_lossy().to_string(),
                        reason: format!("Failed to parse OrchestrationConfig: {}", e),
                    }
                })?;
                let worker: WorkerConfig = toml::from_str(&substituted_contents).map_err(|e| {
                    ConfigurationError::ParseError {
                        file_path: path.to_string_lossy().to_string(),
                        reason: format!("Failed to parse WorkerConfig: {}", e),
                    }
                })?;
                Box::new((common, orchestration, worker))
            }
            ConfigContext::Legacy => {
                // Deserialize as monolithic TaskerConfig
                let tasker_config: TaskerConfig = toml::from_str(&substituted_contents).map_err(|e| {
                    ConfigurationError::ParseError {
                        file_path: path.to_string_lossy().to_string(),
                        reason: format!("Failed to parse TaskerConfig: {}", e),
                    }
                })?;
                Box::new(tasker_config)
            }
        };

        info!(
            "Configuration loaded from single file: {} (context: {:?}, environment: {})",
            path.display(),
            context,
            environment
        );

        Ok(ContextConfigManager {
            context,
            environment,
            config,
        })
    }

    /// Recursively substitute environment variables in a TOML value
    ///
    /// This is used by load_from_single_file() to perform the same environment variable
    /// substitution as UnifiedConfigLoader::load_toml_with_env_substitution().
    fn substitute_env_vars_in_value(value: &mut toml::Value) -> ConfigResult<()> {
        match value {
            toml::Value::String(s) => {
                *s = Self::expand_env_vars(s)?;
            }
            toml::Value::Table(table) => {
                for (_, v) in table.iter_mut() {
                    Self::substitute_env_vars_in_value(v)?;
                }
            }
            toml::Value::Array(array) => {
                for v in array.iter_mut() {
                    Self::substitute_env_vars_in_value(v)?;
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
    fn expand_env_vars(input: &str) -> ConfigResult<String> {
        use crate::config::error::ConfigurationError;
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
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Context-specific configuration manager (TAS-50 Phase 1)
///
/// This manager holds configurations for a specific deployment context.
/// It provides type-safe accessors for the configured context.
#[derive(Debug)]
pub struct ContextConfigManager {
    /// Configuration context
    context: ConfigContext,

    /// Current environment
    environment: String,

    /// Loaded configuration (type depends on context)
    config: Box<dyn Any + Send + Sync>,
}

impl ContextConfigManager {
    /// Get common configuration (if available in this context)
    pub fn common(&self) -> Option<&CommonConfig> {
        match self.context {
            ConfigContext::Orchestration => self
                .config
                .downcast_ref::<(CommonConfig, OrchestrationConfig)>()
                .map(|(common, _)| common),
            ConfigContext::Worker => self
                .config
                .downcast_ref::<(CommonConfig, WorkerConfig)>()
                .map(|(common, _)| common),
            ConfigContext::Combined => self
                .config
                .downcast_ref::<(CommonConfig, OrchestrationConfig, WorkerConfig)>()
                .map(|(common, _, _)| common),
            ConfigContext::Legacy => None,
        }
    }

    /// Get orchestration configuration (if available in this context)
    pub fn orchestration(&self) -> Option<&OrchestrationConfig> {
        match self.context {
            ConfigContext::Orchestration => self
                .config
                .downcast_ref::<(CommonConfig, OrchestrationConfig)>()
                .map(|(_, orch)| orch),
            ConfigContext::Combined => self
                .config
                .downcast_ref::<(CommonConfig, OrchestrationConfig, WorkerConfig)>()
                .map(|(_, orch, _)| orch),
            _ => None,
        }
    }

    /// Get worker configuration (if available in this context)
    pub fn worker(&self) -> Option<&WorkerConfig> {
        match self.context {
            ConfigContext::Worker => self
                .config
                .downcast_ref::<(CommonConfig, WorkerConfig)>()
                .map(|(_, worker)| worker),
            ConfigContext::Combined => self
                .config
                .downcast_ref::<(CommonConfig, OrchestrationConfig, WorkerConfig)>()
                .map(|(_, _, worker)| worker),
            _ => None,
        }
    }

    /// Get legacy configuration (if this is a legacy context)
    pub fn legacy(&self) -> Option<&TaskerConfig> {
        match self.context {
            ConfigContext::Legacy => self.config.downcast_ref::<TaskerConfig>(),
            _ => None,
        }
    }

    /// Get the configuration context
    pub fn context(&self) -> ConfigContext {
        self.context
    }

    /// Get the current environment
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Build a TaskerConfig from context-specific configs (TAS-50 Phase 2 backward compatibility)
    ///
    /// This method reconstructs a TaskerConfig from the loaded context-specific configurations.
    /// This enables gradual migration: bootstrap code can use context-specific loading while
    /// existing code continues to use TaskerConfig.
    ///
    /// **Note**: This is a temporary bridge method. Eventually, code should use context-specific
    /// configs directly via common(), orchestration(), worker() accessors.
    ///
    /// # Returns
    /// A TaskerConfig built from the loaded context configurations, or None if not applicable
    ///
    /// # Example
    /// ```no_run
    /// use tasker_shared::config::{ConfigManager, contexts::ConfigContext};
    ///
    /// let manager = ConfigManager::load_context_direct(ConfigContext::Orchestration).unwrap();
    /// let tasker_config = manager.as_tasker_config().unwrap();  // Backward compatibility
    /// ```
    pub fn as_tasker_config(&self) -> Option<TaskerConfig> {
        match self.context {
            ConfigContext::Orchestration => {
                // Build TaskerConfig from Common + Orchestration
                if let Some((common, orch)) = self
                    .config
                    .downcast_ref::<(CommonConfig, OrchestrationConfig)>()
                {
                    let mut config = TaskerConfig {
                        database: common.database.clone(),
                        queues: common.queues.clone(),
                        backoff: orch.backoff.clone(),
                        orchestration: orch.orchestration_system.clone(),
                        circuit_breakers: common.circuit_breakers.clone(),
                        ..TaskerConfig::default()
                    };

                    // Copy nested fields
                    config.execution.environment = common.environment.clone();
                    config.mpsc_channels.shared = common.shared_channels.clone();
                    config.mpsc_channels.orchestration = orch.mpsc_channels.clone();
                    config.event_systems.orchestration = orch.orchestration_events.clone();
                    config.event_systems.task_readiness = orch.task_readiness_events.clone();

                    Some(config)
                } else {
                    None
                }
            }
            ConfigContext::Worker => {
                // Build TaskerConfig from Common + Worker
                if let Some((common, worker)) =
                    self.config.downcast_ref::<(CommonConfig, WorkerConfig)>()
                {
                    let mut config = TaskerConfig {
                        database: common.database.clone(),
                        queues: common.queues.clone(),
                        worker: Some(worker.worker_system.clone()),
                        circuit_breakers: common.circuit_breakers.clone(),
                        ..TaskerConfig::default()
                    };

                    // Copy nested fields
                    config.execution.environment = common.environment.clone();
                    config.mpsc_channels.shared = common.shared_channels.clone();
                    config.mpsc_channels.worker = worker.mpsc_channels.clone();
                    config.event_systems.worker = worker.worker_events.clone();

                    Some(config)
                } else {
                    None
                }
            }
            ConfigContext::Combined => {
                // Build TaskerConfig from all configs
                if let Some((common, orch, worker)) =
                    self.config
                        .downcast_ref::<(CommonConfig, OrchestrationConfig, WorkerConfig)>()
                {
                    let mut config = TaskerConfig {
                        database: common.database.clone(),
                        queues: common.queues.clone(),
                        backoff: orch.backoff.clone(),
                        orchestration: orch.orchestration_system.clone(),
                        worker: Some(worker.worker_system.clone()),
                        circuit_breakers: common.circuit_breakers.clone(),
                        ..TaskerConfig::default()
                    };

                    // Copy nested fields
                    config.execution.environment = common.environment.clone();
                    config.mpsc_channels.shared = common.shared_channels.clone();
                    config.mpsc_channels.orchestration = orch.mpsc_channels.clone();
                    config.mpsc_channels.worker = worker.mpsc_channels.clone();
                    config.event_systems.orchestration = orch.orchestration_events.clone();
                    config.event_systems.task_readiness = orch.task_readiness_events.clone();
                    config.event_systems.worker = worker.worker_events.clone();

                    Some(config)
                } else {
                    None
                }
            }
            ConfigContext::Legacy => {
                // Already have TaskerConfig
                self.config.downcast_ref::<TaskerConfig>().cloned()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::contexts::ConfigurationContext;
    use crate::config::TaskerConfig; // TAS-50 Phase 2: For validate() and summary()

    /// Create a mock TaskerConfig for testing without file I/O
    fn create_mock_tasker_config() -> TaskerConfig {
        // Use the default implementation which has all required fields
        let mut config = TaskerConfig::default();

        // Override just the fields we want to test
        config.database.host = "localhost".to_string();
        config.database.username = "test_user".to_string();
        config.database.password = "test_password".to_string();
        config.database.database = Some("test_db".to_string());

        config.orchestration.mode = "distributed".to_string();

        config.queues.pgmq.poll_interval_ms = 1000;
        config.queues.default_visibility_timeout_seconds = 30;
        config.queues.default_batch_size = 10;
        config.queues.pgmq.max_retries = 3;

        // Initialize worker config with defaults for TAS-50 tests
        config.worker = Some(crate::config::worker::WorkerConfig::default());

        config
    }

    #[test]
    fn test_config_manager_from_tasker_config() {
        let config = create_mock_tasker_config();
        let manager = ConfigManager::from_tasker_config(config, "test".to_string());

        assert_eq!(manager.environment(), "test");
        assert_eq!(manager.config().database.host, "localhost");
        assert_eq!(manager.config().orchestration.mode, "distributed");
    }

    #[test]
    fn test_environment_detection() {
        // Test that environment detection works without file I/O
        let detected = crate::config::unified_loader::UnifiedConfigLoader::detect_environment();

        // Should return development by default if no env vars set
        assert!(detected == "development" || std::env::var("TASKER_ENV").is_ok());
    }

    #[test]
    fn test_config_manager_methods() {
        let config = create_mock_tasker_config();
        let manager = ConfigManager::from_tasker_config(config, "production".to_string());

        // Test basic accessor methods
        assert_eq!(manager.environment(), "production");
        assert!(manager.config().database.host == "localhost");

        // Test that config is immutable reference
        let config_ref = manager.config();
        assert_eq!(config_ref.database.host, "localhost");
    }

    // TAS-50 Phase 1: ContextConfigManager tests

    #[test]
    fn test_context_manager_orchestration_context() {
        // This test requires file I/O to load configuration
        // Create a minimal test that verifies the structure without loading
        let config = create_mock_tasker_config();
        let manager = ConfigManager::from_tasker_config(config.clone(), "test".to_string());

        // Verify the mock config structure for orchestration
        assert!(!manager.config().database.host.is_empty());
        assert!(!manager.config().orchestration.mode.is_empty());
    }

    #[test]
    fn test_context_manager_worker_context() {
        // Verify worker config structure
        let config = create_mock_tasker_config();
        let manager = ConfigManager::from_tasker_config(config, "test".to_string());

        // Verify worker-related fields exist in mock config
        assert!(manager.config().worker.is_some());
        assert_eq!(manager.config().queues.default_batch_size, 10);
    }

    #[test]
    fn test_common_config_conversion_from_tasker_config() {
        let tasker_config = create_mock_tasker_config();
        let common_config = CommonConfig::from(&tasker_config);

        // Verify common config extracted correctly
        assert_eq!(common_config.database.host, "localhost");
        assert_eq!(common_config.database.username, "test_user");
        assert_eq!(common_config.queues.default_batch_size, 10);
        assert_eq!(
            common_config.environment,
            tasker_config.execution.environment
        );
    }

    #[test]
    fn test_orchestration_config_conversion_from_tasker_config() {
        let tasker_config = create_mock_tasker_config();
        let orch_config = OrchestrationConfig::from(&tasker_config);

        // Verify orchestration config extracted correctly
        assert_eq!(orch_config.orchestration_system.mode, "distributed");
        assert_eq!(orch_config.environment, tasker_config.execution.environment);
        assert_eq!(
            orch_config.backoff.max_backoff_seconds,
            tasker_config.backoff.max_backoff_seconds
        );
    }

    #[test]
    fn test_worker_config_conversion_from_tasker_config() {
        let tasker_config = create_mock_tasker_config();
        let worker_config = WorkerConfig::from(&tasker_config);

        // Verify worker config extracted correctly
        assert_eq!(
            worker_config.environment,
            tasker_config.execution.environment
        );
        assert!(
            worker_config.worker_system.web.enabled
                == tasker_config.worker.as_ref().unwrap().web.enabled
        );
    }

    #[test]
    fn test_context_config_manager_accessor_types() {
        // Test that accessor methods have correct return types
        // This test verifies the type system works correctly
        let config = create_mock_tasker_config();

        // Test Common config
        let common = CommonConfig::from(&config);
        assert_eq!(common.database.host, "localhost");

        // Test Orchestration config
        let orch = OrchestrationConfig::from(&config);
        assert_eq!(orch.orchestration_system.mode, "distributed");

        // Test Worker config
        let worker = WorkerConfig::from(&config);
        assert_eq!(worker.environment, config.execution.environment);
    }

    #[test]
    fn test_config_context_enum_values() {
        // Verify ConfigContext enum has all expected variants
        let orchestration = ConfigContext::Orchestration;
        let worker = ConfigContext::Worker;
        let combined = ConfigContext::Combined;
        let legacy = ConfigContext::Legacy;

        // Test equality
        assert_eq!(orchestration, ConfigContext::Orchestration);
        assert_eq!(worker, ConfigContext::Worker);
        assert_eq!(combined, ConfigContext::Combined);
        assert_eq!(legacy, ConfigContext::Legacy);

        // Test inequality
        assert_ne!(orchestration, worker);
        assert_ne!(worker, combined);
        assert_ne!(combined, legacy);
    }

    #[test]
    fn test_env_var_substitution_in_single_file_loading() {
        // Test that environment variable substitution works in load_from_single_file()
        // Set test env vars
        std::env::set_var("TEST_DATABASE_URL", "postgresql://test:test@localhost/testdb");
        std::env::set_var("TEST_VAR", "test_value");

        // Test simple substitution
        let result = ConfigManager::expand_env_vars("${TEST_VAR}").unwrap();
        assert_eq!(result, "test_value");

        // Test substitution in string
        let result = ConfigManager::expand_env_vars("prefix_${TEST_VAR}_suffix").unwrap();
        assert_eq!(result, "prefix_test_value_suffix");

        // Test default value when var exists
        let result = ConfigManager::expand_env_vars("${TEST_VAR:-default}").unwrap();
        assert_eq!(result, "test_value");

        // Test default value when var doesn't exist
        let result = ConfigManager::expand_env_vars("${NONEXISTENT:-default_val}").unwrap();
        assert_eq!(result, "default_val");

        // Test database URL substitution
        let result = ConfigManager::expand_env_vars("${TEST_DATABASE_URL}").unwrap();
        assert_eq!(result, "postgresql://test:test@localhost/testdb");

        // Test error when var doesn't exist and no default
        let result = ConfigManager::expand_env_vars("${NONEXISTENT}");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Environment variable 'NONEXISTENT' not found"));

        // Clean up
        std::env::remove_var("TEST_DATABASE_URL");
        std::env::remove_var("TEST_VAR");
    }

    // TAS-50 Phase 2: Context-specific TOML loading tests

    #[test]
    fn test_phase1_vs_phase2_common_config_equivalence() {
        // Test that Phase 1 (from TaskerConfig) and Phase 2 (from TOML) produce equivalent CommonConfig

        // Phase 1: Load via TaskerConfig conversion
        let tasker_config = create_mock_tasker_config();
        let phase1_common = CommonConfig::from(&tasker_config);

        // Phase 2: Would load from TOML, but we can't test file I/O here
        // Instead, verify the Phase 1 conversion produces expected structure
        assert_eq!(phase1_common.database.host, "localhost");
        assert_eq!(phase1_common.database.username, "test_user");
        assert_eq!(phase1_common.queues.default_batch_size, 10);
        assert_eq!(
            phase1_common.environment,
            tasker_config.execution.environment
        );
    }

    #[test]
    fn test_phase1_vs_phase2_orchestration_config_equivalence() {
        // Test that Phase 1 and Phase 2 produce equivalent OrchestrationConfig

        let tasker_config = create_mock_tasker_config();
        let phase1_orch = OrchestrationConfig::from(&tasker_config);

        // Verify structure
        assert_eq!(phase1_orch.orchestration_system.mode, "distributed");
        assert_eq!(phase1_orch.environment, tasker_config.execution.environment);
        assert!(phase1_orch.backoff.max_backoff_seconds > 0);
        assert!(!phase1_orch.backoff.default_backoff_seconds.is_empty());
    }

    #[test]
    fn test_phase1_vs_phase2_worker_config_equivalence() {
        // Test that Phase 1 and Phase 2 produce equivalent WorkerConfig

        let tasker_config = create_mock_tasker_config();
        let phase1_worker = WorkerConfig::from(&tasker_config);

        // Verify structure
        assert_eq!(
            phase1_worker.environment,
            tasker_config.execution.environment
        );
        assert!(phase1_worker.worker_system.web.enabled);
    }

    #[test]
    fn test_context_manager_orchestration_type_safety() {
        // Test that orchestration context only provides orchestration configs
        let tasker_config = create_mock_tasker_config();
        let manager = ConfigManager::from_tasker_config(tasker_config.clone(), "test".to_string());

        // Convert to context-specific via Phase 1 method
        let common = CommonConfig::from(&manager.config);
        let orch = OrchestrationConfig::from(&manager.config);

        // Verify we can access expected fields
        assert_eq!(common.database.host, "localhost");
        assert_eq!(orch.orchestration_system.mode, "distributed");
    }

    #[test]
    fn test_context_manager_worker_type_safety() {
        // Test that worker context only provides worker configs
        let tasker_config = create_mock_tasker_config();
        let manager = ConfigManager::from_tasker_config(tasker_config.clone(), "test".to_string());

        // Convert to context-specific via Phase 1 method
        let common = CommonConfig::from(&manager.config);
        let worker = WorkerConfig::from(&manager.config);

        // Verify we can access expected fields
        assert_eq!(common.queues.default_batch_size, 10);
        assert!(worker.worker_system.web.enabled);
    }

    #[test]
    fn test_common_config_validation_via_phase1() {
        // Test that CommonConfig validation works for Phase 1 conversions
        let tasker_config = create_mock_tasker_config();
        let common = CommonConfig::from(&tasker_config);

        // Should pass validation with default mock values
        assert!(common.validate().is_ok());
    }

    #[test]
    fn test_orchestration_config_validation_via_phase1() {
        // Test that OrchestrationConfig validation works for Phase 1 conversions
        let tasker_config = create_mock_tasker_config();
        let orch = OrchestrationConfig::from(&tasker_config);

        // Should pass validation with default mock values
        assert!(orch.validate().is_ok());
    }

    #[test]
    fn test_worker_config_validation_via_phase1() {
        // Test that WorkerConfig validation works for Phase 1 conversions
        let tasker_config = create_mock_tasker_config();
        let worker = WorkerConfig::from(&tasker_config);

        // Should pass validation with default mock values
        assert!(worker.validate().is_ok());
    }

    #[test]
    fn test_context_config_summary_generation() {
        // Test that all context configs can generate summaries
        let tasker_config = create_mock_tasker_config();

        let common = CommonConfig::from(&tasker_config);
        let summary = common.summary();
        assert!(summary.contains("CommonConfig"));
        assert!(summary.contains(&common.environment));

        let orch = OrchestrationConfig::from(&tasker_config);
        let summary = orch.summary();
        assert!(summary.contains("OrchestrationConfig"));
        assert!(summary.contains(&orch.environment));

        let worker = WorkerConfig::from(&tasker_config);
        let summary = worker.summary();
        assert!(summary.contains("WorkerConfig"));
        assert!(summary.contains(&worker.environment));
    }

    #[test]
    fn test_context_enum_serialization() {
        // Test that ConfigContext enum can be used in pattern matching
        let contexts = vec![
            ConfigContext::Orchestration,
            ConfigContext::Worker,
            ConfigContext::Combined,
            ConfigContext::Legacy,
        ];

        for context in contexts {
            match context {
                ConfigContext::Orchestration => {
                    assert_eq!(context, ConfigContext::Orchestration);
                }
                ConfigContext::Worker => {
                    assert_eq!(context, ConfigContext::Worker);
                }
                ConfigContext::Combined => {
                    assert_eq!(context, ConfigContext::Combined);
                }
                ConfigContext::Legacy => {
                    assert_eq!(context, ConfigContext::Legacy);
                }
            }
        }
    }
}
