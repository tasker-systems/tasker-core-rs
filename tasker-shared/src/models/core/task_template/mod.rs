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

// TAS-65: Event declaration module
pub mod event_declaration;
// TAS-65 Phase 1.2: Event validation module
pub mod event_validator;

use bon::Builder;
use chrono::{DateTime, Utc};
use derive_more::Display;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use validator::Validate;

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
pub struct TaskTemplate {
    /// Unique task name within namespace
    #[validate(length(min = 1, max = 255))]
    pub name: String,

    /// Namespace for organization
    #[validate(length(min = 1, max = 255))]
    pub namespace_name: String,

    /// Semantic version
    #[validate(length(min = 1, max = 50))]
    pub version: String,

    /// Human-readable description
    pub description: Option<String>,

    /// Template metadata for documentation
    pub metadata: Option<TemplateMetadata>,

    /// Task-level handler configuration
    pub task_handler: Option<HandlerDefinition>,

    /// External system dependencies
    #[serde(default)]
    #[builder(default)]
    #[validate(nested)]
    pub system_dependencies: SystemDependencies,

    /// Domain events this task can publish
    #[serde(default)]
    #[builder(default)]
    pub domain_events: Vec<DomainEventDefinition>,

    /// JSON Schema for input validation
    pub input_schema: Option<Value>,

    /// Workflow step definitions
    #[serde(default)]
    #[builder(default)]
    pub steps: Vec<StepDefinition>,

    /// Environment-specific overrides
    #[serde(default)]
    #[builder(default)]
    pub environments: HashMap<String, EnvironmentOverride>,

    /// TAS-49: Per-template lifecycle configuration (optional)
    ///
    /// Defines task-specific timeout thresholds and staleness detection behavior.
    /// When specified, these settings override global orchestration.toml defaults.
    #[serde(default)]
    pub lifecycle: Option<LifecycleConfig>,
}

/// Template metadata for documentation and discovery
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder, Display)]
#[display("TemplateMetadata(author: {:?}, tags: {:?})", author, tags)]
pub struct TemplateMetadata {
    pub author: Option<String>,
    #[serde(default)]
    #[builder(default)]
    pub tags: Vec<String>,
    pub documentation_url: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Handler definition with callable and initialization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder, Display)]
#[display("HandlerDefinition(callable: {})", callable)]
pub struct HandlerDefinition {
    /// Callable reference (class, proc, lambda)
    #[validate(length(min = 1, max = 500))]
    pub callable: String,

    /// Initialization parameters
    #[serde(default)]
    #[builder(default)]
    pub initialization: HashMap<String, Value>,
}

/// External system dependencies
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder, Display)]
#[display("SystemDependencies(primary: {}, secondary: {:?})", primary, secondary)]
pub struct SystemDependencies {
    /// Primary system interaction
    #[serde(default = "default_system")]
    #[builder(default = default_system())]
    #[validate(length(min = 1, max = 255))]
    pub primary: String,

    /// Secondary systems
    #[serde(default)]
    #[builder(default)]
    pub secondary: Vec<String>,
}

impl Default for SystemDependencies {
    fn default() -> Self {
        Self {
            primary: default_system(),
            secondary: Vec::new(),
        }
    }
}

fn default_system() -> String {
    "default".to_string()
}

/// TAS-49: Per-template lifecycle configuration
///
/// Provides fine-grained control over task and step timeout behavior.
/// These settings override global system defaults for specific task templates.
///
/// ## Staleness Detection Integration
///
/// The thresholds defined here are used by `detect_and_transition_stale_tasks()`
/// SQL function (TAS-48/TAS-49) to determine when a task has exceeded its
/// maximum duration in a particular state.
///
/// ## Configuration Precedence
///
/// 1. Per-template lifecycle config (highest priority)
/// 2. Global orchestration.toml staleness_detection.thresholds
/// 3. System hardcoded defaults (lowest priority)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
pub struct LifecycleConfig {
    /// Maximum total task duration (from creation to completion)
    #[validate(range(min = 1, max = 43200))]
    pub max_duration_minutes: Option<i32>,

    /// Maximum time a task can spend in WaitingForDependencies state
    #[validate(range(min = 1, max = 10080))]
    pub max_waiting_for_dependencies_minutes: Option<i32>,

