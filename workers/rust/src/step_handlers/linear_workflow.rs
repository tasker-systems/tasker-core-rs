//! # Linear Workflow Handlers
//!
//! Native Rust implementation of the linear workflow pattern that exactly replicates
//! the Ruby handlers in `workers/ruby/spec/handlers/examples/linear_workflow/`.
//!
//! ## Workflow Pattern
//!
//! 1. **Step 1**: Square the initial even number (e.g., 6 -> 36)
//! 2. **Step 2**: Square the result from step 1 (36 -> 1296)
//! 3. **Step 3**: Square the result from step 2 (1296 -> 1679616)
//! 4. **Step 4**: Square the result from step 3 (final result)
//!
//! This creates a pattern where the final result is `input^(2^4) = input^8`.
//! For input=6: 6^8 = 1,679,616
//!
//! ## Performance Benefits
//!
//! Native Rust implementation provides:
//! - Zero-overhead abstractions
//! - Compile-time type checking
//! - Memory safety without garbage collection
//! - Predictable performance characteristics

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{error, info};

/// Linear Step 1: Square the initial even number (6 -> 36)
pub struct LinearStep1Handler {
    #[allow(dead_code)] // api compatibility
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for LinearStep1Handler {
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

        info!("Linear Step 1: {}² = {}", even_number, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("initial"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "even_number": "task.context.even_number"
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
        "linear_step_1"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Linear Step 2: Square the result from step 1 (36 -> 1296)
pub struct LinearStep2Handler {
    #[allow(dead_code)] // api compatibility
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for LinearStep2Handler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from previous step (linear_step_1)
        let previous_result =
            match step_data.get_dependency_result_column_value::<i64>("linear_step_1") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from linear_step_1: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Previous step result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("linear_step_1"),
                        )])),
                    ));
                }
            };

        // Square the previous result
        let result = previous_result * previous_result;

        info!(
            "Linear Step 2: {} * {} = {}",
            previous_result, previous_result, result
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("intermediate"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "previous_result": "sequence.linear_step_1.result"
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
        "linear_step_2"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Linear Step 3: Square the result from step 2 (1296 -> 1679616)
pub struct LinearStep3Handler {
    #[allow(dead_code)] // api compatibility
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for LinearStep3Handler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from previous step (linear_step_2)
        let previous_result =
            match step_data.get_dependency_result_column_value::<i64>("linear_step_2") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from linear_step_2: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Previous step result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("linear_step_2"),
                        )])),
                    ));
                }
            };

        // Square the previous result (single parent operation)
        let result = previous_result * previous_result;

        info!("Linear Step 3: {}² = {}", previous_result, result);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("single_parent"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "previous_result": "sequence.linear_step_2.result"
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
        "linear_step_3"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Linear Step 4: Square the result from step 3 (final step with verification)
pub struct LinearStep4Handler {
    #[allow(dead_code)] // api compatibility
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for LinearStep4Handler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get result from previous step (linear_step_3)
        let previous_result =
            match step_data.get_dependency_result_column_value::<i64>("linear_step_3") {
                Ok(value) => value,
                Err(e) => {
                    error!("Missing result from linear_step_3: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Previous step result not found".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true, // Retryable - might be available later
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "required_step".to_string(),
                            json!("linear_step_3"),
                        )])),
                    ));
                }
            };

        // Square the previous result (single parent operation)
        let result = previous_result * previous_result;

        info!("Linear Step 4 (Final): {}² = {}", previous_result, result);

        // Get original number for verification (matching Ruby pattern)
        let original_number: i64 = step_data.get_context_field("even_number").unwrap_or(0);

        // Calculate expected result: original^8 (squaring 4 times: 2^4 = 8)
        let expected = original_number.pow(8);
        let matches = result == expected;

        info!(
            "Linear Workflow Complete: {} -> {}",
            original_number, result
        );
        info!(
            "Verification: {}^8 = {} (match: {})",
            original_number, expected, matches
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("square"));
        metadata.insert("step_type".to_string(), json!("final"));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "previous_result": "sequence.linear_step_3.result"
            }),
        );
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
        "linear_step_4"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
