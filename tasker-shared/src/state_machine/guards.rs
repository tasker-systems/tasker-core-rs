//! # State Machine Guards
//!
//! Preconditions that must be satisfied before state transitions are allowed.
//!
//! ## Overview
//!
//! Guards are the gatekeepers of state transitions. Before any transition occurs,
//! applicable guards are checked to ensure business rules and data integrity constraints
//! are met. If a guard fails, the transition is rejected.
//!
//! ## Guard Types
//!
//! | Guard | Entity | Purpose |
//! |-------|--------|---------|
//! | `TransitionGuard` | Task | Validates state/event combinations per TAS-41 spec |
//! | `AllStepsCompleteGuard` | Task | Ensures all steps finished before task completion |
//! | `StepDependenciesMetGuard` | Step | Checks parent step completion |
//! | `TaskNotInProgressGuard` | Task | Prevents concurrent processing |
//! | `StepNotInProgressGuard` | Step | Prevents concurrent step execution |
//! | `TaskCanBeResetGuard` | Task | Validates reset from Error state only |
//! | `StepCanBeRetriedGuard` | Step | Validates retry from Error state only |
//!
//! ## Guard Trait
//!
//! Guards implement the `StateGuard<T>` trait:
//!
//! ```rust,ignore
//! #[async_trait]
//! pub trait StateGuard<T> {
//!     async fn check(&self, entity: &T, pool: &PgPool) -> GuardResult<bool>;
//!     fn description(&self) -> &'static str;
//! }
//! ```
//!
//! ## Processor Ownership
//!
//! The `TransitionGuard::check_ownership()` method enforces that only the processor
//! that claimed a task can transition it through certain states, preventing race
//! conditions in distributed orchestration (TAS-41 spec lines 290-301).

use super::errors::{business_rule_violation, dependencies_not_met, GuardResult};
use super::events::TaskEvent;
use super::states::{TaskState, WorkflowStepState};
use crate::models::{Task, WorkflowStep};
use async_trait::async_trait;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Trait for implementing state transition guards
#[async_trait]
pub trait StateGuard<T> {
    /// Check if a transition is allowed
    async fn check(&self, entity: &T, pool: &PgPool) -> GuardResult<bool>;

    /// Get a description of this guard for logging
    fn description(&self) -> &'static str;
}

/// Guard conditions for state transitions (TAS-41 specification lines 147-249)
#[derive(Debug)]
pub struct TransitionGuard;

impl TransitionGuard {
    /// Check if a transition is valid
    pub fn can_transition(
        from: TaskState,
        to: TaskState,
        event: &TaskEvent,
        _task: &Task,
    ) -> GuardResult<()> {
        use TaskEvent::*;
        use TaskState::*;

        // Terminal states cannot transition
        if from.is_terminal() {
            return Err(business_rule_violation(format!(
                "Cannot transition from terminal state {from:?}"
            )));
        }

        // Validate specific transitions
        let valid = match (from, to, event) {
            // Initial transitions
            (Pending, Initializing, Start) => true,

            // From Initializing
            (Initializing, EnqueuingSteps, ReadyStepsFound(_)) => true,
            (Initializing, TaskState::Complete, NoStepsFound) => true,
            (Initializing, WaitingForDependencies, NoDependenciesReady) => true,

            // From EnqueuingSteps
            (EnqueuingSteps, StepsInProcess, StepsEnqueued(_)) => true,
            (EnqueuingSteps, Error, EnqueueFailed(_)) => true,

            // From StepsInProcess
            (StepsInProcess, EvaluatingResults, AllStepsCompleted) => true,
            (StepsInProcess, EvaluatingResults, StepCompleted(_)) => true,
            (StepsInProcess, WaitingForRetry, StepFailed(_)) => true,

            // From EvaluatingResults
            (EvaluatingResults, TaskState::Complete, AllStepsSuccessful) => true,
            (EvaluatingResults, EnqueuingSteps, ReadyStepsFound(_)) => true,
            (EvaluatingResults, WaitingForDependencies, NoDependenciesReady) => true,
            (EvaluatingResults, BlockedByFailures, PermanentFailure(_)) => true,

            // From waiting states
            (WaitingForDependencies, EvaluatingResults, DependenciesReady) => true,
            (WaitingForRetry, EnqueuingSteps, RetryReady) => true,
            (BlockedByFailures, Error, GiveUp) => true,
            (BlockedByFailures, ResolvedManually, ManualResolution) => true,

            // Cancellation from any non-terminal state
            (from, Cancelled, Cancel) if !from.is_terminal() => true,

            // Legacy transitions for backward compatibility
            (Pending, StepsInProcess, Start) => true, // Maps old "in_progress"
            (StepsInProcess, TaskState::Complete, TaskEvent::Complete) => true,
            (StepsInProcess, Error, Fail(_)) => true,
            (_, Cancelled, Cancel) if !from.is_terminal() => true,
            (_, ResolvedManually, ResolveManually) => true,
            (Error, Pending, Reset) => true,

            // Invalid transition
            _ => false,
        };

        if valid {
            Ok(())
        } else {
            Err(business_rule_violation(format!(
                "Invalid transition from {from:?} to {to:?} with event {event:?}"
            )))
        }
    }

