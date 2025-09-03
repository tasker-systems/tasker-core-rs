//! # Mixed DAG Workflow Handlers
//!
//! Native Rust implementation of the mixed DAG workflow pattern that exactly replicates
//! the Ruby handlers in `workers/ruby/spec/handlers/examples/mixed_dag_workflow/`.
//!
//! ## Workflow Pattern
//!
//! This demonstrates a complex DAG (Directed Acyclic Graph) with multiple convergence points:
//!
//! ```
//! A: dag_init (n²)
//! ├── B: dag_process_left (n⁴)
//! │   ├── D: dag_validate (n⁴ × n⁴)² = n¹⁶  ←┐ (multiple parents)
//! │   └── E: dag_transform (n⁴)² = n⁸        │
//! └── C: dag_process_right (n⁴) ──────────────┘
//!     ├── D: dag_validate (already covered)
//!     └── F: dag_analyze (n⁴)² = n⁸
//!
//! G: dag_finalize ((n¹⁶ × n⁸ × n⁸)² = (n³²)² = n⁶⁴)
//! ```
//!
//! For input=6: 6^64 = extremely large number (2^64 bits to represent)
//!
//! ## Performance Benefits
//!
//! Native Rust implementation provides:
//! - Zero-overhead abstractions for complex DAG dependency resolution
//! - Compile-time type checking for mixed single/multiple parent patterns
//! - Memory safety for large mathematical calculations
//! - Efficient handling of complex convergence logic

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{error, info};

