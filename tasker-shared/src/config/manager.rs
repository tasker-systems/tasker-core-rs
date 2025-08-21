//! Enhanced Configuration Manager
//!
//! This module provides a configuration manager that combines:
//! - Modern TOML-based configuration loading via UnifiedConfigLoader
//! - Legacy YAML-based TaskTemplate loading for backward compatibility
//! - Comprehensive TaskTemplate validation
//!
//! Follows fail-fast principle: any configuration error results in immediate failure.

use crate::config::unified_loader::UnifiedConfigLoader;
use crate::config::{
    error::{ConfigResult, ConfigurationError},
    EnvironmentConfig, TaskTemplate, TaskerConfig,
};
use regex::Regex;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

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

    /// Load configuration with automatic environment detection
    ///
    /// Note: dotenv() should be called earlier in the chain (e.g., in embedded_bridge.rs)
    /// to ensure environment variables are loaded before configuration loading.
    pub fn load() -> ConfigResult<Arc<ConfigManager>> {
        Self::load_from_environment(None)
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
            "✅ Configuration loaded successfully for environment: {}",
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

    /// Load and parse a TaskTemplate from YAML string
    ///
    /// This method provides the ability to load TaskTemplates from YAML content with environment variable interpolation.
    pub fn load_task_template_from_yaml(&self, yaml_content: &str) -> ConfigResult<TaskTemplate> {
        // Interpolate environment variables in the YAML content
        let interpolated_content = Self::interpolate_env_vars(yaml_content);

        // Parse the YAML into a TaskTemplate
        let mut template: TaskTemplate =
            serde_yaml::from_str(&interpolated_content).map_err(|e| {
                ConfigurationError::ParseError {
                    file_path: "yaml_string".to_string(),
                    reason: format!("Failed to parse task template YAML: {}", e),
                }
            })?;

        // Auto-populate named_steps from step_templates if it's empty
        if template.named_steps.is_empty() {
            template.named_steps = template
                .step_templates
                .iter()
                .map(|st| st.name.clone())
                .collect();
        }

        // Apply environment-specific overrides if present
        if let Some(environments) = &template.environments {
            if let Some(env_config) = environments.get(&self.environment) {
                let env_config_clone = env_config.clone();
                self.apply_environment_overrides(&mut template, &env_config_clone);
            }
        }

        debug!("Task template loaded successfully: {}", template.name);
        Ok(template)
    }

    /// Validate a TaskTemplate structure and dependencies
    ///
    /// This method provides backward compatibility with the legacy ConfigurationManager
    /// for comprehensive TaskTemplate validation including dependency checks.
    pub fn validate_task_template(&self, template: &TaskTemplate) -> ConfigResult<()> {
        // Check required fields
        if template.name.is_empty() {
            return Err(ConfigurationError::ValidationError {
                error: "Task template name cannot be empty".to_string(),
            });
        }

        if template.task_handler_class.is_empty() {
            return Err(ConfigurationError::ValidationError {
                error: "Task handler class cannot be empty".to_string(),
            });
        }

        if template.namespace_name.is_empty() {
            return Err(ConfigurationError::ValidationError {
                error: "Namespace name cannot be empty".to_string(),
            });
        }

        // Validate named steps exist in step templates
        for named_step in &template.named_steps {
            if !template
                .step_templates
                .iter()
                .any(|st| st.name == *named_step)
            {
                return Err(ConfigurationError::ValidationError {
                    error: format!("Named step '{named_step}' not found in step templates"),
                });
            }
        }

        // Validate step dependencies
        for step_template in &template.step_templates {
            if let Some(depends_on) = &step_template.depends_on_step {
                if !template
                    .step_templates
                    .iter()
                    .any(|st| st.name == *depends_on)
                {
                    return Err(ConfigurationError::ValidationError {
                        error: format!("Step dependency '{depends_on}' not found"),
                    });
                }
            }

            if let Some(depends_on_steps) = &step_template.depends_on_steps {
                for dep in depends_on_steps {
                    if !template.step_templates.iter().any(|st| st.name == *dep) {
                        return Err(ConfigurationError::ValidationError {
                            error: format!("Step dependency '{dep}' not found"),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    // Private helper methods

    fn load_from_environment(environment: Option<&str>) -> ConfigResult<Arc<ConfigManager>> {
        let unified_env = UnifiedConfigLoader::detect_environment();
        let env = environment.unwrap_or(unified_env.as_str());
        Self::load_from_env(env)
    }

    /// Interpolate environment variables in configuration strings
    ///
    /// Replaces ${VAR_NAME} patterns with actual environment variable values
    fn interpolate_env_vars(template: &str) -> String {
        let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
        re.replace_all(template, |caps: &regex::Captures| {
            let var_name = &caps[1];
            std::env::var(var_name).unwrap_or_else(|_| format!("${{{}}}", var_name))
        })
        .to_string()
    }

    /// Apply environment-specific overrides to a task template
    ///
    /// Updates step template configurations based on environment-specific settings
    fn apply_environment_overrides(
        &self,
        template: &mut TaskTemplate,
        env_config: &EnvironmentConfig,
    ) {
        for override_config in &env_config.step_templates {
            if let Some(step_template) = template
                .step_templates
                .iter_mut()
                .find(|s| s.name == override_config.name)
            {
                if let Some(handler_config) = &override_config.handler_config {
                    step_template.handler_config = Some(handler_config.clone());
                }
            }
        }
    }
}

// Global configuration singleton for backward compatibility
static GLOBAL_CONFIG: RwLock<Option<Arc<ConfigManager>>> = RwLock::new(None);

impl ConfigManager {
    /// Get the global configuration instance
    pub fn global() -> ConfigResult<Arc<ConfigManager>> {
        {
            let guard = GLOBAL_CONFIG.read().unwrap();
            if let Some(ref config) = *guard {
                return Ok(Arc::clone(config));
            }
        }

        // Load and store configuration if not already loaded
        let config = Self::load()?;
        let mut guard = GLOBAL_CONFIG.write().unwrap();
        *guard = Some(Arc::clone(&config));
        Ok(config)
    }

    /// Initialize global configuration with specific environment
    pub fn initialize_global(environment: Option<&str>) -> ConfigResult<Arc<ConfigManager>> {
        let config = Self::load_from_environment(environment)?;
        let mut guard = GLOBAL_CONFIG.write().unwrap();
        *guard = Some(Arc::clone(&config));
        Ok(config)
    }

    /// Reset global configuration (for testing only)
    #[cfg(test)]
    pub fn reset_global_for_testing() {
        let mut guard = GLOBAL_CONFIG.write().unwrap();
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TaskerConfig;

    /// Create a mock TaskerConfig for testing without file I/O
    fn create_mock_tasker_config() -> TaskerConfig {
        // Use the default implementation which has all required fields
        let mut config = TaskerConfig::default();

        // Override just the fields we want to test
        config.database.host = "localhost".to_string();
        config.database.username = "test_user".to_string();
        config.database.password = "test_password".to_string();
        config.database.database = Some("test_db".to_string());

        config.orchestration.mode = "test".to_string();
        config.orchestration.active_namespaces = vec!["test".to_string()];
        config.orchestration.max_concurrent_orchestrators = 5;

        config.pgmq.poll_interval_ms = 1000;
        config.pgmq.visibility_timeout_seconds = 30;
        config.pgmq.batch_size = 10;
        config.pgmq.max_retries = 3;

        config
    }

    #[test]
    fn test_config_manager_from_tasker_config() {
        let config = create_mock_tasker_config();
        let manager = ConfigManager::from_tasker_config(config, "test".to_string());

        assert_eq!(manager.environment(), "test");
        assert_eq!(manager.config().database.host, "localhost");
        assert_eq!(manager.config().orchestration.mode, "test");
    }

    #[test]
    fn test_environment_detection() {
        // Test that environment detection works without file I/O
        let detected = crate::config::unified_loader::UnifiedConfigLoader::detect_environment();

        // Should return development by default if no env vars set
        assert!(detected == "development" || std::env::var("TASKER_ENV").is_ok());
    }

    #[tokio::test]
    async fn test_global_singleton_reset() {
        // Test the reset functionality for global singleton
        ConfigManager::reset_global_for_testing();

        // After reset, global config should be None
        // We can't easily test the actual loading without file I/O,
        // but we can test that reset works
        let guard = GLOBAL_CONFIG.read().unwrap();
        assert!(guard.is_none());
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
}
