//! # Configuration Manager
//!
//! The Configuration Manager is responsible for loading and managing the configuration settings for the Tasker orchestration system.
//! It provides a unified interface for accessing configuration values across different components of the system.
//!
//! Re-export shared types instead of redefining them
pub use tasker_shared::config::orchestration::{
    OrchestrationSystemConfig, StepEnqueuerConfig, StepResultProcessorConfig,
    TaskClaimStepEnqueuerConfig,
};

// TAS-61 Phase 6C/6D: Re-export V2 config types
pub use tasker_shared::config::orchestration::OrchestrationConfig;
pub use tasker_shared::config::tasker::{BackoffConfig, DatabaseConfig, ReenqueueDelaysConfig};
pub use tasker_shared::config::tasker::{
    ExecutionConfig, SystemConfig, TaskTemplatesConfig, TaskerConfig, TelemetryConfig,
};
pub use tasker_shared::config::ConfigManager;

// Use canonical TaskTemplate from models instead of legacy config types
pub use tasker_shared::errors::{OrchestrationError, OrchestrationResult};
pub use tasker_shared::models::core::task_template::{StepDefinition, TaskTemplate};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_loading() {
        // Note: ConfigManager now requires TASKER_CONFIG_PATH to be set
        // These tests are disabled until integration tests can set up proper config files
        // The type re-exports still work:
        let _config_manager_type = std::marker::PhantomData::<ConfigManager>;
    }

    #[test]
    fn test_configuration_manager_creation() {
        // Note: ConfigManager now requires TASKER_CONFIG_PATH to be set
        // This is tested in integration tests with proper config file setup
        let _config_manager_type = std::marker::PhantomData::<ConfigManager>;
    }
}
