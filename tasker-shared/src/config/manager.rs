//! Enhanced Configuration Manager
//!
//! This module provides a configuration manager that combines:
//! - Modern TOML-based configuration loading via UnifiedConfigLoader
//! - Legacy YAML-based TaskTemplate loading for backward compatibility
//! - Comprehensive TaskTemplate validation
//!
//! Follows fail-fast principle: any configuration error results in immediate failure.

use crate::config::unified_loader::UnifiedConfigLoader;
use crate::config::{error::ConfigResult, TaskerConfig};
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

    // Private helper methods

    fn load_from_environment(environment: Option<&str>) -> ConfigResult<Arc<ConfigManager>> {
        let unified_env = UnifiedConfigLoader::detect_environment();
        let env = environment.unwrap_or(unified_env.as_str());
        Self::load_from_env(env)
    }

    // Legacy environment override methods removed
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
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

        config.orchestration.mode = "distributed".to_string();

        config.queues.pgmq.poll_interval_ms = 1000;
        config.queues.default_visibility_timeout_seconds = 30;
        config.queues.default_batch_size = 10;
        config.queues.pgmq.max_retries = 3;

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
}