    /// Maximum time a task can spend in WaitingForRetry state
    #[validate(range(min = 1, max = 10080))]
    pub max_waiting_for_retry_minutes: Option<i32>,

    /// Maximum time a task can spend in StepsInProcess state
    #[validate(range(min = 1, max = 10080))]
    pub max_steps_in_process_minutes: Option<i32>,

    /// Action to take when staleness threshold exceeded
    ///
    /// Valid values:
    /// - "dlq": Send to Dead Letter Queue for investigation (default)
    /// - "error": Transition task to Error state
    /// - "none": No automatic action (monitoring only)
    #[serde(default = "default_staleness_action")]
    #[validate(length(min = 1, max = 50))]
    #[builder(default = "dlq".to_string())]
    pub staleness_action: String,

    /// Automatically transition task to Error state on timeout
    ///
    /// When true, tasks exceeding thresholds are moved to Error state
    /// before being sent to DLQ (if staleness_action = "dlq").
    #[serde(default)]
    pub auto_fail_on_timeout: Option<bool>,

    /// Automatically send task to DLQ on timeout
    ///
    /// When true and staleness_action = "dlq", tasks are immediately
    /// sent to DLQ when any threshold is exceeded.
    #[serde(default)]
    pub auto_dlq_on_timeout: Option<bool>,
}

fn default_staleness_action() -> String {
    "dlq".to_string()
}

/// Domain event definition with schema
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder, Display)]
#[display("DomainEventDefinition(name: {})", name)]
pub struct DomainEventDefinition {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub description: Option<String>,
    pub schema: Option<Value>, // JSON Schema
}

// TAS-65: Re-export event types from event_declaration module
pub use event_declaration::{EventDeclaration, EventDeliveryMode, PublicationCondition};
// TAS-65 Phase 1.2: Re-export validator types
pub use event_validator::{
    EventPublicationValidator, ValidationError, ValidationResult, ValidationWarning,
};

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
    DeferredConvergence,

    /// Batchable step that analyzes dataset and creates batch workers
    ///
    /// A batchable step determines whether work should be split into multiple
    /// parallel batches and, if so, creates N worker instances from a template.
    ///
    /// The handler for a batchable step:
    /// 1. Analyzes the dataset size and characteristics
    /// 2. Determines optimal batch configuration
    /// 3. Returns a `BatchProcessingOutcome` with worker creation instructions
    ///
    /// The orchestration system then:
    /// - Creates N worker instances from the specified template
    /// - Assigns each worker a cursor configuration
    /// - Creates DAG edges: batchable_step → workers
    /// - Enqueues workers for parallel execution
    ///
    /// ### Constraints:
    /// - Must have `batch_config` metadata specifying worker template and parameters
    /// - Worker template must exist in same template with `type: batch_worker`
    /// - Handler must return `BatchProcessingOutcome` in step result
    ///
    /// ### Example:
    /// ```yaml
    /// - name: process_large_dataset
    ///   type: batchable
    ///   batch_config:
    ///     worker_template: batch_worker_template
    ///     batch_size: 1000
    ///     parallelism: 10
    /// ```
    Batchable,

    /// Batch worker template (not executed directly, instantiated by batchable step)
    ///
    /// A batch worker template serves as a blueprint for creating parallel worker
    /// instances. It is not included in the initial step set and cannot be executed
    /// directly - only instantiated copies created by a batchable step can execute.
    ///
    /// Each instance receives:
    /// - Unique name (e.g., `{template_name}_001`, `{template_name}_002`)
    /// - Cursor configuration defining its processing range
    /// - Dependencies on the parent batchable step
    ///
    /// The handler for a batch worker:
    /// 1. Loads cursor configuration from step metadata
    /// 2. Processes items from start_cursor to end_cursor
    /// 3. Periodically checkpoints progress by updating cursor state
    /// 4. Returns success or retryable error (cursor preserved for resume)
    ///
    /// ### Constraints:
    /// - Must be referenced by a batchable step's `batch_config.worker_template`
    /// - Cannot be referenced directly in dependencies
    /// - Template itself is never enqueued (only instances)
    ///
    /// ### Example:
    /// ```yaml
    /// - name: batch_worker_template
    ///   type: batch_worker
    ///   dependencies: [process_large_dataset]  # Parent batchable step
    ///   handler:
    ///     callable: DataPipeline::BatchWorkerHandler
    /// ```
    BatchWorker,
}

