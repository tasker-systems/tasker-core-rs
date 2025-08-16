//! Simplified Configuration Manager
//!
//! This module provides a thin wrapper around UnifiedConfigLoader for backward compatibility.
//! All actual configuration loading is delegated to UnifiedConfigLoader with no fallbacks.
//!
//! Follows fail-fast principle: any configuration error results in immediate failure.

use crate::config::unified_loader::UnifiedConfigLoader;
use crate::config::{error::ConfigResult, TaskerConfig};
use std::sync::{Arc, RwLock};
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

    // Private helper methods

    fn load_from_environment(environment: Option<&str>) -> ConfigResult<Arc<ConfigManager>> {
        let unified_env = UnifiedConfigLoader::detect_environment();
        let env = environment.unwrap_or(unified_env.as_str());
        Self::load_from_env(env)
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
        config.database.pool = 10;

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

    #[test]
    fn test_global_singleton_reset() {
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
