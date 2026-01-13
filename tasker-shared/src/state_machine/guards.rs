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
