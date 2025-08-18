//! Handler Configuration System
//!
//! Provides YAML-driven configuration for task handlers including step templates,
//! environment overrides, and schema validation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{Result, TaskerError};

/// HandlerConfiguration represents the complete configuration for a task handler
/// This matches the YAML structure exactly as stored in handler_config field
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandlerConfiguration {
    /// The name of the task (from YAML: name field)
    pub name: String,

    /// Module namespace for organization (optional in YAML)
    pub module_namespace: Option<String>,

    /// Task handler class name (from YAML: task_handler_class field)
    pub task_handler_class: String,

    /// Namespace name for categorization (from YAML: namespace_name field)
    pub namespace_name: String,

    /// Version of the task handler (from YAML: version field)
    pub version: String,

    /// Description of the task handler (from YAML: description field)
    pub description: Option<String>,

    /// Default dependent system for steps
    pub default_dependent_system: Option<String>,

    /// List of named steps that are valid for this task
    /// If empty, will be automatically populated from step_templates
    #[serde(default)]
    pub named_steps: Vec<String>,

    /// JSON schema for task input validation (from YAML: schema field)
    pub schema: Option<serde_json::Value>,

    /// Step template definitions (from YAML: step_templates array)
    pub step_templates: Vec<StepTemplate>,

    /// Environment-specific overrides (from YAML: environments field)
    pub environments: Option<HashMap<String, EnvironmentConfig>>,

    /// Handler-specific configuration (from YAML: handler_config field)
    pub handler_config: Option<serde_json::Value>,

    /// Default context values (deprecated/unused in current YAML)
    pub default_context: Option<serde_json::Value>,

    /// Default options for task execution (deprecated/unused in current YAML)
    pub default_options: Option<HashMap<String, serde_json::Value>>,
}

/// StepTemplate represents a single step definition within a handler
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepTemplate {
    /// The name identifier for this step template
    pub name: String,

    /// A human-readable description of what this step does
    pub description: Option<String>,

    /// The system that this step depends on for execution
    pub dependent_system: Option<String>,

    /// Whether this step can be retried by default
    pub default_retryable: Option<bool>,

    /// The default maximum number of retry attempts
    #[serde(
        deserialize_with = "crate::utils::serde::deserialize_optional_numeric",
        default
    )]
    pub default_retry_limit: Option<i32>,

    /// Whether this step can be skipped in the workflow
    pub skippable: Option<bool>,

    /// Step-specific timeout in seconds
    /// If not specified, uses the global timeout configuration
    #[serde(
        deserialize_with = "crate::utils::serde::deserialize_optional_numeric",
        default
    )]
    pub timeout_seconds: Option<i32>,

    /// The class that implements the step's logic
    pub handler_class: String,

    /// Optional configuration for the step handler
    pub handler_config: Option<serde_json::Value>,

    /// Optional name of a step that must be completed before this one
    pub depends_on_step: Option<String>,

    /// Names of steps that must be completed before this one
    pub depends_on_steps: Option<Vec<String>>,

    /// Custom events that this step handler can publish
    pub custom_events: Option<Vec<serde_json::Value>>,
}

/// Environment-specific configuration overrides
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    /// Step template overrides for this environment
    pub step_templates: Option<Vec<StepTemplateOverride>>,

    /// Environment-specific default context
    pub default_context: Option<serde_json::Value>,

    /// Environment-specific default options
    pub default_options: Option<HashMap<String, serde_json::Value>>,
}

/// Step template override for environment-specific configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepTemplateOverride {
    /// Name of the step template to override
    pub name: String,

    /// Override for handler configuration
    pub handler_config: Option<serde_json::Value>,

    /// Override for description
    pub description: Option<String>,

    /// Override for dependent system
    pub dependent_system: Option<String>,

    /// Override for retryable setting
    pub default_retryable: Option<bool>,

    /// Override for retry limit
    #[serde(
        deserialize_with = "crate::utils::serde::deserialize_optional_numeric",
        default
    )]
    pub default_retry_limit: Option<i32>,

    /// Override for skippable setting
    pub skippable: Option<bool>,

    /// Override for timeout_seconds setting
    #[serde(
        deserialize_with = "crate::utils::serde::deserialize_optional_numeric",
        default
    )]
    pub timeout_seconds: Option<i32>,
}

/// Resolved handler configuration with environment-specific overrides applied
#[derive(Debug, Clone)]
pub struct ResolvedHandlerConfiguration {
    pub base_config: HandlerConfiguration,
    pub environment: String,
    pub resolved_step_templates: Vec<StepTemplate>,
    pub resolved_default_context: serde_json::Value,
    pub resolved_default_options: HashMap<String, serde_json::Value>,
}

