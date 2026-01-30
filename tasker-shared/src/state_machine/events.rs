//! # State Machine Events
//!
//! Events that trigger state transitions in tasks and workflow steps.
//!
//! ## Overview
//!
//! Events are the triggers that cause state transitions in the state machine.
//! Each event represents something that happened which may change the entity's state.
//!
//! ## Task Events
//!
//! Task events drive the task lifecycle through orchestration phases:
//!
//! | Event Category | Events | Description |
//! |----------------|--------|-------------|
//! | **Lifecycle** | `Start`, `Cancel`, `GiveUp`, `ManualResolution` | Control task execution flow |
//! | **Discovery** | `ReadyStepsFound`, `NoStepsFound`, `NoDependenciesReady` | Step readiness detection |
//! | **Processing** | `StepsEnqueued`, `StepCompleted`, `StepFailed`, `AllStepsSuccessful` | Step execution tracking |
//! | **Failure** | `PermanentFailure`, `RetryReady` | Error handling and retry |
//! | **System** | `Timeout`, `ProcessorCrashed` | Infrastructure events |
//!
//! ## Step Events
//!
//! Step events drive individual step execution:
//!
//! | Event | Description |
//! |-------|-------------|
//! | `Enqueue` | Queue step for worker processing |
//! | `Start` | Worker begins execution |
//! | `EnqueueForOrchestration` | Worker completed, queue for orchestration |
//! | `Complete` | Orchestration finalized step success |
//! | `Fail` | Step execution failed |
//! | `WaitForRetry` | Enter retry backoff |
//! | `ResetForRetry` | Operator reset for fresh retry |
//!
//! ## Event Properties
//!
//! Events provide metadata via helper methods:
//! - `event_type()` - String name for logging
//! - `is_terminal()` - Whether this completes the entity lifecycle
//! - `requires_ownership()` - Whether processor ownership is needed
//! - `error_message()` - Extract error details from failure events

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
    /// Manually resolve the step (transitions to resolved_manually state)
    ResolveManually,
    /// Manually complete the step with execution results (transitions to complete state)
    ///
    /// This allows operators to provide properly-shaped execution results that
    /// dependent steps can consume, enabling the task to continue normal execution.
    CompleteManually(Option<Value>),
    /// Reset attempt counter and return to pending for natural retry (error → pending)
    ///
    /// This allows operators to reset failed steps when they believe the transient
    /// issue has been resolved and natural retry should succeed.
    ResetForRetry,
    /// Retry the step (from error state)
    Retry,
    /// Wait for retry delay with backoff (error → waiting_for_retry)
    WaitForRetry(String),
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
            Self::CompleteManually(_) => "complete_manually",
            Self::ResetForRetry => "reset_for_retry",
            Self::Retry => "retry",
            Self::WaitForRetry(_) => "wait_for_retry",
        }
    }

    /// Extract error message if this is a failure event
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Fail(msg) | Self::WaitForRetry(msg) => Some(msg),
            _ => None,
        }
    }

    /// Extract results if this is a completion or enqueue for orchestration event
    pub fn results(&self) -> Option<&Value> {
        match self {
            Self::Complete(results) => results.as_ref(),
            Self::CompleteManually(results) => results.as_ref(),
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
            Self::Complete(_) | Self::CompleteManually(_) | Self::Cancel | Self::ResolveManually
        )
    }

    /// Check if this event can be applied from error state
    pub fn allows_retry(&self) -> bool {
        matches!(
            self,
            Self::Retry | Self::Cancel | Self::ResolveManually | Self::CompleteManually(_)
        )
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

    /// Create a wait for retry event with error message
    pub fn wait_for_retry(error_message: impl Into<String>) -> Self {
        Self::WaitForRetry(error_message.into())
    }

    /// Create a manual completion event with results
    pub fn complete_manually_with_results(results: Value) -> Self {
        Self::CompleteManually(Some(results))
    }

    /// Create a simple manual completion event without results
    pub fn complete_manually_simple() -> Self {
        Self::CompleteManually(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // =========================================================================
    // TaskEvent::event_type() - all 22 variants
    // =========================================================================

    #[test]
    fn test_task_event_type_start() {
        assert_eq!(TaskEvent::Start.event_type(), "start");
    }

    #[test]
    fn test_task_event_type_cancel() {
        assert_eq!(TaskEvent::Cancel.event_type(), "cancel");
    }

    #[test]
    fn test_task_event_type_give_up() {
        assert_eq!(TaskEvent::GiveUp.event_type(), "give_up");
    }

    #[test]
    fn test_task_event_type_manual_resolution() {
        assert_eq!(
            TaskEvent::ManualResolution.event_type(),
            "manual_resolution"
        );
    }

    #[test]
    fn test_task_event_type_ready_steps_found() {
        assert_eq!(
            TaskEvent::ReadyStepsFound(3).event_type(),
            "ready_steps_found"
        );
    }

    #[test]
    fn test_task_event_type_no_steps_found() {
        assert_eq!(TaskEvent::NoStepsFound.event_type(), "no_steps_found");
    }

    #[test]
    fn test_task_event_type_no_dependencies_ready() {
        assert_eq!(
            TaskEvent::NoDependenciesReady.event_type(),
            "no_dependencies_ready"
        );
    }

    #[test]
    fn test_task_event_type_dependencies_ready() {
        assert_eq!(
            TaskEvent::DependenciesReady.event_type(),
            "dependencies_ready"
        );
    }

    #[test]
    fn test_task_event_type_steps_enqueued() {
        let uuids = vec![Uuid::now_v7()];
        assert_eq!(
            TaskEvent::StepsEnqueued(uuids).event_type(),
            "steps_enqueued"
        );
    }

    #[test]
    fn test_task_event_type_enqueue_failed() {
        assert_eq!(
            TaskEvent::EnqueueFailed("err".into()).event_type(),
            "enqueue_failed"
        );
    }

    #[test]
    fn test_task_event_type_step_completed() {
        assert_eq!(
            TaskEvent::StepCompleted(Uuid::now_v7()).event_type(),
            "step_completed"
        );
    }

    #[test]
    fn test_task_event_type_step_failed() {
        assert_eq!(
            TaskEvent::StepFailed(Uuid::now_v7()).event_type(),
            "step_failed"
        );
    }

    #[test]
    fn test_task_event_type_all_steps_completed() {
        assert_eq!(
            TaskEvent::AllStepsCompleted.event_type(),
            "all_steps_completed"
        );
    }

    #[test]
    fn test_task_event_type_all_steps_successful() {
        assert_eq!(
            TaskEvent::AllStepsSuccessful.event_type(),
            "all_steps_successful"
        );
    }

    #[test]
    fn test_task_event_type_permanent_failure() {
        assert_eq!(
            TaskEvent::PermanentFailure("fatal".into()).event_type(),
            "permanent_failure"
        );
    }

    #[test]
    fn test_task_event_type_retry_ready() {
        assert_eq!(TaskEvent::RetryReady.event_type(), "retry_ready");
    }

    #[test]
    fn test_task_event_type_timeout() {
        assert_eq!(TaskEvent::Timeout.event_type(), "timeout");
    }

    #[test]
    fn test_task_event_type_processor_crashed() {
        assert_eq!(
            TaskEvent::ProcessorCrashed.event_type(),
            "processor_crashed"
        );
    }

    #[test]
    fn test_task_event_type_complete() {
        assert_eq!(TaskEvent::Complete.event_type(), "complete");
    }

    #[test]
    fn test_task_event_type_fail() {
        assert_eq!(TaskEvent::Fail("bad".into()).event_type(), "fail");
    }

    #[test]
    fn test_task_event_type_resolve_manually() {
        assert_eq!(TaskEvent::ResolveManually.event_type(), "resolve_manually");
    }

    #[test]
    fn test_task_event_type_reset() {
        assert_eq!(TaskEvent::Reset.event_type(), "reset");
    }

    // =========================================================================
    // TaskEvent::error_message()
    // =========================================================================

    #[test]
    fn test_task_event_error_message_fail() {
        let event = TaskEvent::Fail("something went wrong".into());
        assert_eq!(event.error_message(), Some("something went wrong"));
    }

    #[test]
    fn test_task_event_error_message_enqueue_failed() {
        let event = TaskEvent::EnqueueFailed("queue full".into());
        assert_eq!(event.error_message(), Some("queue full"));
    }

    #[test]
    fn test_task_event_error_message_permanent_failure() {
        let event = TaskEvent::PermanentFailure("unrecoverable".into());
        assert_eq!(event.error_message(), Some("unrecoverable"));
    }

    #[test]
    fn test_task_event_error_message_none_for_start() {
        assert_eq!(TaskEvent::Start.error_message(), None);
    }

    #[test]
    fn test_task_event_error_message_none_for_complete() {
        assert_eq!(TaskEvent::Complete.error_message(), None);
    }

    #[test]
    fn test_task_event_error_message_none_for_cancel() {
        assert_eq!(TaskEvent::Cancel.error_message(), None);
    }

    #[test]
    fn test_task_event_error_message_none_for_ready_steps_found() {
        assert_eq!(TaskEvent::ReadyStepsFound(1).error_message(), None);
    }

    // =========================================================================
    // TaskEvent::is_terminal()
    // =========================================================================

    #[test]
    fn test_task_event_is_terminal_complete() {
        assert!(TaskEvent::Complete.is_terminal());
    }

    #[test]
    fn test_task_event_is_terminal_cancel() {
        assert!(TaskEvent::Cancel.is_terminal());
    }

    #[test]
    fn test_task_event_is_terminal_resolve_manually() {
        assert!(TaskEvent::ResolveManually.is_terminal());
    }

    #[test]
    fn test_task_event_is_terminal_manual_resolution() {
        assert!(TaskEvent::ManualResolution.is_terminal());
    }

    #[test]
    fn test_task_event_is_terminal_give_up() {
        assert!(TaskEvent::GiveUp.is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_start() {
        assert!(!TaskEvent::Start.is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_fail() {
        assert!(!TaskEvent::Fail("err".into()).is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_ready_steps_found() {
        assert!(!TaskEvent::ReadyStepsFound(5).is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_steps_enqueued() {
        assert!(!TaskEvent::StepsEnqueued(vec![]).is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_step_completed() {
        assert!(!TaskEvent::StepCompleted(Uuid::now_v7()).is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_permanent_failure() {
        assert!(!TaskEvent::PermanentFailure("fatal".into()).is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_retry_ready() {
        assert!(!TaskEvent::RetryReady.is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_timeout() {
        assert!(!TaskEvent::Timeout.is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_processor_crashed() {
        assert!(!TaskEvent::ProcessorCrashed.is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_reset() {
        assert!(!TaskEvent::Reset.is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_no_steps_found() {
        assert!(!TaskEvent::NoStepsFound.is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_no_dependencies_ready() {
        assert!(!TaskEvent::NoDependenciesReady.is_terminal());
    }

    #[test]
    fn test_task_event_is_not_terminal_dependencies_ready() {
        assert!(!TaskEvent::DependenciesReady.is_terminal());
    }

    // =========================================================================
    // TaskEvent::requires_ownership()
    // =========================================================================

    #[test]
    fn test_task_event_requires_ownership_start() {
        assert!(TaskEvent::Start.requires_ownership());
    }

    #[test]
    fn test_task_event_requires_ownership_ready_steps_found() {
        assert!(TaskEvent::ReadyStepsFound(2).requires_ownership());
    }

    #[test]
    fn test_task_event_requires_ownership_steps_enqueued() {
        assert!(TaskEvent::StepsEnqueued(vec![Uuid::now_v7()]).requires_ownership());
    }

    #[test]
    fn test_task_event_requires_ownership_step_completed() {
        assert!(TaskEvent::StepCompleted(Uuid::now_v7()).requires_ownership());
    }

    #[test]
    fn test_task_event_requires_ownership_all_steps_completed() {
        assert!(TaskEvent::AllStepsCompleted.requires_ownership());
    }

    #[test]
    fn test_task_event_requires_ownership_all_steps_successful() {
        assert!(TaskEvent::AllStepsSuccessful.requires_ownership());
    }

    #[test]
    fn test_task_event_does_not_require_ownership_cancel() {
        assert!(!TaskEvent::Cancel.requires_ownership());
    }

    #[test]
    fn test_task_event_does_not_require_ownership_fail() {
        assert!(!TaskEvent::Fail("err".into()).requires_ownership());
    }

    #[test]
    fn test_task_event_does_not_require_ownership_complete() {
        assert!(!TaskEvent::Complete.requires_ownership());
    }

    #[test]
    fn test_task_event_does_not_require_ownership_timeout() {
        assert!(!TaskEvent::Timeout.requires_ownership());
    }

    #[test]
    fn test_task_event_does_not_require_ownership_permanent_failure() {
        assert!(!TaskEvent::PermanentFailure("fatal".into()).requires_ownership());
    }

    #[test]
    fn test_task_event_does_not_require_ownership_step_failed() {
        assert!(!TaskEvent::StepFailed(Uuid::now_v7()).requires_ownership());
    }

    #[test]
    fn test_task_event_does_not_require_ownership_give_up() {
        assert!(!TaskEvent::GiveUp.requires_ownership());
    }

    #[test]
    fn test_task_event_does_not_require_ownership_reset() {
        assert!(!TaskEvent::Reset.requires_ownership());
    }

    // =========================================================================
    // TaskEvent::ready_steps_count()
    // =========================================================================

    #[test]
    fn test_task_event_ready_steps_count_some() {
        assert_eq!(TaskEvent::ReadyStepsFound(5).ready_steps_count(), Some(5));
    }

    #[test]
    fn test_task_event_ready_steps_count_zero() {
        assert_eq!(TaskEvent::ReadyStepsFound(0).ready_steps_count(), Some(0));
    }

    #[test]
    fn test_task_event_ready_steps_count_none_for_start() {
        assert_eq!(TaskEvent::Start.ready_steps_count(), None);
    }

    #[test]
    fn test_task_event_ready_steps_count_none_for_complete() {
        assert_eq!(TaskEvent::Complete.ready_steps_count(), None);
    }

    // =========================================================================
    // TaskEvent::enqueued_steps()
    // =========================================================================

    #[test]
    fn test_task_event_enqueued_steps_some() {
        let uuid1 = Uuid::now_v7();
        let uuid2 = Uuid::now_v7();
        let event = TaskEvent::StepsEnqueued(vec![uuid1, uuid2]);
        let steps = event.enqueued_steps().expect("should return Some");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0], uuid1);
        assert_eq!(steps[1], uuid2);
    }

    #[test]
    fn test_task_event_enqueued_steps_empty_vec() {
        let event = TaskEvent::StepsEnqueued(vec![]);
        let steps = event
            .enqueued_steps()
            .expect("should return Some even when empty");
        assert!(steps.is_empty());
    }

    #[test]
    fn test_task_event_enqueued_steps_none_for_start() {
        assert!(TaskEvent::Start.enqueued_steps().is_none());
    }

    #[test]
    fn test_task_event_enqueued_steps_none_for_complete() {
        assert!(TaskEvent::Complete.enqueued_steps().is_none());
    }

    // =========================================================================
    // TaskEvent::step_uuid()
    // =========================================================================

    #[test]
    fn test_task_event_step_uuid_step_completed() {
        let uuid = Uuid::now_v7();
        assert_eq!(TaskEvent::StepCompleted(uuid).step_uuid(), Some(uuid));
    }

    #[test]
    fn test_task_event_step_uuid_step_failed() {
        let uuid = Uuid::now_v7();
        assert_eq!(TaskEvent::StepFailed(uuid).step_uuid(), Some(uuid));
    }

    #[test]
    fn test_task_event_step_uuid_none_for_start() {
        assert_eq!(TaskEvent::Start.step_uuid(), None);
    }

    #[test]
    fn test_task_event_step_uuid_none_for_steps_enqueued() {
        assert_eq!(
            TaskEvent::StepsEnqueued(vec![Uuid::now_v7()]).step_uuid(),
            None
        );
    }

    // =========================================================================
    // TaskEvent helper constructors
    // =========================================================================

    #[test]
    fn test_task_event_fail_with_error() {
        let event = TaskEvent::fail_with_error("connection timeout");
        assert_eq!(event.event_type(), "fail");
        assert_eq!(event.error_message(), Some("connection timeout"));
    }

    #[test]
    fn test_task_event_fail_with_error_from_string() {
        let msg = String::from("owned error");
        let event = TaskEvent::fail_with_error(msg);
        assert_eq!(event.error_message(), Some("owned error"));
    }

    #[test]
    fn test_task_event_permanent_failure_constructor() {
        let event = TaskEvent::permanent_failure("schema mismatch");
        assert_eq!(event.event_type(), "permanent_failure");
        assert_eq!(event.error_message(), Some("schema mismatch"));
    }

    #[test]
    fn test_task_event_enqueue_failed_constructor() {
        let event = TaskEvent::enqueue_failed("queue unavailable");
        assert_eq!(event.event_type(), "enqueue_failed");
        assert_eq!(event.error_message(), Some("queue unavailable"));
    }

    #[test]
    fn test_task_event_ready_steps_found_constructor() {
        let event = TaskEvent::ready_steps_found(10);
        assert_eq!(event.event_type(), "ready_steps_found");
        assert_eq!(event.ready_steps_count(), Some(10));
    }

    #[test]
    fn test_task_event_steps_enqueued_constructor() {
        let uuid = Uuid::now_v7();
        let event = TaskEvent::steps_enqueued(vec![uuid]);
        assert_eq!(event.event_type(), "steps_enqueued");
        let steps = event.enqueued_steps().expect("should have steps");
        assert_eq!(steps, &[uuid]);
    }

    #[test]
    fn test_task_event_step_completed_constructor() {
        let uuid = Uuid::now_v7();
        let event = TaskEvent::step_completed(uuid);
        assert_eq!(event.event_type(), "step_completed");
        assert_eq!(event.step_uuid(), Some(uuid));
    }

    #[test]
    fn test_task_event_step_failed_constructor() {
        let uuid = Uuid::now_v7();
        let event = TaskEvent::step_failed(uuid);
        assert_eq!(event.event_type(), "step_failed");
        assert_eq!(event.step_uuid(), Some(uuid));
    }

    // =========================================================================
    // TaskEvent serde round-trip
    // =========================================================================

    #[test]
    fn test_task_event_serde_roundtrip_start() {
        let event = TaskEvent::Start;
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: TaskEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.event_type(), "start");
    }

    #[test]
    fn test_task_event_serde_roundtrip_fail() {
        let event = TaskEvent::Fail("test error".into());
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: TaskEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.event_type(), "fail");
        assert_eq!(deserialized.error_message(), Some("test error"));
    }

    #[test]
    fn test_task_event_serde_roundtrip_steps_enqueued() {
        let uuid = Uuid::now_v7();
        let event = TaskEvent::StepsEnqueued(vec![uuid]);
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: TaskEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.event_type(), "steps_enqueued");
        let steps = deserialized.enqueued_steps().expect("should have steps");
        assert_eq!(steps, &[uuid]);
    }

    #[test]
    fn test_task_event_serde_roundtrip_ready_steps_found() {
        let event = TaskEvent::ReadyStepsFound(42);
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: TaskEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.ready_steps_count(), Some(42));
    }

    #[test]
    fn test_task_event_serde_roundtrip_step_completed() {
        let uuid = Uuid::now_v7();
        let event = TaskEvent::StepCompleted(uuid);
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: TaskEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.step_uuid(), Some(uuid));
    }

    // =========================================================================
    // StepEvent::event_type() - all 12 variants
    // =========================================================================

    #[test]
    fn test_step_event_type_enqueue() {
        assert_eq!(StepEvent::Enqueue.event_type(), "enqueue");
    }

    #[test]
    fn test_step_event_type_start() {
        assert_eq!(StepEvent::Start.event_type(), "start");
    }

    #[test]
    fn test_step_event_type_enqueue_for_orchestration() {
        assert_eq!(
            StepEvent::EnqueueForOrchestration(None).event_type(),
            "enqueue_for_orchestration"
        );
    }

    #[test]
    fn test_step_event_type_enqueue_as_error_for_orchestration() {
        assert_eq!(
            StepEvent::EnqueueAsErrorForOrchestration(None).event_type(),
            "enqueue_as_error_for_orchestration"
        );
    }

    #[test]
    fn test_step_event_type_complete() {
        assert_eq!(StepEvent::Complete(None).event_type(), "complete");
    }

    #[test]
    fn test_step_event_type_fail() {
        assert_eq!(StepEvent::Fail("err".into()).event_type(), "fail");
    }

    #[test]
    fn test_step_event_type_cancel() {
        assert_eq!(StepEvent::Cancel.event_type(), "cancel");
    }

    #[test]
    fn test_step_event_type_resolve_manually() {
        assert_eq!(StepEvent::ResolveManually.event_type(), "resolve_manually");
    }

    #[test]
    fn test_step_event_type_complete_manually() {
        assert_eq!(
            StepEvent::CompleteManually(None).event_type(),
            "complete_manually"
        );
    }

    #[test]
    fn test_step_event_type_reset_for_retry() {
        assert_eq!(StepEvent::ResetForRetry.event_type(), "reset_for_retry");
    }

    #[test]
    fn test_step_event_type_retry() {
        assert_eq!(StepEvent::Retry.event_type(), "retry");
    }

    #[test]
    fn test_step_event_type_wait_for_retry() {
        assert_eq!(
            StepEvent::WaitForRetry("backoff".into()).event_type(),
            "wait_for_retry"
        );
    }

    // =========================================================================
    // StepEvent::error_message()
    // =========================================================================

    #[test]
    fn test_step_event_error_message_fail() {
        let event = StepEvent::Fail("step failed".into());
        assert_eq!(event.error_message(), Some("step failed"));
    }

    #[test]
    fn test_step_event_error_message_wait_for_retry() {
        let event = StepEvent::WaitForRetry("retrying after timeout".into());
        assert_eq!(event.error_message(), Some("retrying after timeout"));
    }

    #[test]
    fn test_step_event_error_message_none_for_enqueue() {
        assert_eq!(StepEvent::Enqueue.error_message(), None);
    }

    #[test]
    fn test_step_event_error_message_none_for_start() {
        assert_eq!(StepEvent::Start.error_message(), None);
    }

    #[test]
    fn test_step_event_error_message_none_for_complete() {
        assert_eq!(StepEvent::Complete(None).error_message(), None);
    }

    #[test]
    fn test_step_event_error_message_none_for_cancel() {
        assert_eq!(StepEvent::Cancel.error_message(), None);
    }

    #[test]
    fn test_step_event_error_message_none_for_retry() {
        assert_eq!(StepEvent::Retry.error_message(), None);
    }

    #[test]
    fn test_step_event_error_message_none_for_complete_manually() {
        assert_eq!(StepEvent::CompleteManually(None).error_message(), None);
    }

    // =========================================================================
    // StepEvent::results()
    // =========================================================================

    #[test]
    fn test_step_event_results_complete_with_some() {
        let val = json!({"status": "ok"});
        let event = StepEvent::Complete(Some(val.clone()));
        assert_eq!(event.results(), Some(&val));
    }

    #[test]
    fn test_step_event_results_complete_with_none() {
        let event = StepEvent::Complete(None);
        assert_eq!(event.results(), None);
    }

    #[test]
    fn test_step_event_results_complete_manually_with_some() {
        let val = json!({"resolved": true});
        let event = StepEvent::CompleteManually(Some(val.clone()));
        assert_eq!(event.results(), Some(&val));
    }

    #[test]
    fn test_step_event_results_complete_manually_with_none() {
        let event = StepEvent::CompleteManually(None);
        assert_eq!(event.results(), None);
    }

    #[test]
    fn test_step_event_results_enqueue_for_orchestration_with_some() {
        let val = json!({"data": [1, 2, 3]});
        let event = StepEvent::EnqueueForOrchestration(Some(val.clone()));
        assert_eq!(event.results(), Some(&val));
    }

    #[test]
    fn test_step_event_results_enqueue_for_orchestration_with_none() {
        let event = StepEvent::EnqueueForOrchestration(None);
        assert_eq!(event.results(), None);
    }

    #[test]
    fn test_step_event_results_enqueue_as_error_for_orchestration_with_some() {
        let val = json!({"error_context": "partial"});
        let event = StepEvent::EnqueueAsErrorForOrchestration(Some(val.clone()));
        assert_eq!(event.results(), Some(&val));
    }

    #[test]
    fn test_step_event_results_enqueue_as_error_for_orchestration_with_none() {
        let event = StepEvent::EnqueueAsErrorForOrchestration(None);
        assert_eq!(event.results(), None);
    }

    #[test]
    fn test_step_event_results_none_for_enqueue() {
        assert_eq!(StepEvent::Enqueue.results(), None);
    }

    #[test]
    fn test_step_event_results_none_for_start() {
        assert_eq!(StepEvent::Start.results(), None);
    }

    #[test]
    fn test_step_event_results_none_for_fail() {
        assert_eq!(StepEvent::Fail("err".into()).results(), None);
    }

    #[test]
    fn test_step_event_results_none_for_cancel() {
        assert_eq!(StepEvent::Cancel.results(), None);
    }

    #[test]
    fn test_step_event_results_none_for_retry() {
        assert_eq!(StepEvent::Retry.results(), None);
    }

    #[test]
    fn test_step_event_results_none_for_resolve_manually() {
        assert_eq!(StepEvent::ResolveManually.results(), None);
    }

    #[test]
    fn test_step_event_results_none_for_reset_for_retry() {
        assert_eq!(StepEvent::ResetForRetry.results(), None);
    }

    #[test]
    fn test_step_event_results_none_for_wait_for_retry() {
        assert_eq!(StepEvent::WaitForRetry("msg".into()).results(), None);
    }

    // =========================================================================
    // StepEvent::is_terminal()
    // =========================================================================

    #[test]
    fn test_step_event_is_terminal_complete() {
        assert!(StepEvent::Complete(None).is_terminal());
    }

    #[test]
    fn test_step_event_is_terminal_complete_with_results() {
        assert!(StepEvent::Complete(Some(json!({"ok": true}))).is_terminal());
    }

    #[test]
    fn test_step_event_is_terminal_complete_manually() {
        assert!(StepEvent::CompleteManually(None).is_terminal());
    }

    #[test]
    fn test_step_event_is_terminal_cancel() {
        assert!(StepEvent::Cancel.is_terminal());
    }

    #[test]
    fn test_step_event_is_terminal_resolve_manually() {
        assert!(StepEvent::ResolveManually.is_terminal());
    }

    #[test]
    fn test_step_event_is_not_terminal_enqueue() {
        assert!(!StepEvent::Enqueue.is_terminal());
    }

    #[test]
    fn test_step_event_is_not_terminal_start() {
        assert!(!StepEvent::Start.is_terminal());
    }

    #[test]
    fn test_step_event_is_not_terminal_fail() {
        assert!(!StepEvent::Fail("err".into()).is_terminal());
    }

    #[test]
    fn test_step_event_is_not_terminal_enqueue_for_orchestration() {
        assert!(!StepEvent::EnqueueForOrchestration(None).is_terminal());
    }

    #[test]
    fn test_step_event_is_not_terminal_enqueue_as_error_for_orchestration() {
        assert!(!StepEvent::EnqueueAsErrorForOrchestration(None).is_terminal());
    }

    #[test]
    fn test_step_event_is_not_terminal_retry() {
        assert!(!StepEvent::Retry.is_terminal());
    }

    #[test]
    fn test_step_event_is_not_terminal_wait_for_retry() {
        assert!(!StepEvent::WaitForRetry("backoff".into()).is_terminal());
    }

    #[test]
    fn test_step_event_is_not_terminal_reset_for_retry() {
        assert!(!StepEvent::ResetForRetry.is_terminal());
    }

    // =========================================================================
    // StepEvent::allows_retry()
    // =========================================================================

    #[test]
    fn test_step_event_allows_retry_retry() {
        assert!(StepEvent::Retry.allows_retry());
    }

    #[test]
    fn test_step_event_allows_retry_cancel() {
        assert!(StepEvent::Cancel.allows_retry());
    }

    #[test]
    fn test_step_event_allows_retry_resolve_manually() {
        assert!(StepEvent::ResolveManually.allows_retry());
    }

    #[test]
    fn test_step_event_allows_retry_complete_manually() {
        assert!(StepEvent::CompleteManually(None).allows_retry());
    }

    #[test]
    fn test_step_event_allows_retry_complete_manually_with_results() {
        assert!(StepEvent::CompleteManually(Some(json!({"x": 1}))).allows_retry());
    }

    #[test]
    fn test_step_event_does_not_allow_retry_enqueue() {
        assert!(!StepEvent::Enqueue.allows_retry());
    }

    #[test]
    fn test_step_event_does_not_allow_retry_start() {
        assert!(!StepEvent::Start.allows_retry());
    }

    #[test]
    fn test_step_event_does_not_allow_retry_complete() {
        assert!(!StepEvent::Complete(None).allows_retry());
    }

    #[test]
    fn test_step_event_does_not_allow_retry_fail() {
        assert!(!StepEvent::Fail("err".into()).allows_retry());
    }

    #[test]
    fn test_step_event_does_not_allow_retry_enqueue_for_orchestration() {
        assert!(!StepEvent::EnqueueForOrchestration(None).allows_retry());
    }

    #[test]
    fn test_step_event_does_not_allow_retry_enqueue_as_error_for_orchestration() {
        assert!(!StepEvent::EnqueueAsErrorForOrchestration(None).allows_retry());
    }

    #[test]
    fn test_step_event_does_not_allow_retry_wait_for_retry() {
        assert!(!StepEvent::WaitForRetry("msg".into()).allows_retry());
    }

    #[test]
    fn test_step_event_does_not_allow_retry_reset_for_retry() {
        assert!(!StepEvent::ResetForRetry.allows_retry());
    }

    // =========================================================================
    // StepEvent helper constructors
    // =========================================================================

    #[test]
    fn test_step_event_fail_with_error() {
        let event = StepEvent::fail_with_error("handler crashed");
        assert_eq!(event.event_type(), "fail");
        assert_eq!(event.error_message(), Some("handler crashed"));
    }

    #[test]
    fn test_step_event_fail_with_error_from_string() {
        let msg = String::from("owned msg");
        let event = StepEvent::fail_with_error(msg);
        assert_eq!(event.error_message(), Some("owned msg"));
    }

    #[test]
    fn test_step_event_complete_with_results() {
        let val = json!({"output": "data"});
        let event = StepEvent::complete_with_results(val.clone());
        assert_eq!(event.event_type(), "complete");
        assert_eq!(event.results(), Some(&val));
        assert!(event.is_terminal());
    }

    #[test]
    fn test_step_event_complete_simple() {
        let event = StepEvent::complete_simple();
        assert_eq!(event.event_type(), "complete");
        assert_eq!(event.results(), None);
        assert!(event.is_terminal());
    }

    #[test]
    fn test_step_event_enqueue_for_orchestration_with_results() {
        let val = json!({"partial": true});
        let event = StepEvent::enqueue_for_orchestration_with_results(val.clone());
        assert_eq!(event.event_type(), "enqueue_for_orchestration");
        assert_eq!(event.results(), Some(&val));
        assert!(!event.is_terminal());
    }

    #[test]
    fn test_step_event_enqueue_for_orchestration_simple() {
        let event = StepEvent::enqueue_for_orchestration_simple();
        assert_eq!(event.event_type(), "enqueue_for_orchestration");
        assert_eq!(event.results(), None);
        assert!(!event.is_terminal());
    }

    #[test]
    fn test_step_event_wait_for_retry_constructor() {
        let event = StepEvent::wait_for_retry("timeout exceeded");
        assert_eq!(event.event_type(), "wait_for_retry");
        assert_eq!(event.error_message(), Some("timeout exceeded"));
        assert!(!event.is_terminal());
    }

    #[test]
    fn test_step_event_wait_for_retry_from_string() {
        let msg = String::from("owned retry msg");
        let event = StepEvent::wait_for_retry(msg);
        assert_eq!(event.error_message(), Some("owned retry msg"));
    }

    #[test]
    fn test_step_event_complete_manually_with_results() {
        let val = json!({"manual": "fix"});
        let event = StepEvent::complete_manually_with_results(val.clone());
        assert_eq!(event.event_type(), "complete_manually");
        assert_eq!(event.results(), Some(&val));
        assert!(event.is_terminal());
        assert!(event.allows_retry());
    }

    #[test]
    fn test_step_event_complete_manually_simple() {
        let event = StepEvent::complete_manually_simple();
        assert_eq!(event.event_type(), "complete_manually");
        assert_eq!(event.results(), None);
        assert!(event.is_terminal());
        assert!(event.allows_retry());
    }

    // =========================================================================
    // StepEvent serde round-trip
    // =========================================================================

    #[test]
    fn test_step_event_serde_roundtrip_enqueue() {
        let event = StepEvent::Enqueue;
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: StepEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.event_type(), "enqueue");
    }

    #[test]
    fn test_step_event_serde_roundtrip_complete_with_results() {
        let val = json!({"key": "value"});
        let event = StepEvent::Complete(Some(val.clone()));
        let json_str = serde_json::to_string(&event).expect("serialize");
        let deserialized: StepEvent = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(deserialized.event_type(), "complete");
        assert_eq!(deserialized.results(), Some(&val));
    }

    #[test]
    fn test_step_event_serde_roundtrip_fail() {
        let event = StepEvent::Fail("round-trip error".into());
        let json_str = serde_json::to_string(&event).expect("serialize");
        let deserialized: StepEvent = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(deserialized.error_message(), Some("round-trip error"));
    }

    #[test]
    fn test_step_event_serde_roundtrip_wait_for_retry() {
        let event = StepEvent::WaitForRetry("backoff reason".into());
        let json_str = serde_json::to_string(&event).expect("serialize");
        let deserialized: StepEvent = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(deserialized.error_message(), Some("backoff reason"));
    }

    // =========================================================================
    // TaskEvent Debug trait
    // =========================================================================

    #[test]
    fn test_task_event_debug_format() {
        let event = TaskEvent::Start;
        let debug_str = format!("{:?}", event);
        assert_eq!(debug_str, "Start");
    }

    #[test]
    fn test_task_event_debug_format_with_data() {
        let event = TaskEvent::Fail("test".into());
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("Fail"));
        assert!(debug_str.contains("test"));
    }

    // =========================================================================
    // StepEvent Debug trait
    // =========================================================================

    #[test]
    fn test_step_event_debug_format() {
        let event = StepEvent::Enqueue;
        let debug_str = format!("{:?}", event);
        assert_eq!(debug_str, "Enqueue");
    }

    #[test]
    fn test_step_event_debug_format_with_data() {
        let event = StepEvent::Fail("debug test".into());
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("Fail"));
        assert!(debug_str.contains("debug test"));
    }

    // =========================================================================
    // Clone trait verification
    // =========================================================================

    #[test]
    fn test_task_event_clone() {
        let uuid = Uuid::now_v7();
        let event = TaskEvent::StepCompleted(uuid);
        let cloned = event.clone();
        assert_eq!(cloned.step_uuid(), Some(uuid));
        assert_eq!(cloned.event_type(), event.event_type());
    }

    #[test]
    fn test_step_event_clone() {
        let val = json!({"cloned": true});
        let event = StepEvent::Complete(Some(val.clone()));
        let cloned = event.clone();
        assert_eq!(cloned.results(), Some(&val));
        assert_eq!(cloned.event_type(), event.event_type());
    }
}
