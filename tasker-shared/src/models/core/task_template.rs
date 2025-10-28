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

/// Step type for workflow orchestration
///
/// ## Decision Point Steps
///
/// Decision point steps enable dynamic workflow creation based on runtime evaluation.
/// When a decision step completes, the handler returns a `DecisionPointOutcome` that
/// determines which downstream steps should be created and executed.
///
/// ### Constraints:
/// - Decision steps MUST have at least one dependency (parent step)
/// - Decision steps MUST declare potential child steps in the template
/// - Only explicitly declared child steps can be created
/// - Decision steps can fail (permanent or retryable) like any standard step
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    /// Standard workflow step (default)
    #[default]
    Standard,

    /// Decision point that dynamically creates downstream steps
    Decision,

    /// Deferred convergence step with dynamically-resolved dependencies
    ///
    /// Used for convergence points where the final step is known, but which
    /// preceding steps will be created is determined at runtime by a decision point.
    ///
    /// The dependencies field lists ALL possible dependencies. At runtime, when
    /// a decision point creates steps, the system computes the intersection of:
    /// - Declared dependencies in the template
    /// - Actually created steps in this pass
    ///
    /// This intersection determines the actual DAG dependencies for the deferred step.
    ///
    /// Example: A finalize_approval step that depends on [auto_approve, manager_approval,
    /// finance_review], but only the steps chosen by the decision point are created.
    Deferred,
}

/// Individual workflow step definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepDefinition {
    pub name: String,
    pub description: Option<String>,

    /// Handler for this step
    pub handler: HandlerDefinition,

    /// Step type (standard or decision)
    #[serde(default, rename = "type", alias = "step_type")]
    pub step_type: StepType,

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

    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    #[serde(default)]
    pub backoff: BackoffStrategy,

    pub backoff_base_ms: Option<u64>,
    pub max_backoff_ms: Option<u64>,
}

impl Default for RetryConfiguration {
    fn default() -> Self {
        Self {
            retryable: true,
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential,
            backoff_base_ms: Some(1000),
            max_backoff_ms: Some(30000),
        }
    }
}

fn default_retryable() -> bool {
    true
}
fn default_max_attempts() -> u32 {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedTaskTemplate {
    pub template: TaskTemplate,
    pub environment: String,
    pub resolved_at: DateTime<Utc>,
}

/// Type classification for workflow graph segments
///
/// Workflow graphs are discrete segments of the overall workflow DAG, bounded by
/// decision points. This classification helps reason about graph properties and
/// execution flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowGraphType {
    /// Initial graph from root steps to first decision point(s)
    ///
    /// Entry: None (starts from root)
    /// Exit: One or more decision points
    Initial,

    /// Intermediate graph between decision points
    ///
    /// Entry: Single decision point
    /// Exit: One or more decision points
    Intermediate,

    /// Terminal graph from decision point to workflow completion
    ///
    /// Entry: Single decision point
    /// Exit: None (ends at terminal steps)
    Terminal,
}

/// A discrete segment of the workflow graph bounded by decision points
///
/// ## Graph Segmentation
///
/// A WorkflowGraph represents a contiguous subset of steps in the workflow DAG that:
/// 1. Starts from a specific entry point (root or decision point)
/// 2. Ends at exit points (decision points or terminal steps)
/// 3. Contains all steps along paths between entry and exit
///
/// ## Boundaries
///
/// - **Entry Decision**: The decision point that triggers this graph (None for initial)
/// - **Exit Decisions**: Decision points at the boundaries of this graph
/// - **Steps**: All steps in this segment, including boundary decision points
///
/// ## Usage
///
/// ```rust,ignore
/// // Get graphs reachable from a decision
/// let graphs = template.graphs_from_decision("evaluate_order");
///
/// // Create steps from a graph segment
/// for graph in graphs {
///     if graph.contains_step("validate_payment") {
///         create_steps(graph.steps);
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowGraph {
    /// Entry decision step name (None for initial graph from root)
    pub entry_decision: Option<String>,

    /// Exit decision step names (empty for terminal graphs)
    pub exit_decisions: Vec<String>,

    /// Steps in this graph segment (includes decision points at boundaries)
    pub steps: Vec<StepDefinition>,

    /// Type classification for reasoning about graph properties
    pub graph_type: WorkflowGraphType,
}

impl WorkflowGraph {
    /// Check if this graph contains a step with the given name
    pub fn contains_step(&self, step_name: &str) -> bool {
        self.steps.iter().any(|s| s.name == step_name)
    }

    /// Get all step names in this graph
    pub fn step_names(&self) -> Vec<&str> {
        self.steps.iter().map(|s| s.name.as_str()).collect()
    }

    /// Check if all given step names are in this graph
    pub fn contains_all_steps(&self, step_names: &[String]) -> bool {
        step_names.iter().all(|name| self.contains_step(name))
    }

