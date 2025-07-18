//! # Factory Wrappers - Test Helpers Only
//!
//! **DEVELOPMENT ONLY**: These functions wrap the existing Rust factory system
//! to provide Ruby test access without reimplementing database operations.
//!
//! ## Design Principles
//!
//! 1. **Delegate to Existing Factories**: All functions call the established
//!    factory system in `tests/factories/` instead of reimplementing database logic
//!
//! 2. **Thin Wrappers**: Minimal FFI conversion between Ruby arguments and Rust factory calls
//!
//! 3. **Same Patterns**: Use identical patterns as Rust integration tests

use crate::context::{json_to_ruby_value, ValidationConfig};
use magnus::{Error, RModule, Value};
use tracing::{debug, warn};

// Import core models directly (available from main crate)
use tasker_core::models::{
    Task, WorkflowStep, TaskNamespace, NamedTask, NamedStep, DependentSystem, WorkflowStepEdge,
    core::{
        task::NewTask,
        workflow_step::NewWorkflowStep,
        task_namespace::NewTaskNamespace,
        named_task::NewNamedTask,
        named_step::NewNamedStep,
        dependent_system::NewDependentSystem,
        workflow_step_edge::NewWorkflowStepEdge,
    }
};

use sqlx::PgPool;
use serde_json::json;
use std::sync::OnceLock;
use std::sync::Arc;

static GLOBAL_TESTING_FACTORY: OnceLock<Arc<TestingFactory>> = OnceLock::new();

/// 🎯 UNIFIED TESTING FACTORY - References OrchestrationSystem
///
/// This struct follows the unified architecture pattern where:
/// - TestingFactory references the OrchestrationSystem (not embedded in it)
/// - Database pool access goes through orchestration system
/// - Single source of truth for all orchestration resources
///
/// ## New Architecture Usage
///
/// ```rust
/// // Get from unified orchestration system (recommended)
/// let factory = TestingFactory::from_orchestration_system();
///
/// // Or inject orchestration system reference directly
/// let orchestration = initialize_unified_orchestration_system();
/// let factory = TestingFactory::with_orchestration_system(orchestration);
///
/// // Legacy: direct pool injection (discouraged)
/// let factory = TestingFactory::with_pool(my_pool);
/// ```
///
/// ## Architecture Benefits
///
/// - **Unified Resource Access**: Pool accessed through orchestration system
/// - **Single Initialization**: Prevents multiple pool creation
/// - **Configuration-Driven**: Inherits configuration from orchestration system
/// - **Proper Separation**: TestingFactory references orchestration, not embedded
#[derive(Clone)]
pub struct TestingFactory {
    orchestration_system: Option<Arc<crate::globals::OrchestrationSystem>>,
    pool: Option<PgPool>,  // Legacy direct pool support
}

impl TestingFactory {
    /// Create a new TestingFactory with the unified orchestration system
    pub fn new() -> Self {
        // 🎯 CRITICAL FIX: Use existing orchestration system instead of re-initializing
        // This prevents excessive calls to initialize_unified_orchestration_system
        debug!("🔧 TestingFactory::new() - reusing existing orchestration system singleton");
        let orchestration_system = crate::globals::get_global_orchestration_system();
        Self {
            orchestration_system: Some(orchestration_system),
            pool: None
        }
    }

    /// Create TestingFactory with orchestration system reference
    ///
    /// This constructor allows explicit injection of an orchestration system reference.
    pub fn with_orchestration_system(orchestration_system: Arc<crate::globals::OrchestrationSystem>) -> Self {
        Self {
            orchestration_system: Some(orchestration_system),
            pool: None
        }
    }

    /// Get database pool reference - unified access method
    ///
    /// This method provides unified access to the database pool regardless of
    /// how the TestingFactory was constructed.
    fn pool(&self) -> &PgPool {
        if let Some(orchestration_system) = &self.orchestration_system {
            orchestration_system.database_pool()
        } else if let Some(pool) = &self.pool {
            pool
        } else {
            panic!("TestingFactory not properly initialized - no pool or orchestration system available")
        }
    }

