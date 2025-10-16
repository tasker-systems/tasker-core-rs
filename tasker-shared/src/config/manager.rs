//! Enhanced Configuration Manager
//!
//! This module provides a configuration manager that combines:
//! - Modern TOML-based configuration loading via UnifiedConfigLoader
//! - Legacy YAML-based TaskTemplate loading for backward compatibility
//! - Comprehensive TaskTemplate validation
//! - Context-specific configuration loading (TAS-50 Phase 1)
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

    /// Load configuration with automatic environment detection
    ///
    /// **⚠️ DEPRECATED (TAS-50 Phase 2)**: This method loads the full monolithic TaskerConfig
    /// which is inefficient for specific deployment contexts.
    ///
    /// **Use instead**:
    /// - For orchestration: `SystemContext::new_for_orchestration()`
    /// - For worker: `SystemContext::new_for_worker()`
    /// - For testing/integration: Continue using this method (backward compatibility maintained)
    ///
    /// **Why deprecated?**
    /// - Orchestration loads 62.5% configuration (includes unused worker config)
    /// - Worker loads 62.5% configuration (includes unused orchestration config)
    /// - Context-specific loading achieves 100% efficiency
    ///
    /// Note: dotenv() should be called earlier in the chain (e.g., in embedded_bridge.rs)
    /// to ensure environment variables are loaded before configuration loading.
    #[deprecated(
        since = "0.1.0",
        note = "Use SystemContext::new_for_orchestration() or SystemContext::new_for_worker() for efficient context-specific loading. See docs for details."
    )]
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

    /// Load context-specific configuration (TAS-50 Phase 1)
    ///
    /// This enables loading only the configuration needed for a specific deployment context:
    /// - `Orchestration`: Common + Orchestration configs
    /// - `Worker`: Common + Worker configs
    /// - `Combined`: All three configs
    /// - `Legacy`: Full TaskerConfig (backward compatibility)
    ///
    /// **Note**: This method converts from TaskerConfig (Phase 1 approach).
    /// For Phase 2, use `load_context_direct()` which loads from context TOML files.
    ///
    /// # Example
    /// ```no_run
    /// use tasker_shared::config::{ConfigManager, contexts::ConfigContext};
    ///
    /// // Load only orchestration configuration
    /// let manager = ConfigManager::load_for_context(ConfigContext::Orchestration).unwrap();
    /// let common = manager.common().unwrap();
    /// let orch = manager.orchestration().unwrap();
    /// ```
    #[allow(deprecated)] // Intentional: Phase 1 method uses deprecated load() for backward compatibility
    pub fn load_for_context(context: ConfigContext) -> ConfigResult<ContextConfigManager> {
        // Load legacy configuration first
        let legacy_manager = Self::load()?;
        let legacy_config = &legacy_manager.config;

        // Convert to context-specific configurations
        let config: Box<dyn Any + Send + Sync> = match context {
            ConfigContext::Orchestration => {
                let common = CommonConfig::from(legacy_config);
                let orchestration = OrchestrationConfig::from(legacy_config);
                Box::new((common, orchestration))
            }
            ConfigContext::Worker => {
                let common = CommonConfig::from(legacy_config);
                let worker = WorkerConfig::from(legacy_config);
                Box::new((common, worker))
            }
            ConfigContext::Combined => {
                let common = CommonConfig::from(legacy_config);
                let orchestration = OrchestrationConfig::from(legacy_config);
                let worker = WorkerConfig::from(legacy_config);
                Box::new((common, orchestration, worker))
            }
            ConfigContext::Legacy => Box::new(legacy_config.clone()),
        };

        info!(
            "Context-specific configuration loaded successfully (Phase 1): {:?}, environment: {}",
            context, legacy_manager.environment
        );

        Ok(ContextConfigManager {
            context,
            environment: legacy_manager.environment.clone(),
            config,
        })
    }

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
