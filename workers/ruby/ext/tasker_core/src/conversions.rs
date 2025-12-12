use magnus::{RHash, Value as RValue};
use serde_magnus::serialize;
use tasker_shared::errors::{TaskerError, TaskerResult};
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tasker_worker::worker::FfiStepEvent;

// Convert TaskSequenceStep to Ruby hash with full fidelity
fn convert_task_sequence_step_to_ruby(tss: &TaskSequenceStep) -> TaskerResult<RHash> {
    let ruby = magnus::Ruby::get().map_err(|err| {
        TaskerError::FFIError(format!("Ruby Magnus FFI Ruby Acquire Error: {err}"))
    })?;

    let ruby_hash: RHash = serialize(&ruby, tss).map_err(|err| {
        TaskerError::FFIError(format!(
            "Could not serialize TaskSequenceStep to Ruby hash: {err}"
        ))
    })?;

    Ok(ruby_hash)
}

pub fn tasker_error_from_magnus_error(err: &magnus::Error) -> TaskerError {
    TaskerError::FFIError(format!("Ruby Magnus FFI Error: {err}"))
}

/// TAS-67: Convert FfiStepEvent to Ruby hash
///
/// This converts the new dispatch channel event format to Ruby.
/// Includes the event_id for correlation when completing the step.
pub fn convert_ffi_step_event_to_ruby(event: &FfiStepEvent) -> TaskerResult<RHash> {
    let ruby = magnus::Ruby::get().map_err(|err| {
        TaskerError::FFIError(format!("Ruby Magnus FFI Ruby Acquire Error: {err}"))
    })?;

    let event_hash = ruby.hash_new();

    // TAS-67: Event ID is critical for completion correlation
    event_hash
        .aset("event_id", event.event_id.to_string())
        .map_err(|err| tasker_error_from_magnus_error(&err))?;

    event_hash
        .aset("task_uuid", event.task_uuid.to_string())
        .map_err(|err| tasker_error_from_magnus_error(&err))?;

    event_hash
        .aset("step_uuid", event.step_uuid.to_string())
        .map_err(|err| tasker_error_from_magnus_error(&err))?;

    event_hash
        .aset("correlation_id", event.correlation_id.to_string())
        .map_err(|err| tasker_error_from_magnus_error(&err))?;

    // Optional trace context
    if let Some(ref trace_id) = event.trace_id {
        event_hash
            .aset("trace_id", trace_id.clone())
            .map_err(|err| tasker_error_from_magnus_error(&err))?;
    }
    if let Some(ref span_id) = event.span_id {
        event_hash
            .aset("span_id", span_id.clone())
            .map_err(|err| tasker_error_from_magnus_error(&err))?;
    }

    // Convert the full execution event payload (contains TaskSequenceStep)
    let payload = &event.execution_event.payload;

    // TAS-29: Also expose correlation_id from task if different
    let task_correlation_id = payload.task_sequence_step.task.task.correlation_id;
    event_hash
        .aset("task_correlation_id", task_correlation_id.to_string())
        .map_err(|err| tasker_error_from_magnus_error(&err))?;

    // Parent correlation ID if present
    if let Some(parent_id) = payload.task_sequence_step.task.task.parent_correlation_id {
        event_hash
            .aset("parent_correlation_id", parent_id.to_string())
            .map_err(|err| tasker_error_from_magnus_error(&err))?;
    }

    // Convert TaskSequenceStep to Ruby hash
    let task_sequence_step_hash = convert_task_sequence_step_to_ruby(&payload.task_sequence_step)?;
    event_hash
        .aset("task_sequence_step", task_sequence_step_hash)
        .map_err(|err| tasker_error_from_magnus_error(&err))?;

    Ok(event_hash)
}

/// TAS-67: Convert Ruby completion hash to StepExecutionResult
///
/// This is the new conversion for the FfiDispatchChannel flow.
/// The Ruby hash should contain the result data that can be deserialized
/// into a StepExecutionResult.
pub fn convert_ruby_completion_to_step_result(value: RValue) -> TaskerResult<StepExecutionResult> {
    let ruby = magnus::Ruby::get().map_err(|err| {
        TaskerError::FFIError(format!("Ruby Magnus FFI Ruby Acquire Error: {err}"))
    })?;

    let result: StepExecutionResult = serde_magnus::deserialize(&ruby, value).map_err(|err| {
        TaskerError::FFIError(format!(
            "Could not deserialize to StepExecutionResult: {err}"
        ))
    })?;

    Ok(result)
}
