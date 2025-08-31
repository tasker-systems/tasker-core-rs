//! # Tree Workflow Handlers
//!
//! Native Rust implementation of the tree workflow pattern that exactly replicates
//! the Ruby handlers in `workers/ruby/spec/handlers/examples/tree_workflow/`.
//!
//! ## Workflow Pattern
//!
//! This demonstrates a hierarchical tree structure with complex fan-out and convergence:
//!
//! ```
//! Tree Root (n²)
//! ├── Tree Branch Left (n²)²
//! │   ├── Tree Leaf D ((n²)²)²
//! │   └── Tree Leaf E ((n²)²)²
//! └── Tree Branch Right (n²)²
//!     ├── Tree Leaf F ((n²)²)²
//!     └── Tree Leaf G ((n²)²)²
//! Final Convergence: ((n²)² × (n²)² × (n²)² × (n²)²)² = (n^16)² = n^32
//! ```
//!
//! For input=6: 6^32 = 68,719,476,736 × 10^18 (approximately)
//!
//! ## Performance Benefits
//!
//! Native Rust implementation provides:
//! - Zero-overhead abstractions for complex dependency trees
//! - Compile-time type checking for hierarchical structures
//! - Memory safety for large result calculations
//! - Efficient handling of multiple dependency convergence

use super::{
    error_result, get_context_field, get_dependency_result, success_result, RustStepHandler,
    StepHandlerConfig,
};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{error, info};

