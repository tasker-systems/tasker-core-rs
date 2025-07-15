//! # Ruby Step Handler Bridge
//!
//! This module provides a bridge between the Rust StepHandler trait and Ruby
//! business logic methods. Instead of storing Ruby Values (which have thread
//! safety issues), we use a different approach where Ruby handlers are
//! registered and called through a registry system.
//!
//! ## Ruby API
//!
//! Ruby developers implement these methods:
//! ```ruby
//! def process(task, sequence, step)
//!   # Business logic here
//!   { result: "success" }
//! end
//!
//! def process_results(step, process_output, initial_results = nil)
//!   # Optional result transformation
//!   process_output
//! end
//! ```

use crate::globals::get_global_database_pool;
use crate::context::{json_to_ruby_value, ruby_value_to_json};
use crate::models::{RubyTask, RubyStep, RubyStepSequence};
use magnus::{Error, Value, Module};
use magnus::value::ReprValue;
use tasker_core::orchestration::step_handler::{StepExecutionContext, StepResult};
use tasker_core::orchestration::errors::{OrchestrationResult, ExecutionError};
use tasker_core::models::{Task, WorkflowStep};
use sqlx::PgPool;

/// Convert Rust Task to Ruby object
async fn task_to_ruby_object(task: &Task) -> Result<RubyTask, ExecutionError> {
    Ok(RubyTask::from_task(task))
}

/// Convert WorkflowStep to Ruby object
async fn step_to_ruby_object(step: &WorkflowStep) -> Result<RubyStep, ExecutionError> {
    Ok(RubyStep::from_workflow_step(step))
}

/// Build step sequence using dependencies
async fn build_ruby_step_sequence(step: &WorkflowStep, pool: &PgPool) -> Result<RubyStepSequence, ExecutionError> {
    RubyStepSequence::from_workflow_step(step, pool).await
        .map_err(|e| ExecutionError::StepExecutionFailed {
            step_id: step.workflow_step_id,
            reason: format!("Failed to get step dependencies: {}", e),
            error_code: Some("DATABASE_ERROR".to_string()),
        })
}

/// Call Ruby process method with the given handler and context
pub async fn call_ruby_process_method(
    ruby_handler: Value,
    context: &StepExecutionContext,
) -> OrchestrationResult<serde_json::Value> {
    let pool = get_global_database_pool();

    // Load models from database
    let task = Task::find_by_id(&pool, context.task_id).await
        .map_err(|e| ExecutionError::StepExecutionFailed {
            step_id: context.step_id,
            reason: format!("Failed to load task: {}", e),
            error_code: Some("DATABASE_ERROR".to_string()),
        })?
        .ok_or_else(|| ExecutionError::StepExecutionFailed {
            step_id: context.step_id,
            reason: "Task not found".to_string(),
            error_code: Some("TASK_NOT_FOUND".to_string()),
        })?;

    let step = WorkflowStep::find_by_id(&pool, context.step_id).await
        .map_err(|e| ExecutionError::StepExecutionFailed {
            step_id: context.step_id,
            reason: format!("Failed to load workflow step: {}", e),
            error_code: Some("DATABASE_ERROR".to_string()),
        })?
        .ok_or_else(|| ExecutionError::StepExecutionFailed {
            step_id: context.step_id,
            reason: "Workflow step not found".to_string(),
            error_code: Some("STEP_NOT_FOUND".to_string()),
        })?;

    // Convert to Ruby objects
    let task_obj = task_to_ruby_object(&task).await?;
    let step_obj = step_to_ruby_object(&step).await?;
    let sequence_obj = build_ruby_step_sequence(&step, &pool).await?;

    // Call Ruby process method
    let ruby_result = ruby_handler.funcall::<_, _, Value>(
        "process",
        (task_obj, sequence_obj, step_obj)
    ).map_err(|e| ExecutionError::StepExecutionFailed {
        step_id: context.step_id,
        reason: format!("Ruby process method failed: {}", e),
        error_code: Some("RUBY_EXECUTION_ERROR".to_string()),
    })?;

    // Convert Ruby result back to JSON
    let result_json = ruby_value_to_json(ruby_result)
        .map_err(|e| ExecutionError::StepExecutionFailed {
            step_id: context.step_id,
            reason: format!("Failed to convert Ruby result to JSON: {}", e),
            error_code: Some("CONVERSION_ERROR".to_string()),
        })?;

    Ok(result_json)
}

