use super::errors::{business_rule_violation, dependencies_not_met, GuardResult};
use super::states::{TaskState, WorkflowStepState};
use crate::models::{Task, WorkflowStep};
use async_trait::async_trait;
use sqlx::PgPool;
use std::str::FromStr;

/// Trait for implementing state transition guards
#[async_trait]
pub trait StateGuard<T> {
    /// Check if a transition is allowed
    async fn check(&self, entity: &T, pool: &PgPool) -> GuardResult<bool>;

    /// Get a description of this guard for logging
    fn description(&self) -> &'static str;
}

/// Guard to check if all workflow steps are complete before completing a task
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
                    let state = TaskState::from_str(&state_str)
                        .map_err(|e| business_rule_violation(format!("Invalid task state: {}", e)))?;
                    Err(business_rule_violation(format!(
                        "Task {} cannot be reset from state '{}', must be in Error state",
                        task.task_uuid, state
                    )))
                },
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
                    let state = WorkflowStepState::from_str(&state_str)
                        .map_err(|e| business_rule_violation(format!("Invalid workflow step state: {}", e)))?;
                    Err(business_rule_violation(format!(
                        "Step {} cannot be retried from state '{}', must be in Error state",
                        step.workflow_step_uuid, state
                    )))
                },
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

/// Guard to check if step can be enqueued for orchestration (must be in InProgress state)
pub struct StepCanBeEnqueuedForOrchestrationGuard;

#[async_trait]
impl StateGuard<WorkflowStep> for StepCanBeEnqueuedForOrchestrationGuard {
    async fn check(&self, step: &WorkflowStep, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if step.can_be_enqueued_for_orchestration(pool).await? {
            Ok(true)
        } else {
            let current_state = step.get_current_state(pool).await?;
            match current_state {
                Some(state_str) => {
                    let state = WorkflowStepState::from_str(&state_str)
                        .map_err(|e| business_rule_violation(format!("Invalid workflow step state: {}", e)))?;
                    Err(business_rule_violation(format!(
                        "Step {} cannot be enqueued for orchestration from state '{}', must be InProgress",
                        step.workflow_step_uuid, state
                    )))
                },
                None => Err(business_rule_violation(format!(
                    "Step {} has no current state",
                    step.workflow_step_uuid
                ))),
            }
        }
    }

    fn description(&self) -> &'static str {
        "Step must be in InProgress state to be enqueued for orchestration"
    }
}

/// Guard to check if step can be completed from orchestration (must be in EnqueuedForOrchestration state)
pub struct StepCanBeCompletedFromOrchestrationGuard;

#[async_trait]
impl StateGuard<WorkflowStep> for StepCanBeCompletedFromOrchestrationGuard {
    async fn check(&self, step: &WorkflowStep, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if step.can_be_completed_from_orchestration(pool).await? {
            Ok(true)
        } else {
            let current_state = step.get_current_state(pool).await?;
            match current_state {
                Some(state_str) => {
                    let state = WorkflowStepState::from_str(&state_str)
                        .map_err(|e| business_rule_violation(format!("Invalid workflow step state: {}", e)))?;
                    Err(business_rule_violation(format!(
                        "Step {} cannot be completed from orchestration from state '{}', must be EnqueuedForOrchestration",
                        step.workflow_step_uuid, state
                    )))
                },
                None => Err(business_rule_violation(format!(
                    "Step {} has no current state",
                    step.workflow_step_uuid
                ))),
            }
        }
    }

    fn description(&self) -> &'static str {
        "Step must be in EnqueuedForOrchestration state to be completed by orchestration"
    }
}

/// Guard to check if step can be failed from orchestration (must be in EnqueuedForOrchestration state)
pub struct StepCanBeFailedFromOrchestrationGuard;

#[async_trait]
impl StateGuard<WorkflowStep> for StepCanBeFailedFromOrchestrationGuard {
    async fn check(&self, step: &WorkflowStep, pool: &PgPool) -> GuardResult<bool> {
        // Delegate to the model's implementation
        if step.can_be_failed_from_orchestration(pool).await? {
            Ok(true)
        } else {
            let current_state = step.get_current_state(pool).await?;
            match current_state {
                Some(state_str) => {
                    let state = WorkflowStepState::from_str(&state_str)
                        .map_err(|e| business_rule_violation(format!("Invalid workflow step state: {}", e)))?;
                    Err(business_rule_violation(format!(
                        "Step {} cannot be failed from orchestration from state '{}', must be EnqueuedForOrchestration",
                        step.workflow_step_uuid, state
                    )))
                },
                None => Err(business_rule_violation(format!(
                    "Step {} has no current state",
                    step.workflow_step_uuid
                ))),
            }
        }
    }

    fn description(&self) -> &'static str {
        "Step must be in EnqueuedForOrchestration state to be failed by orchestration"
    }
}

// Helper functions have been removed - guards now delegate to model implementations
// The model implementations (Task::get_current_state, WorkflowStep::get_current_state)
// handle state resolution directly using the same SQL logic but with proper error handling