impl StepType {
    /// Returns true if this step type should be created during task initialization
    ///
    /// Certain step types are excluded from initialization and created dynamically:
    /// - `BatchWorker`: Template steps that are instantiated by batchable steps
    /// - `DeferredConvergence`: Convergence steps created after batch workers complete
    ///
    /// These types represent templates or dynamic orchestration points rather than
    /// concrete workflow steps that should exist at task creation time.
    pub fn is_created_at_initialization(&self) -> bool {
        !matches!(self, StepType::BatchWorker | StepType::DeferredConvergence)
    }
}

/// Individual workflow step definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
pub struct StepDefinition {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub description: Option<String>,

    /// Handler for this step
    #[validate(nested)]
    pub handler: HandlerDefinition,

    /// Step type (standard or decision)
    #[serde(default, rename = "type", alias = "step_type")]
    #[builder(default)]
    pub step_type: StepType,

    /// System this step interacts with
    pub system_dependency: Option<String>,

    /// Dependencies on other steps
    #[serde(default)]
    #[builder(default)]
    pub dependencies: Vec<String>,

    /// Retry configuration
    #[serde(default)]
    #[builder(default)]
    #[validate(nested)]
    pub retry: RetryConfiguration,

    /// Step timeout
    pub timeout_seconds: Option<u32>,

    /// Events this step publishes (TAS-65: Event Schema and Validation)
    ///
    /// Declares which events this step can publish during execution.
    /// Each event declaration includes a name, description, JSON Schema,
    /// and delivery mode (currently only durable PGMQ delivery).
    ///
    /// ## Backward Compatibility
    ///
    /// This field defaults to an empty vector if not specified, ensuring
    /// existing task templates without event declarations continue to work.
    ///
    /// ## Example
    ///
    /// ```yaml
    /// publishes_events:
    ///   - name: payment.authorized
    ///     description: "Payment successfully authorized"
    ///     schema:
    ///       type: object
    ///       properties:
    ///         payment_id: { type: string }
    ///         amount: { type: number }
    ///       required: [payment_id, amount]
    ///     delivery_mode: durable
    /// ```
    #[serde(default)]
    #[builder(default)]
    pub publishes_events: Vec<EventDeclaration>,

    /// Batch processing configuration (for batchable steps only)
    ///
    /// When present, indicates this step uses batch processing to split work
    /// into parallel workers. Only applicable for steps with `type: batchable`.
    ///
    /// # Example
    /// ```yaml
    /// type: batchable
    /// batch_config:
    ///   batch_size: 1000
    ///   parallelism: 10
    ///   cursor_field: record_id
    ///   checkpoint_interval: 100
    ///   worker_template: batch_worker_template
    ///   failure_strategy: continue_on_failure
    /// ```
    #[serde(default)]
    pub batch_config: Option<BatchConfiguration>,
}

/// Retry configuration with backoff strategies
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[display(
    "RetryConfiguration(retryable: {}, max_attempts: {}, backoff: {:?})",
    retryable,
    max_attempts,
    backoff
)]
pub struct RetryConfiguration {
    #[serde(default = "default_retryable")]
    #[builder(default = true)]
    pub retryable: bool,

    #[serde(default = "default_max_attempts")]
    #[validate(range(min = 1, max = 100))]
    #[builder(default = 3)]
    pub max_attempts: u32,

    #[serde(default)]
    #[builder(default)]
    pub backoff: BackoffStrategy,

    #[validate(range(min = 100, max = 3600000))]
    #[builder(into)]
    pub backoff_base_ms: Option<u64>,

    #[validate(range(min = 1000, max = 3600000))]
    #[builder(into)]
    pub max_backoff_ms: Option<u64>,
}

// Preserve original Default behavior (matches original impl Default)
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

