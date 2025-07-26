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
//! # tasker-config.yaml
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
//! ```rust
//! use tasker_core::orchestration::config::ConfigurationManager;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load system configuration
//! let config_manager = ConfigurationManager::load_from_file("config/tasker-config.yaml").await?;
//! let system_config = config_manager.system_config();
//!
//! // Load task template
//! let task_template = config_manager.load_task_template("config/tasks/payment_processing.yaml").await?;
//!
//! // Access configuration values
//! let auth_enabled = system_config.auth.authentication_enabled;
//! let retry_limit = system_config.backoff.default_backoff_seconds.len();
//!
//! // Verify the values are as expected
//! assert!(!auth_enabled); // Default is false
//! assert_eq!(retry_limit, 6); // [1, 2, 4, 8, 16, 32]
//! assert_eq!(task_template.name, "payment_processing/credit_card_payment");
//! # Ok(())
//! # }
//! ```

use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, instrument};

/// Custom deserializer for numeric values that may be integers or floats in YAML
/// Converts floats to i32 by truncating (e.g., 0.0 -> 0, 10.5 -> 10)
fn deserialize_optional_numeric<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let value: Option<serde_yaml::Value> = Option::deserialize(deserializer)?;

    match value {
        None => Ok(None),
        Some(serde_yaml::Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                Ok(Some(i as i32))
            } else if let Some(f) = n.as_f64() {
                // Truncate floating point to integer
                Ok(Some(f as i32))
            } else {
                Err(D::Error::custom(format!("Invalid numeric value: {n}")))
            }
        }
        Some(serde_yaml::Value::String(s)) => {
            // Try to parse string as number
            s.parse::<i32>()
                .map(Some)
                .or_else(|_| s.parse::<f64>().map(|f| Some(f as i32)))
                .map_err(|_| D::Error::custom(format!("Cannot parse '{s}' as numeric")))
        }
        Some(other) => Err(D::Error::custom(format!(
            "Expected numeric value, found: {other:?}"
        ))),
    }
}

/// Main system configuration struct that mirrors Rails engine configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskerConfig {
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
    pub telemetry: TelemetryConfig,
    pub engine: EngineConfig,
    pub health: HealthConfig,
    pub dependency_graph: DependencyGraphConfig,
    pub system: SystemConfig,
    pub backoff: BackoffConfig,
    pub execution: ExecutionConfig,
    pub reenqueue: ReenqueueDelays,
    pub events: EventConfig,
    pub cache: CacheConfig,
    pub zeromq: ZeroMqConfig,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub authentication_enabled: bool,
    pub authenticator_class: Option<String>,
    pub current_user_method: String,
    pub authenticate_user_method: String,
    pub authorization_enabled: bool,
    pub authorization_coordinator_class: String,
    pub user_class: Option<String>,
    pub strategy: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            authentication_enabled: false,
            authenticator_class: None,
            current_user_method: "current_user".to_string(),
            authenticate_user_method: "authenticate_user!".to_string(),
            authorization_enabled: false,
            authorization_coordinator_class: "Tasker::Authorization::BaseCoordinator".to_string(),
            user_class: None,
            strategy: "none".to_string(),
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatabaseConfig {
    pub name: Option<String>,
    pub enable_secondary_database: bool,
    pub url: Option<String>,
    pub pool: DatabasePoolConfig,
}

/// Database connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabasePoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
    pub max_lifetime_seconds: u64,
}

impl Default for DatabasePoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            acquire_timeout_seconds: 30,
            idle_timeout_seconds: 300,
            max_lifetime_seconds: 3600,
        }
    }
}

/// Telemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub service_name: String,
    pub sample_rate: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            service_name: "tasker-core-rs".to_string(),
            sample_rate: 1.0,
        }
    }
}

/// Engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub task_handler_directory: String,
    pub task_config_directory: String,
    pub default_module_namespace: Option<String>,
    pub identity_strategy: String,
    pub identity_strategy_class: Option<String>,
    pub custom_events_directories: Vec<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            task_handler_directory: "tasks".to_string(),
            task_config_directory: "tasker/tasks".to_string(),
            default_module_namespace: None,
            identity_strategy: "default".to_string(),
            identity_strategy_class: None,
            custom_events_directories: vec!["config/tasker/events".to_string()],
        }
    }
}

/// Health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub enabled: bool,
    pub check_interval_seconds: u64,
    pub alert_thresholds: HashMap<String, f64>,
}

