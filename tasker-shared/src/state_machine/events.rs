use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Events that can trigger task state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum TaskEvent {
    // Lifecycle events
    /// Start processing the task
    Start,
    /// Cancel the task
    Cancel,
    /// Give up on the task (BlockedByFailures -> Error)
    GiveUp,
    /// Manually resolve the task
    ManualResolution,

    // Discovery events
    /// Ready steps found during initialization (count of ready steps)
    ReadyStepsFound(u32),
    /// No steps found - task can complete immediately
    NoStepsFound,
    /// No dependencies are ready - must wait
    NoDependenciesReady,
    /// Dependencies became ready - can proceed
    DependenciesReady,

    // Processing events
    /// Steps successfully enqueued (list of step UUIDs)
    StepsEnqueued(Vec<Uuid>),
    /// Failed to enqueue steps
    EnqueueFailed(String),
    /// A single step completed
    StepCompleted(Uuid),
    /// A single step failed
    StepFailed(Uuid),
    /// All steps in current batch completed
    AllStepsCompleted,
    /// All steps completed successfully
    AllStepsSuccessful,

    // Failure events
    /// Permanent failure that blocks progress
    PermanentFailure(String),
    /// Retry timeout expired - ready to retry
    RetryReady,

    // System events
    /// Timeout occurred
    Timeout,
    /// Processor crashed or became unavailable
    ProcessorCrashed,

    // Legacy events for backward compatibility
    /// Legacy complete event
    Complete,
    /// Legacy fail event
    Fail(String),
    /// Legacy manually resolve event
    ResolveManually,
    /// Legacy reset event
    Reset,
}

impl TaskEvent {
    /// Get a string representation of the event type for logging
    pub fn event_type(&self) -> &'static str {
        match self {
            // Lifecycle events
            Self::Start => "start",
            Self::Cancel => "cancel",
            Self::GiveUp => "give_up",
            Self::ManualResolution => "manual_resolution",

            // Discovery events
            Self::ReadyStepsFound(_) => "ready_steps_found",
            Self::NoStepsFound => "no_steps_found",
            Self::NoDependenciesReady => "no_dependencies_ready",
            Self::DependenciesReady => "dependencies_ready",

            // Processing events
            Self::StepsEnqueued(_) => "steps_enqueued",
            Self::EnqueueFailed(_) => "enqueue_failed",
            Self::StepCompleted(_) => "step_completed",
            Self::StepFailed(_) => "step_failed",
            Self::AllStepsCompleted => "all_steps_completed",
            Self::AllStepsSuccessful => "all_steps_successful",

            // Failure events
            Self::PermanentFailure(_) => "permanent_failure",
            Self::RetryReady => "retry_ready",

            // System events
            Self::Timeout => "timeout",
            Self::ProcessorCrashed => "processor_crashed",

            // Legacy events
            Self::Complete => "complete",
            Self::Fail(_) => "fail",
            Self::ResolveManually => "resolve_manually",
            Self::Reset => "reset",
        }
    }

    /// Extract error message if this is a failure event
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Fail(msg) | Self::EnqueueFailed(msg) | Self::PermanentFailure(msg) => Some(msg),
            _ => None,
        }
    }

    /// Check if this event represents a terminal transition
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Complete
                | Self::Cancel
                | Self::ResolveManually
                | Self::ManualResolution
                | Self::GiveUp
        )
    }

    /// Check if this event requires processor ownership (TAS-41 spec lines 290-301)
    pub fn requires_ownership(&self) -> bool {
        matches!(
            self,
            Self::Start
                | Self::ReadyStepsFound(_)
                | Self::StepsEnqueued(_)
                | Self::StepCompleted(_)
                | Self::AllStepsCompleted
                | Self::AllStepsSuccessful
        )
    }

    /// Get ready steps count from ReadyStepsFound event
    pub fn ready_steps_count(&self) -> Option<u32> {
        match self {
            Self::ReadyStepsFound(count) => Some(*count),
            _ => None,
        }
    }

    /// Get enqueued step UUIDs from StepsEnqueued event
    pub fn enqueued_steps(&self) -> Option<&[Uuid]> {
        match self {
            Self::StepsEnqueued(steps) => Some(steps),
            _ => None,
        }
    }

    /// Get completed or failed step UUID
    pub fn step_uuid(&self) -> Option<Uuid> {
        match self {
            Self::StepCompleted(uuid) | Self::StepFailed(uuid) => Some(*uuid),
            _ => None,
        }
    }
}

