//! # Diamond Workflow Handlers
//!
//! Native Rust implementation of the diamond workflow pattern that exactly replicates
//! the Ruby handlers in `workers/ruby/spec/handlers/examples/diamond_workflow/`.
//!
//! ## Workflow Pattern
//!
//! 1. **Diamond Start**: Square the initial even number (e.g., 6 -> 36)
//! 2. **Diamond Branch B** (Left): Square the start result (36 -> 1,296)
//! 3. **Diamond Branch C** (Right): Square the start result (36 -> 1,296)
//! 4. **Diamond End**: Multiply both branch results and square (1,296 × 1,296 -> 1,679,616 -> 2,821,109,907,456)
//!
//! This demonstrates parallel execution followed by convergence.
//! The final result is `input^16` (6^16 = 2,821,109,907,456).
//!
//! ## Performance Benefits
//!
//! Native Rust implementation provides:
//! - Zero-overhead abstractions for parallel branch processing
//! - Compile-time type checking for dependency resolution
//! - Memory safety without garbage collection
//! - Predictable performance for convergence operations

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{error, info};

/// Diamond Start: Square the initial even number (6 -> 36)
#[derive(Debug)]
pub struct DiamondStartHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DiamondStartHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract even_number from task context using new ergonomic method
        let even_number: i64 = match step_data.get_context_field("even_number") {
            Ok(value) => value,
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

        info!("Diamond Start: {}² = {}", even_number, result);

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
            json!(["diamond_branch_b", "diamond_branch_c"]),
        );

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "diamond_start"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Diamond Branch B: Left parallel branch that squares the start result (36 -> 1,296)
#[derive(Debug)]
pub struct DiamondBranchBHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DiamondBranchBHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from diamond_start using new ergonomic method
        let start_result: i64 = match step_data.get_dependency_result_column_value("diamond_start")
        {
            Ok(value) => value,
            Err(e) => {
                error!("Missing result from diamond_start: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Diamond start result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("diamond_start"),
                    )])),
                ));
            }
        };

        // Square the start result (single parent operation)
        let result = start_result * start_result;

        info!("Diamond Branch B: {}² = {}", start_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "start_result": "sequence.diamond_start.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("left"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "diamond_branch_b"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Diamond Branch C: Right parallel branch that squares the start result (36 -> 1,296)
#[derive(Debug)]
pub struct DiamondBranchCHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DiamondBranchCHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from diamond_start using new ergonomic method
        let start_result: i64 = match step_data.get_dependency_result_column_value("diamond_start")
        {
            Ok(value) => value,
            Err(e) => {
                error!("Missing result from diamond_start: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Diamond start result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("diamond_start"),
                    )])),
                ));
            }
        };

        // Square the start result (single parent operation)
        let result = start_result * start_result;

        info!("Diamond Branch C: {}² = {}", start_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "start_result": "sequence.diamond_start.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("right"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "diamond_branch_c"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Diamond End: Convergence step that multiplies results from both branches and squares
#[derive(Debug)]
pub struct DiamondEndHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DiamondEndHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get results from both parallel branches using new ergonomic methods
        let branch_b_result: i64 =
            match step_data.get_dependency_result_column_value("diamond_branch_b") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from diamond_branch_b: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Branch B result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("diamond_branch_b"),
                        )])),
                    ));
                }
            };

        let branch_c_result: i64 =
            match step_data.get_dependency_result_column_value("diamond_branch_c") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from diamond_branch_c: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Branch C result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("diamond_branch_c"),
                        )])),
                    ));
                }
            };

        // Multiple parent logic: multiply the results together, then square
        let multiplied = branch_b_result * branch_c_result;
        let result = multiplied * multiplied;

        info!(
            "Diamond End: ({} × {})² = {}² = {}",
            branch_b_result, branch_c_result, multiplied, result
        );

        // Get original number for verification using new ergonomic method
        let original_number: i64 = step_data.get_context_field("even_number").unwrap_or(0);

        // Calculate expected result: original^16
        // Path: n -> n² -> (n²)² and (n²)² -> ((n²)² × (n²)²)² = (n^8)² = n^16
        let expected = original_number.pow(16);
        let matches = result == expected;

        info!(
            "Diamond Workflow Complete: {} -> {}",
            original_number, result
        );
        info!(
            "Verification: {}^16 = {} (match: {})",
            original_number, expected, matches
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("multiply_and_square"));
        metadata.insert("step_type".to_string(), json!("multiple_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "branch_b_result": "sequence.diamond_branch_b.result",
                "branch_c_result": "sequence.diamond_branch_c.result"
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

    fn name(&self) -> &'static str {
        "diamond_end"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
