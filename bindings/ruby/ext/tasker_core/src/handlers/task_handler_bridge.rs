//! # Task Handler FFI Bridge
//!
//! Proper FFI bridge that delegates to core task handler logic instead of
//! reimplementing it. Uses singleton pattern for shared resources.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use crate::globals::{get_global_orchestration_system, execute_async};
use magnus::{Error, RModule, Ruby, Value};
use tasker_core::models::core::task_request::TaskRequest;
use tasker_core::orchestration::task_handler::TaskExecutionContext;
use serde_json;

/// Create a task using core TaskInitializer
fn create_task_from_request_wrapper(task_request_value: Value) -> Result<Value, Error> {
    let task_request_data = ruby_value_to_json(task_request_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid task request data: {}", e)))?;

    let result = execute_async(async {
        // Parse TaskRequest from Ruby data
        let task_request: TaskRequest = serde_json::from_value(task_request_data)
            .map_err(|e| format!("Failed to parse TaskRequest: {}", e))?;

        // Get global orchestration system (singleton)
        let orchestration = get_global_orchestration_system();

        // Delegate to core task initializer
        let result = orchestration.task_initializer
            .create_task_from_request(task_request)
            .await
            .map_err(|e| format!("Task initialization failed: {}", e))?;

        Ok::<serde_json::Value, String>(serde_json::json!({
            "task_id": result.task_id,
            "step_count": result.step_count,
            "step_mapping": result.step_mapping,
            "handler_config_name": result.handler_config_name,
            "status": "success"
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

/// Handle a task using core WorkflowCoordinator
fn handle_task_wrapper(task_data_value: Value) -> Result<Value, Error> {
    let task_data = ruby_value_to_json(task_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid task data: {}", e)))?;

    let result = execute_async(async {
        // Extract task_id from Ruby data for state evaluation
        let task_id = task_data.get("task_id")
            .and_then(|v| v.as_i64())
            .ok_or("Missing or invalid task_id")?;

        // Get global orchestration system (singleton)
        let orchestration = get_global_orchestration_system();

        // Use state manager to evaluate task state (this is the main task execution interface)
        let result = orchestration.state_manager
            .evaluate_task_state(task_id)
            .await
            .map_err(|e| format!("Task evaluation failed: {}", e))?;

        Ok::<serde_json::Value, String>(serde_json::json!({
            "task_id": task_id,
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

/// Initialize a task handler using core components
fn initialize_task_handler_wrapper(config_value: Value) -> Result<Value, Error> {
    let config_data = ruby_value_to_json(config_value).unwrap_or_else(|_| serde_json::json!({}));

    let result = execute_async(async {
        // Get global orchestration system (singleton) - this initializes everything
        let orchestration = get_global_orchestration_system();

        // Extract configuration from Ruby input
        let default_system_id = config_data.get("default_system_id")
            .and_then(|v| v.as_i64())
            .unwrap_or(1) as i32;

        let initialize_state_machine = config_data.get("initialize_state_machine")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok::<serde_json::Value, String>(serde_json::json!({
            "status": "initialized",
            "message": "Task handler ready using core orchestration system",
            "config": {
                "default_system_id": default_system_id,
                "initialize_state_machine": initialize_state_machine
            },
            "capabilities": [
                "task_creation",
                "workflow_step_management",
                "state_machine_integration",
                "event_publishing",
                "singleton_resource_management"
            ]
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

/// Parse Ruby task data into TaskExecutionContext
fn parse_task_execution_context(task_data: serde_json::Value) -> Result<TaskExecutionContext, String> {
    // Extract required fields from Ruby data
    let task_id = task_data.get("task_id")
        .and_then(|v| v.as_i64())
        .ok_or("Missing or invalid task_id")?;

    let namespace = task_data.get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    let input_data = task_data.get("input_data")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let environment = task_data.get("environment")
        .and_then(|v| v.as_str())
        .unwrap_or("production")
        .to_string();

    let metadata = task_data.get("metadata")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    Ok(TaskExecutionContext {
        task_id,
        namespace,
        input_data,
        environment,
        metadata,
    })
}

/// Register task handler FFI bridge functions
pub fn register_task_handler_functions(module: RModule) -> Result<(), Error> {
    module.define_module_function(
        "create_task_from_request",
        magnus::function!(create_task_from_request_wrapper, 1),
    )?;

    module.define_module_function(
        "handle_task",
        magnus::function!(handle_task_wrapper, 1),
    )?;

    module.define_module_function(
        "initialize_task_handler",
        magnus::function!(initialize_task_handler_wrapper, 1),
    )?;

    Ok(())
}
