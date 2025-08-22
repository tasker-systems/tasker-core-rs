//! # Configuration Manager
//!
//! ## Architecture: YAML-driven Configuration System
//!
//! The ConfigurationManager provides a comprehensive configuration system that mirrors
//! the Rails Tasker engine's configuration capabilities while leveraging Rust's type
//! safety and performance characteristics.
//!
//! ## Key Components:
//!
//! - **System Configuration**: Complete system-level configuration matching Rails engine defaults
//! - **Task Templates**: YAML-based task and step template definitions
//! - **Environment Support**: Environment-specific configuration overlays
//! - **Type Safety**: Strong typing for all configuration values
//! - **Validation**: Comprehensive validation for required fields and constraints
//!
//! ## Configuration Structure:
//!
//! ```yaml
//! # config/tasker/database.yaml
//! auth:
//!   authentication_enabled: false
//!   strategy: "none"
//!
//! database:
//!   enable_secondary_database: false
//!
//! backoff:
//!   default_backoff_seconds: [1, 2, 4, 8, 16, 32]
//!   max_backoff_seconds: 300
//!   jitter_enabled: true
//! ```
//!
//! ## Usage:
//!
//! ```rust,no_run
//! use tasker_orchestration::orchestration::ConfigurationManager;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load system configuration using component-based config
//! let config_manager = ConfigurationManager::new();
//! let system_config = config_manager.system_config();
//!
//! // Load task template (example file path - would need to exist)
//! // let task_template = config_manager.load_task_template("config/tasks/payment_processing.yaml").await?;
//!
//! // Access configuration values
//! let auth_enabled = system_config.auth.authentication_enabled;
//! let retry_limit = system_config.backoff.default_backoff_seconds.len();
//!
//! // Verify the values are as expected
//! assert!(!auth_enabled); // Default is false
//! assert_eq!(retry_limit, 6); // [1, 2, 4, 8, 16, 32]
//! // Example task template validation (would require actual file)
//! // assert_eq!(task_template.name, "payment_processing/credit_card_payment");
//! # Ok(())
//! # }
//! ```

pub use tasker_shared::config::orchestration::{
    OrchestrationLoopConfig, OrchestrationSystemConfig, StepEnqueuerConfig,
    StepResultProcessorConfig, TaskClaimerConfig,
};

// Re-export shared types instead of redefining them
pub use tasker_shared::config::orchestration::OrchestrationConfig;
pub use tasker_shared::config::{
    AuthConfig, BackoffConfig, CacheConfig, CustomEvent, DatabaseConfig, DatabasePoolConfig,
    DependencyGraphConfig, EngineConfig, EnvironmentConfig, ExecutionConfig, HealthConfig,
    ReenqueueDelays, StepTemplate, StepTemplateOverride, SystemConfig, TaskTemplate,
    TaskTemplatesConfig, TaskerConfig, TelemetryConfig,
};
pub use tasker_shared::errors::{OrchestrationError, OrchestrationResult};
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

    #[test]
    fn test_load_task_template_from_yaml() {
        let yaml_content = r#"
name: test_task
task_handler_class: TestHandler
namespace_name: test_namespace
version: "1.0.0"
named_steps:
  - step1
  - step2
step_templates:
  - name: step1
    handler_class: Step1Handler
  - name: step2
    handler_class: Step2Handler
    depends_on_step: step1
"#;

        let config_manager = ConfigurationManager::new();
        let template = config_manager
            .load_task_template_from_yaml(yaml_content)
            .unwrap();

        assert_eq!(template.name, "test_task");
        assert_eq!(template.task_handler_class, "TestHandler");
        assert_eq!(template.namespace_name, "test_namespace");
        assert_eq!(template.version, "1.0.0");
        assert_eq!(template.named_steps.len(), 2);
        assert_eq!(template.step_templates.len(), 2);
    }

    #[test]
    fn test_task_template_validation() {
        let config_manager = ConfigurationManager::new();

        // Valid template
        let valid_template = TaskTemplate {
            name: "test_task".to_string(),
            task_handler_class: "TestHandler".to_string(),
            namespace_name: "test_namespace".to_string(),
            version: "1.0.0".to_string(),
            named_steps: vec!["step1".to_string()],
            step_templates: vec![StepTemplate {
                name: "step1".to_string(),
                handler_class: "Step1Handler".to_string(),
                description: None,
                handler_config: None,
                depends_on_step: None,
                depends_on_steps: None,
                default_retryable: None,
                default_retry_limit: None,
                timeout_seconds: None,
                retry_backoff: None,
            }],
            module_namespace: None,
            description: None,
            default_dependent_system: None,
            schema: None,
            environments: None,
            custom_events: None,
        };

        assert!(config_manager
            .validate_task_template(&valid_template)
            .is_ok());

        // Invalid template - missing named step
        let invalid_template = TaskTemplate {
            named_steps: vec!["missing_step".to_string()],
            ..valid_template.clone()
        };

        assert!(config_manager
            .validate_task_template(&invalid_template)
            .is_err());
    }
}
