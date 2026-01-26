//! # Step Query Service
//!
//! TAS-76: Database queries for step operations, extracted from step handlers.
//! This service handles all database interactions for workflow steps.

use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use tasker_shared::database::sql_functions::{SqlFunctionExecutor, StepReadinessStatus};
use tasker_shared::models::core::workflow_step::WorkflowStep;
use tasker_shared::models::core::workflow_step_result_audit::{
    StepAuditWithTransition, WorkflowStepResultAudit,
};
use tasker_shared::types::api::orchestration::{StepAuditResponse, StepResponse};

/// Errors that can occur during step query operations.
#[derive(Error, Debug)]
pub enum StepQueryError {
    #[error("Step not found: {0}")]
    NotFound(Uuid),

    #[error("Step does not belong to task: step {step_uuid} not in task {task_uuid}")]
    OwnershipMismatch { step_uuid: Uuid, task_uuid: Uuid },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Result type for step query operations.
pub type StepQueryResult<T> = Result<T, StepQueryError>;

/// Step with readiness context for API responses.
#[derive(Debug, Clone)]
pub struct StepWithReadiness {
    pub step: WorkflowStep,
    pub readiness: Option<StepReadinessStatus>,
}

/// Service for step-related database queries.
#[derive(Debug, Clone)]
pub struct StepQueryService {
    pool: PgPool,
}

impl StepQueryService {
    /// Create a new step query service.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List all steps for a task with readiness status.
    pub async fn list_steps_for_task(
        &self,
        task_uuid: Uuid,
    ) -> StepQueryResult<Vec<StepWithReadiness>> {
        let sql_executor = SqlFunctionExecutor::new(self.pool.clone());

        // Get workflow steps
        let workflow_steps = WorkflowStep::for_task(&self.pool, task_uuid).await?;

        if workflow_steps.is_empty() {
            return Ok(vec![]);
        }

        // Get readiness status for all steps
        let readiness_statuses = sql_executor
            .get_step_readiness_status(task_uuid, None)
            .await?;

        // Combine steps with their readiness status
        let steps_with_readiness: Vec<StepWithReadiness> = workflow_steps
            .into_iter()
            .map(|step| {
                let readiness = readiness_statuses
                    .iter()
                    .find(|r| r.workflow_step_uuid == step.workflow_step_uuid)
                    .cloned();

                StepWithReadiness { step, readiness }
            })
            .collect();

        Ok(steps_with_readiness)
    }

    /// Get a single step with readiness status.
    pub async fn get_step_with_readiness(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> StepQueryResult<StepWithReadiness> {
        let sql_executor = SqlFunctionExecutor::new(self.pool.clone());

        // Find the step
        let step = WorkflowStep::find_by_id(&self.pool, step_uuid)
            .await?
            .ok_or(StepQueryError::NotFound(step_uuid))?;

        // Verify ownership
        if step.task_uuid != task_uuid {
            return Err(StepQueryError::OwnershipMismatch {
                step_uuid,
                task_uuid,
            });
        }

        // Get readiness status
        let readiness_statuses = sql_executor
            .get_step_readiness_status(task_uuid, Some(vec![step_uuid]))
            .await?;

        let readiness = readiness_statuses.into_iter().next();

        Ok(StepWithReadiness { step, readiness })
    }

    /// Get audit history for a step.
    pub async fn get_step_audit_history(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> StepQueryResult<Vec<StepAuditWithTransition>> {
        // Verify step exists and belongs to task
        let step = WorkflowStep::find_by_id(&self.pool, step_uuid)
            .await?
            .ok_or(StepQueryError::NotFound(step_uuid))?;

        if step.task_uuid != task_uuid {
            return Err(StepQueryError::OwnershipMismatch {
                step_uuid,
                task_uuid,
            });
        }

        // Get audit history
        let audit_records =
            WorkflowStepResultAudit::get_audit_history(&self.pool, step_uuid).await?;

        Ok(audit_records)
    }

    /// Convert a StepWithReadiness to a StepResponse.
    pub fn to_step_response(swr: &StepWithReadiness) -> StepResponse {
        let step = &swr.step;

        if let Some(readiness) = &swr.readiness {
            StepResponse {
                step_uuid: step.workflow_step_uuid.to_string(),
                task_uuid: step.task_uuid.to_string(),
                name: readiness.name.clone(),
                created_at: step.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                updated_at: step.updated_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                completed_at: step
                    .processed_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                results: step.results.clone(),
                current_state: readiness.current_state.clone(),
                dependencies_satisfied: readiness.dependencies_satisfied,
                retry_eligible: readiness.retry_eligible,
                ready_for_execution: readiness.ready_for_execution,
                total_parents: readiness.total_parents,
                completed_parents: readiness.completed_parents,
                attempts: readiness.attempts,
                max_attempts: readiness.max_attempts,
                last_failure_at: readiness
                    .last_failure_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                next_retry_at: readiness
                    .next_retry_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                last_attempted_at: readiness
                    .last_attempted_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            }
        } else {
            // Fallback when readiness data is not available
            StepResponse {
                step_uuid: step.workflow_step_uuid.to_string(),
                task_uuid: step.task_uuid.to_string(),
                name: format!("step_{}", step.workflow_step_uuid),
                created_at: step.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                updated_at: step.updated_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
                completed_at: step
                    .processed_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
                results: step.results.clone(),
                current_state: "unknown".to_string(),
                dependencies_satisfied: false,
                retry_eligible: false,
                ready_for_execution: false,
                total_parents: 0,
                completed_parents: 0,
                attempts: step.attempts.unwrap_or(0),
                max_attempts: step.max_attempts.unwrap_or(3),
                last_failure_at: None,
                next_retry_at: None,
                last_attempted_at: step
                    .last_attempted_at
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            }
        }
    }

    /// Convert audit records to API response format.
    pub fn to_audit_responses(audits: &[StepAuditWithTransition]) -> Vec<StepAuditResponse> {
        audits
            .iter()
            .map(StepAuditResponse::from_audit_with_transition)
            .collect()
    }
}