    /// Check processor ownership for transitions requiring it
    pub fn check_ownership(
        state: TaskState,
        task_processor: Option<Uuid>,
        requesting_processor: Uuid,
    ) -> GuardResult<()> {
        if !state.requires_ownership() {
            return Ok(());
        }

        match task_processor {
            Some(owner) if owner == requesting_processor => Ok(()),
            Some(owner) => Err(business_rule_violation(format!(
                "Processor {requesting_processor} does not own task in state {state:?} (owned by {owner})"
            ))),
            None => Ok(()), // No owner yet, can claim
        }
    }
}

/// Guard to check if all workflow steps are complete before completing a task
#[derive(Debug)]
pub struct AllStepsCompleteGuard;

#[async_trait]
impl StateGuard<Task> for AllStepsCompleteGuard {
    async fn check(&self, task: &Task, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if task.all_workflow_steps_complete(pool).await? {
            Ok(true)
        } else {
            let incomplete_count = task.count_incomplete_workflow_steps(pool).await?;
            Err(dependencies_not_met(format!(
                "Task {} has {} incomplete workflow steps",
                task.task_uuid, incomplete_count
            )))
        }
    }

    fn description(&self) -> &'static str {
        "All workflow steps must be complete"
    }
}

/// Guard to check if step dependencies are satisfied before starting a step
#[derive(Debug)]
pub struct StepDependenciesMetGuard;

#[async_trait]
impl StateGuard<WorkflowStep> for StepDependenciesMetGuard {
    async fn check(&self, step: &WorkflowStep, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if step.dependencies_met(pool).await? {
            Ok(true)
        } else {
            let unmet_count = step.count_unmet_dependencies(pool).await?;
            Err(dependencies_not_met(format!(
                "Step {} has {} unmet dependencies",
                step.workflow_step_uuid, unmet_count
            )))
        }
    }

    fn description(&self) -> &'static str {
        "All step dependencies must be satisfied"
    }
}

/// Guard to check if task is not already in progress by another process
#[derive(Debug)]
pub struct TaskNotInProgressGuard;

#[async_trait]
impl StateGuard<Task> for TaskNotInProgressGuard {
    async fn check(&self, task: &Task, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if task.not_in_progress(pool).await? {
            Ok(true)
        } else {
            Err(business_rule_violation(format!(
                "Task {} is already in progress",
                task.task_uuid
            )))
        }
    }

    fn description(&self) -> &'static str {
        "Task must not already be in progress"
    }
}

/// Guard to check if step is not already in progress
#[derive(Debug)]
pub struct StepNotInProgressGuard;

#[async_trait]
impl StateGuard<WorkflowStep> for StepNotInProgressGuard {
    async fn check(&self, step: &WorkflowStep, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if step.not_in_progress(pool).await? {
            Ok(true)
        } else {
            Err(business_rule_violation(format!(
                "Step {} is already in progress",
                step.workflow_step_uuid
            )))
        }
    }

    fn description(&self) -> &'static str {
        "Step must not already be in progress"
    }
}

/// Guard to check if task can be reset (must be in error state)
#[derive(Debug)]
pub struct TaskCanBeResetGuard;