    /// Create a test task with the given options
    ///
    /// This method encapsulates all the logic for creating a test task,
    /// including creating dependent entities (named tasks, namespaces) as needed.
    pub async fn create_task(&self, options: serde_json::Value) -> serde_json::Value {
        // Validate input configuration ranges
        if let Err(validation_error) = self.validate_task_options(&options) {
            return json!({
                "error": format!("Task validation failed: {}", validation_error)
            });
        }
        // Extract options
        let task_name = options.get("name").and_then(|v| v.as_str()).unwrap_or("test_task");
        let namespace_name = options.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
        let version = options.get("version").and_then(|v| v.as_str()).unwrap_or("0.1.0");

        // Step 1: Create or find namespace
        let namespace = match create_or_find_namespace(self.pool(), namespace_name, "Test namespace created by Ruby helper").await {
            Ok(ns) => ns,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create namespace: {}", e)
                });
            }
        };

        // Step 2: Create or find named task
        let named_task = match create_or_find_named_task(self.pool(), task_name, namespace.task_namespace_id as i64, version).await {
            Ok(nt) => nt,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named task: {}", e)
                });
            }
        };

        // Handle context properly - if provided context is empty object, keep it empty
        let context = if let Some(provided_context) = options.get("context") {
            if provided_context.is_object() && provided_context.as_object().unwrap().is_empty() {
                // Explicitly empty context - respect it
                json!({})
            } else {
                // Use provided context as-is, don't merge with defaults
                provided_context.clone()
            }
        } else {
            // No context provided - use defaults
            json!({
                "test": true,
                "created_by": "ruby_test_helper"
            })
        };

        let tags = options.get("tags").cloned().unwrap_or_else(|| json!({
            "test": true,
            "auto_generated": true
        }));

        // Create NewTask (mimicking TaskFactory::create)
        let new_task = NewTask {
            named_task_id: named_task.named_task_id,
            requested_at: None, // Will default to NOW()
            initiator: options.get("initiator").and_then(|v| v.as_str()).map(|s| s.to_string()),
            source_system: Some("ruby_test_helpers".to_string()),
            reason: Some("Created by Ruby test helper".to_string()),
            bypass_steps: None,
            tags: Some(tags),
            context: Some(context),
            identity_hash: generate_test_identity_hash(),
        };

        // Create task using core model
        match Task::create(self.pool(), new_task).await {
            Ok(task) => json!({
                "task_id": task.task_id,
                "named_task_id": task.named_task_id,
                "name": named_task.name,
                "complete": task.complete,
                "context": task.context,
                "tags": task.tags,
                "status": if task.complete { "complete" } else { "pending" },
                "initiator": task.initiator,
                "source_system": task.source_system,
                "reason": task.reason,
                "requested_at": task.requested_at.and_utc().to_rfc3339(),
                "identity_hash": task.identity_hash,
                "created_by": "testing_factory"
            }),
            Err(e) => json!({
                "error": format!("Failed to create task: {}", e)
            })
        }
    }

    /// Create a test workflow step with the given options
    ///
    /// This method encapsulates all the logic for creating a test workflow step,
    /// including creating dependent entities (tasks, named steps) as needed.
    pub async fn create_workflow_step(&self, options: serde_json::Value) -> serde_json::Value {
        // Validate input configuration ranges
        if let Err(validation_error) = self.validate_workflow_step_options(&options) {
            return json!({
                "error": format!("Workflow step validation failed: {}", validation_error)
            });
        }
        // Require task_id (don't create dummy tasks)
        let task_id = match options.get("task_id") {
            Some(val) => {
                // Handle both integer and float representations
                if let Some(id) = val.as_i64() {
                    debug!("🔍 FACTORY: Using provided task_id (int): {}", id);
                    id
                } else if let Some(id) = val.as_f64() {
                    let id = id as i64;
                    debug!("🔍 FACTORY: Using provided task_id (float): {}", id);
                    id
                } else {
                    return json!({
                        "error": "task_id must be a number"
                    });
                }
            },
            None => {
                return json!({
                    "error": "task_id is required for workflow step creation"
                });
            }
        };

        let dependent_system_name = options.get("dependent_system").and_then(|v| v.as_str()).unwrap_or("default_system");
        let dependent_system = match create_or_find_dependent_system(self.pool(), dependent_system_name, dependent_system_name).await {
            Ok(ds) => ds,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create dependent system: {}", e)
                });
            }
        };

        // Extract step name from options (check both "name" and "step_name" for compatibility)
        let step_name = options.get("name").and_then(|v| v.as_str())
            .or_else(|| options.get("step_name").and_then(|v| v.as_str()))
            .unwrap_or("test_step");

        // Create or find named step using provided name
        let named_step = match create_or_find_named_step(self.pool(), step_name, dependent_system.dependent_system_id).await {
            Ok(ns) => ns,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named step: {}", e)
                });
            }
        };

        // Extract options with defaults (mimicking WorkflowStepFactory defaults)
        let inputs = options.get("inputs").cloned().unwrap_or_else(|| json!({
            "test_input": true,
            "created_by": "ruby_test_helper"
        }));

        // Create NewWorkflowStep (mimicking WorkflowStepFactory::create)
        let new_step = NewWorkflowStep {
            task_id,
            named_step_id: named_step.named_step_id,
            retryable: Some(true),
            retry_limit: Some(3),
            inputs: Some(inputs),
            skippable: Some(false),
        };

        debug!("🔍 FACTORY: About to create WorkflowStep with task_id: {}", task_id);

        // Create workflow step using core model
        match WorkflowStep::create(self.pool(), new_step).await {
            Ok(step) => {
                debug!("🔍 FACTORY: Created WorkflowStep with task_id: {}, expected: {}", step.task_id, task_id);
                json!({
                    "workflow_step_id": step.workflow_step_id,
                    "task_id": step.task_id,
                    "named_step_id": step.named_step_id,
                    "name": named_step.name,
                    "retryable": step.retryable,
                    "retry_limit": step.retry_limit,
                    "inputs": step.inputs,
                    "results": step.results,
                    "in_process": step.in_process,
                    "processed": step.processed,
                    "skippable": step.skippable,
                    "created_by": "testing_factory"
                })
            },
            Err(e) => json!({
                "error": format!("Failed to create workflow step: {}", e)
            })
        }
    }

    /// Create a complex workflow with multiple steps and dependencies
    ///
    /// This method creates workflows with different patterns:
    /// - Linear: A -> B -> C -> D
    /// - Diamond: A -> (B, C) -> D
    /// - Parallel: (A, B, C) -> D
    /// - Tree: A -> (B -> (D, E), C -> (F, G))
    pub async fn create_complex_workflow(&self, options: serde_json::Value) -> serde_json::Value {
        // Validate input configuration ranges
        if let Err(validation_error) = self.validate_complex_workflow_options(&options) {
            return json!({
                "error": format!("Complex workflow validation failed: {}", validation_error)
            });
        }
        // Extract pattern type (default to linear)
        let pattern = options.get("pattern").and_then(|v| v.as_str()).unwrap_or("linear");
        let task_name = options.get("task_name").and_then(|v| v.as_str()).unwrap_or("complex_workflow");
        let namespace_name = options.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
        let version = options.get("version").and_then(|v| v.as_str()).unwrap_or("0.1.0");
        let dependent_system_name = options.get("dependent_system").and_then(|v| v.as_str()).unwrap_or("default_system");
        let dependent_system = match create_or_find_dependent_system(self.pool(), dependent_system_name, dependent_system_name).await {
            Ok(ds) => ds,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create dependent system: {}", e)
                });
            }
        };

        // First create the task
        let task_options = json!({
            "name": task_name,
            "namespace": namespace_name,
            "version": version,
            "context": options.get("context").cloned().unwrap_or(json!({"workflow_pattern": pattern}))
        });

        let task_result = self.create_task(task_options).await;
        if let Some(error) = task_result.get("error") {
            return task_result;
        }

        let task_id = match task_result.get("task_id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => return json!({"error": "Failed to get task_id from created task"})
        };

        // Create steps based on pattern
        let step_ids = match pattern {
            "linear" => self.create_linear_steps(task_id, dependent_system.dependent_system_id).await,
            "diamond" => self.create_diamond_steps(task_id, dependent_system.dependent_system_id).await,
            "parallel" => self.create_parallel_steps(task_id, dependent_system.dependent_system_id).await,
            "tree" => self.create_tree_steps(task_id, dependent_system.dependent_system_id).await,
            _ => return json!({"error": format!("Unknown workflow pattern: {}", pattern)})
        };

        match step_ids {
            Ok(ids) => json!({
                "task_id": task_id,
                "pattern": pattern,
                "step_ids": ids,
                "step_count": ids.len(),
                "created_by": "testing_factory"
            }),
            Err(e) => json!({
                "error": format!("Failed to create workflow steps: {}", e)
            })
        }
    }

    /// Create linear workflow steps: A -> B -> C -> D
    async fn create_linear_steps(&self, task_id: i64, dependent_system_id: i32) -> Result<Vec<i64>, sqlx::Error> {
        let mut step_ids = Vec::new();

        for i in 1..=4 {
            let step_name = format!("linear_step_{}", i);
            let named_step = create_or_find_named_step(self.pool(), &step_name, dependent_system_id).await?;

            let new_step = NewWorkflowStep {
                task_id,
                named_step_id: named_step.named_step_id,
                retryable: Some(true),
                retry_limit: Some(3),
                inputs: Some(json!({
                    "step_number": i,
                    "pattern": "linear"
                })),
                skippable: Some(false),
            };

            let workflow_step = WorkflowStep::create(self.pool(), new_step).await?;

            // Create dependency edge from previous step
            if i > 1 {
                let edge = NewWorkflowStepEdge {
                    from_step_id: step_ids[i - 2],
                    to_step_id: workflow_step.workflow_step_id,
                    name: "provides".to_string(),
                };
                WorkflowStepEdge::create(self.pool(), edge).await?;
            }

            step_ids.push(workflow_step.workflow_step_id);
        }

        Ok(step_ids)
    }

    /// Create diamond workflow steps: A -> (B, C) -> D
    async fn create_diamond_steps(&self, task_id: i64, dependent_system_id: i32) -> Result<Vec<i64>, sqlx::Error> {
        let mut step_ids = Vec::new();

        // Step A (start)
        let step_a = create_or_find_named_step(self.pool(), "diamond_start", dependent_system_id).await?;
        let ws_a = WorkflowStep::create(self.pool(), NewWorkflowStep {
            task_id,
            named_step_id: step_a.named_step_id,
            retryable: Some(true),
            retry_limit: Some(3),
            inputs: Some(json!({"position": "start"})),
            skippable: Some(false),
        }).await?;
        step_ids.push(ws_a.workflow_step_id);

        // Steps B and C (branches)
        for letter in ['b', 'c'].iter() {
            let step_name = format!("diamond_branch_{}", letter);
            let named_step = create_or_find_named_step(self.pool(), &step_name, dependent_system_id).await?;

            let ws = WorkflowStep::create(self.pool(), NewWorkflowStep {
                task_id,
                named_step_id: named_step.named_step_id,
                retryable: Some(true),
                retry_limit: Some(3),
                inputs: Some(json!({"branch": letter.to_string()})),
                skippable: Some(false),
            }).await?;

            // Create edge from A to B/C
            WorkflowStepEdge::create(self.pool(), NewWorkflowStepEdge {
                from_step_id: ws_a.workflow_step_id,
                to_step_id: ws.workflow_step_id,
                name: "provides".to_string(),
            }).await?;

            step_ids.push(ws.workflow_step_id);
        }

        // Step D (merge)
        let step_d = create_or_find_named_step(self.pool(), "diamond_merge", dependent_system_id).await?;
        let ws_d = WorkflowStep::create(self.pool(), NewWorkflowStep {
            task_id,
            named_step_id: step_d.named_step_id,
            retryable: Some(true),
            retry_limit: Some(3),
            inputs: Some(json!({"position": "merge"})),
            skippable: Some(false),
        }).await?;

        // Create edges from B and C to D
        for i in 1..=2 {
            WorkflowStepEdge::create(self.pool(), NewWorkflowStepEdge {
                from_step_id: step_ids[i],
                to_step_id: ws_d.workflow_step_id,
                name: "provides".to_string(),
            }).await?;
        }

        step_ids.push(ws_d.workflow_step_id);
        Ok(step_ids)
    }

    /// Create parallel workflow steps: (A, B, C) -> D
    async fn create_parallel_steps(&self, task_id: i64, dependent_system_id: i32) -> Result<Vec<i64>, sqlx::Error> {
        let mut step_ids = Vec::new();

        // Create parallel steps A, B, C
        for letter in ['a', 'b', 'c'].iter() {
            let step_name = format!("parallel_step_{}", letter);
            let named_step = create_or_find_named_step(self.pool(), &step_name, dependent_system_id).await?;

            let ws = WorkflowStep::create(self.pool(), NewWorkflowStep {
                task_id,
                named_step_id: named_step.named_step_id,
                retryable: Some(true),
                retry_limit: Some(3),
                inputs: Some(json!({"parallel_branch": letter.to_string()})),
                skippable: Some(false),
            }).await?;

            step_ids.push(ws.workflow_step_id);
        }

        // Create merge step D
        let step_d = create_or_find_named_step(self.pool(), "parallel_merge", dependent_system_id).await?;
        let ws_d = WorkflowStep::create(self.pool(), NewWorkflowStep {
            task_id,
            named_step_id: step_d.named_step_id,
            retryable: Some(true),
            retry_limit: Some(3),
            inputs: Some(json!({"position": "merge", "depends_on_count": 3})),
            skippable: Some(false),
        }).await?;

        // Create edges from all parallel steps to merge
        for i in 0..3 {
            WorkflowStepEdge::create(self.pool(), NewWorkflowStepEdge {
                from_step_id: step_ids[i],
                to_step_id: ws_d.workflow_step_id,
                name: "provides".to_string(),
            }).await?;
        }

        step_ids.push(ws_d.workflow_step_id);
        Ok(step_ids)
    }

    /// Create tree workflow steps: A -> (B -> (D, E), C -> (F, G))
    async fn create_tree_steps(&self, task_id: i64, dependent_system_id: i32) -> Result<Vec<i64>, sqlx::Error> {
        let mut step_ids = Vec::new();

        // Root step A
        let step_a = create_or_find_named_step(self.pool(), "tree_root", dependent_system_id).await?;
        let ws_a = WorkflowStep::create(self.pool(), NewWorkflowStep {
            task_id,
            named_step_id: step_a.named_step_id,
            retryable: Some(true),
            retry_limit: Some(3),
            inputs: Some(json!({"position": "root"})),
            skippable: Some(false),
        }).await?;
        step_ids.push(ws_a.workflow_step_id);

        // Branch steps B and C
        for (i, letter) in ['b', 'c'].iter().enumerate() {
            let step_name = format!("tree_branch_{}", letter);
            let named_step = create_or_find_named_step(self.pool(), &step_name, dependent_system_id).await?;

            let ws = WorkflowStep::create(self.pool(), NewWorkflowStep {
                task_id,
                named_step_id: named_step.named_step_id,
                retryable: Some(true),
                retry_limit: Some(3),
                inputs: Some(json!({"branch": letter.to_string()})),
                skippable: Some(false),
            }).await?;

            // Edge from A to B/C
            WorkflowStepEdge::create(self.pool(), NewWorkflowStepEdge {
                from_step_id: ws_a.workflow_step_id,
                to_step_id: ws.workflow_step_id,
                name: "provides".to_string(),
            }).await?;

            step_ids.push(ws.workflow_step_id);

            // Create leaf steps for each branch
            let leaves = if i == 0 { ['d', 'e'] } else { ['f', 'g'] };
            for leaf in leaves.iter() {
                let leaf_name = format!("tree_leaf_{}", leaf);
                let leaf_step = create_or_find_named_step(self.pool(), &leaf_name, dependent_system_id).await?;

                let ws_leaf = WorkflowStep::create(self.pool(), NewWorkflowStep {
                    task_id,
                    named_step_id: leaf_step.named_step_id,
                    retryable: Some(true),
                    retry_limit: Some(3),
                    inputs: Some(json!({"leaf": leaf.to_string()})),
                    skippable: Some(false),
                }).await?;

                // Edge from branch to leaf
                WorkflowStepEdge::create(self.pool(), NewWorkflowStepEdge {
                    from_step_id: ws.workflow_step_id,
                    to_step_id: ws_leaf.workflow_step_id,
                    name: "provides".to_string(),
                }).await?;

                step_ids.push(ws_leaf.workflow_step_id);
            }
        }

        Ok(step_ids)
    }

    /// Validation methods for configuration ranges
    fn validate_task_options(&self, options: &serde_json::Value) -> Result<(), String> {
        // Validate task name length
        if let Some(name) = options.get("name").and_then(|v| v.as_str()) {
            if name.is_empty() {
                return Err("Task name cannot be empty".to_string());
            }
            if name.len() > 100 {
                return Err(format!("Task name length {} exceeds maximum of 100", name.len()));
            }
            if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                return Err("Task name can only contain alphanumeric characters, underscores, and hyphens".to_string());
            }
        }

        // Validate namespace name
        if let Some(namespace) = options.get("namespace").and_then(|v| v.as_str()) {
            if namespace.len() > 50 {
                return Err(format!("Namespace length {} exceeds maximum of 50", namespace.len()));
            }
        }

        // Validate version format
        if let Some(version) = options.get("version").and_then(|v| v.as_str()) {
            if !version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
                return Err("Version can only contain alphanumeric characters, dots, and hyphens".to_string());
            }
            if version.len() > 20 {
                return Err(format!("Version length {} exceeds maximum of 20", version.len()));
            }
        }

        // Validate context size if provided
        if let Some(context) = options.get("context") {
            if context.is_object() && context.as_object().unwrap().len() > 50 {
                return Err("Context object cannot have more than 50 keys".to_string());
            }
        }

        Ok(())
    }

    fn validate_workflow_step_options(&self, options: &serde_json::Value) -> Result<(), String> {
        // Validate task_id is present and valid
        if let Some(task_id_val) = options.get("task_id") {
            if let Some(task_id) = task_id_val.as_i64() {
                if task_id <= 0 {
                    return Err("Task ID must be positive".to_string());
                }
                // Note: i64::MAX check is redundant since task_id is already i64, but keeping for consistency
                if task_id == i64::MAX {
                    return Err("Task ID cannot be maximum i64 value (reserved)".to_string());
                }
            } else if let Some(task_id_float) = task_id_val.as_f64() {
                if task_id_float <= 0.0 || task_id_float.fract() != 0.0 {
                    return Err("Task ID must be a positive integer".to_string());
                }
            } else {
                return Err("Task ID must be a number".to_string());
            }
        } else {
            return Err("Task ID is required".to_string());
        }

        // Validate step name
        if let Some(name) = options.get("name").and_then(|v| v.as_str()) {
            if name.is_empty() {
                return Err("Step name cannot be empty".to_string());
            }
            if name.len() > 100 {
                return Err(format!("Step name length {} exceeds maximum of 100", name.len()));
            }
        }
        if let Some(step_name) = options.get("step_name").and_then(|v| v.as_str()) {
            if step_name.is_empty() {
                return Err("Step name cannot be empty".to_string());
            }
            if step_name.len() > 100 {
                return Err(format!("Step name length {} exceeds maximum of 100", step_name.len()));
            }
        }

        // Validate dependent system name
        if let Some(dep_sys) = options.get("dependent_system").and_then(|v| v.as_str()) {
            if dep_sys.len() > 50 {
                return Err(format!("Dependent system name length {} exceeds maximum of 50", dep_sys.len()));
            }
        }

        // Validate inputs object size
        if let Some(inputs) = options.get("inputs") {
            if inputs.is_object() && inputs.as_object().unwrap().len() > 100 {
                return Err("Inputs object cannot have more than 100 keys".to_string());
            }
        }

        Ok(())
    }

    fn validate_complex_workflow_options(&self, options: &serde_json::Value) -> Result<(), String> {
        // Validate pattern type
        if let Some(pattern) = options.get("pattern").and_then(|v| v.as_str()) {
            let valid_patterns = ["linear", "diamond", "parallel", "tree"];
            if !valid_patterns.contains(&pattern) {
                return Err(format!("Invalid workflow pattern '{}'. Valid patterns: {}", pattern, valid_patterns.join(", ")));
            }
        }

        // Validate task name
        if let Some(task_name) = options.get("task_name").and_then(|v| v.as_str()) {
            if task_name.is_empty() {
                return Err("Task name cannot be empty".to_string());
            }
            if task_name.len() > 100 {
                return Err(format!("Task name length {} exceeds maximum of 100", task_name.len()));
            }
        }

        // Validate namespace
        if let Some(namespace) = options.get("namespace").and_then(|v| v.as_str()) {
            if namespace.len() > 50 {
                return Err(format!("Namespace length {} exceeds maximum of 50", namespace.len()));
            }
        }

        Ok(())
    }

    fn validate_foundation_options(&self, options: &serde_json::Value) -> Result<(), String> {
        // Validate step count
        if let Some(step_count) = options.get("step_count").and_then(|v| v.as_u64()) {
            if step_count == 0 {
                return Err("Step count must be positive".to_string());
            }
            if step_count > 100 {
                return Err(format!("Step count {} exceeds maximum of 100", step_count));
            }
        }

        // Validate namespace name
        if let Some(namespace) = options.get("namespace").and_then(|v| v.as_str()) {
            if namespace.is_empty() {
                return Err("Namespace cannot be empty".to_string());
            }
            if namespace.len() > 50 {
                return Err(format!("Namespace length {} exceeds maximum of 50", namespace.len()));
            }
        }

        // Validate task name
        if let Some(task_name) = options.get("task_name").and_then(|v| v.as_str()) {
            if task_name.is_empty() {
                return Err("Task name cannot be empty".to_string());
            }
            if task_name.len() > 100 {
                return Err(format!("Task name length {} exceeds maximum of 100", task_name.len()));
            }
        }

        Ok(())
    }

    /// Create test foundation data with the given options
    ///
    /// This method creates a complete test workflow with:
    /// - namespace, named task, named step (foundation entities)
    /// - An actual task instance
    /// - Multiple workflow steps with dependencies
    pub async fn create_foundation(&self, options: serde_json::Value) -> serde_json::Value {
        // Validate input configuration ranges
        if let Err(validation_error) = self.validate_foundation_options(&options) {
            return json!({
                "error": format!("Foundation validation failed: {}", validation_error)
            });
        }
        // Extract options with defaults
        let namespace_name = options.get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let task_name = options.get("task_name")
            .and_then(|v| v.as_str())
            .unwrap_or("dummy_task");
        let step_count = options.get("step_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;
        let pattern = options.get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("linear");
        let dependent_system_name = options.get("dependent_system")
            .and_then(|v| v.as_str())
            .unwrap_or("default_system");
        let dependent_system = match create_or_find_dependent_system(self.pool(), dependent_system_name, dependent_system_name).await {
            Ok(ds) => ds,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create dependent system: {}", e)
                });
            }
        };

        // Create foundation data
        let namespace = match create_or_find_namespace(self.pool(), namespace_name, "Test namespace created by Ruby helper").await {
            Ok(ns) => ns,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create namespace: {}", e)
                });
            }
        };

        let named_task = match create_or_find_named_task(self.pool(), task_name, namespace.task_namespace_id as i64, "0.1.0").await {
            Ok(nt) => nt,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named task: {}", e)
                });
            }
        };

        // Create the actual task instance
        let new_task = NewTask {
            named_task_id: named_task.named_task_id,
            requested_at: None,
            initiator: Some("foundation_factory".to_string()),
            source_system: Some("ruby_test_helpers".to_string()),
            reason: Some("Foundation data for testing".to_string()),
            bypass_steps: None,
            tags: Some(json!({"foundation": true, "pattern": pattern})),
            context: Some(json!({"test": true, "pattern": pattern})),
            identity_hash: generate_test_identity_hash(),
        };

        let task = match Task::create(self.pool(), new_task).await {
            Ok(t) => t,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create task: {}", e)
                });
            }
        };

        // Create workflow steps with dependencies
        let mut workflow_steps = Vec::new();
        let mut previous_step_id: Option<i64> = None;

        for i in 1..=step_count {
            let step_name = format!("foundation_step_{}", i);
            let named_step = match create_or_find_named_step(self.pool(), &step_name, dependent_system.dependent_system_id).await {
                Ok(ns) => ns,
                Err(e) => {
                    return json!({
                        "error": format!("Failed to create named step {}: {}", i, e)
                    });
                }
            };

            let new_step = NewWorkflowStep {
                task_id: task.task_id,
                named_step_id: named_step.named_step_id,
                retryable: Some(true),
                retry_limit: Some(3),
                inputs: Some(json!({
                    "step_number": i,
                    "foundation": true
                })),
                skippable: Some(false),
            };

            let workflow_step = match WorkflowStep::create(self.pool(), new_step).await {
                Ok(ws) => ws,
                Err(e) => {
                    return json!({
                        "error": format!("Failed to create workflow step {}: {}", i, e)
                    });
                }
            };

            // Create dependency edge from previous step
            if let Some(prev_id) = previous_step_id {
                let edge = NewWorkflowStepEdge {
                    from_step_id: prev_id,
                    to_step_id: workflow_step.workflow_step_id,
                    name: "provides".to_string(),
                };

                if let Err(e) = WorkflowStepEdge::create(self.pool(), edge).await {
                    warn!("Failed to create edge from {} to {}: {}", prev_id, workflow_step.workflow_step_id, e);
                }
            }

            workflow_steps.push(json!({
                "workflow_step_id": workflow_step.workflow_step_id,
                "name": named_step.name,
                "named_step_id": workflow_step.named_step_id,
                "in_process": workflow_step.in_process,
                "processed": workflow_step.processed
            }));

            previous_step_id = Some(workflow_step.workflow_step_id);
        }

        // Get the first named_step from the workflow steps we created (for backward compatibility)
        let first_named_step = if let Some(first_step) = workflow_steps.first() {
            let step_id = first_step.get("named_step_id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            match NamedStep::find_by_id(self.pool(), step_id).await {
                Ok(Some(ns)) => Some(json!({
                    "named_step_id": ns.named_step_id,
                    "name": ns.name,
                    "description": ns.description
                })),
                _ => None
            }
        } else {
            None
        };

        json!({
            "namespace": {
                "namespace_id": namespace.task_namespace_id,
                "name": namespace.name,
                "description": namespace.description
            },
            "named_task": {
                "named_task_id": named_task.named_task_id,
                "name": named_task.name,
                "task_namespace_id": named_task.task_namespace_id,
                "namespace_id": named_task.task_namespace_id, // Add for backward compatibility
                "version": named_task.version,
                "description": named_task.description
            },
            "named_step": first_named_step.unwrap_or(json!({
                "named_step_id": 0,
                "name": "dummy_step",
                "description": "No steps created"
            })),
            "task": {
                "task_id": task.task_id,
                "named_task_id": task.named_task_id,
                "name": named_task.name, // Add name for backward compatibility
                "complete": task.complete,
                "context": task.context,
                "tags": task.tags,
                "requested_at": task.requested_at.and_utc().to_rfc3339()
            },
            "workflow_steps": workflow_steps,
            "step_count": workflow_steps.len(),
            "created_by": "testing_factory"
        })
    }
}

