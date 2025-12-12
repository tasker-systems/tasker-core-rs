//! # TAS-65 Phase 2.4a: Ruby FFI Bindings for Domain Event Publishing
//!
//! Exposes `DomainEventPublisher` to Ruby for step handler event publishing.
//! Allows Ruby handlers to publish domain events with full execution context.
//!
//! ## Architecture
//!
//! This FFI layer accepts Ruby hashes that match our domain types and deserializes
//! them into proper Rust structs using `serde_magnus`. The key types are:
//!
//! - `EventPublishRequest`: Top-level FFI input containing all event data
//! - `TaskSequenceStep`: Full task/step context (reuses tasker_shared types)
//! - `StepExecutionResult`: Complete execution result (reuses tasker_shared types)
//! - `EventMetadataInput`: FFI-friendly metadata that converts to `EventMetadata`

use crate::bridge::WORKER_SYSTEM;
use chrono::{DateTime, Utc};
use magnus::{
    function, prelude::*, Error as MagnusError, ExceptionClass, RModule, Ruby, Value as RValue,
};
use serde::Deserialize;
use serde_magnus::deserialize;
use tasker_shared::events::domain_events::{DomainEventPayload, EventMetadata};
use tasker_shared::messaging::execution_types::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{debug, error};
use uuid::Uuid;

/// Helper to get RuntimeError exception class (magnus 0.8 API)
fn runtime_error_class() -> ExceptionClass {
    Ruby::get()
        .expect("Ruby runtime should be available")
        .exception_runtime_error()
}

/// Helper to get ArgumentError exception class (magnus 0.8 API)
fn arg_error_class() -> ExceptionClass {
    Ruby::get()
        .expect("Ruby runtime should be available")
        .exception_arg_error()
}

/// FFI input structure for publishing domain events from Ruby
///
/// This struct is deserialized directly from a Ruby hash using serde_magnus.
/// All nested types use the same structures as tasker_shared for consistency.
#[derive(Debug, Deserialize)]
pub struct EventPublishRequest {
    /// Event name in dot notation (e.g., "payment.processed")
    pub event_name: String,

    /// Full task sequence step context
    pub task_sequence_step: TaskSequenceStep,

    /// Complete step execution result
    pub execution_result: StepExecutionResult,

    /// Business-specific event payload
    pub business_payload: serde_json::Value,

    /// Event metadata for routing and correlation
    pub metadata: EventMetadataInput,
}

/// FFI-friendly event metadata input
///
/// Similar to `EventMetadata` but with String UUIDs for easier Ruby interop.
/// Converts to `EventMetadata` with proper UUID parsing.
#[derive(Debug, Deserialize)]
pub struct EventMetadataInput {
    /// Task UUID as string
    pub task_uuid: String,

    /// Step UUID as string (optional)
    pub step_uuid: Option<String>,

    /// Step name (optional)
    pub step_name: Option<String>,

    /// Namespace for queue routing
    pub namespace: String,

    /// Correlation ID as string
    pub correlation_id: String,

    /// Handler name that fired the event
    pub fired_by: String,

    /// When the event was fired (optional, defaults to now)
    #[serde(default = "Utc::now")]
    pub fired_at: DateTime<Utc>,
}

impl TryFrom<EventMetadataInput> for EventMetadata {
    type Error = MagnusError;

    fn try_from(input: EventMetadataInput) -> Result<Self, Self::Error> {
        let task_uuid = parse_uuid(&input.task_uuid, "task_uuid")?;
        let correlation_id = parse_uuid(&input.correlation_id, "correlation_id")?;
        let step_uuid = input
            .step_uuid
            .map(|s| parse_uuid(&s, "step_uuid"))
            .transpose()?;

        Ok(EventMetadata {
            task_uuid,
            step_uuid,
            step_name: input.step_name,
            namespace: input.namespace,
            correlation_id,
            fired_at: input.fired_at,
            fired_by: input.fired_by,
        })
    }
}