impl Default for HealthConfig {
    fn default() -> Self {
        let mut alert_thresholds = HashMap::new();
        alert_thresholds.insert("error_rate".to_string(), 0.05);
        alert_thresholds.insert("queue_depth".to_string(), 1000.0);

        Self {
            enabled: true,
            check_interval_seconds: 60,
            alert_thresholds,
        }
    }
}

/// Dependency graph configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraphConfig {
    pub max_depth: i32,
    pub cycle_detection_enabled: bool,
    pub optimization_enabled: bool,
}

impl Default for DependencyGraphConfig {
    fn default() -> Self {
        Self {
            max_depth: 50,
            cycle_detection_enabled: true,
            optimization_enabled: true,
        }
    }
}

/// System-level configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub default_dependent_system_id: i64,
    pub default_queue_name: String,
    pub version: String,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            default_dependent_system_id: 1,
            default_queue_name: "default".to_string(),
            version: "1.0.0".to_string(),
        }
    }
}

/// Backoff and retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    pub default_backoff_seconds: Vec<i32>,
    pub max_backoff_seconds: i32,
    pub backoff_multiplier: f64,
    pub jitter_enabled: bool,
    pub jitter_max_percentage: f64,
    pub reenqueue_delays: HashMap<String, i32>,
    pub default_reenqueue_delay: i32,
    pub buffer_seconds: i32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        let mut reenqueue_delays = HashMap::new();
        reenqueue_delays.insert("has_ready_steps".to_string(), 0);
        reenqueue_delays.insert("waiting_for_dependencies".to_string(), 45);
        reenqueue_delays.insert("processing".to_string(), 10);

        Self {
            default_backoff_seconds: vec![1, 2, 4, 8, 16, 32],
            max_backoff_seconds: 300,
            backoff_multiplier: 2.0,
            jitter_enabled: true,
            jitter_max_percentage: 0.1,
            reenqueue_delays,
            default_reenqueue_delay: 30,
            buffer_seconds: 5,
        }
    }
}

/// Execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    pub max_concurrent_tasks: usize,
    pub max_concurrent_steps: usize,
    pub default_timeout_seconds: u64,
    pub step_execution_timeout_seconds: u64,
    pub max_discovery_attempts: u32,
    pub step_batch_size: usize,
    pub processing_mode: String,
    pub environment: String,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 100,
            max_concurrent_steps: 1000,
            default_timeout_seconds: 3600,
            step_execution_timeout_seconds: 300,
            max_discovery_attempts: 3,
            step_batch_size: 10,
            processing_mode: "concurrent".to_string(),
            environment: "development".to_string(),
        }
    }
}

/// Reenqueue delays based on task status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReenqueueDelays {
    /// Delay when task has steps ready for execution (seconds)
    pub has_ready_steps: u32,
    /// Delay when waiting for dependencies to complete (seconds)
    pub waiting_for_dependencies: u32,
    /// Delay when task is currently processing (seconds)
    pub processing: u32,
}

impl Default for ReenqueueDelays {
    fn default() -> Self {
        Self {
            has_ready_steps: 1,
            waiting_for_dependencies: 5,
            processing: 2,
        }
    }
}

/// Event system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// Maximum number of events to batch together
    pub batch_size: usize,
    /// Whether event processing is enabled
    pub enabled: bool,
    /// Buffer timeout for event batching (milliseconds)
    pub batch_timeout_ms: u64,
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            enabled: true,
            batch_timeout_ms: 1000,
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub ttl_seconds: u64,
    pub max_size: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_seconds: 3600,
            max_size: 10000,
        }
    }
}

/// ZeroMQ configuration for batch processing communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroMqConfig {
    /// Endpoint for publishing batches to Ruby orchestrator (e.g., "tcp://127.0.0.1:8555")
    pub batch_endpoint: String,
    /// Endpoint for receiving results from Ruby orchestrator (e.g., "tcp://127.0.0.1:8556")
    pub result_endpoint: String,
    /// High water mark for publishing socket
    pub send_hwm: i32,
    /// High water mark for receiving socket
    pub recv_hwm: i32,
    /// Whether ZeroMQ batch processing is enabled
    pub enabled: bool,
    pub poll_interval_ms: u64,
}

impl Default for ZeroMqConfig {
    fn default() -> Self {
        Self {
            batch_endpoint: "tcp://127.0.0.1:8555".to_string(),
            result_endpoint: "tcp://127.0.0.1:8556".to_string(),
            send_hwm: 1000,
            recv_hwm: 1000,
            enabled: true,
            poll_interval_ms: 1000,
        }
    }
}