// Helper functions (mimicking factory utility functions)

fn generate_test_identity_hash() -> String {
    use uuid::Uuid;
    Uuid::new_v4().to_string()
}

async fn create_or_find_named_task(pool: &PgPool, name: &str, namespace_id: i64, version: &str) -> Result<NamedTask, sqlx::Error> {
    // Try to find existing task first (find-or-create pattern)
    if let Some(existing) = NamedTask::find_by_name_version_namespace(
        pool,
        name,
        version,
        namespace_id,
    ).await? {
        return Ok(existing);
    }

    // Create new named task
    let new_named_task = NewNamedTask {
        name: name.to_string(),
        task_namespace_id: namespace_id,
        version: Some(version.to_string()),
        description: Some(format!("Test task: {}", name)),
        configuration: Some(json!({"test": true})),
    };

    NamedTask::create(pool, new_named_task).await
}

async fn create_or_find_named_step(pool: &PgPool, name: &str, dependent_system_id: i32) -> Result<NamedStep, sqlx::Error> {
    // Try to find existing step first
    let existing_steps = NamedStep::find_by_name(pool, name).await?;
    if let Some(existing) = existing_steps.first() {
        return Ok(existing.clone());
    }

    // Create new named step
    let new_named_step = NewNamedStep {
        dependent_system_id: dependent_system_id, // Default system ID for testing
        name: name.to_string(),
        description: Some(format!("Test step: {}", name)),
    };

    NamedStep::create(pool, new_named_step).await
}

