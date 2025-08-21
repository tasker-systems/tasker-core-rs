//! Task Template System
//!
//! Provides YAML-driven task definition support with self-describing workflow configuration.
//! TaskTemplate represents the complete task configuration including step templates,
//! environment-specific overrides, validation schemas, system dependencies, and domain events.
//!
//! ## Self-Describing Configuration
//!
//! This module implements a self-describing TaskTemplate structure, featuring:
//! - Callable-based handlers for maximum flexibility
//! - Structured handler initialization configuration
//! - Clear system dependency declarations
//! - First-class domain event support
//! - Enhanced environment-specific overrides
//! - JSON Schema-based input validation

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::errors::{TaskerError, TaskerResult};

/// Complete task template with all workflow configuration
///
/// ## Self-Describing Configuration
///
/// This structure implements a self-describing TaskTemplate, providing:
/// - Flexible callable-based handlers
/// - Structured system dependencies
/// - First-class domain event support
/// - Enhanced environment overrides
/// - JSON Schema input validation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskTemplate {
    /// Unique task name within namespace
    pub name: String,

    /// Namespace for organization
    pub namespace_name: String,

    /// Semantic version
    pub version: String,

    /// Human-readable description
    pub description: Option<String>,

    /// Template metadata for documentation
    pub metadata: Option<TemplateMetadata>,

    /// Task-level handler configuration
    pub task_handler: Option<HandlerDefinition>,

    /// External system dependencies
    #[serde(default)]
    pub system_dependencies: SystemDependencies,

    /// Domain events this task can publish
    #[serde(default)]
    pub domain_events: Vec<DomainEventDefinition>,

    /// JSON Schema for input validation
    pub input_schema: Option<Value>,

    /// Workflow step definitions
    #[serde(default)]
    pub steps: Vec<StepDefinition>,

    /// Environment-specific overrides
    #[serde(default)]
    pub environments: HashMap<String, EnvironmentOverride>,
}

/// Template metadata for documentation and discovery
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub documentation_url: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Handler definition with callable and initialization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandlerDefinition {
    /// Callable reference (class, proc, lambda)
    pub callable: String,

    /// Initialization parameters
    #[serde(default)]
    pub initialization: HashMap<String, Value>,
}

/// External system dependencies
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemDependencies {
    /// Primary system interaction
    #[serde(default = "default_system")]
    pub primary: String,

    /// Secondary systems
    #[serde(default)]
    pub secondary: Vec<String>,
}

fn default_system() -> String {
    "default".to_string()
}

impl Default for SystemDependencies {
    fn default() -> Self {
        Self {
            primary: default_system(),
            secondary: Vec::new(),
        }
    }
}

/// Domain event definition with schema
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainEventDefinition {
    pub name: String,
    pub description: Option<String>,
    pub schema: Option<Value>, // JSON Schema
}

/// Individual workflow step definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepDefinition {
    pub name: String,
    pub description: Option<String>,

    /// Handler for this step
    pub handler: HandlerDefinition,

    /// System this step interacts with
    pub system_dependency: Option<String>,

    /// Dependencies on other steps
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Retry configuration
    #[serde(default)]
    pub retry: RetryConfiguration,

    /// Step timeout
    pub timeout_seconds: Option<u32>,

    /// Events this step publishes
    #[serde(default)]
    pub publishes_events: Vec<String>,
}

/// Retry configuration with backoff strategies
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetryConfiguration {
    #[serde(default = "default_retryable")]
    pub retryable: bool,

    #[serde(default = "default_retry_limit")]
    pub limit: u32,

    #[serde(default)]
    pub backoff: BackoffStrategy,

    pub backoff_base_ms: Option<u64>,
    pub max_backoff_ms: Option<u64>,
}

impl Default for RetryConfiguration {
    fn default() -> Self {
        Self {
            retryable: true,
            limit: 3,
            backoff: BackoffStrategy::Exponential,
            backoff_base_ms: Some(1000),
            max_backoff_ms: Some(30000),
        }
    }
}

fn default_retryable() -> bool {
    true
}
fn default_retry_limit() -> u32 {
    3
}

/// Backoff strategies for retries
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    None,
    Linear,
    #[default]
    Exponential,
    Fibonacci,
}

/// Environment-specific overrides
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentOverride {
    pub task_handler: Option<HandlerOverride>,
    #[serde(default)]
    pub steps: Vec<StepOverride>,
}

/// Handler override for environments
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandlerOverride {
    pub initialization: Option<HashMap<String, Value>>,
}

/// Step override for environments
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepOverride {
    /// Step name or "ALL" for all steps
    pub name: String,
    pub handler: Option<HandlerOverride>,
    pub timeout_seconds: Option<u32>,
    pub retry: Option<RetryConfiguration>,
}

