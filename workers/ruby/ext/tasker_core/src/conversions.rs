use magnus::{RHash, Value as RValue};
use serde_magnus::serialize;
use tasker_shared::errors::{TaskerError, TaskerResult};
use tasker_shared::types::{StepExecutionCompletionEvent, StepExecutionEvent, TaskSequenceStep};

// Convert Rust StepExecutionEvent to Ruby hash
pub fn convert_step_execution_event_to_ruby(event: StepExecutionEvent) -> TaskerResult<RHash> {
    let ruby = magnus::Ruby::get().map_err(|err| {
        TaskerError::FFIError(format!("Ruby Magnus FFI Ruby Acquire Error: {err}"))
    })?;

    let event_hash = ruby.hash_new();
    event_hash
        .aset("event_id", event.event_id.to_string())
        .map_err(|err| tasker_error_from_magnus_error(&err))?;
    event_hash
        .aset("task_uuid", event.payload.task_uuid.to_string())
        .map_err(|err| tasker_error_from_magnus_error(&err))?;
    event_hash
        .aset("step_uuid", event.payload.step_uuid.to_string())
        .map_err(|err| tasker_error_from_magnus_error(&err))?;

    // Convert TaskSequenceStep to Ruby hash
    let task_sequence_step_hash =
        convert_task_sequence_step_to_ruby(&event.payload.task_sequence_step)?;
    event_hash
        .aset("task_sequence_step", task_sequence_step_hash)
        .map_err(|err| tasker_error_from_magnus_error(&err))?;

    Ok(event_hash)
}

// Convert Ruby completion hash to Rust StepExecutionCompletionEvent
pub fn convert_ruby_completion_to_rust(
    value: RValue,
) -> TaskerResult<StepExecutionCompletionEvent> {
    // Convert Ruby result to serde_json::Value
    let result: StepExecutionCompletionEvent = serde_magnus::deserialize(value).map_err(|err| {
        TaskerError::FFIError(format!(
            "Could not deserialize to StepExecutionCompletionEvent, {err}"
        ))
    })?;

    Ok(result)
}

// Convert TaskSequenceStep to Ruby hash with full fidelity
fn convert_task_sequence_step_to_ruby(tss: &TaskSequenceStep) -> TaskerResult<RHash> {
    let ruby_hash: RHash = serialize(tss).map_err(|err| {
        TaskerError::FFIError(format!(
            "Could not deserialize to StepExecutionCompletionEvent, {err}"
        ))
    })?;

    Ok(ruby_hash)
}

pub fn tasker_error_from_magnus_error(err: &magnus::Error) -> TaskerError {
    TaskerError::FFIError(format!("Ruby Magnus FFI Error: {err}"))
}