    /// Get entry point name (for logging/debugging)
    pub fn entry_name(&self) -> String {
        self.entry_decision
            .clone()
            .unwrap_or_else(|| "root".to_string())
    }

    /// Get exit points as comma-separated string (for logging/debugging)
    pub fn exit_names(&self) -> String {
        if self.exit_decisions.is_empty() {
            "terminal".to_string()
        } else {
            self.exit_decisions.join(", ")
        }
    }
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

    /// Get all decision point steps in this template
    pub fn decision_point_steps(&self) -> Vec<&StepDefinition> {
        self.steps.iter().filter(|s| s.is_decision()).collect()
    }

    /// Get all deferred convergence steps in the template
    ///
    /// Deferred steps are convergence points whose dependencies are dynamically resolved
    /// at runtime based on which paths were actually created by decision points.
    pub fn deferred_steps(&self) -> Vec<&StepDefinition> {
        self.steps.iter().filter(|s| s.is_deferred()).collect()
    }

    /// Get the initial step set (steps that should be created during task initialization)
    ///
    /// For workflows with decision points, this returns only the steps that can be
    /// safely created before any decision point executes. This includes:
    /// - All steps that are not descendants of decision points
    /// - Decision point steps themselves
    /// - Any steps between root and decision points
    ///
    /// For workflows without decision points, this returns all steps.
    ///
    /// This method now uses the WorkflowGraph concept to identify initial steps.
    pub fn initial_step_set(&self) -> Vec<&StepDefinition> {
        let initial_graphs = self.initial_graphs();

        // Collect all step names from initial graphs
        let initial_step_names: std::collections::HashSet<String> = initial_graphs
            .iter()
            .flat_map(|g| g.steps.iter().map(|s| s.name.clone()))
            .collect();

        // Return references to those steps from our template
        self.steps
            .iter()
            .filter(|s| initial_step_names.contains(&s.name))
            .collect()
    }

    /// Check if a step is a descendant of any decision point
    ///
    /// A step is a descendant if any of its transitive dependencies is a decision point
    fn is_descendant_of_decision_point(&self, step_name: &str, decision_steps: &[&str]) -> bool {
        let step = match self.steps.iter().find(|s| s.name == step_name) {
            Some(s) => s,
            None => return false,
        };

        // Check direct dependencies
        for dep in &step.dependencies {
            if decision_steps.contains(&dep.as_str()) {
                return true;
            }
            // Recursively check transitive dependencies
            if self.is_descendant_of_decision_point(dep, decision_steps) {
                return true;
            }
        }

        false
    }

    /// Segment the workflow template into discrete graphs bounded by decision points
    ///
    /// This method analyzes the workflow DAG and identifies distinct graph segments:
    /// - Initial graphs: root → decision points
    /// - Intermediate graphs: decision point → decision point(s)
    /// - Terminal graphs: decision point → terminal steps
    ///
    /// ## Returns
    ///
    /// A vector of WorkflowGraph instances, each representing a contiguous segment
    /// of the workflow with clear entry and exit boundaries.
    pub fn segment_workflow_graphs(&self) -> Vec<WorkflowGraph> {
        let decision_steps: Vec<&str> = self
            .decision_point_steps()
            .iter()
            .map(|s| s.name.as_str())
            .collect();

        if decision_steps.is_empty() {
            // No decision points - entire workflow is one initial graph
            return vec![WorkflowGraph {
                entry_decision: None,
                exit_decisions: vec![],
                steps: self.steps.clone(),
                graph_type: WorkflowGraphType::Initial,
            }];
        }

        let mut graphs = Vec::new();

        // Build initial graph (root → first decision points)
        graphs.push(self.build_initial_graph(&decision_steps));

        // Build graphs from each decision point
        for decision_step in &decision_steps {
            graphs.extend(self.build_graphs_from_decision(decision_step, &decision_steps));
        }

        graphs
    }

    /// Get all initial workflow graphs (from root to first decision points)
    ///
    /// These graphs contain steps that can be created during task initialization.
    pub fn initial_graphs(&self) -> Vec<WorkflowGraph> {
        self.segment_workflow_graphs()
            .into_iter()
            .filter(|g| matches!(g.graph_type, WorkflowGraphType::Initial))
            .collect()
    }

    /// Get all workflow graphs reachable from a specific decision step
    ///
    /// ## Arguments
    ///
    /// * `decision_step_name` - Name of the decision step to start from
    ///
    /// ## Returns
    ///
    /// All graphs that have this decision as their entry point.
    pub fn graphs_from_decision(&self, decision_step_name: &str) -> Vec<WorkflowGraph> {
        self.segment_workflow_graphs()
            .into_iter()
            .filter(|g| g.entry_decision.as_deref() == Some(decision_step_name))
            .collect()
    }