#[async_trait]
impl StateGuard<Task> for TaskCanBeResetGuard {
    async fn check(&self, task: &Task, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if task.can_be_reset(pool).await? {
            Ok(true)
        } else {
            let current_state = task.get_current_state(pool).await?;
            match current_state {
                Some(state_str) => {
                    let state = TaskState::from_str(&state_str).map_err(|e| {
                        business_rule_violation(format!("Invalid task state: {}", e))
                    })?;
                    Err(business_rule_violation(format!(
                        "Task {} cannot be reset from state '{}', must be in Error state",
                        task.task_uuid, state
                    )))
                }
                None => Err(business_rule_violation(format!(
                    "Task {} has no current state",
                    task.task_uuid
                ))),
            }
        }
    }

    fn description(&self) -> &'static str {
        "Task must be in error state to be reset"
    }
}

/// Guard to check if step can be retried (must be in error state)
#[derive(Debug)]
pub struct StepCanBeRetriedGuard;

#[async_trait]
impl StateGuard<WorkflowStep> for StepCanBeRetriedGuard {
    async fn check(&self, step: &WorkflowStep, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if step.can_be_retried(pool).await? {
            Ok(true)
        } else {
            let current_state = step.get_current_state(pool).await?;
            match current_state {
                Some(state_str) => {
                    let state = WorkflowStepState::from_str(&state_str).map_err(|e| {
                        business_rule_violation(format!("Invalid workflow step state: {}", e))
                    })?;
                    Err(business_rule_violation(format!(
                        "Step {} cannot be retried from state '{}', must be in Error state",
                        step.workflow_step_uuid, state
                    )))
                }
                None => Err(business_rule_violation(format!(
                    "Step {} has no current state",
                    step.workflow_step_uuid
                ))),
            }
        }
    }

    fn description(&self) -> &'static str {
        "Step must be in error state to be retried"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    /// Create a dummy Task for testing TransitionGuard::can_transition.
    /// The task parameter is unused in can_transition (prefixed with `_`),
    /// so the field values are irrelevant beyond satisfying the type system.
    fn dummy_task() -> Task {
        let now = NaiveDateTime::default();
        Task {
            task_uuid: Uuid::nil(),
            named_task_uuid: Uuid::nil(),
            complete: false,
            requested_at: now,
            initiator: None,
            source_system: None,
            reason: None,
            tags: None,
            context: None,
            identity_hash: String::new(),
            priority: 0,
            created_at: now,
            updated_at: now,
            correlation_id: Uuid::nil(),
            parent_correlation_id: None,
        }
    }

    // =========================================================================
    // TransitionGuard::can_transition() — valid transitions
    // =========================================================================