async fn create_dummy_named_task(pool: &PgPool) -> Result<NamedTask, sqlx::Error> {
    // Create or find default namespace first
    let namespace = create_or_find_namespace(pool, "default", "Default test namespace").await?;

    // Try to find existing dummy task first (find-or-create pattern)
    if let Some(existing) = NamedTask::find_by_name_version_namespace(
        pool,
        "dummy_task",
        "0.1.0",
        namespace.task_namespace_id as i64
    ).await? {
        return Ok(existing);
    }

    // Create new dummy task
    let new_named_task = NewNamedTask {
        name: "dummy_task".to_string(),
        task_namespace_id: namespace.task_namespace_id as i64,
        version: Some("0.1.0".to_string()),
        description: Some("Dummy task for testing".to_string()),
        configuration: Some(json!({"test": true})),
    };

    NamedTask::create(pool, new_named_task).await
}

async fn create_or_find_dependent_system(pool: &PgPool, name: &str, description: &str) -> Result<DependentSystem, sqlx::Error> {
    // Try to find existing dependent system first
    if let Some(existing) = DependentSystem::find_by_name(pool, name).await? {
        return Ok(existing);
    }

    // Create new dependent system
    let new_dependent_system = NewDependentSystem {
        name: name.to_string(),
        description: Some(description.to_string()),
    };

    DependentSystem::create(pool, new_dependent_system).await
}

