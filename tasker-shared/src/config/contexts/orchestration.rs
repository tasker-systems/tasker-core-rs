// TAS-50: OrchestrationConfig - Orchestration-specific configuration
//
// This configuration is used by the orchestration system only.
// It contains orchestration-specific components:
// - Backoff and retry logic for task/step retries
// - Orchestration system settings (web API, operational state)
// - Orchestration event systems (real-time coordination)
// - Task readiness event systems (task discovery)
// - Orchestration MPSC channels (command processor, event listeners)
//
// Phase 1 implementation: Full implementation with conversion from TaskerConfig

use serde::{Deserialize, Serialize};

use super::ConfigurationContext;
use crate::config::components::{BackoffConfig, OrchestrationChannelsConfig};
use crate::config::error::ConfigurationError;
use crate::config::event_systems::{
    OrchestrationEventSystemConfig, TaskReadinessEventSystemConfig,
};
use crate::config::orchestration::OrchestrationConfig as LegacyOrchestrationConfig;
use crate::config::TaskerConfig;

/// Orchestration-specific configuration
///
/// Contains all configuration needed by the orchestration system:
/// - Retry and backoff logic
/// - Orchestration system behavior
/// - Event-driven coordination
/// - MPSC channel buffer sizing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    /// Backoff and retry configuration for task/step retries
    pub backoff: BackoffConfig,

    /// Orchestration system configuration (web API, operational state)
    pub orchestration_system: LegacyOrchestrationConfig,

    /// Orchestration event system configuration
    pub orchestration_events: OrchestrationEventSystemConfig,

    /// Task readiness event system configuration
    pub task_readiness_events: TaskReadinessEventSystemConfig,

    /// MPSC channels configuration for orchestration
    pub mpsc_channels: OrchestrationChannelsConfig,

    /// Current environment (cached from common config)
    pub environment: String,
}

