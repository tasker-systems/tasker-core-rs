//! # Worker Actor Messages
//!
//! TAS-69: Message types for worker actors following the actor pattern.
//!
//! Each message type implements the `Message` trait with an associated
//! response type for type-safe message handling.

use pgmq::Message as PgmqMessage;
use pgmq_notify::MessageReadyEvent;
use uuid::Uuid;

use tasker_shared::messaging::message::SimpleStepMessage;
use tasker_shared::messaging::StepExecutionResult;

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
    pub message: PgmqMessage<SimpleStepMessage>,
    pub queue_name: String,
}

impl Message for ExecuteStepMessage {
    type Response = bool; // true if claimed and executed, false if not claimed
}

/// Execute a step with correlation ID for event tracking
#[derive(Debug)]
pub struct ExecuteStepWithCorrelationMessage {
    pub message: PgmqMessage<SimpleStepMessage>,
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

/// Execute step from MessageReadyEvent (TAS-43 pgmq-notify)
#[derive(Debug)]
pub struct ExecuteStepFromEventMessage {
    pub message_event: MessageReadyEvent,
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