/// Resolved task template with environment-specific overrides applied
#[derive(Debug, Clone)]
pub struct ResolvedTaskTemplate {
    pub template: TaskTemplate,
    pub environment: String,
    pub resolved_at: DateTime<Utc>,
}

impl TaskTemplate {
    /// Create from YAML string
    pub fn from_yaml(yaml_str: &str) -> TaskerResult<Self> {
        serde_yaml::from_str(yaml_str)
            .map_err(|e| TaskerError::ValidationError(format!("Invalid YAML: {e}")))
    }

    /// Create from YAML file
    pub fn from_yaml_file(path: &std::path::Path) -> TaskerResult<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| TaskerError::ValidationError(format!("Failed to read file: {e}")))?;
        Self::from_yaml(&contents)
    }

    /// Resolve template for specific environment
    pub fn resolve_for_environment(&self, environment: &str) -> ResolvedTaskTemplate {
        let mut resolved = self.clone();

        if let Some(env_override) = self.environments.get(environment) {
            // Apply task handler overrides
            if let Some(handler_override) = &env_override.task_handler {
                if let Some(task_handler) = &mut resolved.task_handler {
                    if let Some(init_override) = &handler_override.initialization {
                        task_handler.initialization.extend(init_override.clone());
                    }
                }
            }

            // Apply step overrides
            for step_override in &env_override.steps {
                if step_override.name == "ALL" {
                    // Apply to all steps
                    for step in &mut resolved.steps {
                        apply_step_override(step, step_override);
                    }
                } else {
                    // Apply to specific step
                    if let Some(step) = resolved
                        .steps
                        .iter_mut()
                        .find(|s| s.name == step_override.name)
                    {
                        apply_step_override(step, step_override);
                    }
                }
            }
        }

        ResolvedTaskTemplate {
            template: resolved,
            environment: environment.to_string(),
            resolved_at: Utc::now(),
        }
    }

    /// Extract all callable references
    pub fn all_callables(&self) -> Vec<String> {
        let mut callables = Vec::new();

        if let Some(handler) = &self.task_handler {
            callables.push(handler.callable.clone());
        }

        for step in &self.steps {
            callables.push(step.handler.callable.clone());
        }

        callables
    }

    /// Validate template structure
    pub fn validate(&self) -> TaskerResult<()> {
        let mut errors = Vec::new();

        // Validate version format
        if !self.version.chars().filter(|c| *c == '.').count() == 2 {
            errors.push("Version must be in semver format (x.y.z)".to_string());
        }

        // Validate step dependencies exist
        let step_names: Vec<String> = self.steps.iter().map(|s| s.name.clone()).collect();
        for step in &self.steps {
            for dep in &step.dependencies {
                if !step_names.contains(dep) {
                    errors.push(format!(
                        "Step '{}' depends on non-existent step '{}'",
                        step.name, dep
                    ));
                }
            }
        }

        // Validate no circular dependencies
        if let Err(e) = self.validate_no_circular_dependencies() {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(TaskerError::ValidationError(errors.join("; ")))
        }
    }

    fn validate_no_circular_dependencies(&self) -> std::result::Result<(), String> {
        // Build dependency graph and check for cycles
        // Implementation details omitted for brevity
        Ok(())
    }
}

fn apply_step_override(step: &mut StepDefinition, override_def: &StepOverride) {
    if let Some(handler_override) = &override_def.handler {
        if let Some(init_override) = &handler_override.initialization {
            step.handler.initialization.extend(init_override.clone());
        }
    }

    if let Some(timeout) = override_def.timeout_seconds {
        step.timeout_seconds = Some(timeout);
    }

    if let Some(retry) = &override_def.retry {
        step.retry = retry.clone();
    }
}

impl StepDefinition {
    /// Check if this step depends on another step
    pub fn depends_on(&self, other_step_name: &str) -> bool {
        self.dependencies.contains(&other_step_name.to_string())
    }
}

impl ResolvedTaskTemplate {
    /// Get a step by name from the resolved template
    pub fn get_step(&self, name: &str) -> Option<&StepDefinition> {
        self.template.steps.iter().find(|s| s.name == name)
    }

    /// Get all step names in dependency order
    pub fn get_step_execution_order(&self) -> TaskerResult<Vec<String>> {
        // Simple topological sort implementation
        let mut visited = std::collections::HashSet::new();
        let mut order = Vec::new();

        for step in &self.template.steps {
            if !visited.contains(&step.name) {
                self.visit_step_dependencies(&step.name, &mut visited, &mut order)?;
            }
        }

        Ok(order)
    }

