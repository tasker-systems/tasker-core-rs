use async_trait::async_trait;
use sqlx::PgPool;
use crate::models::{Task, WorkflowStep};
use super::errors::{GuardError, GuardResult, dependencies_not_met, business_rule_violation};
use super::states::{TaskState, WorkflowStepState};

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
        // Query to find incomplete steps for this task
        let incomplete_count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count 
            FROM tasker_workflow_steps ws
            LEFT JOIN (
                SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state
                FROM tasker_workflow_step_transitions 
                WHERE most_recent = true
                ORDER BY workflow_step_id, sort_key DESC
            ) current_states ON current_states.workflow_step_id = ws.workflow_step_id
            WHERE ws.task_id = $1 
              AND (current_states.to_state IS NULL 
                   OR current_states.to_state NOT IN ('complete', 'resolved_manually'))
            "#,
            task.task_id
        )
        .fetch_one(pool)
        .await?;

        let incomplete = incomplete_count.count.unwrap_or(1);
        
        if incomplete > 0 {
            return Err(dependencies_not_met(format!(
                "Task {} has {} incomplete workflow steps", 
                task.task_id, 
                incomplete
            )));
        }

        Ok(true)
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
        // Query to find unsatisfied dependencies
        let unmet_dependencies = sqlx::query!(
            r#"
            SELECT COUNT(*) as count 
            FROM tasker_workflow_step_edges wse
            INNER JOIN tasker_workflow_steps parent_ws ON wse.from_step_id = parent_ws.workflow_step_id
            LEFT JOIN (
                SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state
                FROM tasker_workflow_step_transitions 
                WHERE most_recent = true
                ORDER BY workflow_step_id, sort_key DESC
            ) parent_states ON parent_states.workflow_step_id = parent_ws.workflow_step_id
            WHERE wse.to_step_id = $1
              AND (parent_states.to_state IS NULL 
                   OR parent_states.to_state NOT IN ('complete', 'resolved_manually'))
            "#,
            step.workflow_step_id
        )
        .fetch_one(pool)
        .await?;

        let unmet = unmet_dependencies.count.unwrap_or(1);
        
        if unmet > 0 {
            return Err(dependencies_not_met(format!(
                "Step {} has {} unmet dependencies", 
                step.workflow_step_id, 
                unmet
            )));
        }

        Ok(true)
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
        // Check current task state
        let current_state = resolve_current_task_state(task.task_id, pool).await?;
        
        if let Some(state) = current_state {
            if state == TaskState::InProgress {
                return Err(business_rule_violation(format!(
                    "Task {} is already in progress", 
                    task.task_id
                )));
            }
        }

        Ok(true)
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
        let current_state = resolve_current_step_state(step.workflow_step_id, pool).await?;
        
        if let Some(state) = current_state {
            if state == WorkflowStepState::InProgress {
                return Err(business_rule_violation(format!(
                    "Step {} is already in progress", 
                    step.workflow_step_id
                )));
            }
        }

        Ok(true)
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
        let current_state = resolve_current_task_state(task.task_id, pool).await?;
        
        match current_state {
            Some(TaskState::Error) => Ok(true),
            Some(state) => Err(business_rule_violation(format!(
                "Task {} cannot be reset from state {:?}, must be in Error state", 
                task.task_id, 
                state
            ))),
            None => Err(business_rule_violation(format!(
                "Task {} has no current state", 
                task.task_id
            ))),
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
        let current_state = resolve_current_step_state(step.workflow_step_id, pool).await?;
        
        match current_state {
            Some(WorkflowStepState::Error) => Ok(true),
            Some(state) => Err(business_rule_violation(format!(
                "Step {} cannot be retried from state {:?}, must be in Error state", 
                step.workflow_step_id, 
                state
            ))),
            None => Err(business_rule_violation(format!(
                "Step {} has no current state", 
                step.workflow_step_id
            ))),
        }
    }

    fn description(&self) -> &'static str {
        "Step must be in error state to be retried"
    }
}

/// Helper function to resolve current task state from transitions
pub async fn resolve_current_task_state(task_id: i64, pool: &PgPool) -> GuardResult<Option<TaskState>> {
    let row = sqlx::query!(
        r#"
        SELECT to_state 
        FROM tasker_task_transitions 
        WHERE task_id = $1 AND most_recent = true
        ORDER BY sort_key DESC 
        LIMIT 1
        "#,
        task_id
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            // to_state is character varying NOT NULL, so it should always be present
            let state = row.to_state.parse::<TaskState>().map_err(|_| {
                GuardError::InvalidState { 
                    state: format!("Invalid task state: {}", row.to_state) 
                }
            })?;
            
            Ok(Some(state))
        }
        None => Ok(None), // No transitions yet, task is in default state
    }
}

/// Helper function to resolve current step state from transitions
pub async fn resolve_current_step_state(step_id: i64, pool: &PgPool) -> GuardResult<Option<WorkflowStepState>> {
    let row = sqlx::query!(
        r#"
        SELECT to_state 
        FROM tasker_workflow_step_transitions 
        WHERE workflow_step_id = $1 AND most_recent = true
        ORDER BY sort_key DESC 
        LIMIT 1
        "#,
        step_id
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            // to_state is character varying NOT NULL, so it should always be present
            let state = row.to_state.parse::<WorkflowStepState>().map_err(|_| {
                GuardError::InvalidState { 
                    state: format!("Invalid step state: {}", row.to_state) 
                }
            })?;
            
            Ok(Some(state))
        }
        None => Ok(None), // No transitions yet, step is in default state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require a test database setup
    // For now, we're just testing the structure

    #[test]
    fn test_guard_descriptions() {
        assert_eq!(AllStepsCompleteGuard.description(), "All workflow steps must be complete");
        assert_eq!(StepDependenciesMetGuard.description(), "All step dependencies must be satisfied");
        assert_eq!(TaskNotInProgressGuard.description(), "Task must not already be in progress");
    }

    #[tokio::test]
    async fn test_state_parsing() {
        // Test that state parsing works correctly
        let valid_state = "complete".parse::<TaskState>().unwrap();
        assert_eq!(valid_state, TaskState::Complete);

        let invalid_state = "invalid_state".parse::<TaskState>();
        assert!(invalid_state.is_err());
    }
}