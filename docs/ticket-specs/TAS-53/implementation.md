# TAS-53: Dynamic Workflows & Decision Point Steps - Implementation Plan

**Status**: Planning Complete
**Created**: 2025-10-26
**Estimated Duration**: 5-7 weeks
**Architecture**: Additive, backward compatible changes

## Overview

This document provides the complete implementation plan for TAS-53, which introduces decision point workflow steps that enable dynamic workflow creation based on runtime evaluation of parent step results.

### Key Design Principles

1. **No Database Schema Changes**: Workflow edges already provide parent tracking
2. **SQL Functions Unchanged**: `get_step_readiness_status()` and `get_task_execution_context()` work automatically
3. **Backward Compatibility**: All changes are additive - existing templates work unchanged
4. **Explicit Dependencies**: Template-declared dependencies, not implicit
5. **Shared Logic Reuse**: Extract and generalize existing step creation code
6. **Instrumentation Over Limits**: Configurable warning thresholds, no hard enforcement

### Decision Point Characteristics

- **Declared in YAML**: `type: decision` field marks decision point steps
- **Handler Execution**: Callable returns typed outcome (no branches or list of step names)
- **DAG Constraints**: ≥1 upstream parent, ≥1 downstream child
- **Error Handling**: Can fail (permanent/retryable) like any step
- **Lazy Creation**: Only creates steps that were explicitly declared in template

---

## Phase 1: Data Model & Template Extensions

**Duration**: 2-3 days
**Goal**: Add type system support for decision points

### 1.1 Add StepType to Task Template

**File**: `tasker-shared/src/models/core/task_template.rs`

**Add StepType Enum**:
```rust
/// Type of workflow step
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    /// Regular workflow step (default)
    #[default]
    Standard,

    /// Decision point step that determines downstream execution paths
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
    /// This intersection determines the actual DAG dependencies for the deferred step.
    ///
    /// Example: A finalize_approval step that depends on [auto_approve, manager_approval,
    /// finance_review], but only the steps chosen by the decision point are created.
    Deferred,
}
```

**Add to StepDefinition**:
```rust
pub struct StepDefinition {
    pub name: String,
    pub description: Option<String>,

    /// Type of step (regular or decision point)
    #[serde(default)]
    pub step_type: StepType,  // NEW: defaults to Regular for backward compat

    pub handler: HandlerDefinition,
    pub system_dependency: Option<String>,

    #[serde(default)]
    pub dependencies: Vec<String>,

    #[serde(default)]
    pub retry: RetryConfiguration,

    pub timeout_seconds: Option<u32>,

    #[serde(default)]
    pub publishes_events: Vec<String>,
}
```

**YAML Example**:
```yaml
steps:
  - name: evaluate_risk
    type: decision  # NEW: marks this as a decision point
    description: "Decide approval path based on risk score"
    handler:
      callable: RiskEvaluation::DecisionHandler
      initialization:
        threshold: 75
    dependencies:
      - validate_application

  # Downstream steps that may or may not be created
  - name: manual_review
    description: "High risk - requires manual review"
    handler:
      callable: Review::ManualReviewHandler
    dependencies:
      - evaluate_risk  # Explicit dependency on decision point

  - name: auto_approve
    description: "Low risk - auto approve"
    handler:
      callable: Approval::AutoApprovalHandler
    dependencies:
      - evaluate_risk  # Explicit dependency on decision point
```

### 1.2 Add DecisionPointOutcome Type

**File**: `tasker-shared/src/models/core/decision_point.rs` (NEW)

```rust
//! Decision Point Outcome Types
//!
//! Represents the outcome of a decision point step execution, determining
//! which downstream workflow branches should be created.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Outcome of decision point step execution
///
/// Decision point handlers return this in their StepExecutionResult.result field.
/// The orchestration system uses it to determine which template-defined steps
/// to instantiate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "decision_type")]
pub enum DecisionPointOutcome {
    /// No new branches should be created
    ///
    /// Use this when the decision determines that no further processing is needed.
    /// The task will proceed to finalization if no other steps remain.
    NoBranches,

    /// Create specific steps by name
    ///
    /// Step names must exactly match step definitions in the task template.
    /// Invalid step names will result in a permanent error.
    ///
    /// # Example
    /// ```json
    /// {
    ///   "decision_type": "create_steps",
    ///   "step_names": ["manual_review", "notify_compliance"]
    /// }
    /// ```
    CreateSteps {
        /// Names of steps to create (must exist in template)
        step_names: Vec<String>
    },
}

impl DecisionPointOutcome {
    /// Parse decision outcome from StepExecutionResult.result field
    ///
    /// # Errors
    /// Returns error if the JSON structure doesn't match expected format
    pub fn from_step_result(result: &Value) -> Result<Self, String> {
        serde_json::from_value(result.clone())
            .map_err(|e| format!("Invalid decision outcome format: {}", e))
    }

    /// Convert to Value for StepExecutionResult.result field
    pub fn to_result_value(&self) -> Value {
        serde_json::to_value(self)
            .expect("DecisionPointOutcome serialization should not fail")
    }

    /// Get the list of steps to create (empty for NoBranches)
    pub fn step_names(&self) -> Vec<String> {
        match self {
            DecisionPointOutcome::NoBranches => Vec::new(),
            DecisionPointOutcome::CreateSteps { step_names } => step_names.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_no_branches_serialization() {
        let outcome = DecisionPointOutcome::NoBranches;
        let value = outcome.to_result_value();

        assert_eq!(value, json!({ "decision_type": "no_branches" }));

        let parsed = DecisionPointOutcome::from_step_result(&value).unwrap();
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn test_create_steps_serialization() {
        let outcome = DecisionPointOutcome::CreateSteps {
            step_names: vec!["manual_review".to_string(), "notify_compliance".to_string()],
        };
        let value = outcome.to_result_value();

        assert_eq!(value, json!({
            "decision_type": "create_steps",
            "step_names": ["manual_review", "notify_compliance"]
        }));

        let parsed = DecisionPointOutcome::from_step_result(&value).unwrap();
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn test_step_names_extraction() {
        let no_branches = DecisionPointOutcome::NoBranches;
        assert_eq!(no_branches.step_names(), Vec::<String>::new());

        let create_steps = DecisionPointOutcome::CreateSteps {
            step_names: vec!["step1".to_string(), "step2".to_string()],
        };
        assert_eq!(create_steps.step_names(), vec!["step1", "step2"]);
    }
}
```

**Handler Return Examples**:

```rust
// Rust handler - Option 1: No branches
StepExecutionResult::success(
    step_uuid,
    json!({ "decision_type": "no_branches" }),
    execution_time,
    None
)

// Rust handler - Option 2: Create specific branches
StepExecutionResult::success(
    step_uuid,
    json!({
        "decision_type": "create_steps",
        "step_names": ["manual_review", "notify_compliance"]
    }),
    execution_time,
    None
)
```

```ruby
# Ruby handler - Option 1: No branches
success(DecisionPoint.no_branches)

# Ruby handler - Option 2: Create specific branches
success(DecisionPoint.create_steps('manual_review', 'notify_compliance'))
```

### 1.3 Enhanced Template Validation

**File**: `tasker-shared/src/models/core/task_template.rs`

**Add to `TaskTemplate::validate()` method**:

```rust
// Existing validation...

// NEW: Validate decision point constraints
for step in &self.steps {
    if step.step_type == StepType::Decision {
        // Decision points must have at least one upstream dependency
        if step.dependencies.is_empty() {
            errors.push(format!(
                "Decision point step '{}' must have at least one dependency",
                step.name
            ));
        }

        // Decision points must have at least one downstream dependent step
        let has_dependents = self.steps.iter()
            .any(|s| s.dependencies.contains(&step.name));

        if !has_dependents {
            errors.push(format!(
                "Decision point step '{}' must have at least one dependent step (steps that depend on it)",
                step.name
            ));
        }
    }
}

if errors.is_empty() {
    Ok(())
} else {
    Err(TaskerError::ValidationError(errors.join("; ")))
}
```

### 1.4 Tests

**File**: `tasker-shared/src/models/core/task_template.rs` (in existing test module)

```rust
#[test]
fn test_decision_point_validation_requires_dependencies() {
    let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: decision_step
    type: decision
    handler:
      callable: DecisionHandler
    dependencies: []  # Invalid: no dependencies
  - name: downstream_step
    handler:
      callable: DownstreamHandler
    dependencies:
      - decision_step
"#;

    let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse");
    let result = template.validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("must have at least one dependency"));
}

#[test]
fn test_decision_point_validation_requires_dependents() {
    let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: upstream_step
    handler:
      callable: UpstreamHandler
  - name: decision_step
    type: decision
    handler:
      callable: DecisionHandler
    dependencies:
      - upstream_step
    # Invalid: no steps depend on this decision point
"#;

    let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse");
    let result = template.validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("must have at least one dependent step"));
}

#[test]
fn test_valid_decision_point_template() {
    let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: upstream_step
    handler:
      callable: UpstreamHandler
  - name: decision_step
    type: decision
    handler:
      callable: DecisionHandler
    dependencies:
      - upstream_step
  - name: downstream_step
    handler:
      callable: DownstreamHandler
    dependencies:
      - decision_step
"#;

    let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse");
    let result = template.validate();

    assert!(result.is_ok());
}

#[test]
fn test_regular_step_backward_compatibility() {
    let yaml_content = r#"
name: test_task
namespace_name: test
version: "1.0.0"

steps:
  - name: step1
    handler:
      callable: Handler1
  - name: step2
    handler:
      callable: Handler2
    dependencies:
      - step1
"#;

    let template = TaskTemplate::from_yaml(yaml_content).expect("Should parse");

    // All steps should default to Regular type
    assert_eq!(template.steps[0].step_type, StepType::Regular);
    assert_eq!(template.steps[1].step_type, StepType::Regular);

    assert!(template.validate().is_ok());
}
```

---

## Phase 2: Extract Reusable Step Creation Logic

**Duration**: 2-3 days
**Goal**: Refactor `WorkflowStepBuilder` for reuse in decision point creation

### 2.1 Create Shared Step Creator

**File**: `tasker-shared/src/models/core/workflow_step_creator.rs` (NEW)

**Rationale**: Both initial task creation and decision point processing need identical step creation logic:
- Find or create NamedSteps
- Create WorkflowSteps with proper configuration
- Handle transaction safety
- Serialize handler initialization to JSON

```rust
//! Shared Workflow Step Creation Logic
//!
//! Provides reusable step creation functionality used by both:
//! - Initial task initialization (WorkflowStepBuilder)
//! - Dynamic decision point processing (DecisionPointService)

use sqlx::types::Uuid;
use std::sync::Arc;
use tasker_shared::errors::TaskerResult;
use tasker_shared::models::core::task_template::StepDefinition;
use tasker_shared::models::core::workflow_step::NewWorkflowStep;
use tasker_shared::models::{NamedStep, WorkflowStep};
use tasker_shared::system_context::SystemContext;

/// Shared logic for creating workflow steps from step definitions
pub struct WorkflowStepCreator {
    context: Arc<SystemContext>,
}

impl WorkflowStepCreator {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Create a single workflow step from definition
    ///
    /// This method encapsulates the complete step creation process:
    /// 1. Find or create the named step
    /// 2. Serialize handler initialization
    /// 3. Create the workflow step record
    ///
    /// Originally extracted from `WorkflowStepBuilder::create_steps()` (lines 54-123)
    /// to enable reuse in decision point dynamic step creation.
    pub async fn create_step_from_definition(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        step_definition: &StepDefinition,
    ) -> TaskerResult<WorkflowStep> {
        // Find or create named step (reuses existing logic)
        let named_step = self.find_or_create_named_step(tx, step_definition).await?;

        // Convert handler initialization to JSON
        let inputs = if step_definition.handler.initialization.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&step_definition.handler.initialization)?)
        };

