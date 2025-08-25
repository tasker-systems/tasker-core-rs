//! # Configuration Manager
//!
//! The Configuration Manager is responsible for loading and managing the configuration settings for the Tasker orchestration system.
//! It provides a unified interface for accessing configuration values across different components of the system.
//!
//! Re-export shared types instead of redefining them
pub use tasker_shared::config::orchestration::{
    OrchestrationSystemConfig, StepEnqueuerConfig, StepResultProcessorConfig,
    TaskClaimStepEnqueuerConfig, TaskClaimerConfig,
};

// Re-export shared types instead of redefining them
pub use tasker_shared::config::orchestration::OrchestrationConfig;
pub use tasker_shared::config::{
    AuthConfig, BackoffConfig, CacheConfig, DatabaseConfig, DatabasePoolConfig,
    DependencyGraphConfig, EngineConfig, ExecutionConfig, HealthConfig, ReenqueueDelays,
    SystemConfig, TaskTemplatesConfig, TaskerConfig, TelemetryConfig,
};
// Use canonical TaskTemplate from models instead of legacy config types
pub use tasker_shared::errors::{OrchestrationError, OrchestrationResult};
pub use tasker_shared::models::core::task_template::{StepDefinition, TaskTemplate};
pub type EventConfig = tasker_shared::config::EventsConfig;
pub type ConfigurationManager = tasker_shared::config::ConfigManager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_configuration() {
        let config = TaskerConfig::default();
        assert!(!config.auth.authentication_enabled);
        assert_eq!(config.auth.strategy, "none");
        assert!(!config.database.enable_secondary_database);
        assert_eq!(
            config.backoff.default_backoff_seconds,
            vec![1, 2, 4, 8, 16, 32]
        );
        assert_eq!(config.backoff.max_backoff_seconds, 300);
        assert!(config.backoff.jitter_enabled);
    }

    #[test]
    fn test_configuration_manager_creation() {
        let config_manager = ConfigurationManager::new();
        // Environment can be overridden by TASKER_ENV, so just verify it's not empty
        assert!(!config_manager.environment().is_empty());
        assert!(!config_manager.system_config().auth.authentication_enabled);
    }
}