impl ZeroMqConfig {
    pub fn from_config(config: &TaskerConfig) -> Self {
        Self {
            batch_endpoint: config.zeromq.batch_endpoint.clone(),
            result_endpoint: config.zeromq.result_endpoint.clone(),
            send_hwm: config.zeromq.send_hwm,
            recv_hwm: config.zeromq.recv_hwm,
            enabled: config.zeromq.enabled,
            poll_interval_ms: config.zeromq.poll_interval_ms,
        }
    }
}

/// Task template definition structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    pub name: String,
    pub module_namespace: Option<String>,
    pub task_handler_class: String,
    pub namespace_name: String,
    pub version: String,
    pub description: Option<String>,
    pub default_dependent_system: Option<String>,
    #[serde(default)]
    pub named_steps: Vec<String>,
    pub schema: Option<serde_json::Value>,
    pub step_templates: Vec<StepTemplate>,
    pub environments: Option<HashMap<String, EnvironmentConfig>>,
    pub custom_events: Option<Vec<CustomEvent>>,
}

/// Step template definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTemplate {
    pub name: String,
    pub description: Option<String>,
    pub handler_class: String,
    pub handler_config: Option<HashMap<String, serde_json::Value>>,
    pub depends_on_step: Option<String>,
    pub depends_on_steps: Option<Vec<String>>,
    pub default_retryable: Option<bool>,
    #[serde(deserialize_with = "deserialize_optional_numeric", default)]
    pub default_retry_limit: Option<i32>,
    #[serde(deserialize_with = "deserialize_optional_numeric", default)]
    pub timeout_seconds: Option<i32>,
    pub retry_backoff: Option<String>,
}

/// Environment-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub step_templates: Vec<StepTemplateOverride>,
}

/// Step template override for environment-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTemplateOverride {
    pub name: String,
    pub handler_config: Option<HashMap<String, serde_json::Value>>,
}

/// Custom event definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEvent {
    pub name: String,
    pub description: Option<String>,
    pub schema: Option<serde_json::Value>,
}

/// Main configuration manager
pub struct ConfigurationManager {
    system_config: Arc<TaskerConfig>,
    environment: String,
    config_directory: String,
}

impl ConfigurationManager {
    /// Create a new configuration manager with default configuration
    pub fn new() -> Self {
        Self {
            system_config: Arc::new(TaskerConfig::default()),
            environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
            config_directory: "config".to_string(),
        }
    }