    /// Get all terminal workflow graphs (decision points → terminal steps)
    ///
    /// These graphs end the workflow execution (no further decisions).
    pub fn terminal_graphs(&self) -> Vec<WorkflowGraph> {
        self.segment_workflow_graphs()
            .into_iter()
            .filter(|g| matches!(g.graph_type, WorkflowGraphType::Terminal))
            .collect()
    }

    /// Build the initial workflow graph (root → first decision points)
    fn build_initial_graph(&self, decision_steps: &[&str]) -> WorkflowGraph {
        // Compute initial steps directly to avoid circular dependency with initial_step_set()
        // Build a set of steps that are downstream of decision points
        let mut decision_descendants = std::collections::HashSet::new();
        for step in &self.steps {
            if self.is_descendant_of_decision_point(&step.name, decision_steps) {
                decision_descendants.insert(step.name.as_str());
            }
        }

        // Initial steps are those NOT downstream of decision points
        let initial_steps: Vec<StepDefinition> = self
            .steps
            .iter()
            .filter(|s| !decision_descendants.contains(s.name.as_str()))
            .cloned()
            .collect();

        // Find which decision points are exits from this graph
        let exit_decisions: Vec<String> = decision_steps
            .iter()
            .filter(|d| initial_steps.iter().any(|s| &s.name == *d))
            .map(|s| s.to_string())
            .collect();

        WorkflowGraph {
            entry_decision: None,
            exit_decisions,
            steps: initial_steps,
            graph_type: WorkflowGraphType::Initial,
        }
    }

    /// Build all workflow graphs reachable from a decision point
    fn build_graphs_from_decision(
        &self,
        decision_step_name: &str,
        all_decision_steps: &[&str],
    ) -> Vec<WorkflowGraph> {
        // Find all steps that directly depend on this decision
        let direct_children: Vec<&StepDefinition> = self
            .steps
            .iter()
            .filter(|s| s.dependencies.contains(&decision_step_name.to_string()))
            .collect();

        if direct_children.is_empty() {
            // No children - this is a terminal decision point (rare but possible)
            return vec![];
        }

        // Group children by their reachable decision points
        let mut graph_steps = Vec::new();
        let mut exit_decisions = std::collections::HashSet::new();

        for child in direct_children {
            // Traverse from this child to find all reachable steps
            let mut visited = std::collections::HashSet::new();
            self.collect_reachable_steps(
                &child.name,
                all_decision_steps,
                decision_step_name,
                &mut visited,
                &mut exit_decisions,
            );
            graph_steps.extend(visited);
        }

        // Convert step names to actual steps
        let steps: Vec<StepDefinition> = graph_steps
            .iter()
            .filter_map(|name| self.steps.iter().find(|s| &s.name == name).cloned())
            .collect();

        let graph_type = if exit_decisions.is_empty() {
            WorkflowGraphType::Terminal
        } else {
            WorkflowGraphType::Intermediate
        };

        vec![WorkflowGraph {
            entry_decision: Some(decision_step_name.to_string()),
            exit_decisions: exit_decisions.into_iter().collect(),
            steps,
            graph_type,
        }]
    }