/// Tree Root: Initial step that squares the even number (6 -> 36)
pub struct TreeRootHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TreeRootHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract even_number from task context
        let even_number = match get_context_field(step_data, "even_number") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("even_number must be a number"))?,
            Err(e) => {
                error!("Missing even_number in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Task context must contain an even number".to_string(),
                    Some("MISSING_CONTEXT_FIELD".to_string()),
                    Some("ValidationError".to_string()),
                    false, // Not retryable - data validation error
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([("field".to_string(), json!("even_number"))])),
                ));
            }
        };

        // Validate that the number is even
        if even_number % 2 != 0 {
            error!("Input number {} is not even", even_number);
            return Ok(error_result(
                step_uuid,
                "Number must be even".to_string(),
                Some("VALIDATION_ERROR".to_string()),
                Some("ValidationError".to_string()),
                false, // Not retryable - data validation error
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([("input".to_string(), json!(even_number))])),
            ));
        }

        // Square the even number (first step operation)
        let result = even_number * even_number;

        info!("Tree Root: {}² = {}", even_number, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("initial"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "even_number": "task.context.even_number"
            }),
        );
        metadata.insert(
            "branches".to_string(),
            json!(["tree_branch_left", "tree_branch_right"]),
        );

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "tree_root"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Tree Branch Left: Left main branch that squares the root result (36 -> 1,296)
pub struct TreeBranchLeftHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TreeBranchLeftHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from tree_root
        let root_result = match get_dependency_result(step_data, "tree_root") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_root result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_root: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Tree root result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_root"),
                    )])),
                ));
            }
        };

        // Square the root result (single parent operation)
        let result = root_result * root_result;

        info!("Tree Branch Left: {}² = {}", root_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "root_result": "sequence.tree_root.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("left_main"));
        metadata.insert(
            "sub_branches".to_string(),
            json!(["tree_leaf_d", "tree_leaf_e"]),
        );

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "tree_branch_left"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Tree Branch Right: Right main branch that squares the root result (36 -> 1,296)
pub struct TreeBranchRightHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TreeBranchRightHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from tree_root
        let root_result = match get_dependency_result(step_data, "tree_root") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_root result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_root: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Tree root result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_root"),
                    )])),
                ));
            }
        };

        // Square the root result (single parent operation)
        let result = root_result * root_result;

        info!("Tree Branch Right: {}² = {}", root_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "root_result": "sequence.tree_root.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("right_main"));
        metadata.insert(
            "sub_branches".to_string(),
            json!(["tree_leaf_f", "tree_leaf_g"]),
        );

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "tree_branch_right"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Tree Leaf D: Left-left leaf that squares the input from left branch (1,296 -> 1,679,616)
pub struct TreeLeafDHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TreeLeafDHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from tree_branch_left
        let branch_result = match get_dependency_result(step_data, "tree_branch_left") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_branch_left result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_branch_left: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Tree branch left result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_branch_left"),
                    )])),
                ));
            }
        };

        // Square the branch result (single parent operation)
        let result = branch_result * branch_result;

        info!("Tree Leaf D: {}² = {}", branch_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "branch_result": "sequence.tree_branch_left.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("left"));
        metadata.insert("leaf".to_string(), json!("d"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "tree_leaf_d"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Tree Leaf E: Left-right leaf that squares the input from left branch (1,296 -> 1,679,616)
pub struct TreeLeafEHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TreeLeafEHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from tree_branch_left
        let branch_result = match get_dependency_result(step_data, "tree_branch_left") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_branch_left result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_branch_left: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Tree branch left result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_branch_left"),
                    )])),
                ));
            }
        };

        // Square the branch result (single parent operation)
        let result = branch_result * branch_result;

        info!("Tree Leaf E: {}² = {}", branch_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "branch_result": "sequence.tree_branch_left.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("left"));
        metadata.insert("leaf".to_string(), json!("e"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "tree_leaf_e"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Tree Leaf F: Right-left leaf that squares the input from right branch (1,296 -> 1,679,616)
pub struct TreeLeafFHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TreeLeafFHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from tree_branch_right
        let branch_result = match get_dependency_result(step_data, "tree_branch_right") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_branch_right result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_branch_right: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Tree branch right result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_branch_right"),
                    )])),
                ));
            }
        };

        // Square the branch result (single parent operation)
        let result = branch_result * branch_result;

        info!("Tree Leaf F: {}² = {}", branch_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "branch_result": "sequence.tree_branch_right.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("right"));
        metadata.insert("leaf".to_string(), json!("f"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "tree_leaf_f"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Tree Leaf G: Right-right leaf that squares the input from right branch (1,296 -> 1,679,616)
pub struct TreeLeafGHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TreeLeafGHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from tree_branch_right
        let branch_result = match get_dependency_result(step_data, "tree_branch_right") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_branch_right result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_branch_right: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Tree branch right result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_branch_right"),
                    )])),
                ));
            }
        };

        // Square the branch result (single parent operation)
        let result = branch_result * branch_result;

        info!("Tree Leaf G: {}² = {}", branch_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "branch_result": "sequence.tree_branch_right.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("right"));
        metadata.insert("leaf".to_string(), json!("g"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "tree_leaf_g"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Tree Final Convergence: Ultimate convergence step that processes all leaf results
pub struct TreeFinalConvergenceHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TreeFinalConvergenceHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get results from all leaf nodes
        let leaf_d_result = match get_dependency_result(step_data, "tree_leaf_d") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_leaf_d result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_leaf_d: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Leaf D result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_leaf_d"),
                    )])),
                ));
            }
        };

        let leaf_e_result = match get_dependency_result(step_data, "tree_leaf_e") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_leaf_e result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_leaf_e: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Leaf E result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_leaf_e"),
                    )])),
                ));
            }
        };

        let leaf_f_result = match get_dependency_result(step_data, "tree_leaf_f") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_leaf_f result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_leaf_f: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Leaf F result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_leaf_f"),
                    )])),
                ));
            }
        };

        let leaf_g_result = match get_dependency_result(step_data, "tree_leaf_g") {
            Ok(value) => value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("tree_leaf_g result must be a number"))?,
            Err(e) => {
                error!("Missing result from tree_leaf_g: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Leaf G result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("tree_leaf_g"),
                    )])),
                ));
            }
        };

        // Multiple parent logic: multiply all leaf results together, then square
        let multiplied = leaf_d_result * leaf_e_result * leaf_f_result * leaf_g_result;
        let result = multiplied * multiplied;

        info!(
            "Tree Final Convergence: ({} × {} × {} × {})² = {}² = {}",
            leaf_d_result, leaf_e_result, leaf_f_result, leaf_g_result, multiplied, result
        );

        // Get original number for verification using new ergonomic method
        let original_number: i64 = step_data.get_context_field("even_number").unwrap_or(0);

        // Calculate expected result: original^32
        // Path: n -> n² -> (n²)² for both branches -> 4 leaves each (n²)² -> final convergence ((n²)²)^4 squared = n^32
        let expected = original_number.pow(32);
        let matches = result == expected;

        info!("Tree Workflow Complete: {} -> {}", original_number, result);
        info!(
            "Verification: {}^32 = {} (match: {})",
            original_number, expected, matches
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("multiply_all_and_square"));
        metadata.insert("step_type".to_string(), json!("multiple_parent_final"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "leaf_d_result": "sequence.tree_leaf_d.result",
                "leaf_e_result": "sequence.tree_leaf_e.result",
                "leaf_f_result": "sequence.tree_leaf_f.result",
                "leaf_g_result": "sequence.tree_leaf_g.result"
            }),
        );
        metadata.insert("multiplied".to_string(), json!(multiplied));
        metadata.insert(
            "verification".to_string(),
            json!({
                "original_number": original_number,
                "expected_result": expected,
                "actual_result": result,
                "matches": matches
            }),
        );

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "tree_final_convergence"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