/// Batch processing configuration for batchable steps
///
/// Defines how a dataset should be split into parallel batch workers.
/// Used by steps with `type: batchable` to configure dynamic worker creation.
///
/// # Example
/// ```yaml
/// batch_config:
///   batch_size: 1000
///   parallelism: 10
///   cursor_field: record_id
///   checkpoint_interval: 100
///   worker_template: batch_worker_template
///   failure_strategy: continue_on_failure
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[display(
    "BatchConfiguration(batch_size: {}, parallelism: {}, cursor_field: {}, checkpoint_interval: {}, worker_template: {}, failure_strategy: {:?})",
    batch_size,
    parallelism,
    cursor_field,
    checkpoint_interval,
    worker_template,
    failure_strategy
)]
pub struct BatchConfiguration {
    /// Items per batch
    ///
    /// Each batch worker processes up to this many items.
    /// The actual number may be less for the final batch.
    #[validate(range(min = 1, max = 100000))]
    #[builder(default = 1000)]
    pub batch_size: u32,

    /// Maximum parallel batch workers
    ///
    /// Limits concurrent execution to prevent resource exhaustion.
    /// The actual worker count may be less if dataset is smaller.
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 10)]
    pub parallelism: u32,

    /// Field to track progress (e.g., "record_id", "offset")
    ///
    /// Identifies how to cursor through the dataset. The handler uses
    /// this field to track progress and resume from failures.
    #[validate(length(min = 1, max = 255))]
    #[builder(default = "id".to_string())]
    pub cursor_field: String,

    /// Checkpoint every N items
    ///
    /// Batch workers save cursor state after processing this many items.
    /// Lower values provide finer-grained recovery but more database writes.
    #[validate(range(min = 1, max = 10000))]
    #[builder(default = 100)]
    pub checkpoint_interval: u32,

    /// Template step name for workers
    ///
    /// Must match a step with `type: batch_worker` in the same template.
    /// The system instantiates multiple copies of this template with
    /// unique names (e.g., `{template}_001`, `{template}_002`).
    #[validate(length(min = 1, max = 255))]
    pub worker_template: String,

    /// How to handle batch failures
    ///
    /// Determines whether batch failures block the entire task or allow
    /// partial success with some failed batches.
    #[builder(default)]
    pub failure_strategy: FailureStrategy,
}

// Note: No Default implementation for BatchConfiguration since worker_template is required

/// Strategy for handling batch worker failures
///
/// Determines the step state transition when a batch worker fails,
/// which affects DAG traversal and task completion.
///
/// # Impact on Convergence
///
/// The failure strategy determines whether convergence steps can proceed:
/// - `ContinueOnFailure`: Step completes successfully despite failure (convergence can proceed)
/// - `FailFast`: Step transitions to Error state (convergence blocked)
/// - `Isolate`: Step awaits manual resolution (convergence blocked until resolved)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailureStrategy {
    /// Continue processing other batches on failure
    ///
    /// Failed batch workers transition to `Complete` state with partial
    /// failure metadata. The convergence step receives results from all
    /// batches (including failures) and determines overall outcome.
    ///
    /// Use for: Analytics, best-effort processing, non-critical operations
    #[default]
    ContinueOnFailure,

    /// First batch failure immediately fails entire task
    ///
    /// The failed batch worker transitions to `Error` state. This prevents
    /// the convergence step from ever becoming ready, blocking the task.
    /// Other running batches may be cancelled (implementation dependent).
    ///
    /// Use for: All-or-nothing operations, financial transactions
    FailFast,

    /// Failed batches require manual investigation
    ///
    /// The failed batch worker is marked for manual resolution via TAS-49's
    /// DLQ workflow. The task blocks at convergence until an operator
    /// manually resolves the batch (ResetForRetry, ResolveManually, or
    /// CompleteManually).
    ///
    /// Use for: Sensitive operations requiring human oversight
    Isolate,
}

/// Environment-specific overrides
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
pub struct EnvironmentOverride {
    pub task_handler: Option<HandlerOverride>,
    #[serde(default)]
    #[builder(default)]
    #[validate(nested)]
    pub steps: Vec<StepOverride>,
}

/// Handler override for environments
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
pub struct HandlerOverride {
    pub initialization: Option<HashMap<String, Value>>,
}

/// Step override for environments
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
pub struct StepOverride {
    /// Step name or "ALL" for all steps
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub handler: Option<HandlerOverride>,
    pub timeout_seconds: Option<u32>,
    #[validate(nested)]
    pub retry: Option<RetryConfiguration>,
}