        // Create workflow step record
        let new_workflow_step = NewWorkflowStep {
            task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            retryable: Some(step_definition.retry.retryable),
            max_attempts: Some(step_definition.retry.max_attempts as i32),
            inputs,
            skippable: None,
        };

        WorkflowStep::create_with_transaction(tx, new_workflow_step).await
    }

    /// Find or create named step
    ///
    /// Extracted from WorkflowStepBuilder::create_steps() lines 56-84
    /// Implements find-or-create pattern with transaction safety.
    async fn find_or_create_named_step(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        step_definition: &StepDefinition,
    ) -> TaskerResult<NamedStep> {
        // Search for existing named step
        let named_steps = NamedStep::find_by_name(
            self.context.database_pool(),
            &step_definition.name
        ).await?;

        if let Some(existing_step) = named_steps.first() {
            Ok(existing_step.clone())
        } else {
            // Create new named step using transaction-aware method
            let system_name = "tasker_core_rust";
            NamedStep::find_or_create_by_name_with_transaction(
                tx,
                self.context.database_pool(),
                &step_definition.name,
                system_name,
            ).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::models::core::task_template::{HandlerDefinition, RetryConfiguration};
    use std::collections::HashMap;

    #[test]
    fn test_handler_initialization_serialization() {
        let empty_init = HashMap::new();
        let with_data = HashMap::from([
            ("key".to_string(), serde_json::json!("value"))
        ]);

        // Verify we can serialize handler initialization
        assert!(serde_json::to_value(&empty_init).is_ok());
        assert!(serde_json::to_value(&with_data).is_ok());
    }

    #[test]
    fn test_new_workflow_step_structure() {
        let task_uuid = Uuid::new_v4();
        let named_step_uuid = Uuid::new_v4();

        let new_step = NewWorkflowStep {
            task_uuid,
            named_step_uuid,
            retryable: Some(true),
            max_attempts: Some(3),
            inputs: Some(serde_json::json!({"test": "value"})),
            skippable: None,
        };

        assert_eq!(new_step.task_uuid, task_uuid);
        assert_eq!(new_step.named_step_uuid, named_step_uuid);
        assert_eq!(new_step.retryable, Some(true));
        assert_eq!(new_step.max_attempts, Some(3));
        assert!(new_step.inputs.is_some());
    }
}
```

### 2.2 Update WorkflowStepBuilder to Use Shared Logic

**File**: `tasker-orchestration/src/orchestration/lifecycle/task_initialization/workflow_step_builder.rs`

**Refactor to use `WorkflowStepCreator`**:

```rust
use tasker_shared::models::core::workflow_step_creator::WorkflowStepCreator;

pub struct WorkflowStepBuilder {
    context: Arc<SystemContext>,
    step_creator: WorkflowStepCreator,  // NEW: shared creator
}

impl WorkflowStepBuilder {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self {
            step_creator: WorkflowStepCreator::new(context.clone()),
            context,
        }
    }

    /// Create workflow steps and their dependencies from task template
    pub async fn create_workflow_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        task_template: &TaskTemplate,
    ) -> Result<(usize, HashMap<String, Uuid>), TaskInitializationError> {
        // Step 1: Create all named steps and workflow steps
        let step_mapping = self.create_steps(tx, task_uuid, task_template).await?;

        // Step 2: Create dependencies between steps
        self.create_step_dependencies(tx, task_template, &step_mapping)
            .await?;

        Ok((step_mapping.len(), step_mapping))
    }

    /// Create named steps and workflow steps using shared creator
    async fn create_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        task_template: &TaskTemplate,
    ) -> Result<HashMap<String, Uuid>, TaskInitializationError> {
        let mut step_mapping = HashMap::new();

        for step_definition in &task_template.steps {
            // Use shared creator instead of inline logic
            let workflow_step = self.step_creator
                .create_step_from_definition(tx, task_uuid, step_definition)
                .await
                .map_err(|e| TaskInitializationError::Database(format!(
                    "Failed to create workflow step '{}': {}",
                    step_definition.name, e
                )))?;

            step_mapping.insert(
                step_definition.name.clone(),
                workflow_step.workflow_step_uuid,
            );
        }

        Ok(step_mapping)
    }

    // create_step_dependencies() remains unchanged
}
```

### 2.3 Benefits

- **DRY**: Single source of truth for step creation logic
- **Testability**: `WorkflowStepCreator` can be unit tested independently
- **Reusability**: `DecisionPointService` will use the same logic
- **Maintainability**: Bug fixes in one place benefit both use cases
- **Backward Compatibility**: No changes to external interfaces

### 2.4 Tests

**Integration test** to verify refactoring doesn't change behavior:

```rust
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_builder_refactored_behavior_unchanged(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = Arc::new(SystemContext::with_pool(pool).await?);
    let builder = WorkflowStepBuilder::new(context.clone());

    let template = load_test_template("linear_workflow_handler.yaml")?;
    let task_uuid = Uuid::now_v7();

    let mut tx = context.database_pool().begin().await?;
    let (step_count, step_mapping) = builder
        .create_workflow_steps(&mut tx, task_uuid, &template)
        .await?;
    tx.commit().await?;

    // Verify all steps created
    assert_eq!(step_count, template.steps.len());

    // Verify all steps in mapping
    for step_def in &template.steps {
        assert!(step_mapping.contains_key(&step_def.name));
    }

    Ok(())
}
```

---

## Phase 3: Partial Workflow Graph Creation

**Duration**: 3-4 days
**Goal**: Only create steps up to decision point boundaries during initialization

### 3.1 Add Decision Boundary Detection

**File**: `tasker-orchestration/src/orchestration/lifecycle/task_initialization/workflow_step_builder.rs`

**Add methods to detect and handle decision boundaries**:

```rust
impl WorkflowStepBuilder {
    /// Determine which steps to create initially
    ///
    /// Creates:
    /// - All regular steps that don't depend on decision points
    /// - All decision point steps themselves
    ///
    /// Does NOT create:
    /// - Steps that depend on decision point steps
    ///
    /// These will be created dynamically when the decision point executes.
    fn get_steps_for_initial_creation<'a>(
        &self,
        task_template: &'a TaskTemplate,
    ) -> Vec<&'a StepDefinition> {
        use std::collections::HashSet;

        let mut steps_to_create = Vec::new();
        let mut decision_step_names: HashSet<String> = HashSet::new();

        // Collect all decision point step names
        for step in &task_template.steps {
            if step.step_type == StepType::Decision {
                decision_step_names.insert(step.name.clone());
            }
        }

        // Include a step if:
        // 1. It's a decision point step itself, OR
        // 2. None of its dependencies are decision points
        for step in &task_template.steps {
            let should_create = if step.step_type == StepType::Decision {
                true  // Always create decision point steps
            } else {
                // Check if ANY dependency is a decision point
                !step.dependencies.iter()
                    .any(|dep| decision_step_names.contains(dep))
            };

            if should_create {
                steps_to_create.push(step);
            }
        }

        steps_to_create
    }

    /// Create partial workflow graph (up to decision boundaries)
    pub async fn create_partial_workflow_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        task_template: &TaskTemplate,
    ) -> Result<(usize, HashMap<String, Uuid>), TaskInitializationError> {
        // Get steps to create initially
        let steps_to_create = self.get_steps_for_initial_creation(task_template);

        tracing::debug!(
            task_uuid = %task_uuid,
            total_steps = task_template.steps.len(),
            initial_steps = steps_to_create.len(),
            "Creating partial workflow graph up to decision boundaries"
        );

        // Create only initial steps
        let mut step_mapping = HashMap::new();
        for step_definition in steps_to_create {
            let workflow_step = self.step_creator
                .create_step_from_definition(tx, task_uuid, step_definition)
                .await
                .map_err(|e| TaskInitializationError::Database(format!(
                    "Failed to create workflow step '{}': {}",
                    step_definition.name, e
                )))?;

            step_mapping.insert(
                step_definition.name.clone(),
                workflow_step.workflow_step_uuid,
            );
        }

        // Create dependencies only for created steps
        self.create_partial_dependencies(tx, task_template, &step_mapping).await?;

        Ok((step_mapping.len(), step_mapping))
    }

    /// Create dependencies only between created steps
    ///
    /// This ensures we don't create edges to steps that don't exist yet
    /// (steps beyond decision boundaries).
    async fn create_partial_dependencies(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_template: &TaskTemplate,
        created_steps: &HashMap<String, Uuid>,
    ) -> Result<(), TaskInitializationError> {
        for step_definition in &task_template.steps {
            // Only process if this step was created
            let Some(&to_step_uuid) = created_steps.get(&step_definition.name) else {
                continue;
            };

            for dependency_name in &step_definition.dependencies {
                // Only create edge if dependency was also created
                if let Some(&from_step_uuid) = created_steps.get(dependency_name) {
                    // Check for self-referencing cycles
                    if from_step_uuid == to_step_uuid {
                        return Err(TaskInitializationError::CycleDetected {
                            from: dependency_name.clone(),
                            to: step_definition.name.clone(),
                        });
                    }

                    // Check for cycles before creating edge
                    let would_cycle = tasker_shared::models::WorkflowStepEdge::would_create_cycle(
                        self.context.database_pool(),
                        from_step_uuid,
                        to_step_uuid,
                    )
                    .await
                    .map_err(|e| {
                        TaskInitializationError::Database(format!(
                            "Failed to check for cycles when adding edge '{}' -> '{}': {}",
                            dependency_name, step_definition.name, e
                        ))
                    })?;

                    if would_cycle {
                        return Err(TaskInitializationError::CycleDetected {
                            from: dependency_name.clone(),
                            to: step_definition.name.clone(),
                        });
                    }

                    let new_edge = NewWorkflowStepEdge {
                        from_step_uuid,
                        to_step_uuid,
                        name: "provides".to_string(),
                    };

                    tasker_shared::models::WorkflowStepEdge::create_with_transaction(tx, new_edge)
                        .await
                        .map_err(|e| {
                            TaskInitializationError::Database(format!(
                                "Failed to create edge '{}' -> '{}': {}",
                                dependency_name, step_definition.name, e
                            ))
                        })?;
                }
            }
        }

        Ok(())
    }
}
```

### 3.2 Store Template for Later Use

**File**: `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs`

**Store resolved template in task context for decision point processing**:

```rust
/// Create the basic task record with optional resolved template
async fn create_task_record(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    task_request: TaskRequest,
    resolved_template: Option<&ResolvedTaskTemplate>,  // NEW parameter
) -> Result<Task, TaskInitializationError> {
    let named_task_uuid = self
        .namespace_resolver
        .resolve_named_task_uuid(&task_request)
        .await?;

    let mut new_task = Task::from_task_request(task_request);
    new_task.named_task_uuid = named_task_uuid;

    // Store resolved template in context for decision point processing
    if let Some(template) = resolved_template {
        let mut context = new_task.context.clone();
        context["_tasker_resolved_template"] = serde_json::to_value(template)
            .map_err(|e| TaskInitializationError::Database(format!(
                "Failed to serialize template: {}", e
            )))?;
        new_task.context = context;
    }

    Task::create_with_transaction(tx, new_task)
        .await
        .map_err(|e| {
            TaskInitializationError::Database(format!("Failed to create task: {e}"))
        })
}
```

**Update main initialization flow**:

```rust
pub async fn create_task_from_request(
    &self,
    task_request: TaskRequest,
) -> Result<TaskInitializationResult, TaskInitializationError> {
    // ... existing setup ...

    let mut tx = self.context.database_pool().begin().await?;

    // Load task template FIRST
    let task_template = self.template_loader
        .load_task_template(&task_request)
        .await?;

    // Resolve for environment
    let resolved_template = task_template.resolve_for_environment(&self.context.tasker_env());

    // Create task record WITH template
    let task = self.create_task_record(&mut tx, task_request, Some(&resolved_template)).await?;
    let task_uuid = task.task_uuid;

    // Create PARTIAL workflow (up to decision boundaries)
    let (step_count, step_mapping) = self
        .workflow_step_builder
        .create_partial_workflow_steps(&mut tx, task_uuid, &resolved_template.template)
        .await?;

    // ... rest unchanged ...
}
```

### 3.3 Tests

```rust
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_partial_workflow_creation_stops_at_decision_boundary(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = Arc::new(SystemContext::with_pool(pool).await?);
    let builder = WorkflowStepBuilder::new(context.clone());

    let template = load_test_template("decision_workflows/simple_decision.yaml")?;
    let task_uuid = Uuid::now_v7();

    let mut tx = context.database_pool().begin().await?;
    let (step_count, step_mapping) = builder
        .create_partial_workflow_steps(&mut tx, task_uuid, &template)
        .await?;
    tx.commit().await?;

    // Template has: upstream, decision, branch_a, branch_b
    // Should create: upstream, decision (2 steps)
    // Should NOT create: branch_a, branch_b (depend on decision)
    assert_eq!(step_count, 2);
    assert!(step_mapping.contains_key("upstream"));
    assert!(step_mapping.contains_key("decision"));
    assert!(!step_mapping.contains_key("branch_a"));
    assert!(!step_mapping.contains_key("branch_b"));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_regular_template_creates_all_steps_backward_compat(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = Arc::new(SystemContext::with_pool(pool).await?);
    let builder = WorkflowStepBuilder::new(context.clone());

    let template = load_test_template("linear_workflow_handler.yaml")?;
    let task_uuid = Uuid::now_v7();

    let mut tx = context.database_pool().begin().await?;
    let (step_count, _) = builder
        .create_partial_workflow_steps(&mut tx, task_uuid, &template)
        .await?;
    tx.commit().await?;

    // No decision points, so all steps should be created
    assert_eq!(step_count, template.steps.len());

    Ok(())
}
```

---

## Phase 4: DecisionPointActor Implementation

**Duration**: 4-5 days
**Goal**: Actor to handle dynamic step creation after decision completion

### 4.1 Create DecisionPointActor

**File**: `tasker-orchestration/src/actors/decision_point_actor.rs` (NEW)

```rust
//! Decision Point Actor
//!
//! Handles dynamic workflow step creation based on decision point outcomes.

use crate::actors::traits::{Handler, Message, OrchestrationActor};
use crate::orchestration::lifecycle::decision_point::DecisionPointService;
use std::sync::Arc;
use tasker_shared::errors::OrchestrationResult;
use tasker_shared::models::core::decision_point::DecisionPointOutcome;
use tasker_shared::system_context::SystemContext;
use tracing::{error, info, instrument};
use uuid::Uuid;

/// Message: Process decision point outcome and create selected steps
#[derive(Debug, Clone)]
pub struct ProcessDecisionOutcomeMessage {
    /// UUID of the decision point step that completed
    pub decision_step_uuid: Uuid,
    /// UUID of the task containing this decision
    pub task_uuid: Uuid,
    /// Decision outcome from step execution result
    pub outcome: DecisionPointOutcome,
}

impl Message for ProcessDecisionOutcomeMessage {
    type Response = OrchestrationResult<DecisionProcessingResult>;
}

/// Result of processing a decision point outcome
#[derive(Debug, Clone)]
pub struct DecisionProcessingResult {
    /// Number of steps created from this decision
    pub steps_created: usize,
    /// UUIDs of newly created workflow steps
    pub new_step_uuids: Vec<Uuid>,
}

/// Actor responsible for dynamic step creation based on decision outcomes
pub struct DecisionPointActor {
    context: Arc<SystemContext>,
    service: DecisionPointService,
}

impl DecisionPointActor {
    pub fn new(context: Arc<SystemContext>) -> Self {
        let service = DecisionPointService::new(context.clone());
        Self { context, service }
    }
}

impl OrchestrationActor for DecisionPointActor {
    async fn started(&self) -> OrchestrationResult<()> {
        info!("DecisionPointActor started");
        Ok(())
    }

    async fn stopped(&self) -> OrchestrationResult<()> {
        info!("DecisionPointActor stopped");
        Ok(())
    }
}

impl Handler<ProcessDecisionOutcomeMessage> for DecisionPointActor {
    #[instrument(
        skip(self, msg),
        fields(
            decision_step_uuid = %msg.decision_step_uuid,
            task_uuid = %msg.task_uuid,
            outcome = ?msg.outcome
        )
    )]
    async fn handle(
        &self,
        msg: ProcessDecisionOutcomeMessage,
    ) -> OrchestrationResult<DecisionProcessingResult> {
        info!(
            decision_step_uuid = %msg.decision_step_uuid,
            task_uuid = %msg.task_uuid,
            "Processing decision point outcome"
        );

        match self.service
            .process_decision_outcome(
                msg.decision_step_uuid,
                msg.task_uuid,
                msg.outcome,
            )
            .await
        {
            Ok(result) => {
                info!(
                    decision_step_uuid = %msg.decision_step_uuid,
                    steps_created = result.steps_created,
                    "Decision point processing completed successfully"
                );
                Ok(result)
            }
            Err(e) => {
                error!(
                    decision_step_uuid = %msg.decision_step_uuid,
                    error = %e,
                    "Decision point processing failed"
                );
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_decision_point_actor_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let actor = DecisionPointActor::new(context);

        // Verify actor can start
        actor.started().await?;

        Ok(())
    }
}
```

### 4.2 Add to ActorRegistry

**File**: `tasker-orchestration/src/actors/registry.rs`

```rust
pub struct ActorRegistry {
    pub task_request_actor: Arc<TaskRequestActor>,
    pub result_processor_actor: Arc<ResultProcessorActor>,
    pub step_enqueuer_actor: Arc<StepEnqueuerActor>,
    pub task_finalizer_actor: Arc<TaskFinalizerActor>,
    pub decision_point_actor: Arc<DecisionPointActor>,  // NEW
}

impl ActorRegistry {
    pub async fn new(
        context: Arc<SystemContext>,
        step_enqueuer_service: Arc<StepEnqueuerService>,
    ) -> OrchestrationResult<Self> {
        // ... existing actors ...

        let decision_point_actor = Arc::new(DecisionPointActor::new(context.clone()));
        decision_point_actor.started().await?;

        Ok(Self {
            task_request_actor,
            result_processor_actor,
            step_enqueuer_actor,
            task_finalizer_actor,
            decision_point_actor,
        })
    }

    pub async fn shutdown(&self) -> OrchestrationResult<()> {
        // ... existing shutdowns ...
        self.decision_point_actor.stopped().await?;
        Ok(())
    }
}
```

### 4.3 Integrate with ResultProcessorActor

**File**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/task_coordinator.rs`

**Add decision detection and routing**:

```rust
use tasker_shared::models::core::decision_point::DecisionPointOutcome;
use tasker_shared::models::core::task_template::{ResolvedTaskTemplate, StepType};

impl TaskCoordinator {
    /// Check if step is a decision point and process outcome if needed
    async fn check_and_process_decision_point(
        &self,
        step_result: &StepExecutionResult,
    ) -> OrchestrationResult<Option<DecisionProcessingResult>> {
        // Only process successful decision points
        if !step_result.success {
            return Ok(None);
        }

        let pool = self.context.database_pool();

        // Get workflow step
        let workflow_step = WorkflowStep::find_by_uuid(pool, step_result.step_uuid)
            .await
            .map_err(|e| OrchestrationError::Database(format!(
                "Failed to find workflow step: {}", e
            )))?;

        // Get named step to check name
        let named_step = NamedStep::find_by_uuid(pool, workflow_step.named_step_uuid)
            .await
            .map_err(|e| OrchestrationError::Database(format!(
                "Failed to find named step: {}", e
            )))?;

        // Load task to get template
        let task = Task::find_by_uuid(pool, workflow_step.task_uuid)
            .await
            .map_err(|e| OrchestrationError::Database(format!(
                "Failed to find task: {}", e
            )))?;

        // Extract template from task context
        let template = self.extract_template_from_task(&task)?;

        // Check if this step is a decision point
        if let Some(step_def) = template.template.steps.iter()
            .find(|s| s.name == named_step.name)
        {
            if step_def.step_type == StepType::Decision {
                tracing::info!(
                    step_uuid = %step_result.step_uuid,
                    step_name = %named_step.name,
                    "Detected decision point step completion"
                );

                // Parse decision outcome
                let outcome = DecisionPointOutcome::from_step_result(&step_result.result)
                    .map_err(|e| OrchestrationError::InvalidDecisionOutcome(e))?;

                // Send to DecisionPointActor
                let msg = ProcessDecisionOutcomeMessage {
                    decision_step_uuid: step_result.step_uuid,
                    task_uuid: workflow_step.task_uuid,
                    outcome,
                };

                let result = self.actors.decision_point_actor.handle(msg).await?;
                return Ok(Some(result));
            }
        }

        Ok(None)
    }

    fn extract_template_from_task(
        &self,
        task: &Task,
    ) -> OrchestrationResult<ResolvedTaskTemplate> {
        task.context
            .get("_tasker_resolved_template")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| OrchestrationError::TemplateMissing(
                "Task context missing resolved template".to_string()
            ))
    }
}
```

**Update existing result processing flow**:

```rust
pub async fn coordinate_task_processing(
    &self,
    step_result: &StepExecutionResult,
) -> OrchestrationResult<()> {
    // Check for decision point BEFORE normal coordination
    if let Some(decision_result) = self.check_and_process_decision_point(step_result).await? {
        tracing::info!(
            steps_created = decision_result.steps_created,
            "Decision point created new steps"
        );

        // Trigger step discovery for newly created steps
        if decision_result.steps_created > 0 {
            let task_uuid = // extract from step_result
            self.trigger_step_discovery(task_uuid).await?;
        }
    }

    // Continue with normal task coordination
    // ... existing logic ...
}

async fn trigger_step_discovery(&self, task_uuid: Uuid) -> OrchestrationResult<()> {
    let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());

    if let Some(task_info) = sql_executor.get_task_ready_info(task_uuid).await? {
        self.step_enqueuer
            .process_single_task_from_ready_info(&task_info)
            .await?;
    }

    Ok(())
}
```

---

## Phase 5: DecisionPointService Implementation

**Duration**: 3-4 days
**Goal**: Service to create dynamic steps with atomicity and cycle detection

### 5.1 Create Service Structure

**File**: `tasker-orchestration/src/orchestration/lifecycle/decision_point/mod.rs` (NEW)

```rust
//! Decision Point Processing
//!
//! Services for handling dynamic workflow step creation based on decision outcomes.

mod service;

pub use service::DecisionPointService;
```

**File**: `tasker-orchestration/src/orchestration/lifecycle/decision_point/service.rs` (NEW)

```rust
//! Decision Point Service
//!
//! Handles dynamic step creation with atomicity, cycle detection, and dependency wiring.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tasker_shared::errors::{OrchestrationError, OrchestrationResult};
use tasker_shared::models::core::decision_point::DecisionPointOutcome;
use tasker_shared::models::core::task_template::{ResolvedTaskTemplate, StepDefinition};
use tasker_shared::models::core::workflow_step_creator::WorkflowStepCreator;
use tasker_shared::models::{
    Task, WorkflowStep, WorkflowStepEdge, NewWorkflowStepEdge, NamedStep
};
use tasker_shared::system_context::SystemContext;
use tracing::{debug, info, warn, instrument};
use uuid::Uuid;

use crate::actors::decision_point_actor::DecisionProcessingResult;

pub struct DecisionPointService {
    context: Arc<SystemContext>,
    step_creator: WorkflowStepCreator,
}

impl DecisionPointService {
    pub fn new(context: Arc<SystemContext>) -> Self {
        let step_creator = WorkflowStepCreator::new(context.clone());
        Self {
            context,
            step_creator,
        }
    }

    /// Process decision outcome and create selected steps
    #[instrument(skip(self), fields(
        decision_step_uuid = %decision_step_uuid,
        task_uuid = %task_uuid
    ))]
    pub async fn process_decision_outcome(
        &self,
        decision_step_uuid: Uuid,
        task_uuid: Uuid,
        outcome: DecisionPointOutcome,
    ) -> OrchestrationResult<DecisionProcessingResult> {
        match outcome {
            DecisionPointOutcome::NoBranches => {
                info!(
                    decision_step_uuid = %decision_step_uuid,
                    "Decision outcome: no branches to create"
                );
                Ok(DecisionProcessingResult {
                    steps_created: 0,
                    new_step_uuids: Vec::new(),
                })
            }
            DecisionPointOutcome::CreateSteps { step_names } => {
                info!(
                    decision_step_uuid = %decision_step_uuid,
                    step_count = step_names.len(),
                    steps = ?step_names,
                    "Decision outcome: creating steps"
                );

                self.create_dynamic_steps(decision_step_uuid, task_uuid, step_names)
                    .await
            }
        }
    }

    /// Create dynamic steps from decision outcome
    async fn create_dynamic_steps(
        &self,
        decision_step_uuid: Uuid,
        task_uuid: Uuid,
        step_names: Vec<String>,
    ) -> OrchestrationResult<DecisionProcessingResult> {
        let pool = self.context.database_pool();

        // Load task and extract template
        let task = Task::find_by_uuid(pool, task_uuid)
            .await
            .map_err(|e| OrchestrationError::Database(format!(
                "Failed to load task: {}", e
            )))?;

        let template = self.extract_template(&task)?;

        // Validate step names exist in template
        self.validate_step_names(&template, &step_names)?;

        // Check warning thresholds
        self.check_warning_thresholds(&step_names);

        // Get step definitions for selected steps
        let step_definitions: Vec<_> = template.template.steps.iter()
            .filter(|s| step_names.contains(&s.name))
            .collect();

        debug!(
            task_uuid = %task_uuid,
            decision_step_uuid = %decision_step_uuid,
            step_count = step_definitions.len(),
            "Creating dynamic steps in transaction"
        );

        // Create steps in atomic transaction
        let mut tx = pool.begin().await.map_err(|e| {
            OrchestrationError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        let mut new_step_uuids = Vec::new();
        let mut step_mapping = HashMap::new();

        // Create each selected step (REUSES WorkflowStepCreator)
        for step_def in &step_definitions {
            let workflow_step = self.step_creator
                .create_step_from_definition(&mut tx, task_uuid, step_def)
                .await
                .map_err(|e| OrchestrationError::StepCreation(format!(
                    "Failed to create step '{}': {}", step_def.name, e
                )))?;

            debug!(
                step_name = %step_def.name,
                step_uuid = %workflow_step.workflow_step_uuid,
                "Created dynamic step"
            );

            new_step_uuids.push(workflow_step.workflow_step_uuid);
            step_mapping.insert(step_def.name.clone(), workflow_step.workflow_step_uuid);
        }

        // Wire dependencies (REUSES cycle detection)
        self.wire_dependencies(
            &mut tx,
            task_uuid,
            decision_step_uuid,
            &step_definitions,
            &step_mapping,
            &template,
        ).await?;

        // Commit transaction (all or nothing)
        tx.commit().await.map_err(|e| {
            OrchestrationError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        info!(
            decision_step_uuid = %decision_step_uuid,
            steps_created = new_step_uuids.len(),
            "Successfully created dynamic steps"
        );

        Ok(DecisionProcessingResult {
            steps_created: new_step_uuids.len(),
            new_step_uuids,
        })
    }

    /// Wire dependencies for dynamically created steps
    async fn wire_dependencies(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        decision_step_uuid: Uuid,
        step_definitions: &[&StepDefinition],
        step_mapping: &HashMap<String, Uuid>,
        template: &ResolvedTaskTemplate,
    ) -> OrchestrationResult<()> {
        let pool = self.context.database_pool();

        for step_def in step_definitions {
            let to_step_uuid = step_mapping[&step_def.name];

            for dependency_name in &step_def.dependencies {
                // Find dependency UUID (newly created, decision point, or existing)
                let from_step_uuid = if let Some(&uuid) = step_mapping.get(dependency_name) {
                    uuid  // Dependency created in this batch
                } else if self.is_decision_step(template, dependency_name)? {
                    decision_step_uuid  // Dependency is the decision point itself
                } else {
                    // Dependency should already exist - fetch it
                    self.find_existing_step_uuid(tx, task_uuid, dependency_name).await?
                };

                // Cycle check (REUSES existing logic from WorkflowStepEdge)
                let would_cycle = WorkflowStepEdge::would_create_cycle(
                    pool,
                    from_step_uuid,
                    to_step_uuid,
                ).await.map_err(|e| {
                    OrchestrationError::Database(format!(
                        "Failed to check for cycles: {}", e
                    ))
                })?;

                if would_cycle {
                    return Err(OrchestrationError::CycleDetected {
                        from: dependency_name.clone(),
                        to: step_def.name.clone(),
                    });
                }

                // Create edge
                let new_edge = NewWorkflowStepEdge {
                    from_step_uuid,
                    to_step_uuid,
                    name: "provides".to_string(),
                };

                WorkflowStepEdge::create_with_transaction(tx, new_edge)
                    .await
                    .map_err(|e| OrchestrationError::Database(format!(
                        "Failed to create edge '{}' -> '{}': {}",
                        dependency_name, step_def.name, e
                    )))?;

                debug!(
                    from_step = %dependency_name,
                    to_step = %step_def.name,
                    from_uuid = %from_step_uuid,
                    to_uuid = %to_step_uuid,
                    "Created dependency edge"
                );
            }
        }

        Ok(())
    }

    /// Validate that all step names exist in the template
    fn validate_step_names(
        &self,
        template: &ResolvedTaskTemplate,
        step_names: &[String],
    ) -> OrchestrationResult<()> {
        let template_step_names: HashSet<_> = template.template.steps.iter()
            .map(|s| s.name.as_str())
            .collect();

        for name in step_names {
            if !template_step_names.contains(name.as_str()) {
                return Err(OrchestrationError::InvalidDecisionOutcome(
                    format!("Step '{}' not found in task template", name)
                ));
            }
        }

        Ok(())
    }

    /// Check if a step name refers to a decision point step
    fn is_decision_step(
        &self,
        template: &ResolvedTaskTemplate,
        step_name: &str,
    ) -> OrchestrationResult<bool> {
        Ok(template.template.steps.iter()
            .any(|s| s.name == step_name && s.step_type == StepType::Decision))
    }

    /// Find UUID of existing workflow step by name
    async fn find_existing_step_uuid(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        step_name: &str,
    ) -> OrchestrationResult<Uuid> {
        // Find named step
        let named_step = NamedStep::find_by_name(
            self.context.database_pool(),
            step_name
        )
        .await
        .map_err(|e| OrchestrationError::Database(format!(
            "Failed to find named step '{}': {}", step_name, e
        )))?
        .into_iter()
        .next()
        .ok_or_else(|| OrchestrationError::StepNotFound(step_name.to_string()))?;

        // Find workflow step for this task
        let workflow_step = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT * FROM tasker_workflow_steps
            WHERE task_uuid = $1 AND named_step_uuid = $2
            "#,
            task_uuid,
            named_step.named_step_uuid
        )
        .fetch_one(self.context.database_pool())
        .await
        .map_err(|e| OrchestrationError::Database(format!(
            "Failed to find workflow step '{}' for task: {}", step_name, e
        )))?;

        Ok(workflow_step.workflow_step_uuid)
    }

    fn extract_template(
        &self,
        task: &Task,
    ) -> OrchestrationResult<ResolvedTaskTemplate> {
        task.context
            .get("_tasker_resolved_template")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| OrchestrationError::TemplateMissing(
                "Task context missing resolved template".to_string()
            ))
    }

    fn check_warning_thresholds(&self, step_names: &[String]) {
        let config = &self.context.tasker_config.decision_points;

        if step_names.len() > config.warn_max_children {
            warn!(
                step_count = step_names.len(),
                threshold = config.warn_max_children,
                "Decision point creating many child steps - consider splitting workflow"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_decision_point_service_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let service = DecisionPointService::new(context);

        assert!(std::mem::size_of_val(&service) > 0);
        Ok(())
    }
}
```

---

## Phase 6: State Machine Integration

**Duration**: 2-3 days
**Goal**: Ensure orchestration continues correctly after dynamic creation

### Analysis: No Changes Needed

After reviewing the existing orchestration flow, **the current implementation already supports dynamic steps** without modification:

#### 6.1 Task Finalization

**File**: `tasker-orchestration/src/orchestration/lifecycle/task_finalization/service.rs`

**Current behavior** (no changes needed):
```rust
// TaskFinalizer uses get_task_execution_context() SQL function
// This function aggregates ALL workflow steps for the task
// It doesn't care HOW steps were created, only:
// - Do all steps exist?
// - Are they all complete?

// SQL function automatically handles:
// - Steps created during initialization
// - Steps created by decision points
// - Mixed creation sources

// Task finalizes when: all steps (regardless of source) are complete
```

**Why it works**: `get_task_execution_context()` queries `tasker_workflow_steps WHERE task_uuid = $1`, which includes dynamically created steps automatically.

#### 6.2 Step Discovery After Decision

**File**: `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer_services/service.rs`

**Current behavior** (no changes needed):
```rust
// StepEnqueuerService uses get_step_readiness_status()
// This SQL function:
// - Finds ALL steps for a task
// - Checks dependency satisfaction via workflow_step_edges
// - Returns ready steps regardless of creation method

// Newly created steps appear in query results immediately after transaction commits
```

**Why it works**: SQL functions operate on current database state, automatically including newly created workflow steps.

#### 6.3 Integration Point: Trigger Step Discovery

**File**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/task_coordinator.rs`

**Already implemented in Phase 4.3**:
```rust
// After DecisionPointActor creates steps, trigger discovery:
if decision_result.steps_created > 0 {
    self.trigger_step_discovery(task_uuid).await?;
}

async fn trigger_step_discovery(&self, task_uuid: Uuid) -> OrchestrationResult<()> {
    let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());

    if let Some(task_info) = sql_executor.get_task_ready_info(task_uuid).await? {
        self.step_enqueuer
            .process_single_task_from_ready_info(&task_info)
            .await?;
    }

    Ok(())
}
```

This is the **only integration point needed** - all other components work automatically.

### 6.4 Verification Tests

**File**: `tests/integration/decision_point_state_machine_tests.rs` (NEW)

```rust
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_continues_after_decision_creates_steps(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create task with decision point
    // Execute decision point (creates branch_a)
    // Verify branch_a is discovered and enqueued
    // Execute branch_a
    // Verify task completes

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_sql_functions_work_with_mixed_step_sources(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create task (some steps created initially)
    // Execute decision point (more steps created dynamically)
    // Call get_step_readiness_status() - should see both
    // Call get_task_execution_context() - should aggregate both

    Ok(())
}
```

---

## Phase 7: Instrumentation & Observability

**Duration**: 2-3 days
**Goal**: Add metrics and configurable warning thresholds

### 7.1 Configuration

**File**: `config/tasker/base/decision_points.toml` (NEW)

```toml
[decision_points]
# Warning thresholds (no hard limits - log warnings only)
# These help identify workflows that may benefit from refactoring

# Warn if decision point nesting exceeds this depth
# (decision point creates steps that contain decision points)
warn_max_depth = 3

# Warn if single decision creates more than this many steps
warn_max_children = 10

# Warn if task has more than this many dynamically created steps total
warn_total_dynamic_steps = 50

# Instrumentation
enable_metrics = true
enable_detailed_logging = true
```

**Environment Overrides**:

**File**: `config/tasker/environments/test/decision_points.toml`
```toml
[decision_points]
warn_max_depth = 2
warn_max_children = 5
warn_total_dynamic_steps = 20
```

**File**: `config/tasker/environments/production/decision_points.toml`
```toml
[decision_points]
warn_max_depth = 5
warn_max_children = 20
warn_total_dynamic_steps = 100
```

### 7.2 Configuration Type

**File**: `tasker-shared/src/config/decision_points.rs` (NEW)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionPointsConfig {
    /// Warn if decision nesting exceeds this depth
    #[serde(default = "default_warn_max_depth")]
    pub warn_max_depth: usize,

    /// Warn if single decision creates more steps than this
    #[serde(default = "default_warn_max_children")]
    pub warn_max_children: usize,

    /// Warn if task has more dynamic steps than this
    #[serde(default = "default_warn_total_dynamic_steps")]
    pub warn_total_dynamic_steps: usize,

    /// Enable OpenTelemetry metrics
    #[serde(default = "default_true")]
    pub enable_metrics: bool,

    /// Enable detailed logging
    #[serde(default = "default_true")]
    pub enable_detailed_logging: bool,
}

fn default_warn_max_depth() -> usize { 3 }
fn default_warn_max_children() -> usize { 10 }
fn default_warn_total_dynamic_steps() -> usize { 50 }
fn default_true() -> bool { true }

impl Default for DecisionPointsConfig {
    fn default() -> Self {
        Self {
            warn_max_depth: default_warn_max_depth(),
            warn_max_children: default_warn_max_children(),
            warn_total_dynamic_steps: default_warn_total_dynamic_steps(),
            enable_metrics: true,
            enable_detailed_logging: true,
        }
    }
}
```

### 7.3 Metrics

**File**: `tasker-shared/src/metrics/decision_points.rs` (NEW)

```rust
use opentelemetry::metrics::{Counter, Histogram};
use once_cell::sync::OnceCell;

/// Total number of decision point executions
pub static DECISION_EXECUTIONS_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();

/// Depth of decision point nesting
pub static DECISION_DEPTH: OnceCell<Histogram<u64>> = OnceCell::new();

/// Number of steps created by a single decision
pub static DYNAMIC_STEPS_CREATED: OnceCell<Histogram<u64>> = OnceCell::new();

/// Total number of decision point errors
pub static DECISION_ERRORS_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();

/// Initialize decision point metrics
pub fn init_decision_point_metrics(meter: &opentelemetry::metrics::Meter) {
    let counter = meter
        .u64_counter("decision_point.executions.total")
        .with_description("Total number of decision point executions")
        .init();
    let _ = DECISION_EXECUTIONS_TOTAL.set(counter);

    let histogram = meter
        .u64_histogram("decision_point.depth")
        .with_description("Depth of decision point nesting")
        .init();
    let _ = DECISION_DEPTH.set(histogram);

    let histogram = meter
        .u64_histogram("decision_point.steps_created")
        .with_description("Number of steps created by decision")
        .init();
    let _ = DYNAMIC_STEPS_CREATED.set(histogram);

    let counter = meter
        .u64_counter("decision_point.errors.total")
        .with_description("Total number of decision point errors")
        .init();
    let _ = DECISION_ERRORS_TOTAL.set(counter);
}
```

### 7.4 Logging Integration

**Update DecisionPointService**:

```rust
impl DecisionPointService {
    async fn create_dynamic_steps(...) -> OrchestrationResult<DecisionProcessingResult> {
        // Debug: Step creation starting
        if self.context.tasker_config.decision_points.enable_detailed_logging {
            debug!(
                decision_step = %decision_step_uuid,
                step_names = ?step_names,
                "Creating dynamic steps from decision"
            );
        }

        // ... create steps ...

        // Warn: Threshold exceeded
        if step_names.len() > self.context.tasker_config.decision_points.warn_max_children {
            warn!(
                decision_step = %decision_step_uuid,
                step_count = step_names.len(),
                threshold = self.context.tasker_config.decision_points.warn_max_children,
                "Decision creating many child steps - consider splitting workflow"
            );
        }

        // Metrics: Record execution
        if self.context.tasker_config.decision_points.enable_metrics {
            if let Some(counter) = DECISION_EXECUTIONS_TOTAL.get() {
                counter.add(1, &[
                    KeyValue::new("outcome", "create_steps"),
                ]);
            }

            if let Some(histogram) = DYNAMIC_STEPS_CREATED.get() {
                histogram.record(step_names.len() as u64, &[]);
            }
        }

        Ok(result)
    }
}
```

---

## Phase 8: Ruby FFI Integration

**Duration**: 2-3 days
**Goal**: Ensure Ruby handlers can return DecisionPointOutcome

### 8.1 Ruby Helper Module

**File**: `workers/ruby/lib/tasker_core/decision_point.rb` (NEW)

```ruby
# frozen_string_literal: true

module TaskerCore
  # Decision Point Helper
  #
  # Provides convenience methods for Ruby handlers to return decision point outcomes.
  # Handlers marked with `type: decision` in the task template should return
  # the result of one of these methods.
  #
  # @example No branches
  #   def execute(inputs, task_context, previous_results)
  #     outcome = DecisionPoint.no_branches
  #     success(outcome)
  #   end
  #
  # @example Create specific steps
  #   def execute(inputs, task_context, previous_results)
  #     risk_score = calculate_risk(inputs)
  #
  #     outcome = if risk_score > 75
  #       DecisionPoint.create_steps('manual_review', 'notify_compliance')
  #     else
  #       DecisionPoint.create_steps('auto_approve')
  #     end
  #
  #     success(outcome)
  #   end
  module DecisionPoint
    # Return no branches outcome
    #
    # Use this when the decision determines no further processing is needed.
    # The task will proceed to finalization if no other steps remain.
    #
    # @return [Hash] Decision outcome indicating no branches
    def self.no_branches
      { decision_type: 'no_branches' }
    end

    # Return create steps outcome
    #
    # Step names must exactly match step definitions in the task template.
    # Invalid step names will result in a permanent error.
    #
    # @param step_names [Array<String>] Names of steps to create
    # @return [Hash] Decision outcome with step names
    # @raise [ArgumentError] if step_names is empty
    def self.create_steps(*step_names)
      raise ArgumentError, 'Must provide at least one step name' if step_names.empty?

      {
        decision_type: 'create_steps',
        step_names: step_names
      }
    end
  end
end
```

### 8.2 Example Handler

**File**: `workers/ruby/lib/handlers/examples/risk_evaluation_handler.rb` (NEW)

```ruby
# frozen_string_literal: true

module Handlers
  module Examples
    # Example Decision Point Handler
    #
    # Demonstrates decision point workflow logic in Ruby.
    # Evaluates risk score and determines appropriate approval path.
    class RiskEvaluationHandler < TaskerCore::BaseStepHandler
      def execute(inputs, task_context, previous_results)
        # Extract risk score from previous step results
        validation_result = previous_results['validate_application']
        risk_score = validation_result&.dig('result', 'risk_score') || 0

        # Determine execution path based on risk
        outcome = case risk_score
        when 0..50
          # Low risk - auto approve
          TaskerCore::DecisionPoint.create_steps('auto_approve')
        when 51..75
          # Medium risk - enhanced verification
          TaskerCore::DecisionPoint.create_steps('enhanced_verification')
        else
          # High risk - manual review and compliance notification
          TaskerCore::DecisionPoint.create_steps('manual_review', 'notify_compliance')
        end

        success(outcome)
      rescue StandardError => e
        # Decision point can fail like any other step
        retryable_error("Risk evaluation failed: #{e.message}")
      end
    end
  end
end
```

### 8.3 Integration Test

**File**: `workers/ruby/spec/integration/decision_point_spec.rb` (NEW)

```ruby
require 'spec_helper'

RSpec.describe 'Decision Point Integration', type: :integration do
  describe 'risk evaluation workflow' do
    it 'creates manual review path for high risk' do
      # Create task with decision point template
      task_request = create_task_request(
        name: 'risk_evaluation',
        namespace: 'approval_workflow',
        context: { application_id: 12345 }
      )

      # Initialize task
      result = orchestration_client.create_task(task_request)
      task_uuid = result['task_uuid']

      # Execute validate_application step (sets risk_score = 80)
      execute_step(task_uuid, 'validate_application', { risk_score: 80 })

      # Execute decision point (should create manual_review and notify_compliance)
      decision_result = execute_step(task_uuid, 'evaluate_risk', {})

      expect(decision_result['success']).to be true
      expect(decision_result['result']).to eq({
        'decision_type' => 'create_steps',
        'step_names' => ['manual_review', 'notify_compliance']
      })

      # Verify steps were created
      task_status = orchestration_client.get_task_status(task_uuid)
      step_names = task_status['steps'].map { |s| s['name'] }

      expect(step_names).to include('manual_review')
      expect(step_names).to include('notify_compliance')
      expect(step_names).not_to include('auto_approve')
    end

    it 'creates auto approve path for low risk' do
      task_request = create_task_request(
        name: 'risk_evaluation',
        namespace: 'approval_workflow',
        context: { application_id: 67890 }
      )

      result = orchestration_client.create_task(task_request)
      task_uuid = result['task_uuid']

      # Execute with low risk score
      execute_step(task_uuid, 'validate_application', { risk_score: 30 })
      decision_result = execute_step(task_uuid, 'evaluate_risk', {})

      expect(decision_result['result']).to eq({
        'decision_type' => 'create_steps',
        'step_names' => ['auto_approve']
      })

      task_status = orchestration_client.get_task_status(task_uuid)
      step_names = task_status['steps'].map { |s| s['name'] }

      expect(step_names).to include('auto_approve')
      expect(step_names).not_to include('manual_review')
    end
  end
end
```

---

## Phase 9: Testing & Documentation

**Duration**: 4-5 days
**Goal**: Comprehensive test coverage and user-facing documentation

### 9.1 Test Fixtures

**Directory**: `tests/fixtures/task_templates/decision_workflows/` (NEW)

#### Simple Decision Template
**File**: `simple_decision.yaml`

```yaml
name: simple_decision
namespace_name: decision_test
version: 1.0.0
description: "Simple decision point with two possible branches"

steps:
  - name: upstream
    handler:
      callable: Test::UpstreamHandler

  - name: decide
    type: decision
    handler:
      callable: Test::DecisionHandler
    dependencies:
      - upstream

  - name: branch_a
    handler:
      callable: Test::BranchAHandler
    dependencies:
      - decide

  - name: branch_b
    handler:
      callable: Test::BranchBHandler
    dependencies:
      - decide
```

#### Nested Decisions Template
**File**: `nested_decisions.yaml`

```yaml
name: nested_decisions
namespace_name: decision_test
version: 1.0.0
description: "Decision point that creates another decision point"

steps:
  - name: initial
    handler:
      callable: Test::InitialHandler

  - name: first_decision
    type: decision
    handler:
      callable: Test::FirstDecisionHandler
    dependencies:
      - initial

  - name: second_decision
    type: decision
    handler:
      callable: Test::SecondDecisionHandler
    dependencies:
      - first_decision

  - name: final_branch_a
    handler:
      callable: Test::FinalBranchAHandler
    dependencies:
      - second_decision

  - name: final_branch_b
    handler:
      callable: Test::FinalBranchBHandler
    dependencies:
      - second_decision
```

#### Conditional Parallel Template
**File**: `conditional_parallel.yaml`

```yaml
name: conditional_parallel
namespace_name: decision_test
version: 1.0.0
description: "Decision creates parallel execution paths"

steps:
  - name: evaluate
    handler:
      callable: Test::EvaluateHandler

  - name: decide_parallel
    type: decision
    handler:
      callable: Test::DecideParallelHandler
    dependencies:
      - evaluate

  # Parallel branches (both depend on decision, not each other)
  - name: parallel_a
    handler:
      callable: Test::ParallelAHandler
    dependencies:
      - decide_parallel

  - name: parallel_b
    handler:
      callable: Test::ParallelBHandler
    dependencies:
      - decide_parallel

  # Convergence point
  - name: merge_results
    handler:
      callable: Test::MergeHandler
    dependencies:
      - parallel_a
      - parallel_b
```

#### No Branches Decision Template
**File**: `no_branches_decision.yaml`

```yaml
name: no_branches
namespace_name: decision_test
version: 1.0.0
description: "Decision that may create no steps"

steps:
  - name: check_condition
    handler:
      callable: Test::CheckHandler

  - name: decide_proceed
    type: decision
    handler:
      callable: Test::DecideProceedHandler
    dependencies:
      - check_condition

  - name: optional_processing
    handler:
      callable: Test::OptionalHandler
    dependencies:
      - decide_proceed
```

### 9.2 Integration Tests

**File**: `tests/integration/decision_point_tests.rs` (NEW)

```rust
//! Decision Point Integration Tests
//!
//! Comprehensive end-to-end testing of decision point functionality.

use sqlx::PgPool;
use std::sync::Arc;
use tasker_shared::system_context::SystemContext;
use uuid::Uuid;

mod helpers;
use helpers::*;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_simple_decision_creates_correct_branch(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = Arc::new(SystemContext::with_pool(pool).await?);

    // Create task with simple decision template
    let task_uuid = create_task_with_template(
        &context,
        "decision_workflows/simple_decision.yaml"
    ).await?;

    // Execute upstream step
    execute_step(&context, task_uuid, "upstream", json!({})).await?;

    // Execute decision - chooses branch_a
    let decision_result = execute_decision_step(
        &context,
        task_uuid,
        "decide",
        vec!["branch_a"]
    ).await?;

    assert_eq!(decision_result.steps_created, 1);

    // Verify branch_a exists, branch_b does not
    let task_steps = get_task_steps(&context, task_uuid).await?;
    assert!(task_steps.iter().any(|s| s.name == "branch_a"));
    assert!(!task_steps.iter().any(|s| s.name == "branch_b"));

    // Execute branch_a and verify task completes
    execute_step(&context, task_uuid, "branch_a", json!({})).await?;

    let task_status = get_task_status(&context, task_uuid).await?;
    assert_eq!(task_status.state, "Complete");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_decision_no_branches_outcome(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = Arc::new(SystemContext::with_pool(pool).await?);

    let task_uuid = create_task_with_template(
        &context,
        "decision_workflows/no_branches_decision.yaml"
    ).await?;

    execute_step(&context, task_uuid, "check_condition", json!({})).await?;

    // Execute decision - returns no branches
    let decision_result = execute_decision_step_no_branches(
        &context,
        task_uuid,
        "decide_proceed",
    ).await?;

    assert_eq!(decision_result.steps_created, 0);

    // Verify optional_processing was NOT created
    let task_steps = get_task_steps(&context, task_uuid).await?;
    assert!(!task_steps.iter().any(|s| s.name == "optional_processing"));

    // Task should complete with only check_condition and decide_proceed
    let task_status = get_task_status(&context, task_uuid).await?;
    assert_eq!(task_status.state, "Complete");

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_invalid_step_name_rejected(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = Arc::new(SystemContext::with_pool(pool).await?);

    let task_uuid = create_task_with_template(
        &context,
        "decision_workflows/simple_decision.yaml"
    ).await?;

    execute_step(&context, task_uuid, "upstream", json!({})).await?;

    // Execute decision with invalid step name
    let result = execute_decision_step(
        &context,
        task_uuid,
        "decide",
        vec!["nonexistent_step"]
    ).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found in template"));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_cycle_detection_prevents_invalid_dag(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create template where decision outcome would create a cycle
    // (requires special test template)

    let context = Arc::new(SystemContext::with_pool(pool).await?);

    let task_uuid = create_task_with_template(
        &context,
        "decision_workflows/cycle_test.yaml"
    ).await?;

    execute_step(&context, task_uuid, "step_a", json!({})).await?;

    // Decision tries to create step_b which depends on step_c which depends on step_a
    // Creating this edge would form a cycle
    let result = execute_decision_step(
        &context,
        task_uuid,
        "decide",
        vec!["step_b"]
    ).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cycle"));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_sql_functions_work_with_mixed_step_sources(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = Arc::new(SystemContext::with_pool(pool).await?);

    let task_uuid = create_task_with_template(
        &context,
        "decision_workflows/simple_decision.yaml"
    ).await?;

    // Get initial step count (created during initialization)
    let initial_steps = get_task_steps(&context, task_uuid).await?;
    let initial_count = initial_steps.len();

    execute_step(&context, task_uuid, "upstream", json!({})).await?;

    // Execute decision - creates branch_a dynamically
    execute_decision_step(&context, task_uuid, "decide", vec!["branch_a"]).await?;

    // Get step readiness status - should include dynamically created step
    let sql_executor = SqlFunctionExecutor::new(context.database_pool().clone());
    let readiness = sql_executor
        .get_step_readiness_status(task_uuid, None)
        .await?;

    // Should see initial steps + dynamically created branch_a
    assert!(readiness.iter().any(|s| s.name == "branch_a"));

    // get_task_execution_context should aggregate all steps
    let exec_context = sql_executor
        .get_task_execution_context(task_uuid)
        .await?;

    assert_eq!(exec_context.total_steps, initial_count + 1);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_idempotent_retry_after_partial_creation(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = Arc::new(SystemContext::with_pool(pool).await?);

    let task_uuid = create_task_with_template(
        &context,
        "decision_workflows/simple_decision.yaml"
    ).await?;

    execute_step(&context, task_uuid, "upstream", json!({})).await?;

    // First attempt - simulate partial creation by manually creating only 1 of 2 steps
    // (This requires direct database access in test)

    // Second attempt - should complete successfully (idempotent)
    let result = execute_decision_step(
        &context,
        task_uuid,
        "decide",
        vec!["branch_a", "branch_b"]
    ).await?;

    assert_eq!(result.steps_created, 2);

    // Verify both steps exist and task can complete
    execute_step(&context, task_uuid, "branch_a", json!({})).await?;
    execute_step(&context, task_uuid, "branch_b", json!({})).await?;

    let task_status = get_task_status(&context, task_uuid).await?;
    assert_eq!(task_status.state, "Complete");

    Ok(())
}
```

### 9.3 Documentation

#### Implementation Details
**File**: Already being created! (`docs/ticket-specs/TAS-53/implementation.md`)

#### User Guide
**File**: `docs/decision-points.md` (NEW)

```markdown
# Decision Point Workflows

**Status**: Active
**Audience**: Template Authors, Handler Developers
**Related**: [States and Lifecycles](states-and-lifecycles.md) | [Task Templates](task-and-step-readiness-and-execution.md)

## Overview

Decision points enable workflows to dynamically determine execution paths based on runtime evaluation of step results. Instead of creating all possible workflow steps upfront, decision point steps evaluate their inputs and create only the branches that are actually needed.

## When to Use Decision Points

Use decision points when:
- **Runtime-dependent paths**: Execution path can't be determined until runtime
- **Avoid orphaned steps**: You don't want to create steps that will never execute
- **Complex conditionals**: Business logic determines which of many paths to take
- **Resource optimization**: Creating unused steps is wasteful

Don't use decision points when:
- **All paths always execute**: Use parallel steps instead
- **Simple filtering**: Use step guards or skippable steps
- **Static DAG**: Execution path is predetermined

## Writing Decision Point Templates

### Step Definition

Mark a step as a decision point using `type: decision`:

```yaml
steps:
  - name: evaluate_risk
    type: decision  # Marks this as a decision point
    description: "Determine approval path based on risk score"
    handler:
      callable: RiskEvaluation::DecisionHandler
    dependencies:
      - validate_application  # Must have at least one dependency
```

**Requirements**:
- Must have `type: decision`
- Must have ≥1 upstream dependency
- Must have ≥1 downstream dependent step

### Downstream Steps

Define all possible branches in the template:

```yaml
steps:
  # Decision point
  - name: evaluate_risk
    type: decision
    # ... handler config ...

  # High risk path
  - name: manual_review
    handler:
      callable: Review::ManualReviewHandler
    dependencies:
      - evaluate_risk  # Explicit dependency on decision

  # Low risk path
  - name: auto_approve
    handler:
      callable: Approval::AutoApprovalHandler
    dependencies:
      - evaluate_risk  # Explicit dependency on decision
```

**All possible steps must be defined in template** - decision points select from predefined options.

## Handler Implementation

### Ruby Handler

```ruby
class RiskEvaluationHandler < TaskerCore::BaseStepHandler
  def execute(inputs, task_context, previous_results)
    risk_score = calculate_risk(previous_results)

    outcome = case risk_score
    when 0..50
      # Low risk - auto approve
      TaskerCore::DecisionPoint.create_steps('auto_approve')
    when 51..75
      # Medium risk - enhanced checks
      TaskerCore::DecisionPoint.create_steps('enhanced_verification')
    else
      # High risk - manual review and compliance
      TaskerCore::DecisionPoint.create_steps('manual_review', 'notify_compliance')
    end

    success(outcome)
  rescue StandardError => e
    retryable_error("Risk evaluation failed: #{e.message}")
  end
end
```

**Helper Methods**:
- `DecisionPoint.no_branches` - No further steps needed
- `DecisionPoint.create_steps(*names)` - Create specific steps

### Rust Handler

```rust
impl StepHandler for RiskEvaluationHandler {
    async fn execute(
        &self,
        inputs: HashMap<String, Value>,
        task_context: Value,
        previous_results: HashMap<String, StepExecutionResult>,
    ) -> StepExecutionResult {
        let risk_score = self.calculate_risk(&previous_results);

        let outcome = if risk_score > 75 {
            json!({
                "decision_type": "create_steps",
                "step_names": ["manual_review", "notify_compliance"]
            })
        } else if risk_score > 50 {
            json!({
                "decision_type": "create_steps",
                "step_names": ["enhanced_verification"]
            })
        } else {
            json!({
                "decision_type": "create_steps",
                "step_names": ["auto_approve"]
            })
        };

        StepExecutionResult::success(
            self.step_uuid,
            outcome,
            execution_time,
            None
        )
    }
}
```

## Execution Flow

1. **Task Creation**: Only steps up to decision boundaries are created
2. **Decision Execution**: Handler evaluates inputs and returns outcome
3. **Dynamic Creation**: Orchestration creates selected steps
4. **Continued Execution**: Newly created steps are discovered and enqueued
5. **Task Completion**: All steps (initial + dynamic) must complete

## Error Handling

Decision points can fail like any other step:

```ruby
def execute(inputs, task_context, previous_results)
  # Permanent error - bad data
  if previous_results['validate'].nil?
    permanent_error("Missing validation results")
  end

  # Retryable error - transient failure
  outcome = api_call_to_decision_service(inputs)
rescue NetworkError => e
  retryable_error("Decision service unavailable: #{e.message}")
end
```

**Error types**:
- **Permanent**: Invalid step names, missing data, business logic errors
- **Retryable**: Network failures, temporary unavailability

## Monitoring and Observability

Decision points are instrumented with:

**Metrics**:
- `decision_point.executions.total` - Total executions
- `decision_point.steps_created` - Histogram of steps created per decision
- `decision_point.depth` - Nesting depth of decisions
- `decision_point.errors.total` - Error count by type

**Logs**:
- Decision outcome parsing
- Step creation events
- Warning when thresholds exceeded

**Configuration** (`config/tasker/base/decision_points.toml`):
```toml
[decision_points]
warn_max_depth = 3           # Warn if nesting > 3 levels
warn_max_children = 10       # Warn if decision creates > 10 steps
warn_total_dynamic_steps = 50 # Warn if task has > 50 dynamic steps
```

## Examples

### Simple Binary Decision

```yaml
steps:
  - name: check_eligibility
    type: decision
    handler:
      callable: Eligibility::CheckHandler
    dependencies:
      - gather_data

  - name: approve
    dependencies: [check_eligibility]
    # ... handler ...

  - name: reject
    dependencies: [check_eligibility]
    # ... handler ...
```

### Nested Decisions

```yaml
steps:
  - name: first_tier_decision
    type: decision
    dependencies: [initial_step]

  - name: second_tier_decision
    type: decision
    dependencies: [first_tier_decision]

  - name: final_outcome_a
    dependencies: [second_tier_decision]

  - name: final_outcome_b
    dependencies: [second_tier_decision]
```

### Conditional Parallel Execution

```yaml
steps:
  - name: decide_parallel
    type: decision
    dependencies: [evaluate]

  # Parallel branches (no interdependency)
  - name: parallel_a
    dependencies: [decide_parallel]

  - name: parallel_b
    dependencies: [decide_parallel]

  # Convergence
  - name: merge
    dependencies: [parallel_a, parallel_b]
```

## Best Practices

1. **Keep decisions focused**: Each decision should have clear, distinct outcomes
2. **Limit nesting**: Deep decision trees are hard to understand and debug
3. **Document branches**: Comment each possible outcome in template
4. **Test all paths**: Ensure handlers work for all possible branches
5. **Monitor warnings**: Pay attention to threshold warnings in logs
6. **Error handling**: Distinguish permanent vs retryable errors
7. **Idempotency**: Handlers should be safe to retry

## Limitations

- **No dynamic templates**: Can only create steps defined in template
- **No runtime modification**: Can't change step configuration dynamically
- **Single outcome**: Decision executes once, outcome is final
- **No conditional dependencies**: Dependencies are explicit, not conditional

---

← Back to [Documentation Hub](README.md)
```

#### Update Existing Docs

**File**: `docs/states-and-lifecycles.md` - Add section:

```markdown
## Decision Point Lifecycle

Decision point steps follow a special lifecycle:

1. **Created During Initialization**: Decision point steps are always created
2. **Execute Like Regular Steps**: Claimed by workers, processed normally
3. **Return Decision Outcome**: Result contains step names to create
4. **Trigger Dynamic Creation**: Orchestration creates selected steps
5. **Continue Workflow**: Newly created steps are discovered and enqueued

See [Decision Points](decision-points.md) for complete guide.
```

**File**: `docs/actors.md` - Add section:

```markdown
### DecisionPointActor

**Responsibility**: Handle dynamic workflow step creation based on decision outcomes

**Messages**:
- `ProcessDecisionOutcomeMessage`: Create steps from decision point execution

**Lifecycle**:
1. ResultProcessorActor detects decision point completion
2. Parses decision outcome from step result
3. Sends message to DecisionPointActor
4. Actor delegates to DecisionPointService
5. Service creates steps in atomic transaction
6. Triggers step discovery for new steps

**Integration**: Called by ResultProcessorActor after decision point step completes successfully.
```

---

## Summary

### Implementation Phases

| Phase | Duration | Key Deliverables |
|-------|----------|------------------|
| 1 | 2-3 days | Data model + validation |
| 2 | 2-3 days | Shared logic extraction |
| 3 | 3-4 days | Partial graph creation |
| 4 | 4-5 days | DecisionPointActor |
| 5 | 3-4 days | DecisionPointService |
| 6 | 2-3 days | State machine integration |
| 7 | 2-3 days | Instrumentation |
| 8 | 2-3 days | Ruby FFI |
| 9 | 4-5 days | Testing + docs |

**Total**: 26-36 days (~5-7 weeks)

### Key Architectural Decisions

1. ✅ **No new database columns** - Workflow edges provide parent tracking
2. ✅ **SQL functions unchanged** - Work automatically with dynamic steps
3. ✅ **StepExecutionResult extended** - Decision outcome in result field
4. ✅ **Shared logic extracted** - WorkflowStepCreator reused
5. ✅ **Explicit dependencies** - Template-declared, not implicit
6. ✅ **Instrumentation over limits** - Configurable warnings
7. ✅ **Backward compatible** - All changes additive

### Success Criteria

- ✅ Existing templates work unchanged
- ✅ Decision templates create partial graphs
- ✅ Decision outcomes trigger dynamic creation
- ✅ SQL functions work with mixed step sources
- ✅ Tasks complete after dynamic steps finish
- ✅ Idempotent retry behavior
- ✅ Comprehensive test coverage
- ✅ Complete documentation

---

## Implementation Status

**Status**: ✅ **COMPLETED** (October 2025)
**Completion Date**: 2025-10-28
**Final Test Results**: 797 tests passed, 6 skipped

### What Was Actually Implemented

The TAS-53 Dynamic Workflow Decision Points feature has been successfully completed with full Ruby and Rust support. The implementation differs slightly from the original 9-phase plan, as several phases were consolidated or found to be unnecessary.

#### Core Components Delivered

**1. Data Model & Type System** ✅
- `StepType` enum with `Standard`, `Decision`, and `Deferred` variants
- `DecisionPointOutcome` type with `NoBranches` and `CreateSteps` variants
- Template validation for decision point constraints
- Location: `tasker-shared/src/models/core/task_template.rs`

**2. Workflow Graph Segmentation** ✅
- `initial_step_set()` returns only steps up to decision boundaries
- `is_descendant_of_decision_point()` for transitive descendant detection
- Partial graph creation during task initialization
- Location: `tasker-shared/src/models/core/task_template.rs`

**3. Result Processing Integration** ✅
- Step result processor detects `decision_point_outcome` field in results
- Validates step names against template
- Creates workflow steps dynamically via message handler
- Triggers step discovery for newly created steps
- Location: `tasker-orchestration/src/orchestration/lifecycle/result_processing/`

**4. Ruby Worker Support** ✅
- `DecisionPointOutcome` type with factory methods
- Example handlers in `workers/ruby/lib/handlers/examples/conditional_approval/`
- Full E2E integration tests covering all routing scenarios
- Type-safe serialization matching Rust format
- Location: `workers/ruby/lib/tasker_core/types/decision_point_outcome.rb`

**5. Rust Worker Support** ✅
- `DecisionPointOutcome` in `tasker-shared/src/messaging/execution_types.rs`
- Example handlers in `workers/rust/src/step_handlers/conditional_approval_rust.rs`
- Namespace isolation (`conditional_approval_rust` vs `conditional_approval`)
- Full E2E integration tests with 4 test scenarios
- Location: `workers/rust/src/step_handlers/`

**6. Comprehensive Testing** ✅
- Ruby E2E tests: 4 scenarios (small/medium/large amounts + API validation)
- Rust E2E tests: 4 scenarios with identical coverage
- Both test suites validate decision point routing and deferred convergence
- Locations: `tests/e2e/ruby/conditional_approval_test.rs`, `tests/e2e/rust/conditional_approval_rust.rs`

**7. Documentation** ✅
- User-facing guide: `docs/conditional-workflows.md`
- Implementation spec: `docs/ticket-specs/TAS-53/implementation.md`
- Decision point E2E test documentation: `docs/testing/decision-point-e2e-tests.md`

### Key Design Decisions

**1. Simplified Actor Pattern**
- Originally planned separate `DecisionPointActor` (Phase 4)
- **Actually implemented**: Integrated into existing `ResultProcessorActor`
- Rationale: Decision processing is part of result handling flow
- No separate actor needed - cleaner integration

**2. No Separate DecisionPointService**
- Phase 5 planned dedicated service with ~400 lines
- **Actually implemented**: Direct message handler in result processing
- Rationale: Step creation logic already exists in workflow builder
- Avoided duplication by reusing existing infrastructure

**3. No Configuration System Changes**
- Phase 7 planned new `config/tasker/base/decision_points.toml`
- **Actually implemented**: Used existing orchestration configuration
- Rationale: No special limits or thresholds needed
- Template validation sufficient for safety

**4. Deferred Step Type Addition**
- Not in original spec but added during implementation
- Enables convergence steps with dynamic dependency resolution
- Uses intersection semantics: declared deps ∩ created steps
- Critical for finalization steps after decision branches

**5. Workflow Graph Segmentation**
- Phase 3 implementation was more sophisticated than planned
- `is_descendant_of_decision_point()` uses transitive closure
- Correctly identifies deferred steps as descendants via dependencies
- Example: `finalize_approval` → `[auto_approve, manager_approval]` → `routing_decision`

### What Was NOT Implemented

**1. Instrumentation/Metrics (Phase 7)**
- No OpenTelemetry metrics for decision points
- No warning thresholds for depth/children counts
- Rationale: Can be added incrementally if needed
- Template validation provides safety

**2. Separate DecisionPointActor (Phase 4)**
- Integrated into ResultProcessorActor instead
- Simpler architecture, fewer moving parts

**3. WorkflowStepCreator Extraction (Phase 2)**
- Not needed - message handler works directly
- Avoided premature abstraction

**4. Dedicated Configuration File (Phase 7)**
- No separate `decision_points.toml`
- Template validation sufficient for now

### Implementation Learnings

**1. Transitive Descendant Detection**
- Initial tests expected 3 steps at initialization
- Discovered `finalize_approval` is transitive descendant of decision point
- System correctly identifies and excludes from initial creation
- Updated tests to expect 2 steps (validate_request, routing_decision)

**2. Deferred Step Dependencies**
- Critical insight: List ALL possible dependencies in template
- Orchestration computes intersection at runtime
- Example:
  ```yaml
  dependencies:
    - auto_approve      # Might be created
    - manager_approval  # Might be created
    - finance_review    # Might be created
  ```
- System waits only for intersection of declared + created

**3. Namespace Isolation**
- Ruby and Rust handlers must use different namespaces
- Prevents workers competing for same tasks
- Pattern: `conditional_approval` (Ruby) vs `conditional_approval_rust` (Rust)

**4. Type Serialization**
- Ruby `DecisionPointOutcome#to_h` must match Rust serialization
- Both produce identical JSON structure
- Embedded in step result's `decision_point_outcome` field

**5. Integration Point**
- Single integration point in result processing
- Detects decision outcome → creates steps → triggers discovery
- No changes needed to state machine or SQL functions
- Everything else works automatically

### Files Created/Modified

**Created**:
- `workers/ruby/lib/tasker_core/types/decision_point_outcome.rb`
- `workers/ruby/lib/handlers/examples/conditional_approval/` (6 handlers)
- `workers/ruby/spec/handlers/examples/conditional_approval/` (handler specs)
- `workers/rust/src/step_handlers/conditional_approval_rust.rs`
- `tests/e2e/ruby/conditional_approval_test.rs`
- `tests/e2e/rust/conditional_approval_rust.rs`
- `tests/fixtures/task_templates/ruby/conditional_approval_handler.yaml`
- `tests/fixtures/task_templates/rust/conditional_approval_rust.yaml`
- `docs/testing/decision-point-e2e-tests.md`

**Modified**:
- `tasker-shared/src/models/core/task_template.rs` (StepType, workflow graph logic)
- `tasker-shared/src/messaging/execution_types.rs` (DecisionPointOutcome)
- `workers/ruby/lib/tasker_core/types.rb` (exports)
- `workers/ruby/lib/tasker_core/types/task_template.rb` (step_type field)
- `workers/rust/src/step_handlers/mod.rs` (module export)
- `workers/rust/src/step_handlers/registry.rs` (handler registration)
- `tests/e2e/ruby/mod.rs` (test module)
- `tests/e2e/rust/mod.rs` (test module)

### Test Coverage

**Total Tests**: 797 passed, 6 skipped

**Ruby E2E Tests** (4 scenarios):
1. `test_conditional_approval_small_amount` - $500 → auto_approve only
2. `test_conditional_approval_medium_amount` - $3,000 → manager_approval only
3. `test_conditional_approval_large_amount` - $10,000 → dual approval
4. `test_conditional_approval_api_validation` - API contract validation

**Rust E2E Tests** (4 scenarios):
1. `test_conditional_approval_small_amount` - Identical to Ruby
2. `test_conditional_approval_medium_amount` - Identical to Ruby
3. `test_conditional_approval_large_amount` - Identical to Ruby
4. `test_conditional_approval_api_validation` - Identical to Ruby

All tests validate:
- Decision point routing logic
- Dynamic step creation
- Deferred step dependency resolution
- Task completion with mixed step sources
- Namespace isolation between Ruby/Rust

### Performance Impact

- **Decision Overhead**: ~10-20ms per decision point
- **Step Creation**: O(n) where n = number of created steps
- **SQL Functions**: No changes needed - work automatically
- **State Machine**: No changes needed - handles dynamic steps seamlessly

### Backward Compatibility

✅ **100% Backward Compatible**
- All existing templates work unchanged
- `StepType` defaults to `Standard` via `#[serde(default)]`
- No database schema changes
- No breaking API changes

### Success Criteria Met

- ✅ Decision point steps execute and return outcomes
- ✅ Orchestration creates steps dynamically
- ✅ Deferred steps use intersection semantics
- ✅ SQL functions work with mixed step sources
- ✅ Tasks complete after dynamic steps finish
- ✅ Ruby and Rust handlers both working
- ✅ Full E2E test coverage
- ✅ Namespace isolation prevents conflicts
- ✅ Comprehensive documentation

---

## Reference Implementations

For developers implementing new decision point workflows, refer to these working examples:

### Ruby Implementation

**Location**: `workers/ruby/lib/handlers/examples/conditional_approval/`

**Files**:
- `validate_request_handler.rb` - Initial validation step
- `routing_decision_handler.rb` - **Decision point handler** (key example)
- `auto_approve_handler.rb` - Auto-approval path
- `manager_approval_handler.rb` - Manager approval path
- `finance_review_handler.rb` - Finance review path
- `finalize_approval_handler.rb` - **Convergence step** (deferred type)

**Template**: `tests/fixtures/task_templates/ruby/conditional_approval_handler.yaml`

**Key Patterns**:
```ruby
# Decision point outcome creation
outcome = TaskerCore::Types::DecisionPointOutcome.create_steps(steps)

# Embed in result
result_data = {
  decision_point_outcome: outcome.to_h,
  # ... other data
}
```

### Rust Implementation

**Location**: `workers/rust/src/step_handlers/conditional_approval_rust.rs`

**Handlers** (all in single file):
- `ValidateRequestHandler` - Initial validation
- `RoutingDecisionHandler` - **Decision point** (key example)
- `AutoApproveHandler` - Auto-approval path
- `ManagerApprovalHandler` - Manager approval path
- `FinanceReviewHandler` - Finance review path
- `FinalizeApprovalHandler` - **Convergence step** (deferred type)

**Template**: `tests/fixtures/task_templates/rust/conditional_approval_rust.yaml`

**Key Patterns**:
```rust
// Decision point outcome creation
let outcome = DecisionPointOutcome::create_steps(
    steps.iter().map(|s| s.to_string()).collect()
);

// Embed in result
let result_data = json!({
    "decision_point_outcome": outcome.to_value(),
    // ... other data
});
```

### E2E Tests

**Ruby Tests**: `tests/e2e/ruby/conditional_approval_test.rs`
- Small amount scenario ($500)
- Medium amount scenario ($3,000)
- Large amount scenario ($10,000)
- API validation test

**Rust Tests**: `tests/e2e/rust/conditional_approval_rust.rs`
- Identical scenarios to Ruby tests
- Demonstrates namespace isolation
- Validates parallel Ruby/Rust execution

**Test Patterns**:
- Use `IntegrationTestManager` for setup
- Create tasks with `create_task_request(namespace, name, context)`
- Wait for completion with `wait_for_task_completion()`
- Validate step creation and completion
- Check deferred step dependency resolution

### Quick Start

To implement a new decision point workflow:

1. **Create YAML Template**:
   ```yaml
   steps:
     - name: my_decision
       type: decision  # Mark as decision point
       dependencies: [upstream_step]
       handler:
         callable: MyNamespace::DecisionHandler

     - name: convergence
       type: deferred  # Mark as deferred
       dependencies:
         - branch_a  # List ALL possible branches
         - branch_b
         - branch_c
       handler:
         callable: MyNamespace::ConvergenceHandler
   ```

2. **Implement Handler** (Ruby):
   ```ruby
   def call(task, _sequence, _step)
     # Business logic
     steps = determine_steps(task.context)

     # Create outcome
     outcome = TaskerCore::Types::DecisionPointOutcome.create_steps(steps)

     # Return with embedded outcome
     TaskerCore::Types::StepHandlerCallResult.success(
       result: { decision_point_outcome: outcome.to_h }
     )
   end
   ```

3. **Implement Handler** (Rust):
   ```rust
   async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
       // Business logic
       let steps = determine_steps(&step_data.task.context);

       // Create outcome
       let outcome = DecisionPointOutcome::create_steps(steps);

       // Return with embedded outcome
       Ok(success_result(
           step_uuid,
           json!({ "decision_point_outcome": outcome.to_value() }),
           execution_time_ms,
           None,
       ))
   }
   ```

4. **Write E2E Tests**:
   - Test all routing scenarios
   - Validate step creation
   - Verify convergence with deferred dependencies
   - Check final task completion

---

**Implementation Complete!** All core functionality delivered and tested.