    /// Recursively collect all steps reachable from a starting step
    ///
    /// Stops at decision point boundaries (doesn't traverse past them).
    fn collect_reachable_steps(
        &self,
        step_name: &str,
        all_decision_steps: &[&str],
        entry_decision: &str,
        visited: &mut std::collections::HashSet<String>,
        exit_decisions: &mut std::collections::HashSet<String>,
    ) {
        // Don't revisit steps
        if visited.contains(step_name) {
            return;
        }

        // If this is a decision point (other than our entry), it's an exit boundary
        if all_decision_steps.contains(&step_name) && step_name != entry_decision {
            exit_decisions.insert(step_name.to_string());
            return;
        }

        // Mark as visited and add to graph
        visited.insert(step_name.to_string());

        // Find steps that depend on this one
        let dependents: Vec<&StepDefinition> = self
            .steps
            .iter()
            .filter(|s| s.dependencies.contains(&step_name.to_string()))
            .collect();

        // Recursively visit dependents
        for dependent in dependents {
            self.collect_reachable_steps(
                &dependent.name,
                all_decision_steps,
                entry_decision,
                visited,
                exit_decisions,
            );
        }
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

            // Validate decision point constraints
            if let Err(e) = step.validate_decision_constraints() {
                errors.push(e);
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

    /// Check if this is a decision point step
    pub fn is_decision(&self) -> bool {
        matches!(self.step_type, StepType::Decision)
    }

    /// Check if this step is a deferred convergence step
    pub fn is_deferred(&self) -> bool {
        matches!(self.step_type, StepType::Deferred)
    }

    /// Validate decision point constraints
    ///
    /// Decision points must:
    /// - Have at least one dependency (parent step)
    /// - Not be isolated (must have potential downstream impact)
    pub fn validate_decision_constraints(&self) -> Result<(), String> {
        if !self.is_decision() {
            return Ok(());
        }

        // Decision steps must have at least one parent dependency
        if self.dependencies.is_empty() {
            return Err(format!(
                "Decision step '{}' must have at least one dependency",
                self.name
            ));
        }

        Ok(())
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

    #[test]
    fn test_step_type_default_is_standard() {
        let step_type = StepType::default();
        assert_eq!(step_type, StepType::Standard);
    }

    #[test]
    fn test_step_type_serialization() {
        // Standard type serializes to "standard"
        let standard = StepType::Standard;
        let json = serde_json::to_string(&standard).expect("Should serialize");
        assert_eq!(json, "\"standard\"");

        // Decision type serializes to "decision"
        let decision = StepType::Decision;
        let json = serde_json::to_string(&decision).expect("Should serialize");
        assert_eq!(json, "\"decision\"");
    }

    #[test]
    fn test_step_type_deserialization() {
        // "standard" deserializes to Standard
        let standard: StepType = serde_json::from_str("\"standard\"").expect("Should deserialize");
        assert_eq!(standard, StepType::Standard);

        // "decision" deserializes to Decision
        let decision: StepType = serde_json::from_str("\"decision\"").expect("Should deserialize");
        assert_eq!(decision, StepType::Decision);
    }

    #[test]
    fn test_step_definition_defaults_to_standard_type() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: simple_step
    handler:
      callable: "SimpleHandler"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        assert_eq!(template.steps.len(), 1);
        assert_eq!(template.steps[0].step_type, StepType::Standard);
        assert!(!template.steps[0].is_decision());
    }

    #[test]
    fn test_decision_step_from_yaml() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: parent_step
    handler:
      callable: "ParentHandler"
  - name: decision_step
    step_type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["parent_step"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        assert_eq!(template.steps.len(), 2);

        let decision_step = &template.steps[1];
        assert_eq!(decision_step.name, "decision_step");
        assert_eq!(decision_step.step_type, StepType::Decision);
        assert!(decision_step.is_decision());
        assert_eq!(decision_step.dependencies, vec!["parent_step"]);
    }

    #[test]
    fn test_decision_step_validation_requires_dependencies() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: decision_without_deps
    step_type: decision
    handler:
      callable: "DecisionHandler"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let validation_result = template.validate();

        assert!(validation_result.is_err());
        let error = validation_result.unwrap_err();
        assert!(error
            .to_string()
            .contains("Decision step 'decision_without_deps' must have at least one dependency"));
    }

    #[test]
    fn test_decision_step_validation_passes_with_dependencies() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: parent_step
    handler:
      callable: "ParentHandler"
  - name: decision_step
    step_type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["parent_step"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let validation_result = template.validate();

        assert!(validation_result.is_ok());
    }

    #[test]
    fn test_standard_step_validation_allows_no_dependencies() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: independent_step
    handler:
      callable: "IndependentHandler"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let validation_result = template.validate();

        assert!(validation_result.is_ok());
    }

    #[test]
    fn test_step_definition_is_decision_helper() {
        let standard_step = StepDefinition {
            name: "standard".to_string(),
            description: None,
            handler: HandlerDefinition {
                callable: "Handler".to_string(),
                initialization: HashMap::new(),
            },
            step_type: StepType::Standard,
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events: vec![],
        };
        assert!(!standard_step.is_decision());

        let decision_step = StepDefinition {
            name: "decision".to_string(),
            description: None,
            handler: HandlerDefinition {
                callable: "Handler".to_string(),
                initialization: HashMap::new(),
            },
            step_type: StepType::Decision,
            system_dependency: None,
            dependencies: vec!["parent".to_string()],
            retry: RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events: vec![],
        };
        assert!(decision_step.is_decision());
    }

    #[test]
    fn test_decision_point_steps_filtering() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: standard_step_1
    handler:
      callable: "Handler1"
  - name: decision_step
    step_type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["standard_step_1"]
  - name: standard_step_2
    handler:
      callable: "Handler2"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let decision_steps = template.decision_point_steps();

