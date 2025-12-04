//! # Worker Command Types
//!
//! TAS-69: Command types for worker operations. The legacy WorkerProcessor has been
//! removed and replaced with the actor-based ActorCommandProcessor in
//! `actor_command_processor.rs`.
//!
//! ## Exported Types
//!
//! - `WorkerCommand`: Commands that can be sent to the worker command processor
//! - `WorkerStatus`: Worker status information
//! - `StepExecutionStats`: Step execution statistics
//! - `EventIntegrationStatus`: Event integration status for FFI communication

use pgmq::Message as PgmqMessage;
use pgmq_notify::MessageReadyEvent;
use tasker_shared::messaging::message::SimpleStepMessage;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::health::WorkerHealthStatus;

/// Worker command responder type
pub type CommandResponder<T> = oneshot::Sender<tasker_shared::TaskerResult<T>>;

/// Commands that can be sent to the worker command processor
///
/// TAS-69: These commands are routed through ActorCommandProcessor to the
/// appropriate actor for handling.
#[derive(Debug)]
pub enum WorkerCommand {
    /// Execute a step from raw SimpleStepMessage - handles database queries, claiming, and deletion
    ExecuteStep {
        message: PgmqMessage<SimpleStepMessage>,
        queue_name: String,
        resp: CommandResponder<()>,
    },
    /// Get current worker status and statistics
    GetWorkerStatus {
        resp: CommandResponder<WorkerStatus>,
    },
    /// Send step execution result to orchestration
    SendStepResult {
        result: tasker_shared::messaging::StepExecutionResult,
        resp: CommandResponder<()>,
    },
    /// Execute step with event correlation for FFI handler tracking
    ExecuteStepWithCorrelation {
        message: PgmqMessage<SimpleStepMessage>,
        queue_name: String,
        correlation_id: Uuid,
        resp: CommandResponder<()>,
    },
    /// Process step completion event from FFI handler (event-driven)
    ProcessStepCompletion {
        step_result: tasker_shared::messaging::StepExecutionResult,
        correlation_id: Option<Uuid>,
        resp: CommandResponder<()>,
    },
    /// Enable or disable event integration at runtime
    SetEventIntegration {
        enabled: bool,
        resp: CommandResponder<()>,
    },
    /// Get event integration statistics and status
    GetEventStatus {
        resp: CommandResponder<EventIntegrationStatus>,
    },
    /// Refresh task template cache
    RefreshTemplateCache {
        namespace: Option<String>,
        resp: CommandResponder<()>,
    },
    /// Health check command for monitoring responsiveness
    HealthCheck {
        resp: CommandResponder<WorkerHealthStatus>,
    },
    /// Shutdown the worker processor
    Shutdown { resp: CommandResponder<()> },
    /// Execute step from PgmqMessage - TAS-43 event system integration
    ExecuteStepFromMessage {
        queue_name: String,
        message: PgmqMessage,
        resp: CommandResponder<()>,
    },
    /// Execute step from MessageReadyEvent - TAS-43 event system integration
    ExecuteStepFromEvent {
        message_event: MessageReadyEvent,
        resp: CommandResponder<()>,
    },
}

/// Worker status information
#[derive(Debug, Clone)]
pub struct WorkerStatus {
    pub worker_id: String,
    pub status: String,
    pub steps_executed: u64,
    pub steps_succeeded: u64,
    pub steps_failed: u64,
    pub uptime_seconds: u64,
    pub registered_handlers: Vec<String>,
}

/// Step execution statistics
#[derive(Debug, Clone, Default)]
pub struct StepExecutionStats {
    pub total_executed: u64,
    pub total_succeeded: u64,
    pub total_failed: u64,
    pub average_execution_time_ms: f64,
}

/// Event integration status for monitoring FFI communication
#[derive(Debug, Clone)]
pub struct EventIntegrationStatus {
    pub enabled: bool,
    pub events_published: u64,
    pub events_received: u64,
    pub correlation_tracking_enabled: bool,
    pub pending_correlations: usize,
    pub ffi_handlers_subscribed: usize,
    pub completion_subscribers: usize,
}