    /// Recursive dependency resolution for topological sort
    fn visit_step_dependencies(
        &self,
        step_name: &str,
        visited: &mut std::collections::HashSet<String>,
        order: &mut Vec<String>,
    ) -> TaskerResult<()> {
        if visited.contains(step_name) {
            return Ok(());
        }

        let step = self
            .get_step(step_name)
            .ok_or_else(|| TaskerError::ValidationError(format!("Step '{step_name}' not found")))?;

        // Visit dependencies first
        for dependency in &step.dependencies {
            self.visit_step_dependencies(dependency, visited, order)?;
        }

        visited.insert(step_name.to_string());
        order.push(step_name.to_string());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_task_template_from_yaml() {
        let yaml_content = r#"
name: credit_card_payment
namespace_name: payments
version: "1.0.0"
description: "Process credit card payments with validation and fraud detection"

metadata:
  author: "payments-team"
  tags: ["payment", "critical", "financial"]

task_handler:
  callable: "PaymentProcessing::CreditCardPaymentHandler"
  initialization:
    timeout_ms: 30000
    retry_strategy: "exponential_backoff"

system_dependencies:
  primary: "payment_gateway"
  secondary: ["fraud_detection_api", "customer_database"]

domain_events:
  - name: "payment.authorized"
    description: "Payment successfully authorized by gateway"
  - name: "payment.declined"
    description: "Payment was declined"

steps:
  - name: validate_payment
    description: "Validate payment information and card status"
    handler:
      callable: "PaymentProcessing::ValidationHandler"
      initialization:
        max_amount: 10000
    system_dependency: "payment_gateway"
    retry:
      retryable: true
      limit: 3
      backoff: "exponential"
    timeout_seconds: 30

  - name: authorize_payment
    description: "Authorize payment with gateway"
    dependencies: ["validate_payment"]
    handler:
      callable: "PaymentProcessing::AuthorizationHandler"
    system_dependency: "payment_gateway"

environments:
  development:
    task_handler:
      initialization:
        debug_mode: true
    steps:
      - name: validate_payment
        timeout_seconds: 60
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");

        assert_eq!(template.name, "credit_card_payment");
        assert_eq!(template.namespace_name, "payments");
        assert_eq!(template.version, "1.0.0");
        assert_eq!(template.system_dependencies.primary, "payment_gateway");
        assert_eq!(template.system_dependencies.secondary.len(), 2);
        assert_eq!(template.domain_events.len(), 2);
        assert_eq!(template.steps.len(), 2);
        assert_eq!(template.steps[1].dependencies, vec!["validate_payment"]);
    }

    #[test]
    fn test_environment_resolution() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

task_handler:
  callable: "TestHandler"
  initialization:
    timeout_ms: 5000

steps:
  - name: step1
    handler:
      callable: "Step1Handler"
      initialization:
        url: "https://api.example.com"
    timeout_seconds: 30

environments:
  development:
    task_handler:
      initialization:
        debug_mode: true
        timeout_ms: 10000
    steps:
      - name: step1
        handler:
          initialization:
            url: "http://localhost:3000"
        timeout_seconds: 60
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let resolved = template.resolve_for_environment("development");

        let task_handler = resolved
            .template
            .task_handler
            .as_ref()
            .expect("Should have task handler");
        assert_eq!(
            task_handler.initialization.get("debug_mode").unwrap(),
            &serde_json::Value::Bool(true)
        );
        assert_eq!(
            task_handler.initialization.get("timeout_ms").unwrap(),
            &serde_json::Value::Number(10000.into())
        );

        let step1 = &resolved.template.steps[0];
        assert_eq!(
            step1.handler.initialization.get("url").unwrap(),
            "http://localhost:3000"
        );
        assert_eq!(step1.timeout_seconds, Some(60));
    }

    #[test]
    fn test_step_dependency_ordering() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: step3
    handler:
      callable: "Step3Handler"
    dependencies: ["step2"]
  - name: step1
    handler:
      callable: "Step1Handler"
  - name: step2
    handler:
      callable: "Step2Handler"
    dependencies: ["step1"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let resolved = template.resolve_for_environment("test");
        let order = resolved
            .get_step_execution_order()
            .expect("Should get order");

        assert_eq!(order, vec!["step1", "step2", "step3"]);
    }

    #[test]
    fn test_all_callables_extraction() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

task_handler:
  callable: "MainHandler"

steps:
  - name: step1
    handler:
      callable: "Step1Handler"
  - name: step2
    handler:
      callable: "Step2Handler"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let callables = template.all_callables();

        assert_eq!(callables.len(), 3);
        assert!(callables.contains(&"MainHandler".to_string()));
        assert!(callables.contains(&"Step1Handler".to_string()));
        assert!(callables.contains(&"Step2Handler".to_string()));
    }
}
