//! # Worker Actor Messages
//!
//! TAS-69: Message types for worker actors following the actor pattern.
//!
//! Each message type implements the `Message` trait with an associated
//! response type for type-safe message handling.

use pgmq::Message as PgmqMessage;
use uuid::Uuid;

use tasker_shared::messaging::message::StepMessage;
use tasker_shared::messaging::service::{MessageEvent, QueuedMessage};
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::base::TaskSequenceStep;

use super::traits::Message;
use crate::health::WorkerHealthStatus;
use crate::worker::command_processor::{EventIntegrationStatus, WorkerStatus};
use crate::worker::domain_event_commands::DomainEventToPublish;

// ============================================================================
// StepExecutorActor Messages
// ============================================================================

/// Execute a step from a PGMQ message
#[derive(Debug)]
pub struct ExecuteStepMessage {
    pub message: PgmqMessage<StepMessage>,
    pub queue_name: String,
}

impl Message for ExecuteStepMessage {
    type Response = bool; // true if claimed and executed, false if not claimed
}

/// Execute a step with correlation ID for event tracking
#[derive(Debug)]
pub struct ExecuteStepWithCorrelationMessage {
    pub message: PgmqMessage<StepMessage>,
    pub queue_name: String,
    pub correlation_id: Uuid,
}

impl Message for ExecuteStepWithCorrelationMessage {
    type Response = ();
}

/// Execute step from raw PGMQ message (TAS-43 event system)
#[derive(Debug)]
pub struct ExecuteStepFromPgmqMessage {
    pub queue_name: String,
    pub message: PgmqMessage,
}

impl Message for ExecuteStepFromPgmqMessage {
    type Response = ();
}

/// Execute step from provider-agnostic QueuedMessage (TAS-133)
///
/// This message type supports multiple messaging providers by using the
/// provider-agnostic `QueuedMessage<serde_json::Value>` wrapper. The queue name
/// and message ID are embedded in the `MessageHandle`.
#[derive(Debug)]
pub struct ExecuteStepFromQueuedMessage {
    pub message: QueuedMessage<serde_json::Value>,
}

impl Message for ExecuteStepFromQueuedMessage {
    type Response = ();
}

/// Execute step from MessageEvent (TAS-43 event system)
///
/// TAS-133: Updated to use provider-agnostic `MessageEvent` for multi-backend support
#[derive(Debug)]
pub struct ExecuteStepFromEventMessage {
    pub message_event: MessageEvent,
}

impl Message for ExecuteStepFromEventMessage {
    type Response = ();
}

// ============================================================================
// FFICompletionActor Messages
// ============================================================================

/// Process step completion from FFI handler
#[derive(Debug)]
pub struct ProcessStepCompletionMessage {
    pub step_result: StepExecutionResult,
    pub correlation_id: Option<Uuid>,
}

impl Message for ProcessStepCompletionMessage {
    type Response = ();
}

/// Send step result to orchestration
#[derive(Debug)]
pub struct SendStepResultMessage {
    pub result: StepExecutionResult,
}

impl Message for SendStepResultMessage {
    type Response = ();
}

// ============================================================================
// TemplateCacheActor Messages
// ============================================================================

/// Refresh task template cache
#[derive(Debug)]
pub struct RefreshTemplateCacheMessage {
    pub namespace: Option<String>,
}

impl Message for RefreshTemplateCacheMessage {
    type Response = ();
}

// ============================================================================
// DomainEventActor Messages
// ============================================================================

/// Dispatch domain events (fire-and-forget)
#[derive(Debug)]
pub struct DispatchEventsMessage {
    pub events: Vec<DomainEventToPublish>,
    pub publisher_name: String,
    pub correlation_id: Uuid,
}

impl Message for DispatchEventsMessage {
    type Response = bool; // true if dispatched, false if channel full
}

// ============================================================================
// WorkerStatusActor Messages
// ============================================================================

/// Get current worker status
#[derive(Debug)]
pub struct GetWorkerStatusMessage;

impl Message for GetWorkerStatusMessage {
    type Response = WorkerStatus;
}

/// Get event integration status
#[derive(Debug)]
pub struct GetEventStatusMessage;

impl Message for GetEventStatusMessage {
    type Response = EventIntegrationStatus;
}

/// Health check request
#[derive(Debug)]
pub struct HealthCheckMessage;

impl Message for HealthCheckMessage {
    type Response = WorkerHealthStatus;
}

/// Enable or disable event integration
#[derive(Debug)]
pub struct SetEventIntegrationMessage {
    pub enabled: bool,
}

impl Message for SetEventIntegrationMessage {
    type Response = ();
}

// ============================================================================
// TAS-67: Handler Dispatch Messages
// ============================================================================

/// Trace context for distributed tracing propagation
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// OpenTelemetry trace ID (32 hex chars)
    pub trace_id: String,
    /// OpenTelemetry span ID (16 hex chars)
    pub span_id: String,
}

/// Dispatch a step to a handler for execution (fire-and-forget)
///
/// TAS-67: This message is sent from StepExecutorActor to the handler dispatch
/// channel. The sender does not wait for a response - completion flows back
/// through a separate completion channel.
///
/// ## Flow
///
/// ```text
/// StepExecutorActor → dispatch_channel → HandlerDispatchService
///                                              │
///                                              ├─→ Rust: registry.get() → handler.call()
///                                              │
///                                              └─→ FFI: FfiDispatchChannel.poll()
/// ```
#[derive(Debug)]
pub struct DispatchHandlerMessage {
    /// Unique event identifier for correlation
    pub event_id: Uuid,
    /// The step UUID being executed
    pub step_uuid: Uuid,
    /// Task UUID for logging and correlation
    pub task_uuid: Uuid,
    /// Fully hydrated step data with all execution context
    pub task_sequence_step: TaskSequenceStep,
    /// Correlation ID for completion tracking
    pub correlation_id: Uuid,
    /// Optional trace context for distributed tracing
    pub trace_context: Option<TraceContext>,
}

impl Message for DispatchHandlerMessage {
    type Response = (); // Fire-and-forget, no response expected
}

impl DispatchHandlerMessage {
    /// Create a continuation message from a checkpoint yield (TAS-125)
    ///
    /// This creates a new dispatch message for a step that has yielded at a
    /// checkpoint. The step will be re-executed with its checkpoint data
    /// available for resumption.
    ///
    /// The new message gets a fresh event_id since this is a new dispatch,
    /// but preserves the step_uuid, task_uuid, and correlation context.
    pub fn from_checkpoint_continuation(
        step_uuid: Uuid,
        task_uuid: Uuid,
        task_sequence_step: TaskSequenceStep,
        correlation_id: Uuid,
        trace_context: Option<TraceContext>,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(), // New event ID for this continuation
            step_uuid,
            task_uuid,
            task_sequence_step,
            correlation_id,
            trace_context,
        }
    }
}
