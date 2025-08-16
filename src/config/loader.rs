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
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_config(dir: &TempDir) -> PathBuf {
        let config_dir = dir.path().join("config").join("tasker");
        let base_dir = config_dir.join("base");
        fs::create_dir_all(&base_dir).unwrap();

        // Create minimal valid TOML configs
        let database_config = r#"
[pool]
max_connections = 10
min_connections = 2

[[read_replicas]]
host = "localhost"
port = 5432
"#;
        fs::write(base_dir.join("database.toml"), database_config).unwrap();

        let orchestration_config = r#"
[coordinator]
auto_scaling_enabled = false
target_utilization = 0.75
scaling_interval_seconds = 30
"#;
        fs::write(base_dir.join("orchestration.toml"), orchestration_config).unwrap();

        dir.path().join("config")
    }

    #[test]
    fn test_load_configuration() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = setup_test_config(&temp_dir);

        std::env::set_var("TASKER_ENV", "test");
        let manager = ConfigManager::load_from_directory(config_dir).unwrap();

        assert_eq!(manager.environment(), "test");
    }

    #[test]
    fn test_fail_fast_on_missing_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("nonexistent");

        let result = ConfigManager::load_from_directory(config_dir);
        assert!(result.is_err());
        // Should fail immediately, not create defaults
    }

    #[test]
    fn test_global_singleton() {
        ConfigManager::reset_global_for_testing();

        let temp_dir = TempDir::new().unwrap();
        let config_dir = setup_test_config(&temp_dir);
        std::env::set_var("WORKSPACE_PATH", temp_dir.path().to_str().unwrap());
        std::env::set_var("TASKER_ENV", "test");

        // First call loads configuration
        let config1 = ConfigManager::global();
        assert!(config1.is_ok());

        // Second call returns same instance
        let config2 = ConfigManager::global();
        assert!(config2.is_ok());

        // Verify they're the same instance
        assert!(Arc::ptr_eq(&config1.unwrap(), &config2.unwrap()));
    }

    #[test]
    fn test_path_resolution() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = setup_test_config(&temp_dir);

        std::env::set_var("TASKER_ENV", "test");
        let manager = ConfigManager::load_from_directory(config_dir.clone()).unwrap();
    }
}