impl ConfigurationContext for OrchestrationConfig {
    fn validate(&self) -> Result<(), Vec<ConfigurationError>> {
        let mut errors = Vec::new();

        // Backoff validation
        if self.backoff.max_backoff_seconds == 0 {
            errors.push(ConfigurationError::InvalidValue {
                field: "backoff.max_backoff_seconds".to_string(),
                value: "0".to_string(),
                context: "max_backoff_seconds must be greater than 0".to_string(),
            });
        }

        if self.backoff.backoff_multiplier <= 0.0 {
            errors.push(ConfigurationError::InvalidValue {
                field: "backoff.backoff_multiplier".to_string(),
                value: self.backoff.backoff_multiplier.to_string(),
                context: "backoff_multiplier must be positive".to_string(),
            });
        }

        if self.backoff.default_backoff_seconds.is_empty() {
            errors.push(ConfigurationError::InvalidValue {
                field: "backoff.default_backoff_seconds".to_string(),
                value: "[]".to_string(),
                context: "default_backoff_seconds must not be empty".to_string(),
            });
        }

        // Environment validation
        if self.environment.is_empty() {
            errors.push(ConfigurationError::MissingRequiredField {
                field: "environment".to_string(),
                context: "OrchestrationConfig".to_string(),
            });
        }

        // MPSC channel validation
        if self.mpsc_channels.command_processor.command_buffer_size == 0 {
            errors.push(ConfigurationError::InvalidValue {
                field: "mpsc_channels.command_processor.command_buffer_size".to_string(),
                value: "0".to_string(),
                context: "command_buffer_size must be greater than 0".to_string(),
            });
        }

        // Orchestration system validation
        if self.orchestration_system.web.enabled
            && self.orchestration_system.web.bind_address.is_empty()
        {
            errors.push(ConfigurationError::InvalidValue {
                field: "orchestration_system.web.bind_address".to_string(),
                value: "".to_string(),
                context: "bind_address must not be empty when web API is enabled".to_string(),
            });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn environment(&self) -> &str {
        &self.environment
    }

    fn summary(&self) -> String {
        format!(
            "OrchestrationConfig: environment={}, max_backoff={}s, web_enabled={}, event_mode={:?}",
            self.environment,
            self.backoff.max_backoff_seconds,
            self.orchestration_system.web.enabled,
            self.orchestration_events.deployment_mode
        )
    }
}

impl From<&TaskerConfig> for OrchestrationConfig {
    /// Convert legacy TaskerConfig to OrchestrationConfig
    ///
    /// This conversion extracts only the orchestration-specific configuration fields
    /// from the monolithic TaskerConfig structure.
    fn from(config: &TaskerConfig) -> Self {
        Self {
            backoff: config.backoff.clone(),
            orchestration_system: config.orchestration.clone(),
            orchestration_events: config.event_systems.orchestration.clone(),
            task_readiness_events: config.event_systems.task_readiness.clone(),
            mpsc_channels: config.mpsc_channels.orchestration.clone(),
            environment: config.execution.environment.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestration_config_from_tasker_config() {
        // Create a test TaskerConfig
        let tasker_config = TaskerConfig::default();

        // Convert to OrchestrationConfig
        let orch_config = OrchestrationConfig::from(&tasker_config);

        // Verify fields were correctly extracted
        assert_eq!(
            orch_config.backoff.max_backoff_seconds,
            tasker_config.backoff.max_backoff_seconds
        );
        assert_eq!(
            orch_config.backoff.backoff_multiplier,
            tasker_config.backoff.backoff_multiplier
        );
        assert_eq!(orch_config.environment, tasker_config.execution.environment);
        assert_eq!(
            orch_config.orchestration_system.web.enabled,
            tasker_config.orchestration.web.enabled
        );
    }

    #[test]
    fn test_orchestration_config_validation_success() {
        let tasker_config = TaskerConfig::default();
        let orch_config = OrchestrationConfig::from(&tasker_config);

        // Should pass validation
        assert!(orch_config.validate().is_ok());
    }

    #[test]
    fn test_orchestration_config_validation_invalid_backoff() {
        let mut tasker_config = TaskerConfig::default();
        tasker_config.backoff.max_backoff_seconds = 0;

        let orch_config = OrchestrationConfig::from(&tasker_config);

        // Should fail validation
        let result = orch_config.validate();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigurationError::InvalidValue { field, .. } if field == "backoff.max_backoff_seconds")));
    }

    #[test]
    fn test_orchestration_config_validation_empty_backoff_seconds() {
        let mut tasker_config = TaskerConfig::default();
        tasker_config.backoff.default_backoff_seconds = vec![];

        let orch_config = OrchestrationConfig::from(&tasker_config);

        // Should fail validation
        let result = orch_config.validate();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigurationError::InvalidValue { field, .. } if field == "backoff.default_backoff_seconds")));
    }

    #[test]
    fn test_orchestration_config_validation_negative_multiplier() {
        let mut tasker_config = TaskerConfig::default();
        tasker_config.backoff.backoff_multiplier = -1.0;

        let orch_config = OrchestrationConfig::from(&tasker_config);

        // Should fail validation
        let result = orch_config.validate();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigurationError::InvalidValue { field, .. } if field == "backoff.backoff_multiplier")));
    }

    #[test]
    fn test_orchestration_config_validation_zero_command_buffer() {
        let mut tasker_config = TaskerConfig::default();
        tasker_config
            .mpsc_channels
            .orchestration
            .command_processor
            .command_buffer_size = 0;

        let orch_config = OrchestrationConfig::from(&tasker_config);

        // Should fail validation
        let result = orch_config.validate();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(
            e,
            ConfigurationError::InvalidValue { field, .. }
                if field == "mpsc_channels.command_processor.command_buffer_size"
        )));
    }

    #[test]
    fn test_orchestration_config_summary() {
        let tasker_config = TaskerConfig::default();
        let orch_config = OrchestrationConfig::from(&tasker_config);

        let summary = orch_config.summary();
        assert!(summary.contains("OrchestrationConfig"));
        assert!(summary.contains(&orch_config.environment));
    }
}