/// Parse a UUID string with a descriptive error message
fn parse_uuid(s: &str, field_name: &str) -> Result<Uuid, MagnusError> {
    Uuid::parse_str(s).map_err(|e| {
        MagnusError::new(
            arg_error_class(),
            format!("Invalid {} format: {}", field_name, e),
        )
    })
}

/// FFI function to publish a domain event from Ruby
///
/// # Arguments
///
/// A Ruby hash containing:
/// - `event_name`: String - Event name in dot notation (e.g., "payment.processed")
/// - `task_sequence_step`: Hash - Full TaskSequenceStep structure
/// - `execution_result`: Hash - Full StepExecutionResult structure
/// - `business_payload`: Hash - Business-specific event data
/// - `metadata`: Hash - Event metadata for routing and correlation
///
/// # Returns
///
/// String - The generated event_id (UUID v7)
///
/// # Ruby Example
///
/// ```ruby
/// TaskerCore::FFI.publish_domain_event({
///   event_name: "payment.processed",
///   task_sequence_step: task_sequence_step.to_h,
///   execution_result: execution_result.to_h,
///   business_payload: { transaction_id: "txn_123", amount: 100.00 },
///   metadata: {
///     task_uuid: task.task_uuid,
///     step_uuid: workflow_step.workflow_step_uuid,
///     step_name: workflow_step.name,
///     namespace: task.namespace_name,
///     correlation_id: task.task.correlation_id,
///     fired_by: handler_name
///   }
/// })
/// # => "0199c8f0-1234-7abc-9def-0123456789ab"
/// ```
pub fn publish_domain_event(event_params: RValue) -> Result<String, MagnusError> {
    // Get Ruby handle for serde_magnus
    let ruby = magnus::Ruby::get().map_err(|e| {
        error!("Failed to acquire Ruby handle: {}", e);
        MagnusError::new(
            runtime_error_class(),
            format!("Failed to acquire Ruby handle: {}", e),
        )
    })?;

    // Deserialize the entire request using serde_magnus
    let request: EventPublishRequest = deserialize(&ruby, event_params).map_err(|e| {
        error!("Failed to deserialize event publish request: {}", e);
        MagnusError::new(
            arg_error_class(),
            format!("Invalid event publish request: {}", e),
        )
    })?;

    // Convert metadata input to EventMetadata
    let metadata: EventMetadata = request.metadata.try_into()?;

    debug!(
        event_name = %request.event_name,
        namespace = %metadata.namespace,
        correlation_id = %metadata.correlation_id,
        step_success = request.execution_result.success,
        "Publishing domain event from Ruby FFI"
    );

    // Construct the domain event payload with full context
    let domain_payload = DomainEventPayload {
        task_sequence_step: request.task_sequence_step,
        execution_result: request.execution_result,
        payload: request.business_payload,
    };

    // Get worker system to access domain event publisher
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        MagnusError::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        MagnusError::new(
            runtime_error_class(),
            "Worker system not running - call bootstrap_worker first",
        )
    })?;

    // Access the domain event publisher created during bootstrap
    let event_publisher = &handle.domain_event_publisher;

    // Publish event using the async runtime
    let runtime_handle = handle.runtime_handle();
    let event_id = runtime_handle
        .block_on(event_publisher.publish_event(&request.event_name, domain_payload, metadata))
        .map_err(|e| {
            error!("Failed to publish domain event: {}", e);
            MagnusError::new(
                runtime_error_class(),
                format!("Event publication failed: {}", e),
            )
        })?;

    debug!(
        event_id = %event_id,
        event_name = %request.event_name,
        "Domain event published successfully from Ruby"
    );

    Ok(event_id.to_string())
}

/// Initialize the event publisher FFI module
pub fn init_event_publisher_ffi(module: &RModule) -> Result<(), MagnusError> {
    module.define_singleton_method("publish_domain_event", function!(publish_domain_event, 1))?;
    Ok(())
}