impl HandlerConfiguration {
    /// Load a HandlerConfiguration from YAML content
    pub fn from_yaml(yaml_content: &str) -> Result<Self> {
        let mut config: HandlerConfiguration = serde_yaml::from_str(yaml_content)
            .map_err(|e| TaskerError::ValidationError(format!("Invalid YAML: {e}")))?;

        // Auto-populate named_steps from step_templates if it's empty
        if config.named_steps.is_empty() {
            config.named_steps = config
                .step_templates
                .iter()
                .map(|st| st.name.clone())
                .collect();
        }

        Ok(config)
    }

    /// Load a HandlerConfiguration from a YAML file
    pub fn from_yaml_file(file_path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| TaskerError::ValidationError(format!("Failed to read file: {e}")))?;
        Self::from_yaml(&content)
    }

    /// Resolve this configuration for a specific environment
    pub fn resolve_for_environment(
        &self,
        environment: &str,
    ) -> Result<ResolvedHandlerConfiguration> {
        // Start with base step templates
        let mut resolved_step_templates = self.step_templates.clone();

        // Apply environment-specific overrides if they exist
        if let Some(environments) = &self.environments {
            if let Some(env_config) = environments.get(environment) {
                resolved_step_templates = self.apply_step_template_overrides(
                    resolved_step_templates,
                    env_config.step_templates.as_ref(),
                )?;
            }
        }

        // Resolve default context
        let resolved_default_context = self.resolve_default_context(environment)?;

        // Resolve default options
        let resolved_default_options = self.resolve_default_options(environment)?;

        Ok(ResolvedHandlerConfiguration {
            base_config: self.clone(),
            environment: environment.to_string(),
            resolved_step_templates,
            resolved_default_context,
            resolved_default_options,
        })
    }

    /// Apply step template overrides from environment configuration
    fn apply_step_template_overrides(
        &self,
        mut base_templates: Vec<StepTemplate>,
        overrides: Option<&Vec<StepTemplateOverride>>,
    ) -> Result<Vec<StepTemplate>> {
        let Some(overrides) = overrides else {
            return Ok(base_templates);
        };

        // Create a map of templates by name for quick lookup
        let mut template_map: HashMap<String, usize> = HashMap::new();
        for (index, template) in base_templates.iter().enumerate() {
            template_map.insert(template.name.clone(), index);
        }

        // Apply overrides to matching templates
        for override_config in overrides {
            if let Some(&index) = template_map.get(&override_config.name) {
                let template = &mut base_templates[index];
                self.apply_single_override(template, override_config)?;
            }
        }

        Ok(base_templates)
    }

    /// Apply a single override to a step template
    fn apply_single_override(
        &self,
        template: &mut StepTemplate,
        override_config: &StepTemplateOverride,
    ) -> Result<()> {
        // Apply overrides only if they are provided (Some)
        if let Some(ref handler_config) = override_config.handler_config {
            // Deep merge the handler config
            template.handler_config =
                Some(self.deep_merge_json(template.handler_config.as_ref(), Some(handler_config))?);
        }

        if let Some(ref description) = override_config.description {
            template.description = Some(description.clone());
        }

        if let Some(ref dependent_system) = override_config.dependent_system {
            template.dependent_system = Some(dependent_system.clone());
        }

        if let Some(default_retryable) = override_config.default_retryable {
            template.default_retryable = Some(default_retryable);
        }

        if let Some(default_retry_limit) = override_config.default_retry_limit {
            template.default_retry_limit = Some(default_retry_limit);
        }

        if let Some(skippable) = override_config.skippable {
            template.skippable = Some(skippable);
        }

        if let Some(timeout_seconds) = override_config.timeout_seconds {
            template.timeout_seconds = Some(timeout_seconds);
        }

        Ok(())
    }

    /// Deep merge JSON values (base + override)
    fn deep_merge_json(
        &self,
        base: Option<&serde_json::Value>,
        override_val: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match (base, override_val) {
            (None, None) => Ok(serde_json::json!({})),
            (Some(base), None) => Ok(base.clone()),
            (None, Some(override_val)) => Ok(override_val.clone()),
            (Some(base), Some(override_val)) => {
                if let (Some(base_obj), Some(override_obj)) =
                    (base.as_object(), override_val.as_object())
                {
                    let mut result = base_obj.clone();
                    for (key, value) in override_obj {
                        result.insert(key.clone(), value.clone());
                    }
                    Ok(serde_json::Value::Object(result))
                } else {
                    // If either is not an object, override takes precedence
                    Ok(override_val.clone())
                }
            }
        }
    }

    /// Resolve default context for the environment
    fn resolve_default_context(&self, environment: &str) -> Result<serde_json::Value> {
        let mut context = self
            .default_context
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        // Apply environment-specific context
        if let Some(environments) = &self.environments {
            if let Some(env_config) = environments.get(environment) {
                if let Some(env_context) = &env_config.default_context {
                    context = self.deep_merge_json(Some(&context), Some(env_context))?;
                }
            }
        }

        Ok(context)
    }

    /// Resolve default options for the environment
    fn resolve_default_options(
        &self,
        environment: &str,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let mut options = self.default_options.clone().unwrap_or_default();

        // Apply environment-specific options
        if let Some(environments) = &self.environments {
            if let Some(env_config) = environments.get(environment) {
                if let Some(env_options) = &env_config.default_options {
                    for (key, value) in env_options {
                        options.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        Ok(options)
    }

    /// Validate the handler configuration
    pub fn validate(&self) -> Result<()> {
        // Validate that all step names are in the named_steps list if provided
        if !self.named_steps.is_empty() {
            let named_steps_set: std::collections::HashSet<&String> =
                self.named_steps.iter().collect();

            for template in &self.step_templates {
                // Validate template name
                if !named_steps_set.contains(&template.name) {
                    return Err(TaskerError::ValidationError(format!(
                        "Step template name '{}' is not in the named_steps list",
                        template.name
                    )));
                }

                // Validate single dependency
                if let Some(ref depends_on_step) = template.depends_on_step {
                    if !named_steps_set.contains(depends_on_step) {
                        return Err(TaskerError::ValidationError(format!(
                            "Dependency step '{depends_on_step}' is not in the named_steps list"
                        )));
                    }
                }

                // Validate multiple dependencies
                if let Some(ref depends_on_steps) = template.depends_on_steps {
                    for dep_step in depends_on_steps {
                        if !named_steps_set.contains(dep_step) {
                            return Err(TaskerError::ValidationError(format!(
                                "Dependency step '{dep_step}' is not in the named_steps list"
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl StepTemplate {
    /// Get all dependency step names as a single array
    pub fn all_dependencies(&self) -> Vec<String> {
        let mut deps = Vec::new();

        if let Some(ref single_dep) = self.depends_on_step {
            deps.push(single_dep.clone());
        }

        if let Some(ref multiple_deps) = self.depends_on_steps {
            deps.extend(multiple_deps.clone());
        }

        deps.into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Get the effective dependent system (using default if not specified)
    pub fn effective_dependent_system(&self, default: Option<&str>) -> Option<String> {
        self.dependent_system
            .clone()
            .or_else(|| default.map(|s| s.to_string()))
    }
}

impl ResolvedHandlerConfiguration {
    /// Get a step template by name from the resolved templates
    pub fn get_step_template(&self, name: &str) -> Option<&StepTemplate> {
        self.resolved_step_templates.iter().find(|t| t.name == name)
    }

    /// Get all step template names in dependency order
    pub fn get_step_execution_order(&self) -> Result<Vec<String>> {
        // Simple topological sort implementation
        let mut visited = std::collections::HashSet::new();
        let mut order = Vec::new();

        for template in &self.resolved_step_templates {
            if !visited.contains(&template.name) {
                self.visit_step_dependencies(&template.name, &mut visited, &mut order)?;
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
    ) -> Result<()> {
        if visited.contains(step_name) {
            return Ok(());
        }

        let template = self.get_step_template(step_name).ok_or_else(|| {
            TaskerError::ValidationError(format!("Step template '{step_name}' not found"))
        })?;

        // Visit dependencies first
        for dependency in template.all_dependencies() {
            self.visit_step_dependencies(&dependency, visited, order)?;
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
    fn test_handler_config_from_yaml() {
        let yaml_content = r#"
name: test_task
task_handler_class: TestHandler
namespace_name: test
version: 0.1.0
named_steps:
  - step1
  - step2
step_templates:
  - name: step1
    description: First step
    handler_class: Step1Handler
  - name: step2
    description: Second step
    depends_on_step: step1
    handler_class: Step2Handler
"#;

        let config = HandlerConfiguration::from_yaml(yaml_content).expect("Should parse YAML");

        assert_eq!(config.name, "test_task");
        assert_eq!(config.namespace_name, "test");
        assert_eq!(config.step_templates.len(), 2);
        assert_eq!(
            config.step_templates[1].depends_on_step,
            Some("step1".to_string())
        );
    }

    #[test]
    fn test_environment_resolution() {
        let yaml_content = r#"
name: test_task
task_handler_class: TestHandler
namespace_name: test
version: 0.1.0
named_steps:
  - step1
step_templates:
  - name: step1
    handler_class: Step1Handler
    handler_config:
      url: https://api.example.com
environments:
  development:
    step_templates:
      - name: step1
        handler_config:
          url: http://localhost:3000
          debug: true
"#;

        let config = HandlerConfiguration::from_yaml(yaml_content).expect("Should parse YAML");
        let resolved = config
            .resolve_for_environment("development")
            .expect("Should resolve");

        let step1 = resolved
            .get_step_template("step1")
            .expect("Should find step1");
        let config = step1.handler_config.as_ref().expect("Should have config");

        assert_eq!(config["url"], "http://localhost:3000");
        assert_eq!(config["debug"], true);
    }
}