async fn create_dummy_named_step(pool: &PgPool) -> Result<NamedStep, sqlx::Error> {
    // Try to find existing dummy step first
    let existing_steps = NamedStep::find_by_name(pool, "dummy_step").await?;
    if let Some(existing) = existing_steps.first() {
        return Ok(existing.clone());
    }

    // Ensure dependent system exists
    let dependent_system = create_or_find_dependent_system(pool, "test_system", "Test system for Ruby helpers").await?;

    // Create new dummy step
    let new_named_step = NewNamedStep {
        name: "dummy_step".to_string(),
        dependent_system_id: dependent_system.dependent_system_id as i32,
        description: Some("Dummy step for testing".to_string()),
    };

    NamedStep::create(pool, new_named_step).await
}

async fn create_or_find_namespace(pool: &PgPool, name: &str, description: &str) -> Result<TaskNamespace, sqlx::Error> {
    // Try to find existing namespace first
    if let Some(existing) = TaskNamespace::find_by_name(pool, name).await? {
        return Ok(existing);
    }

    // Create new namespace
    let new_namespace = NewTaskNamespace {
        name: name.to_string(),
        description: Some(description.to_string()),
    };

    TaskNamespace::create(pool, new_namespace).await
}

async fn create_dummy_task(pool: &PgPool) -> Result<Task, sqlx::Error> {
    let named_task = create_dummy_named_task(pool).await?;

    let new_task = NewTask {
        named_task_id: named_task.named_task_id,
        requested_at: None,
        initiator: Some("test_helper".to_string()),
        source_system: Some("ruby_test_helpers".to_string()),
        reason: Some("Auto-created for step testing".to_string()),
        bypass_steps: None,
        tags: Some(json!({"test": true, "auto_created": true})),
        context: Some(json!({"test": true, "created_by": "helper"})),
        identity_hash: generate_test_identity_hash(),
    };

    Task::create(pool, new_task).await
}