/// Events that can trigger workflow step state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StepEvent {
    /// Enqueue the step for processing (pending → enqueued)
    Enqueue,
    /// Start processing the step (enqueued → in_progress)
    Start,
    /// Enqueue step results for orchestration processing (in_progress → enqueued_for_orchestration)
    EnqueueForOrchestration(Option<Value>),
    /// Enqueue step error results for orchestration processing (in_progress → enqueued_as_error_for_orchestration)
    EnqueueAsErrorForOrchestration(Option<Value>),
    /// Mark step as complete with optional results (enqueued_for_orchestration → complete)
    Complete(Option<Value>),
    /// Mark step as failed with error message (enqueued_for_orchestration → error)
    Fail(String),
    /// Cancel the step
    Cancel,
    /// Manually resolve the step
    ResolveManually,
    /// Retry the step (from error state)
    Retry,
}

impl StepEvent {
    /// Get a string representation of the event type for logging
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Enqueue => "enqueue",
            Self::Start => "start",
            Self::EnqueueForOrchestration(_) => "enqueue_for_orchestration",
            Self::EnqueueAsErrorForOrchestration(_) => "enqueue_as_error_for_orchestration",
            Self::Complete(_) => "complete",
            Self::Fail(_) => "fail",
            Self::Cancel => "cancel",
            Self::ResolveManually => "resolve_manually",
            Self::Retry => "retry",
        }
    }

    /// Extract error message if this is a failure event
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Fail(msg) => Some(msg),
            _ => None,
        }
    }

    /// Extract results if this is a completion or enqueue for orchestration event
    pub fn results(&self) -> Option<&Value> {
        match self {
            Self::Complete(results) => results.as_ref(),
            Self::EnqueueForOrchestration(results) => results.as_ref(),
            Self::EnqueueAsErrorForOrchestration(results) => results.as_ref(),
            _ => None,
        }
    }

    /// Check if this event represents a terminal transition
    /// EnqueueForOrchestration is NOT terminal as orchestration still needs to process
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Complete(_) | Self::Cancel | Self::ResolveManually
        )
    }

    /// Check if this event can be applied from error state
    pub fn allows_retry(&self) -> bool {
        matches!(self, Self::Retry | Self::Cancel | Self::ResolveManually)
    }
}

/// Helper for creating common events
impl TaskEvent {
    /// Create a failure event with the given error message
    pub fn fail_with_error(error: impl Into<String>) -> Self {
        Self::Fail(error.into())
    }

    /// Create a permanent failure event
    pub fn permanent_failure(error: impl Into<String>) -> Self {
        Self::PermanentFailure(error.into())
    }

    /// Create an enqueue failed event
    pub fn enqueue_failed(error: impl Into<String>) -> Self {
        Self::EnqueueFailed(error.into())
    }

    /// Create a ready steps found event
    pub fn ready_steps_found(count: u32) -> Self {
        Self::ReadyStepsFound(count)
    }

    /// Create a steps enqueued event
    pub fn steps_enqueued(step_uuids: Vec<Uuid>) -> Self {
        Self::StepsEnqueued(step_uuids)
    }

    /// Create a step completed event
    pub fn step_completed(step_uuid: Uuid) -> Self {
        Self::StepCompleted(step_uuid)
    }

    /// Create a step failed event
    pub fn step_failed(step_uuid: Uuid) -> Self {
        Self::StepFailed(step_uuid)
    }
}

impl StepEvent {
    /// Create a failure event with the given error message
    pub fn fail_with_error(error: impl Into<String>) -> Self {
        Self::Fail(error.into())
    }

    /// Create a completion event with results
    pub fn complete_with_results(results: Value) -> Self {
        Self::Complete(Some(results))
    }

    /// Create a simple completion event without results
    pub fn complete_simple() -> Self {
        Self::Complete(None)
    }

    /// Create an enqueue for orchestration event with results
    pub fn enqueue_for_orchestration_with_results(results: Value) -> Self {
        Self::EnqueueForOrchestration(Some(results))
    }

    /// Create a simple enqueue for orchestration event without results
    pub fn enqueue_for_orchestration_simple() -> Self {
        Self::EnqueueForOrchestration(None)
    }
}