/// Call Ruby process_results method with the given handler and context
pub async fn call_ruby_process_results_method(
    ruby_handler: Value,
    context: &StepExecutionContext,
    result: &StepResult,
) -> OrchestrationResult<()> {
    // Only call if Ruby method exists
    if ruby_handler.respond_to("process_results", false).unwrap_or(false) {
        let pool = get_global_database_pool();

        // Load step from database
        let step = WorkflowStep::find_by_id(&pool, context.step_id).await
            .map_err(|e| ExecutionError::StepExecutionFailed {
                step_id: context.step_id,
                reason: format!("Failed to load workflow step: {}", e),
                error_code: Some("DATABASE_ERROR".to_string()),
            })?
            .ok_or_else(|| ExecutionError::StepExecutionFailed {
                step_id: context.step_id,
                reason: "Workflow step not found".to_string(),
                error_code: Some("STEP_NOT_FOUND".to_string()),
            })?;

        let step_obj = step_to_ruby_object(&step).await?;

        // Convert process output and initial results
        let process_output = json_to_ruby_value(result.output_data.clone().unwrap_or_default())
            .map_err(|e| ExecutionError::StepExecutionFailed {
                step_id: context.step_id,
                reason: format!("Failed to convert process output to Ruby: {}", e),
                error_code: Some("CONVERSION_ERROR".to_string()),
            })?;

        let initial_results = json_to_ruby_value(step.results.clone().unwrap_or_default())
            .map_err(|e| ExecutionError::StepExecutionFailed {
                step_id: context.step_id,
                reason: format!("Failed to convert initial results to Ruby: {}", e),
                error_code: Some("CONVERSION_ERROR".to_string()),
            })?;

        // Call Ruby process_results method
        let _processed_result = ruby_handler.funcall::<_, _, Value>(
            "process_results",
            (step_obj, process_output, initial_results)
        ).map_err(|e| ExecutionError::StepExecutionFailed {
            step_id: context.step_id,
            reason: format!("Ruby process_results method failed: {}", e),
            error_code: Some("RUBY_EXECUTION_ERROR".to_string()),
        })?;
    }

    Ok(())
}

/// FFI function to call Ruby process method
pub fn call_ruby_process_method_ffi(
    ruby_handler: Value,
    context_json: Value,
) -> Result<Value, Error> {
    // Convert JSON context to StepExecutionContext
    let context_data = ruby_value_to_json(context_json)?;
    let context: StepExecutionContext = serde_json::from_value(context_data)
        .map_err(|e| Error::new(
            magnus::Ruby::get().unwrap().exception_runtime_error(),
            format!("Failed to parse StepExecutionContext: {}", e)
        ))?;

    // Call the async function
    let result = crate::globals::execute_async(async {
        call_ruby_process_method(ruby_handler, &context).await
    });

    match result {
        Ok(json_result) => json_to_ruby_value(json_result),
        Err(e) => json_to_ruby_value(serde_json::json!({
            "error": format!("Ruby process method failed: {}", e)
        }))
    }
}

/// Register Ruby step handler bridge functions
pub fn register_ruby_step_handler_functions(module: magnus::RModule) -> Result<(), Error> {
    // Register the BaseStepHandler class that Ruby can inherit from
    let base_step_handler_class = module.define_class("BaseStepHandler", magnus::class::object())?;

    // Define a simple constructor
    base_step_handler_class.define_method("initialize", magnus::method!(base_step_handler_initialize, 0))?;

    module.define_module_function(
        "call_ruby_process_method",
        magnus::function!(call_ruby_process_method_ffi, 2),
    )?;

    Ok(())
}

/// Simple BaseStepHandler constructor for Ruby inheritance
fn base_step_handler_initialize(_rb_self: Value) -> Result<Value, Error> {
    Ok(_rb_self)
}