pub fn get_global_testing_factory() -> Arc<TestingFactory> {
  debug!("🎯 UNIFIED ENTRY: initialize_global_testing_factory called");
  GLOBAL_TESTING_FACTORY.get_or_init(|| {
      // Use the unified orchestration system to create TestingFactory
      debug!("🎯 UNIFIED ENTRY: Creating TestingFactory from orchestration system");
      let testing_factory = TestingFactory::new();
      Arc::new(testing_factory)
  }).clone()
}


/// Create test task using TestingFactory
/// FFI wrapper that uses the TestingFactory struct for cleaner architecture
fn create_test_task_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    // Use strict validation for test factory inputs
    let validation_config = ValidationConfig {
        max_string_length: 1000,    // Stricter for test data
        max_array_length: 100,      // Reasonable for test scenarios
        max_object_depth: 5,        // Prevent deeply nested test data
        max_object_keys: 50,        // Reasonable for test configurations
        max_numeric_value: 1e12,    // Large but reasonable for test IDs
        min_numeric_value: -1e12,
    };
    
    let options = match crate::context::ruby_value_to_json_with_validation(options_value, &validation_config) {
        Ok(opts) => opts,
        Err(e) => {
            warn!("Input validation failed for create_test_task: {}", e);
            return json_to_ruby_value(json!({
                "error": format!("Input validation failed: {}", e)
            }));
        }
    };

    let result = crate::globals::execute_async(async {
        // Get TestingFactory from unified orchestration system
        let factory = get_global_testing_factory();

        // Use the factory to create the task
        factory.create_task(options).await
    });

    json_to_ruby_value(result)
}