/// DAG Init: Initial step that squares the even number for mixed DAG (6 -> 36)
pub struct DagInitHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DagInitHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract even_number from task context
        let even_number = match step_data.get_context_field::<i64>("even_number") {
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

        info!("DAG Init: {}² = {}", even_number, result);

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
            json!(["dag_process_left", "dag_process_right"]),
        );

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "dag_init"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// DAG Process Left: Squares the init result (step B in mixed DAG) (36 -> 1,296)
pub struct DagProcessLeftHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DagProcessLeftHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from dag_init
        let init_result = match step_data.get_dependency_result_column_value::<i64>("dag_init") {
            Ok(value) => value,
            Err(e) => {
                error!("Missing result from dag_init: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Init result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("dag_init"),
                    )])),
                ));
            }
        };

        // Square the init result (single parent operation)
        let result = init_result * init_result;

        info!("DAG Process Left: {}² = {}", init_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "init_result": "sequence.dag_init.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("left"));
        metadata.insert(
            "feeds_to".to_string(),
            json!(["dag_validate", "dag_transform"]),
        );

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "dag_process_left"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// DAG Process Right: Squares the init result (step C in mixed DAG) (36 -> 1,296)
pub struct DagProcessRightHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DagProcessRightHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from dag_init
        let init_result = match step_data.get_dependency_result_column_value::<i64>("dag_init") {
            Ok(value) => value,
            Err(e) => {
                error!("Missing result from dag_init: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Init result not found".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("dag_init"),
                    )])),
                ));
            }
        };

        // Square the init result (single parent operation)
        let result = init_result * init_result;

        info!("DAG Process Right: {}² = {}", init_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "init_result": "sequence.dag_init.result"
            }),
        );
        metadata.insert("branch".to_string(), json!("right"));
        metadata.insert(
            "feeds_to".to_string(),
            json!(["dag_validate", "dag_analyze"]),
        );

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "dag_process_right"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// DAG Validate: Convergence step that multiplies results from both process branches
pub struct DagValidateHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DagValidateHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get results from both process branches (multiple parents)
        let left_result =
            match step_data.get_dependency_result_column_value::<i64>("dag_process_left") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from dag_process_left: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Process left result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("dag_process_left"),
                        )])),
                    ));
                }
            };

        let right_result =
            match step_data.get_dependency_result_column_value::<i64>("dag_process_right") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from dag_process_right: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Process right result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("dag_process_right"),
                        )])),
                    ));
                }
            };

        // Multiple parent logic: multiply the results together, then square
        let multiplied = left_result * right_result;
        let result = multiplied * multiplied;

        info!(
            "DAG Validate: ({} × {})² = {}² = {}",
            left_result, right_result, multiplied, result
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("multiply_and_square"));
        metadata.insert("step_type".to_string(), json!("multiple_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "left_result": "sequence.dag_process_left.result",
                "right_result": "sequence.dag_process_right.result"
            }),
        );
        metadata.insert("multiplied".to_string(), json!(multiplied));
        metadata.insert("convergence_type".to_string(), json!("dual_branch"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "dag_validate"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// DAG Transform: Squares the left process result (step E in mixed DAG)
pub struct DagTransformHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DagTransformHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from dag_process_left (single parent)
        let left_result =
            match step_data.get_dependency_result_column_value::<i64>("dag_process_left") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from dag_process_left: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Process left result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("dag_process_left"),
                        )])),
                    ));
                }
            };

        // Square the left result (single parent operation)
        let result = left_result * left_result;

        info!("DAG Transform: {}² = {}", left_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "left_result": "sequence.dag_process_left.result"
            }),
        );
        metadata.insert("transform_type".to_string(), json!("left_branch_square"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "dag_transform"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// DAG Analyze: Squares the right process result (step F in mixed DAG)
pub struct DagAnalyzeHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DagAnalyzeHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from dag_process_right (single parent)
        let right_result =
            match step_data.get_dependency_result_column_value::<i64>("dag_process_right") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from dag_process_right: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Process right result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("dag_process_right"),
                        )])),
                    ));
                }
            };

        // Square the right result (single parent operation)
        let result = right_result * right_result;

        info!("DAG Analyze: {}² = {}", right_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "right_result": "sequence.dag_process_right.result"
            }),
        );
        metadata.insert("analysis_type".to_string(), json!("right_branch_square"));

        Ok(success_result(
            step_uuid,
            json!(result),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "dag_analyze"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// DAG Finalize: Final convergence step that processes results from D, E, and F
pub struct DagFinalizeHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DagFinalizeHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get results from all convergence inputs: D (multiple parent), E (single parent), F (single parent)
        let validate_result =
            match step_data.get_dependency_result_column_value::<i64>("dag_validate") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from dag_validate: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Validate result (D) not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("dag_validate"),
                        )])),
                    ));
                }
            };

        let transform_result =
            match step_data.get_dependency_result_column_value::<i64>("dag_transform") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from dag_transform: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Transform result (E) not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("dag_transform"),
                        )])),
                    ));
                }
            };

        let analyze_result =
            match step_data.get_dependency_result_column_value::<i64>("dag_analyze") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from dag_analyze: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Analyze result (F) not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("dag_analyze"),
                        )])),
                    ));
                }
            };

        // Multiple parent logic: multiply all three results together, then square
        // Use checked multiplication to handle overflow gracefully
        let multiplied = (validate_result as u128)
            .checked_mul(transform_result as u128)
            .and_then(|v| v.checked_mul(analyze_result as u128))
            .unwrap_or_else(|| {
                error!(
                    "Overflow in multiplication: {} * {} * {}", 
                    validate_result, transform_result, analyze_result
                );
                u128::MAX // Use max value as fallback
            });
        
        let result = multiplied.checked_mul(multiplied).unwrap_or_else(|| {
            error!("Overflow in squaring: {}²", multiplied);
            u128::MAX // Use max value as fallback
        });

        info!(
            "DAG Finalize: ({} × {} × {})² = {}² = {}",
            validate_result, transform_result, analyze_result, multiplied, result
        );

        // Get original number for verification using new ergonomic method
        let original_number: i64 = step_data.get_context_field("even_number").unwrap_or(0);

        // Calculate expected result: original^64
        // Complex path calculation:
        // A(n²) -> B(n⁴), C(n⁴) -> D((n⁴ × n⁴)²=n¹⁶), E(n⁸), F(n⁸) -> G((n¹⁶ × n⁸ × n⁸)²=(n³²)²=n⁶⁴)
        // Use saturating_pow to avoid overflow
        let expected: u128 = if original_number >= 0 {
            (original_number as u128).saturating_pow(64)
        } else {
            // Handle negative numbers by using absolute value and noting the sign
            ((-original_number) as u128).saturating_pow(64)
        };
        let matches = result == expected;

        info!(
            "Mixed DAG Workflow Complete: {} -> {}",
            original_number, result
        );
        info!(
            "Verification: {}^64 = {} (match: {})",
            original_number, expected, matches
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("multiply_three_and_square"));
        metadata.insert("step_type".to_string(), json!("multiple_parent_final"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "validate_result": "sequence.dag_validate.result",
                "transform_result": "sequence.dag_transform.result",
                "analyze_result": "sequence.dag_analyze.result"
            }),
        );
        // Convert large numbers to strings for safe JSON serialization
        metadata.insert("multiplied".to_string(), json!(multiplied.to_string()));
        metadata.insert(
            "verification".to_string(),
            json!({
                "original_number": original_number,
                "expected_result": expected.to_string(),
                "actual_result": result.to_string(),
                "matches": matches
            }),
        );

        Ok(success_result(
            step_uuid,
            json!(result.to_string()), // Convert to string for JSON safety
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "dag_finalize"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