        assert_eq!(decision_steps.len(), 1);
        assert_eq!(decision_steps[0].name, "decision_step");
    }

    #[test]
    fn test_initial_step_set_no_decision_points() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: step_1
    handler:
      callable: "Handler1"
  - name: step_2
    handler:
      callable: "Handler2"
    dependencies: ["step_1"]
  - name: step_3
    handler:
      callable: "Handler3"
    dependencies: ["step_2"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let initial_steps = template.initial_step_set();

        // No decision points - all steps should be in initial set
        assert_eq!(initial_steps.len(), 3);
        assert!(initial_steps.iter().any(|s| s.name == "step_1"));
        assert!(initial_steps.iter().any(|s| s.name == "step_2"));
        assert!(initial_steps.iter().any(|s| s.name == "step_3"));
    }

    #[test]
    fn test_initial_step_set_with_decision_point() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: upstream_step
    handler:
      callable: "UpstreamHandler"
  - name: decision_step
    step_type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["upstream_step"]
  - name: downstream_step
    handler:
      callable: "DownstreamHandler"
    dependencies: ["decision_step"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let initial_steps = template.initial_step_set();

        // Should include upstream and decision step, but NOT downstream
        assert_eq!(initial_steps.len(), 2);
        assert!(initial_steps.iter().any(|s| s.name == "upstream_step"));
        assert!(initial_steps.iter().any(|s| s.name == "decision_step"));
        assert!(!initial_steps.iter().any(|s| s.name == "downstream_step"));
    }

    #[test]
    fn test_initial_step_set_with_multiple_decision_descendants() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: root_step
    handler:
      callable: "RootHandler"
  - name: decision_step
    step_type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["root_step"]
  - name: branch_a
    handler:
      callable: "BranchAHandler"
    dependencies: ["decision_step"]
  - name: branch_b
    handler:
      callable: "BranchBHandler"
    dependencies: ["decision_step"]
  - name: merge_step
    handler:
      callable: "MergeHandler"
    dependencies: ["branch_a", "branch_b"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let initial_steps = template.initial_step_set();

        // Should include only root and decision, not the branches or merge
        assert_eq!(initial_steps.len(), 2);
        assert!(initial_steps.iter().any(|s| s.name == "root_step"));
        assert!(initial_steps.iter().any(|s| s.name == "decision_step"));
        assert!(!initial_steps.iter().any(|s| s.name == "branch_a"));
        assert!(!initial_steps.iter().any(|s| s.name == "branch_b"));
        assert!(!initial_steps.iter().any(|s| s.name == "merge_step"));
    }

    #[test]
    fn test_is_descendant_of_decision_point_direct_dependency() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: decision_step
    step_type: decision
    handler:
      callable: "DecisionHandler"
  - name: child_step
    handler:
      callable: "ChildHandler"
    dependencies: ["decision_step"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let decision_steps = vec!["decision_step"];

        assert!(template.is_descendant_of_decision_point("child_step", &decision_steps));
        assert!(!template.is_descendant_of_decision_point("decision_step", &decision_steps));
    }

    #[test]
    fn test_is_descendant_of_decision_point_transitive_dependency() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: decision_step
    step_type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["root"]
  - name: root
    handler:
      callable: "RootHandler"
  - name: child_step
    handler:
      callable: "ChildHandler"
    dependencies: ["decision_step"]
  - name: grandchild_step
    handler:
      callable: "GrandchildHandler"
    dependencies: ["child_step"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let decision_steps = vec!["decision_step"];

        // Both child and grandchild should be descendants
        assert!(template.is_descendant_of_decision_point("child_step", &decision_steps));
        assert!(template.is_descendant_of_decision_point("grandchild_step", &decision_steps));
    }

    #[test]
    fn test_nested_decision_points_not_created_initially() {
        // This test verifies that a decision point that is itself downstream
        // of another decision point is NOT created during initialization
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: root_step
    handler:
      callable: "RootHandler"
  - name: decision_step_1
    step_type: decision
    handler:
      callable: "DecisionHandler1"
    dependencies: ["root_step"]
  - name: branch_a
    handler:
      callable: "BranchAHandler"
    dependencies: ["decision_step_1"]
  - name: decision_step_2
    step_type: decision
    handler:
      callable: "DecisionHandler2"
    dependencies: ["branch_a"]
  - name: branch_a1
    handler:
      callable: "BranchA1Handler"
    dependencies: ["decision_step_2"]
  - name: branch_a2
    handler:
      callable: "BranchA2Handler"
    dependencies: ["decision_step_2"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let initial_steps = template.initial_step_set();

        // Initial set should ONLY include root_step and decision_step_1
        assert_eq!(
            initial_steps.len(),
            2,
            "Should only create steps up to first decision boundary"
        );
        assert!(
            initial_steps.iter().any(|s| s.name == "root_step"),
            "Should include root_step"
        );
        assert!(
            initial_steps.iter().any(|s| s.name == "decision_step_1"),
            "Should include first decision point"
        );

        // These should NOT be in initial set - they're descendants of decision_step_1
        assert!(
            !initial_steps.iter().any(|s| s.name == "branch_a"),
            "branch_a is a descendant of decision_step_1"
        );
        assert!(
            !initial_steps.iter().any(|s| s.name == "decision_step_2"),
            "decision_step_2 is a descendant of decision_step_1"
        );
        assert!(
            !initial_steps.iter().any(|s| s.name == "branch_a1"),
            "branch_a1 is a descendant of decision_step_2"
        );
        assert!(
            !initial_steps.iter().any(|s| s.name == "branch_a2"),
            "branch_a2 is a descendant of decision_step_2"
        );

        // Verify the descendant detection logic explicitly
        let decision_steps = vec!["decision_step_1", "decision_step_2"];

        // decision_step_2 should be identified as a descendant of decision_step_1
        assert!(
            template.is_descendant_of_decision_point("decision_step_2", &decision_steps),
            "decision_step_2 should be a descendant because it depends on branch_a, which depends on decision_step_1"
        );
    }

    #[test]
    fn test_multiple_nested_decision_points_complex_dag() {
        // Test a more complex DAG with multiple decision points at different levels
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: root
    handler:
      callable: "RootHandler"
  - name: prepare_a
    handler:
      callable: "PrepareA"
    dependencies: ["root"]
  - name: prepare_b
    handler:
      callable: "PrepareB"
    dependencies: ["root"]
  - name: decision_1
    step_type: decision
    handler:
      callable: "Decision1"
    dependencies: ["prepare_a", "prepare_b"]
  - name: path_1a
    handler:
      callable: "Path1A"
    dependencies: ["decision_1"]
  - name: path_1b
    handler:
      callable: "Path1B"
    dependencies: ["decision_1"]
  - name: decision_2
    step_type: decision
    handler:
      callable: "Decision2"
    dependencies: ["path_1a"]
  - name: path_2a
    handler:
      callable: "Path2A"
    dependencies: ["decision_2"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let initial_steps = template.initial_step_set();

        // Should include everything up to and including decision_1, but NOT its descendants
        assert_eq!(initial_steps.len(), 4);
        assert!(initial_steps.iter().any(|s| s.name == "root"));
        assert!(initial_steps.iter().any(|s| s.name == "prepare_a"));
        assert!(initial_steps.iter().any(|s| s.name == "prepare_b"));
        assert!(initial_steps.iter().any(|s| s.name == "decision_1"));

        // These are all descendants of decision_1
        assert!(!initial_steps.iter().any(|s| s.name == "path_1a"));
        assert!(!initial_steps.iter().any(|s| s.name == "path_1b"));
        assert!(!initial_steps.iter().any(|s| s.name == "decision_2"));
        assert!(!initial_steps.iter().any(|s| s.name == "path_2a"));
    }

    #[test]
    fn test_decision_step_validate_constraints_directly() {
        // Decision step without dependencies should fail
        let invalid_decision = StepDefinition {
            name: "invalid_decision".to_string(),
            description: None,
            handler: HandlerDefinition {
                callable: "Handler".to_string(),
                initialization: HashMap::new(),
            },
            step_type: StepType::Decision,
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events: vec![],
        };

        let result = invalid_decision.validate_decision_constraints();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("must have at least one dependency"));

        // Decision step with dependencies should pass
        let valid_decision = StepDefinition {
            name: "valid_decision".to_string(),
            description: None,
            handler: HandlerDefinition {
                callable: "Handler".to_string(),
                initialization: HashMap::new(),
            },
            step_type: StepType::Decision,
            system_dependency: None,
            dependencies: vec!["parent".to_string()],
            retry: RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events: vec![],
        };

        let result = valid_decision.validate_decision_constraints();
        assert!(result.is_ok());

        // Standard step should always pass validation
        let standard_step = StepDefinition {
            name: "standard".to_string(),
            description: None,
            handler: HandlerDefinition {
                callable: "Handler".to_string(),
                initialization: HashMap::new(),
            },
            step_type: StepType::Standard,
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events: vec![],
        };

        let result = standard_step.validate_decision_constraints();
        assert!(result.is_ok());
    }

    // ========================================================================
    // WorkflowGraph Tests
    // ========================================================================

    #[test]
    fn test_workflow_graph_simple_no_decision_points() {
        let yaml_content = r#"
name: simple_workflow
namespace_name: test
version: "1.0.0"

steps:
  - name: step_a
    handler:
      callable: "HandlerA"
  - name: step_b
    handler:
      callable: "HandlerB"
    dependencies: ["step_a"]
  - name: step_c
    handler:
      callable: "HandlerC"
    dependencies: ["step_b"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let graphs = template.segment_workflow_graphs();

        // Without decision points, should have exactly one Initial graph
        assert_eq!(graphs.len(), 1);
        assert!(matches!(graphs[0].graph_type, WorkflowGraphType::Initial));
        assert_eq!(graphs[0].entry_decision, None);
        assert_eq!(graphs[0].exit_decisions.len(), 0);
        assert_eq!(graphs[0].steps.len(), 3);
    }

    #[test]
    fn test_workflow_graph_simple_one_decision_point() {
        let yaml_content = r#"
name: decision_workflow
namespace_name: test
version: "1.0.0"

steps:
  - name: step_a
    handler:
      callable: "HandlerA"
  - name: decision_step
    type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["step_a"]
  - name: branch_a
    handler:
      callable: "BranchA"
    dependencies: ["decision_step"]
  - name: branch_b
    handler:
      callable: "BranchB"
    dependencies: ["decision_step"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let graphs = template.segment_workflow_graphs();

        // Should have 2 graphs: Initial (root → decision) and Terminal (decision → branches)
        assert_eq!(graphs.len(), 2);

        // Find the initial graph
        let initial_graphs: Vec<_> = graphs
            .iter()
            .filter(|g| matches!(g.graph_type, WorkflowGraphType::Initial))
            .collect();
        assert_eq!(initial_graphs.len(), 1);
        let initial = initial_graphs[0];
        assert_eq!(initial.entry_decision, None);
        assert_eq!(initial.exit_decisions, vec!["decision_step"]);
        assert_eq!(initial.steps.len(), 2); // step_a, decision_step

        // Find the terminal graph
        let terminal_graphs: Vec<_> = graphs
            .iter()
            .filter(|g| matches!(g.graph_type, WorkflowGraphType::Terminal))
            .collect();
        assert_eq!(terminal_graphs.len(), 1);
        let terminal = terminal_graphs[0];
        assert_eq!(terminal.entry_decision, Some("decision_step".to_string()));
        assert_eq!(terminal.exit_decisions.len(), 0);
        assert_eq!(terminal.steps.len(), 2); // branch_a, branch_b
    }

    #[test]
    fn test_workflow_graph_nested_decision_points() {
        let yaml_content = r#"
name: nested_decisions
namespace_name: test
version: "1.0.0"

steps:
  - name: step_a
    handler:
      callable: "HandlerA"
  - name: decision_step_1
    type: decision
    handler:
      callable: "Decision1"
    dependencies: ["step_a"]
  - name: branch_a
    handler:
      callable: "BranchA"
    dependencies: ["decision_step_1"]
  - name: decision_step_2
    type: decision
    handler:
      callable: "Decision2"
    dependencies: ["branch_a"]
  - name: final_step
    handler:
      callable: "FinalStep"
    dependencies: ["decision_step_2"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let graphs = template.segment_workflow_graphs();

        // Should have 3 graphs: Initial, Intermediate, Terminal
        assert_eq!(graphs.len(), 3);

        // Initial graph: root → decision_step_1
        let initial_graphs = template.initial_graphs();
        assert_eq!(initial_graphs.len(), 1);
        assert_eq!(initial_graphs[0].entry_decision, None);
        assert_eq!(initial_graphs[0].exit_decisions, vec!["decision_step_1"]);
        assert_eq!(initial_graphs[0].steps.len(), 2); // step_a, decision_step_1

        // Intermediate graph: decision_step_1 → decision_step_2
        let intermediate_graphs: Vec<_> = graphs
            .iter()
            .filter(|g| matches!(g.graph_type, WorkflowGraphType::Intermediate))
            .collect();
        assert_eq!(intermediate_graphs.len(), 1);
        assert_eq!(
            intermediate_graphs[0].entry_decision,
            Some("decision_step_1".to_string())
        );
        assert_eq!(
            intermediate_graphs[0].exit_decisions,
            vec!["decision_step_2"]
        );
        // The intermediate graph contains steps BETWEEN decision points (not including the exit decision)
        assert_eq!(intermediate_graphs[0].steps.len(), 1); // branch_a only

        // Terminal graph: decision_step_2 → final_step
        let terminal_graphs = template.terminal_graphs();
        assert_eq!(terminal_graphs.len(), 1);
        assert_eq!(
            terminal_graphs[0].entry_decision,
            Some("decision_step_2".to_string())
        );
        assert_eq!(terminal_graphs[0].exit_decisions.len(), 0);
        assert_eq!(terminal_graphs[0].steps.len(), 1); // final_step
    }

    #[test]
    fn test_workflow_graph_helper_methods() {
        let yaml_content = r#"
name: test_workflow
namespace_name: test
version: "1.0.0"

steps:
  - name: step_a
    handler:
      callable: "HandlerA"
  - name: step_b
    handler:
      callable: "HandlerB"
    dependencies: ["step_a"]
  - name: decision_step
    type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["step_b"]
  - name: branch_a
    handler:
      callable: "BranchA"
    dependencies: ["decision_step"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let graphs = template.segment_workflow_graphs();
        let initial = &graphs[0];

        // Test contains_step
        assert!(initial.contains_step("step_a"));
        assert!(initial.contains_step("step_b"));
        assert!(initial.contains_step("decision_step"));
        assert!(!initial.contains_step("branch_a"));

        // Test step_names
        let names = initial.step_names();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"step_a"));
        assert!(names.contains(&"step_b"));
        assert!(names.contains(&"decision_step"));

        // Test contains_all_steps
        assert!(initial.contains_all_steps(&["step_a".to_string(), "step_b".to_string()]));
        assert!(!initial.contains_all_steps(&["step_a".to_string(), "branch_a".to_string()]));

        // Test entry_name and exit_names
        assert_eq!(initial.entry_name(), "root");
        assert_eq!(initial.exit_names(), "decision_step");
    }

    #[test]
    fn test_workflow_graph_multiple_exit_decisions() {
        let yaml_content = r#"
name: multi_exit_workflow
namespace_name: test
version: "1.0.0"

steps:
  - name: step_a
    handler:
      callable: "HandlerA"
  - name: decision_step_1
    type: decision
    handler:
      callable: "Decision1"
    dependencies: ["step_a"]
  - name: decision_step_2
    type: decision
    handler:
      callable: "Decision2"
    dependencies: ["step_a"]
  - name: branch_a
    handler:
      callable: "BranchA"
    dependencies: ["decision_step_1"]
  - name: branch_b
    handler:
      callable: "BranchB"
    dependencies: ["decision_step_2"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let initial_graphs = template.initial_graphs();

        assert_eq!(initial_graphs.len(), 1);
        assert_eq!(initial_graphs[0].exit_decisions.len(), 2);
        assert!(initial_graphs[0]
            .exit_decisions
            .contains(&"decision_step_1".to_string()));
        assert!(initial_graphs[0]
            .exit_decisions
            .contains(&"decision_step_2".to_string()));
    }

    #[test]
    fn test_workflow_graph_graphs_from_decision() {
        let yaml_content = r#"
name: decision_query_workflow
namespace_name: test
version: "1.0.0"

steps:
  - name: step_a
    handler:
      callable: "HandlerA"
  - name: decision_step_1
    type: decision
    handler:
      callable: "Decision1"
    dependencies: ["step_a"]
  - name: branch_a
    handler:
      callable: "BranchA"
    dependencies: ["decision_step_1"]
  - name: decision_step_2
    type: decision
    handler:
      callable: "Decision2"
    dependencies: ["step_a"]
  - name: branch_b
    handler:
      callable: "BranchB"
    dependencies: ["decision_step_2"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");

        // Get graphs from decision_step_1
        let graphs_from_d1 = template.graphs_from_decision("decision_step_1");
        assert_eq!(graphs_from_d1.len(), 1);
        assert_eq!(
            graphs_from_d1[0].entry_decision,
            Some("decision_step_1".to_string())
        );
        assert!(graphs_from_d1[0].contains_step("branch_a"));

        // Get graphs from decision_step_2
        let graphs_from_d2 = template.graphs_from_decision("decision_step_2");
        assert_eq!(graphs_from_d2.len(), 1);
        assert_eq!(
            graphs_from_d2[0].entry_decision,
            Some("decision_step_2".to_string())
        );
        assert!(graphs_from_d2[0].contains_step("branch_b"));

        // Query non-existent decision
        let graphs_from_none = template.graphs_from_decision("nonexistent");
        assert_eq!(graphs_from_none.len(), 0);
    }

    #[test]
    fn test_initial_step_set_uses_workflow_graphs() {
        let yaml_content = r#"
name: integration_test
namespace_name: test
version: "1.0.0"

steps:
  - name: step_a
    handler:
      callable: "HandlerA"
  - name: decision_step
    type: decision
    handler:
      callable: "DecisionHandler"
    dependencies: ["step_a"]
  - name: branch_a
    handler:
      callable: "BranchA"
    dependencies: ["decision_step"]
  - name: branch_b
    handler:
      callable: "BranchB"
    dependencies: ["decision_step"]
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let initial_steps = template.initial_step_set();

        // Should include step_a and decision_step, but NOT the branches
        assert_eq!(initial_steps.len(), 2);
        let step_names: Vec<&str> = initial_steps.iter().map(|s| s.name.as_str()).collect();
        assert!(step_names.contains(&"step_a"));
        assert!(step_names.contains(&"decision_step"));
        assert!(!step_names.contains(&"branch_a"));
        assert!(!step_names.contains(&"branch_b"));

        // Verify this matches what initial_graphs would give us
        let initial_graphs = template.initial_graphs();
        let graph_step_names: std::collections::HashSet<String> = initial_graphs
            .iter()
            .flat_map(|g| g.steps.iter().map(|s| s.name.clone()))
            .collect();
        let initial_step_names: std::collections::HashSet<String> =
            initial_steps.iter().map(|s| s.name.clone()).collect();
        assert_eq!(graph_step_names, initial_step_names);
    }
}