/// Create test workflow step using TestingFactory
/// FFI wrapper that uses the TestingFactory struct for cleaner architecture
fn create_test_workflow_step_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    // Use strict validation for test factory inputs
    let validation_config = ValidationConfig {
        max_string_length: 1000,
        max_array_length: 100,
        max_object_depth: 5,
        max_object_keys: 50,
        max_numeric_value: 1e12,
        min_numeric_value: -1e12,
    };
    
    let options = match crate::context::ruby_value_to_json_with_validation(options_value, &validation_config) {
        Ok(opts) => opts,
        Err(e) => {
            warn!("Input validation failed for create_test_workflow_step: {}", e);
            return json_to_ruby_value(json!({
                "error": format!("Input validation failed: {}", e)
            }));
        }
    };

    let result = crate::globals::execute_async(async {
        // Get TestingFactory from unified orchestration system
        let factory = get_global_testing_factory();

        // Use the factory to create the workflow step
        factory.create_workflow_step(options).await
    });

    json_to_ruby_value(result)
}


/// Create test foundation data using TestingFactory
/// FFI wrapper that uses the TestingFactory struct for cleaner architecture
fn create_test_foundation_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    // Use strict validation for test factory inputs
    let validation_config = ValidationConfig {
        max_string_length: 1000,
        max_array_length: 100,
        max_object_depth: 5,
        max_object_keys: 50,
        max_numeric_value: 1e12,
        min_numeric_value: -1e12,
    };
    
    let options = match crate::context::ruby_value_to_json_with_validation(options_value, &validation_config) {
        Ok(opts) => opts,
        Err(e) => {
            warn!("Input validation failed for create_test_foundation: {}", e);
            return json_to_ruby_value(json!({
                "error": format!("Input validation failed: {}", e)
            }));
        }
    };

    let result = crate::globals::execute_async(async {
        // Get TestingFactory from unified orchestration system
        let factory = get_global_testing_factory();

        // Use the factory to create the foundation data
        factory.create_foundation(options).await
    });

    json_to_ruby_value(result)
}