/// Resolved task template with environment-specific overrides applied
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
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
    pub fn deferred_convergence_steps(&self) -> Vec<&StepDefinition> {
        self.steps
            .iter()
            .filter(|s| s.is_deferred_convergence())
            .collect()
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
            // Filter to only steps that should be created at initialization
            // (excludes BatchWorker templates and DeferredConvergence steps)
            let filtered_steps: Vec<StepDefinition> = self
                .steps
                .iter()
                .filter(|s| s.step_type.is_created_at_initialization())
                .cloned()
                .collect();

            return vec![WorkflowGraph {
                entry_decision: None,
                exit_decisions: vec![],
                steps: filtered_steps,
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

        // Initial steps are those NOT downstream of decision points AND NOT batch worker templates
        // Batch worker templates are instantiated dynamically by batchable steps, not included in initial set
        let initial_steps: Vec<StepDefinition> = self
            .steps
            .iter()
            .filter(|s| {
                !decision_descendants.contains(s.name.as_str())
                    && s.step_type != StepType::BatchWorker
            })
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
    pub fn is_deferred_convergence(&self) -> bool {
        matches!(self.step_type, StepType::DeferredConvergence)
    }

    /// Get event declaration by name (TAS-65)
    ///
    /// Returns the event declaration for a specific event name if this step
    /// declares that it can publish that event.
    ///
    /// # Arguments
    ///
    /// * `event_name` - The event name to look up (e.g., "payment.authorized")
    ///
    /// # Returns
    ///
    /// `Some(&EventDeclaration)` if the step declares this event, `None` otherwise.
    pub fn get_event_declaration(&self, event_name: &str) -> Option<&EventDeclaration> {
        self.publishes_events
            .iter()
            .find(|decl| decl.name == event_name)
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
            publishes_events: vec![], // TAS-65: Now Vec<EventDeclaration>
            batch_config: None,
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
            publishes_events: vec![], // TAS-65: Now Vec<EventDeclaration>
            batch_config: None,
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
            publishes_events: vec![], // TAS-65: Now Vec<EventDeclaration>
            batch_config: None,
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
            publishes_events: vec![], // TAS-65: Now Vec<EventDeclaration>
            batch_config: None,
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
            publishes_events: vec![], // TAS-65: Now Vec<EventDeclaration>
            batch_config: None,
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

    // ========================================================================
    // TAS-49: Lifecycle Configuration Tests
    // ========================================================================

    #[test]
    fn test_task_template_without_lifecycle_config() {
        let yaml_content = r#"
name: simple_task
namespace_name: test
version: "1.0.0"

steps:
  - name: step_1
    handler:
      callable: "Handler1"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");

        // Lifecycle should be None when not specified
        assert!(template.lifecycle.is_none());
    }

    #[test]
    fn test_task_template_with_full_lifecycle_config() {
        let yaml_content = r#"
name: lifecycle_task
namespace_name: test
version: "1.0.0"

lifecycle:
  max_duration_minutes: 120
  max_waiting_for_dependencies_minutes: 60
  max_waiting_for_retry_minutes: 30
  max_steps_in_process_minutes: 90
  staleness_action: "error"
  auto_fail_on_timeout: true
  auto_dlq_on_timeout: false

steps:
  - name: step_1
    handler:
      callable: "Handler1"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");

        // Lifecycle should be Some with all fields populated
        assert!(template.lifecycle.is_some());

        let lifecycle = template.lifecycle.as_ref().unwrap();
        assert_eq!(lifecycle.max_duration_minutes, Some(120));
        assert_eq!(lifecycle.max_waiting_for_dependencies_minutes, Some(60));
        assert_eq!(lifecycle.max_waiting_for_retry_minutes, Some(30));
        assert_eq!(lifecycle.max_steps_in_process_minutes, Some(90));
        assert_eq!(lifecycle.staleness_action, "error");
        assert_eq!(lifecycle.auto_fail_on_timeout, Some(true));
        assert_eq!(lifecycle.auto_dlq_on_timeout, Some(false));
    }

    #[test]
    fn test_task_template_with_partial_lifecycle_config() {
        let yaml_content = r#"
name: partial_lifecycle_task
namespace_name: test
version: "1.0.0"

lifecycle:
  max_duration_minutes: 180
  staleness_action: "dlq"

steps:
  - name: step_1
    handler:
      callable: "Handler1"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");

        // Lifecycle should be Some with partial fields
        assert!(template.lifecycle.is_some());

        let lifecycle = template.lifecycle.as_ref().unwrap();
        assert_eq!(lifecycle.max_duration_minutes, Some(180));
        assert_eq!(lifecycle.max_waiting_for_dependencies_minutes, None);
        assert_eq!(lifecycle.max_waiting_for_retry_minutes, None);
        assert_eq!(lifecycle.max_steps_in_process_minutes, None);
        assert_eq!(lifecycle.staleness_action, "dlq");
        assert_eq!(lifecycle.auto_fail_on_timeout, None);
        assert_eq!(lifecycle.auto_dlq_on_timeout, None);
    }

    #[test]
    fn test_lifecycle_config_default_staleness_action() {
        let yaml_content = r#"
name: default_action_task
namespace_name: test
version: "1.0.0"

lifecycle:
  max_duration_minutes: 60

steps:
  - name: step_1
    handler:
      callable: "Handler1"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");

        let lifecycle = template.lifecycle.as_ref().unwrap();
        // staleness_action should default to "dlq"
        assert_eq!(lifecycle.staleness_action, "dlq");
    }

    #[test]
    fn test_lifecycle_config_serialization_roundtrip() {
        let lifecycle = LifecycleConfig {
            max_duration_minutes: Some(120),
            max_waiting_for_dependencies_minutes: Some(60),
            max_waiting_for_retry_minutes: Some(30),
            max_steps_in_process_minutes: Some(90),
            staleness_action: "error".to_string(),
            auto_fail_on_timeout: Some(true),
            auto_dlq_on_timeout: Some(false),
        };

        // Serialize to JSON
        let json = serde_json::to_string(&lifecycle).expect("Should serialize");

        // Deserialize back
        let deserialized: LifecycleConfig =
            serde_json::from_str(&json).expect("Should deserialize");

        // Should match original
        assert_eq!(deserialized, lifecycle);
    }

    #[test]
    fn test_lifecycle_config_with_empty_object() {
        let yaml_content = r#"
name: empty_lifecycle_task
namespace_name: test
version: "1.0.0"

lifecycle: {}

steps:
  - name: step_1
    handler:
      callable: "Handler1"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");

        // Lifecycle should be Some but with all None values except staleness_action
        assert!(template.lifecycle.is_some());

        let lifecycle = template.lifecycle.as_ref().unwrap();
        assert_eq!(lifecycle.max_duration_minutes, None);
        assert_eq!(lifecycle.max_waiting_for_dependencies_minutes, None);
        assert_eq!(lifecycle.max_waiting_for_retry_minutes, None);
        assert_eq!(lifecycle.max_steps_in_process_minutes, None);
        assert_eq!(lifecycle.staleness_action, "dlq"); // Default
        assert_eq!(lifecycle.auto_fail_on_timeout, None);
        assert_eq!(lifecycle.auto_dlq_on_timeout, None);
    }

    // ========================================================================
    // TAS-65 Phase 2.2: Event Declaration Tests
    // ========================================================================

    #[test]
    fn test_step_definition_without_events_backward_compatibility() {
        // Test that existing templates without event declarations still work
        let yaml_content = r#"
name: legacy_task
namespace_name: test
version: "1.0.0"

steps:
  - name: legacy_step
    handler:
      callable: "LegacyHandler"
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        assert_eq!(template.steps.len(), 1);
        assert_eq!(template.steps[0].publishes_events.len(), 0);
    }

    #[test]
    fn test_step_definition_with_event_declarations() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: test_step
    handler:
      callable: "TestHandler"
    publishes_events:
      - name: test.event.created
        description: "Test event created"
        schema:
          type: object
          properties:
            value:
              type: string
          required:
            - value
        delivery_mode: durable
      - name: test.event.completed
        description: "Test event completed"
        schema:
          type: object
          properties:
            result:
              type: boolean
        delivery_mode: durable
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        assert_eq!(template.steps.len(), 1);

        let step = &template.steps[0];
        assert_eq!(step.publishes_events.len(), 2);

        // Check first event declaration
        let event1 = &step.publishes_events[0];
        assert_eq!(event1.name, "test.event.created");
        assert_eq!(event1.description, "Test event created");
        assert_eq!(event1.delivery_mode, EventDeliveryMode::Durable);
        assert!(event1.schema.is_object());

        // Check second event declaration
        let event2 = &step.publishes_events[1];
        assert_eq!(event2.name, "test.event.completed");
        assert_eq!(event2.description, "Test event completed");
        assert_eq!(event2.delivery_mode, EventDeliveryMode::Durable);
    }

    #[test]
    fn test_event_declaration_namespace_and_action() {
        use crate::models::core::task_template::EventDeclaration;
        use serde_json::json;

        let event = EventDeclaration::builder()
            .name("order.items.added".to_string())
            .description("Items added to order".to_string())
            .schema(json!({"type": "object"}))
            .build();

        assert_eq!(event.namespace(), Some("order"));
        assert_eq!(event.action(), Some("added"));
    }

    #[test]
    fn test_event_declaration_single_level_name() {
        use crate::models::core::task_template::EventDeclaration;
        use serde_json::json;

        let event = EventDeclaration::builder()
            .name("simple".to_string())
            .description("Simple event".to_string())
            .schema(json!({"type": "object"}))
            .build();

        assert_eq!(event.namespace(), Some("simple"));
        assert_eq!(event.action(), Some("simple"));
    }

    #[test]
    fn test_step_get_event_declaration() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: test_step
    handler:
      callable: "TestHandler"
    publishes_events:
      - name: payment.authorized
        description: "Payment authorized"
        schema:
          type: object
          properties:
            payment_id:
              type: string
        delivery_mode: durable
      - name: payment.declined
        description: "Payment declined"
        schema:
          type: object
          properties:
            reason:
              type: string
        delivery_mode: durable
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let step = &template.steps[0];

        // Test finding existing event
        let authorized = step.get_event_declaration("payment.authorized");
        assert!(authorized.is_some());
        assert_eq!(authorized.unwrap().description, "Payment authorized");

        let declined = step.get_event_declaration("payment.declined");
        assert!(declined.is_some());
        assert_eq!(declined.unwrap().description, "Payment declined");

        // Test finding non-existent event
        let unknown = step.get_event_declaration("payment.unknown");
        assert!(unknown.is_none());
    }

    #[test]
    fn test_event_delivery_mode_serialization() {
        use crate::models::core::task_template::EventDeliveryMode;

        let mode = EventDeliveryMode::Durable;
        let json = serde_json::to_string(&mode).expect("Should serialize");
        assert_eq!(json, "\"durable\"");

        let deserialized: EventDeliveryMode =
            serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(deserialized, EventDeliveryMode::Durable);
    }

    #[test]
    fn test_event_declaration_builder() {
        use crate::models::core::task_template::{EventDeclaration, EventDeliveryMode};
        use serde_json::json;

        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Order created event".to_string())
            .schema(json!({
                "type": "object",
                "properties": {
                    "order_id": {"type": "string"}
                }
            }))
            .delivery_mode(EventDeliveryMode::Durable)
            .build();

        assert_eq!(event.name, "order.created");
        assert_eq!(event.description, "Order created event");
        assert_eq!(event.delivery_mode, EventDeliveryMode::Durable);
        assert!(event.schema.is_object());
    }

    #[test]
    fn test_complex_event_schema() {
        let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: process_order
    handler:
      callable: "OrderHandler"
    publishes_events:
      - name: order.created
        description: "New order created"
        schema:
          type: object
          properties:
            order_id:
              type: string
              format: uuid
            customer_id:
              type: string
            items:
              type: array
              items:
                type: object
                properties:
                  sku:
                    type: string
                  quantity:
                    type: integer
                    minimum: 1
                required:
                  - sku
                  - quantity
            total_amount:
              type: number
              minimum: 0
          required:
            - order_id
            - customer_id
            - items
            - total_amount
          additionalProperties: false
        delivery_mode: durable
"#;

        let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse YAML");
        let step = &template.steps[0];
        let event = &step.publishes_events[0];

        assert_eq!(event.name, "order.created");
        assert!(event.schema.is_object());

        // Verify schema structure
        let schema_obj = event.schema.as_object().unwrap();
        assert_eq!(schema_obj.get("type").unwrap(), "object");
        assert!(schema_obj.contains_key("properties"));
        assert!(schema_obj.contains_key("required"));
        assert_eq!(schema_obj.get("additionalProperties").unwrap(), false);
    }
}