    /// Load configuration from a YAML file
    #[instrument]
    pub async fn load_from_file<P: AsRef<Path> + std::fmt::Debug>(
        path: P,
    ) -> OrchestrationResult<Self> {
        let path = path.as_ref();
        info!("Loading configuration from: {:?}", path);

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            OrchestrationError::ConfigurationError {
                source: format!("{path:?}"),
                reason: format!("Failed to read configuration file: {e}"),
            }
        })?;

        let interpolated_content = Self::interpolate_env_vars(&content);
        let config: TaskerConfig = serde_yaml::from_str(&interpolated_content).map_err(|e| {
            OrchestrationError::ConfigurationError {
                source: format!("{path:?}"),
                reason: format!("Failed to parse configuration YAML: {e}"),
            }
        })?;

        debug!("Configuration loaded successfully");
        Ok(Self {
            system_config: Arc::new(config),
            environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
            config_directory: "config".to_string(),
        })
    }

    /// Load configuration from a YAML string
    pub fn load_from_yaml(yaml_content: &str) -> OrchestrationResult<Self> {
        let interpolated_content = Self::interpolate_env_vars(yaml_content);
        let config: TaskerConfig = serde_yaml::from_str(&interpolated_content).map_err(|e| {
            OrchestrationError::ConfigurationError {
                source: "yaml_string".to_string(),
                reason: format!("Failed to parse configuration YAML: {e}"),
            }
        })?;

        Ok(Self {
            system_config: Arc::new(config),
            environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
            config_directory: "config".to_string(),
        })
    }

    /// Get the system configuration
    pub fn system_config(&self) -> Arc<TaskerConfig> {
        Arc::clone(&self.system_config)
    }

    /// Get the current environment
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Set the configuration directory
    pub fn set_config_directory(&mut self, directory: String) {
        self.config_directory = directory;
    }

    /// Load a task template from YAML file
    #[instrument(skip(self))]
    pub async fn load_task_template<P: AsRef<Path> + std::fmt::Debug>(
        &self,
        path: P,
    ) -> OrchestrationResult<TaskTemplate> {
        let path = path.as_ref();
        info!("Loading task template from: {:?}", path);

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            OrchestrationError::ConfigurationError {
                source: format!("{path:?}"),
                reason: format!("Failed to read task template file: {e}"),
            }
        })?;

        let interpolated_content = Self::interpolate_env_vars(&content);
        let mut template: TaskTemplate =
            serde_yaml::from_str(&interpolated_content).map_err(|e| {
                OrchestrationError::ConfigurationError {
                    source: format!("{path:?}"),
                    reason: format!("Failed to parse task template YAML: {e}"),
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

        // Apply environment-specific overrides
        if let Some(environments) = &template.environments {
            if let Some(env_config) = environments.get(&self.environment) {
                let env_config_clone = env_config.clone();
                self.apply_environment_overrides(&mut template, &env_config_clone);
            }
        }

        debug!("Task template loaded successfully: {}", template.name);
        Ok(template)
    }

    /// Load a task template from YAML string
    pub fn load_task_template_from_yaml(
        &self,
        yaml_content: &str,
    ) -> OrchestrationResult<TaskTemplate> {
        let interpolated_content = Self::interpolate_env_vars(yaml_content);
        let mut template: TaskTemplate =
            serde_yaml::from_str(&interpolated_content).map_err(|e| {
                OrchestrationError::ConfigurationError {
                    source: "yaml_string".to_string(),
                    reason: format!("Failed to parse task template YAML: {e}"),
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

        // Apply environment-specific overrides
        if let Some(environments) = &template.environments {
            if let Some(env_config) = environments.get(&self.environment) {
                let env_config_clone = env_config.clone();
                self.apply_environment_overrides(&mut template, &env_config_clone);
            }
        }

        Ok(template)
    }

    /// Apply environment-specific overrides to a task template
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

    /// Interpolate environment variables in configuration strings
    fn interpolate_env_vars(template: &str) -> String {
        let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
        re.replace_all(template, |caps: &regex::Captures| {
            let var_name = &caps[1];
            std::env::var(var_name).unwrap_or_else(|_| format!("${{{var_name}}}"))
        })
        .to_string()
    }

    /// Validate a task template
    pub fn validate_task_template(&self, template: &TaskTemplate) -> OrchestrationResult<()> {
        // Check required fields
        if template.name.is_empty() {
            return Err(OrchestrationError::ConfigurationError {
                source: "task_template_validation".to_string(),
                reason: "Task template name cannot be empty".to_string(),
            });
        }

        if template.task_handler_class.is_empty() {
            return Err(OrchestrationError::ConfigurationError {
                source: "task_template_validation".to_string(),
                reason: "Task handler class cannot be empty".to_string(),
            });
        }

        if template.namespace_name.is_empty() {
            return Err(OrchestrationError::ConfigurationError {
                source: "task_template_validation".to_string(),
                reason: "Namespace name cannot be empty".to_string(),
            });
        }

        // Validate named steps exist in step templates
        for named_step in &template.named_steps {
            if !template
                .step_templates
                .iter()
                .any(|st| st.name == *named_step)
            {
                return Err(OrchestrationError::ConfigurationError {
                    source: "task_template_validation".to_string(),
                    reason: format!("Named step '{named_step}' not found in step templates"),
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
                    return Err(OrchestrationError::ConfigurationError {
                        source: "task_template_validation".to_string(),
                        reason: format!("Step dependency '{depends_on}' not found"),
                    });
                }
            }

            if let Some(depends_on_steps) = &step_template.depends_on_steps {
                for dep in depends_on_steps {
                    if !template.step_templates.iter().any(|st| st.name == *dep) {
                        return Err(OrchestrationError::ConfigurationError {
                            source: "task_template_validation".to_string(),
                            reason: format!("Step dependency '{dep}' not found"),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for ConfigurationManager {
    fn default() -> Self {
        Self::new()
    }
}

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
    fn test_environment_variable_interpolation() {
        std::env::set_var("TEST_VAR", "test_value");
        let template = "url: ${TEST_VAR}/api";
        let result = ConfigurationManager::interpolate_env_vars(template);
        assert_eq!(result, "url: test_value/api");
    }

    #[test]
    fn test_configuration_manager_creation() {
        let config_manager = ConfigurationManager::new();
        assert_eq!(config_manager.environment(), "development");
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