    #[test]
    fn test_can_transition_pending_to_initializing_on_start() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::Initializing,
            &TaskEvent::Start,
            &task,
        );
        assert!(
            result.is_ok(),
            "Pending -> Initializing on Start should be valid"
        );
    }

    #[test]
    fn test_can_transition_initializing_to_enqueuing_steps_on_ready_steps_found() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Initializing,
            TaskState::EnqueuingSteps,
            &TaskEvent::ReadyStepsFound(5),
            &task,
        );
        assert!(
            result.is_ok(),
            "Initializing -> EnqueuingSteps on ReadyStepsFound should be valid"
        );
    }

    #[test]
    fn test_can_transition_initializing_to_complete_on_no_steps_found() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Initializing,
            TaskState::Complete,
            &TaskEvent::NoStepsFound,
            &task,
        );
        assert!(
            result.is_ok(),
            "Initializing -> Complete on NoStepsFound should be valid"
        );
    }

    #[test]
    fn test_can_transition_initializing_to_waiting_for_dependencies_on_no_dependencies_ready() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Initializing,
            TaskState::WaitingForDependencies,
            &TaskEvent::NoDependenciesReady,
            &task,
        );
        assert!(
            result.is_ok(),
            "Initializing -> WaitingForDependencies on NoDependenciesReady should be valid"
        );
    }

    #[test]
    fn test_can_transition_enqueuing_steps_to_steps_in_process_on_steps_enqueued() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::EnqueuingSteps,
            TaskState::StepsInProcess,
            &TaskEvent::StepsEnqueued(vec![]),
            &task,
        );
        assert!(
            result.is_ok(),
            "EnqueuingSteps -> StepsInProcess on StepsEnqueued should be valid"
        );
    }

    #[test]
    fn test_can_transition_enqueuing_steps_to_error_on_enqueue_failed() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::EnqueuingSteps,
            TaskState::Error,
            &TaskEvent::EnqueueFailed("err".into()),
            &task,
        );
        assert!(
            result.is_ok(),
            "EnqueuingSteps -> Error on EnqueueFailed should be valid"
        );
    }

    #[test]
    fn test_can_transition_steps_in_process_to_evaluating_on_all_steps_completed() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::EvaluatingResults,
            &TaskEvent::AllStepsCompleted,
            &task,
        );
        assert!(
            result.is_ok(),
            "StepsInProcess -> EvaluatingResults on AllStepsCompleted should be valid"
        );
    }

    #[test]
    fn test_can_transition_steps_in_process_to_evaluating_on_step_completed() {
        let task = dummy_task();
        let step_uuid = Uuid::now_v7();
        let result = TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::EvaluatingResults,
            &TaskEvent::StepCompleted(step_uuid),
            &task,
        );
        assert!(
            result.is_ok(),
            "StepsInProcess -> EvaluatingResults on StepCompleted should be valid"
        );
    }

    #[test]
    fn test_can_transition_steps_in_process_to_waiting_for_retry_on_step_failed() {
        let task = dummy_task();
        let step_uuid = Uuid::now_v7();
        let result = TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::WaitingForRetry,
            &TaskEvent::StepFailed(step_uuid),
            &task,
        );
        assert!(
            result.is_ok(),
            "StepsInProcess -> WaitingForRetry on StepFailed should be valid"
        );
    }

    #[test]
    fn test_can_transition_evaluating_results_to_complete_on_all_steps_successful() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::EvaluatingResults,
            TaskState::Complete,
            &TaskEvent::AllStepsSuccessful,
            &task,
        );
        assert!(
            result.is_ok(),
            "EvaluatingResults -> Complete on AllStepsSuccessful should be valid"
        );
    }

    #[test]
    fn test_can_transition_evaluating_results_to_enqueuing_steps_on_ready_steps_found() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::EvaluatingResults,
            TaskState::EnqueuingSteps,
            &TaskEvent::ReadyStepsFound(3),
            &task,
        );
        assert!(
            result.is_ok(),
            "EvaluatingResults -> EnqueuingSteps on ReadyStepsFound should be valid"
        );
    }

    #[test]
    fn test_can_transition_evaluating_results_to_waiting_for_dependencies_on_no_deps_ready() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::EvaluatingResults,
            TaskState::WaitingForDependencies,
            &TaskEvent::NoDependenciesReady,
            &task,
        );
        assert!(
            result.is_ok(),
            "EvaluatingResults -> WaitingForDependencies on NoDependenciesReady should be valid"
        );
    }

    #[test]
    fn test_can_transition_evaluating_results_to_blocked_on_permanent_failure() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::EvaluatingResults,
            TaskState::BlockedByFailures,
            &TaskEvent::PermanentFailure("err".into()),
            &task,
        );
        assert!(
            result.is_ok(),
            "EvaluatingResults -> BlockedByFailures on PermanentFailure should be valid"
        );
    }

    #[test]
    fn test_can_transition_waiting_for_dependencies_to_evaluating_on_dependencies_ready() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::WaitingForDependencies,
            TaskState::EvaluatingResults,
            &TaskEvent::DependenciesReady,
            &task,
        );
        assert!(
            result.is_ok(),
            "WaitingForDependencies -> EvaluatingResults on DependenciesReady should be valid"
        );
    }

    #[test]
    fn test_can_transition_waiting_for_retry_to_enqueuing_steps_on_retry_ready() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::WaitingForRetry,
            TaskState::EnqueuingSteps,
            &TaskEvent::RetryReady,
            &task,
        );
        assert!(
            result.is_ok(),
            "WaitingForRetry -> EnqueuingSteps on RetryReady should be valid"
        );
    }

    #[test]
    fn test_can_transition_blocked_by_failures_to_error_on_give_up() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::BlockedByFailures,
            TaskState::Error,
            &TaskEvent::GiveUp,
            &task,
        );
        assert!(
            result.is_ok(),
            "BlockedByFailures -> Error on GiveUp should be valid"
        );
    }

    #[test]
    fn test_can_transition_blocked_by_failures_to_resolved_manually_on_manual_resolution() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::BlockedByFailures,
            TaskState::ResolvedManually,
            &TaskEvent::ManualResolution,
            &task,
        );
        assert!(
            result.is_ok(),
            "BlockedByFailures -> ResolvedManually on ManualResolution should be valid"
        );
    }

    // =========================================================================
    // Legacy transitions
    // =========================================================================

    #[test]
    fn test_can_transition_legacy_pending_to_steps_in_process_on_start() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::StepsInProcess,
            &TaskEvent::Start,
            &task,
        );
        assert!(
            result.is_ok(),
            "Legacy: Pending -> StepsInProcess on Start should be valid"
        );
    }

    #[test]
    fn test_can_transition_legacy_steps_in_process_to_complete_on_complete() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::Complete,
            &TaskEvent::Complete,
            &task,
        );
        assert!(
            result.is_ok(),
            "Legacy: StepsInProcess -> Complete on Complete should be valid"
        );
    }

    #[test]
    fn test_can_transition_legacy_steps_in_process_to_error_on_fail() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::Error,
            &TaskEvent::Fail("err".into()),
            &task,
        );
        assert!(
            result.is_ok(),
            "Legacy: StepsInProcess -> Error on Fail should be valid"
        );
    }

    // =========================================================================
    // Cancellation transition
    // =========================================================================

    #[test]
    fn test_can_transition_steps_in_process_to_cancelled_on_cancel() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task,
        );
        assert!(
            result.is_ok(),
            "StepsInProcess -> Cancelled on Cancel should be valid"
        );
    }

    #[test]
    fn test_can_transition_pending_to_cancelled_on_cancel() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task,
        );
        assert!(
            result.is_ok(),
            "Pending -> Cancelled on Cancel should be valid"
        );
    }

    // =========================================================================
    // Error -> Pending reset and ResolveManually
    // =========================================================================

    #[test]
    fn test_can_transition_error_to_pending_on_reset_rejected_because_error_is_terminal() {
        // Error is a terminal state, so the early terminal-state check rejects
        // this transition even though (Error, Pending, Reset) exists in the match arms.
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Error,
            TaskState::Pending,
            &TaskEvent::Reset,
            &task,
        );
        assert!(
            result.is_err(),
            "Error is terminal, so Error -> Pending on Reset is rejected by the terminal guard"
        );
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("terminal state"),
            "Error should mention terminal state, got: {msg}"
        );
    }

    #[test]
    fn test_can_transition_error_to_resolved_manually_rejected_because_error_is_terminal() {
        // Error is a terminal state, so (_, ResolvedManually, ResolveManually) is
        // unreachable from Error due to the early terminal-state guard.
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Error,
            TaskState::ResolvedManually,
            &TaskEvent::ResolveManually,
            &task,
        );
        assert!(
            result.is_err(),
            "Error is terminal, so Error -> ResolvedManually on ResolveManually is rejected"
        );
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("terminal state"),
            "Error should mention terminal state, got: {msg}"
        );
    }

    // =========================================================================
    // TransitionGuard::can_transition() — invalid transitions
    // =========================================================================

    #[test]
    fn test_can_transition_rejects_from_terminal_state_complete() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Complete,
            TaskState::Pending,
            &TaskEvent::Start,
            &task,
        );
        assert!(
            result.is_err(),
            "Transition from terminal state Complete should be rejected"
        );
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("terminal state"),
            "Error should mention terminal state, got: {msg}"
        );
    }

    #[test]
    fn test_can_transition_rejects_from_terminal_state_cancelled() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Cancelled,
            TaskState::Pending,
            &TaskEvent::Start,
            &task,
        );
        assert!(
            result.is_err(),
            "Transition from terminal state Cancelled should be rejected"
        );
    }

    #[test]
    fn test_can_transition_rejects_from_terminal_state_resolved_manually() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::ResolvedManually,
            TaskState::Pending,
            &TaskEvent::Start,
            &task,
        );
        assert!(
            result.is_err(),
            "Transition from terminal state ResolvedManually should be rejected"
        );
    }

    #[test]
    fn test_can_transition_rejects_invalid_pending_to_complete_on_start() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::Complete,
            &TaskEvent::Start,
            &task,
        );
        assert!(
            result.is_err(),
            "Pending -> Complete on Start should be invalid"
        );
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Invalid transition"),
            "Error should mention invalid transition, got: {msg}"
        );
    }

    #[test]
    fn test_can_transition_rejects_invalid_pending_to_error_on_start() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::Error,
            &TaskEvent::Start,
            &task,
        );
        assert!(
            result.is_err(),
            "Pending -> Error on Start should be invalid"
        );
    }

    #[test]
    fn test_can_transition_rejects_wrong_event_for_valid_from_to() {
        let task = dummy_task();
        // Pending -> Initializing is valid with Start, but not with RetryReady
        let result = TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::Initializing,
            &TaskEvent::RetryReady,
            &task,
        );
        assert!(
            result.is_err(),
            "Pending -> Initializing on RetryReady should be invalid"
        );
    }

    // =========================================================================
    // TransitionGuard::check_ownership()
    // =========================================================================

    #[test]
    fn test_check_ownership_non_ownership_state_returns_ok() {
        // Pending does not require ownership (TAS-54: no state requires ownership)
        let requesting = Uuid::now_v7();
        let result = TransitionGuard::check_ownership(TaskState::Pending, None, requesting);
        assert!(
            result.is_ok(),
            "Non-ownership state should return Ok regardless of processor"
        );
    }

    #[test]
    fn test_check_ownership_matching_processor_returns_ok() {
        let processor = Uuid::now_v7();
        // With TAS-54, all states return false for requires_ownership, so this always passes.
        // We test with an active state to ensure it still passes.
        let result =
            TransitionGuard::check_ownership(TaskState::Initializing, Some(processor), processor);
        assert!(result.is_ok(), "Matching processor should return Ok");
    }

    #[test]
    fn test_check_ownership_mismatched_processor_still_ok_after_tas54() {
        // TAS-54 removed ownership enforcement; mismatched processor should still be Ok
        let owner = Uuid::now_v7();
        let different_processor = Uuid::now_v7();
        let result = TransitionGuard::check_ownership(
            TaskState::Initializing,
            Some(owner),
            different_processor,
        );
        assert!(
            result.is_ok(),
            "After TAS-54, mismatched processor should return Ok (ownership not enforced)"
        );
    }

    #[test]
    fn test_check_ownership_no_owner_returns_ok() {
        let requesting = Uuid::now_v7();
        let result =
            TransitionGuard::check_ownership(TaskState::EvaluatingResults, None, requesting);
        assert!(result.is_ok(), "No owner yet should return Ok");
    }

    #[test]
    fn test_check_ownership_terminal_state_returns_ok() {
        let requesting = Uuid::now_v7();
        let result = TransitionGuard::check_ownership(TaskState::Complete, None, requesting);
        assert!(
            result.is_ok(),
            "Terminal state does not require ownership and should return Ok"
        );
    }

    // =========================================================================
    // Guard description() methods
    // =========================================================================

    #[test]
    fn test_all_steps_complete_guard_description_non_empty() {
        let guard = AllStepsCompleteGuard;
        let desc = guard.description();
        assert!(
            !desc.is_empty(),
            "AllStepsCompleteGuard description should be non-empty"
        );
        assert!(
            desc.contains("complete"),
            "Description should mention completion, got: {desc}"
        );
    }

    #[test]
    fn test_step_dependencies_met_guard_description_non_empty() {
        let guard = StepDependenciesMetGuard;
        let desc = guard.description();
        assert!(
            !desc.is_empty(),
            "StepDependenciesMetGuard description should be non-empty"
        );
        assert!(
            desc.contains("dependencies"),
            "Description should mention dependencies, got: {desc}"
        );
    }

    #[test]
    fn test_task_not_in_progress_guard_description_non_empty() {
        let guard = TaskNotInProgressGuard;
        let desc = guard.description();
        assert!(
            !desc.is_empty(),
            "TaskNotInProgressGuard description should be non-empty"
        );
        assert!(
            desc.contains("progress"),
            "Description should mention progress, got: {desc}"
        );
    }

    #[test]
    fn test_step_not_in_progress_guard_description_non_empty() {
        let guard = StepNotInProgressGuard;
        let desc = guard.description();
        assert!(
            !desc.is_empty(),
            "StepNotInProgressGuard description should be non-empty"
        );
        assert!(
            desc.contains("progress"),
            "Description should mention progress, got: {desc}"
        );
    }

    #[test]
    fn test_task_can_be_reset_guard_description_non_empty() {
        let guard = TaskCanBeResetGuard;
        let desc = guard.description();
        assert!(
            !desc.is_empty(),
            "TaskCanBeResetGuard description should be non-empty"
        );
        assert!(
            desc.contains("error"),
            "Description should mention error state, got: {desc}"
        );
    }

    #[test]
    fn test_step_can_be_retried_guard_description_non_empty() {
        let guard = StepCanBeRetriedGuard;
        let desc = guard.description();
        assert!(
            !desc.is_empty(),
            "StepCanBeRetriedGuard description should be non-empty"
        );
        assert!(
            desc.contains("error"),
            "Description should mention error state, got: {desc}"
        );
    }

    // =========================================================================
    // TransitionGuard Debug trait
    // =========================================================================

    #[test]
    fn test_transition_guard_debug_format() {
        let guard = TransitionGuard;
        let debug_str = format!("{:?}", guard);
        assert_eq!(debug_str, "TransitionGuard");
    }

    // =========================================================================
    // Additional guard struct Debug traits
    // =========================================================================

    #[test]
    fn test_all_steps_complete_guard_debug() {
        let debug_str = format!("{:?}", AllStepsCompleteGuard);
        assert_eq!(debug_str, "AllStepsCompleteGuard");
    }

    #[test]
    fn test_step_dependencies_met_guard_debug() {
        let debug_str = format!("{:?}", StepDependenciesMetGuard);
        assert_eq!(debug_str, "StepDependenciesMetGuard");
    }

    #[test]
    fn test_task_not_in_progress_guard_debug() {
        let debug_str = format!("{:?}", TaskNotInProgressGuard);
        assert_eq!(debug_str, "TaskNotInProgressGuard");
    }

    #[test]
    fn test_step_not_in_progress_guard_debug() {
        let debug_str = format!("{:?}", StepNotInProgressGuard);
        assert_eq!(debug_str, "StepNotInProgressGuard");
    }

    #[test]
    fn test_task_can_be_reset_guard_debug() {
        let debug_str = format!("{:?}", TaskCanBeResetGuard);
        assert_eq!(debug_str, "TaskCanBeResetGuard");
    }

    #[test]
    fn test_step_can_be_retried_guard_debug() {
        let debug_str = format!("{:?}", StepCanBeRetriedGuard);
        assert_eq!(debug_str, "StepCanBeRetriedGuard");
    }

    // =========================================================================
    // Additional can_transition edge cases
    // =========================================================================

    #[test]
    fn test_can_transition_cancellation_from_initializing() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::Initializing,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task,
        );
        assert!(
            result.is_ok(),
            "Initializing -> Cancelled on Cancel should be valid (any non-terminal)"
        );
    }

    #[test]
    fn test_can_transition_cancellation_from_enqueuing_steps() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::EnqueuingSteps,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task,
        );
        assert!(
            result.is_ok(),
            "EnqueuingSteps -> Cancelled on Cancel should be valid"
        );
    }

    #[test]
    fn test_can_transition_cancellation_from_evaluating_results() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::EvaluatingResults,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task,
        );
        assert!(
            result.is_ok(),
            "EvaluatingResults -> Cancelled on Cancel should be valid"
        );
    }

    #[test]
    fn test_can_transition_cancellation_from_waiting_for_dependencies() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::WaitingForDependencies,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task,
        );
        assert!(
            result.is_ok(),
            "WaitingForDependencies -> Cancelled on Cancel should be valid"
        );
    }

    #[test]
    fn test_can_transition_cancellation_from_waiting_for_retry() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::WaitingForRetry,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task,
        );
        assert!(
            result.is_ok(),
            "WaitingForRetry -> Cancelled on Cancel should be valid"
        );
    }

    #[test]
    fn test_can_transition_cancellation_from_blocked_by_failures() {
        let task = dummy_task();
        let result = TransitionGuard::can_transition(
            TaskState::BlockedByFailures,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task,
        );
        assert!(
            result.is_ok(),
            "BlockedByFailures -> Cancelled on Cancel should be valid"
        );
    }

    #[test]
    fn test_can_transition_resolve_manually_from_non_terminal() {
        let task = dummy_task();
        // ResolveManually wildcard: (_, ResolvedManually, ResolveManually)
        let result = TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::ResolvedManually,
            &TaskEvent::ResolveManually,
            &task,
        );
        assert!(
            result.is_ok(),
            "StepsInProcess -> ResolvedManually on ResolveManually should be valid"
        );
    }
}