/// Create complex workflow using TestingFactory
/// FFI wrapper for creating workflows with different patterns (linear, diamond, parallel, tree)
fn create_complex_workflow_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    // Use strict validation for test factory inputs
    let validation_config = ValidationConfig {
        max_string_length: 1000,
        max_array_length: 100,
        max_object_depth: 5,
        max_object_keys: 50,
        max_numeric_value: 1e12,
        min_numeric_value: -1e12,
    };
    
    let options = match crate::context::ruby_value_to_json_with_validation(options_value, &validation_config) {
        Ok(opts) => opts,
        Err(e) => {
            warn!("Input validation failed for create_complex_workflow: {}", e);
            return json_to_ruby_value(json!({
                "error": format!("Input validation failed: {}", e)
            }));
        }
    };

    let result = crate::globals::execute_async(async {
        // Get TestingFactory from unified orchestration system
        let factory = get_global_testing_factory();

        // Use the factory to create the complex workflow
        factory.create_complex_workflow(options).await
    });

    json_to_ruby_value(result)
}

/// Setup the factory singleton - triggers Rust-side singleton initialization
/// This ensures the `get_global_testing_factory()` singleton is created and ready
fn setup_factory_singleton_wrapper() -> Result<Value, Error> {
    debug!("🎯 FACTORY SINGLETON: Initializing factory singleton from Ruby coordination");

    // Trigger the singleton initialization by calling get_global_testing_factory
    let _factory = get_global_testing_factory();

    json_to_ruby_value(serde_json::json!({
        "status": "initialized",
        "type": "TestingFactorySingleton",
        "message": "Rust-side TestingFactory singleton initialized successfully",
        "singleton_active": true
    }))
}

/// Register factory wrapper functions
pub fn register_factory_functions(module: RModule) -> Result<(), Error> {
    // Factory singleton coordination
    module.define_module_function(
        "setup_factory_singleton",
        magnus::function!(setup_factory_singleton_wrapper, 0),
    )?;

    // Factory operation functions
    module.define_module_function(
        "create_test_task_with_factory",
        magnus::function!(create_test_task_with_factory_wrapper, 1),
    )?;
    module.define_module_function(
        "create_test_workflow_step_with_factory",
        magnus::function!(create_test_workflow_step_with_factory_wrapper, 1),
    )?;
    module.define_module_function(
        "create_test_foundation_with_factory",
        magnus::function!(create_test_foundation_with_factory_wrapper, 1),
    )?;
    module.define_module_function(
        "create_complex_workflow_with_factory",
        magnus::function!(create_complex_workflow_with_factory_wrapper, 1),
    )?;
    Ok(())
}
