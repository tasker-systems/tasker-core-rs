//! # Step Service
//!
//! TAS-76: Business logic for step operations, extracted from step handlers.
//! This service handles validation and coordinates step operations.

use std::collections::HashMap;
use std::sync::Arc;

use sqlx::PgPool;
use thiserror::Error;
use tracing::{error, info};
use uuid::Uuid;

use crate::services::step_query_service::{StepQueryError, StepQueryService, StepWithReadiness};
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::messaging::execution_types::StepExecutionResult;
use tasker_shared::models::core::workflow_step::WorkflowStep;
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::state_machine::StateMachineError;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::api::orchestration::{StepAuditResponse, StepManualAction, StepResponse};

/// Errors that can occur during step service operations.
#[derive(Error, Debug)]
pub enum StepServiceError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Step not found: {0}")]
    NotFound(Uuid),

    #[error("Step does not belong to task")]
    OwnershipMismatch,

    #[error("Invalid state transition: {0}")]
    InvalidTransition(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<StepQueryError> for StepServiceError {
    fn from(err: StepQueryError) -> Self {
        match err {
            StepQueryError::NotFound(uuid) => StepServiceError::NotFound(uuid),
            StepQueryError::OwnershipMismatch { .. } => StepServiceError::OwnershipMismatch,
            StepQueryError::Database(e) => StepServiceError::Database(e.to_string()),
        }
    }
}

/// Result type for step service operations.
pub type StepServiceResult<T> = Result<T, StepServiceError>;

/// Service for step business logic.
#[derive(Clone)]
pub struct StepService {
    query_service: StepQueryService,
    write_pool: PgPool,
    system_context: Arc<SystemContext>,
}

impl std::fmt::Debug for StepService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepService")
            .field("write_pool", &"PgPool")
            .finish()
    }
}

impl StepService {
    /// Create a new step service.
    pub fn new(read_pool: PgPool, write_pool: PgPool, system_context: Arc<SystemContext>) -> Self {
        Self {
            query_service: StepQueryService::new(read_pool),
            write_pool,
            system_context,
        }
    }

    /// List all steps for a task.
    pub async fn list_task_steps(&self, task_uuid: Uuid) -> StepServiceResult<Vec<StepResponse>> {
        let steps = self.query_service.list_steps_for_task(task_uuid).await?;

        let mut responses: Vec<StepResponse> = steps
            .iter()
            .map(StepQueryService::to_step_response)
            .collect();

        // Sort by creation order
        responses.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(responses)
    }

    /// Get a single step.
    pub async fn get_step(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> StepServiceResult<StepResponse> {
        let swr = self
            .query_service
            .get_step_with_readiness(task_uuid, step_uuid)
            .await?;

        Ok(StepQueryService::to_step_response(&swr))
    }

    /// Get audit history for a step.
    pub async fn get_step_audit(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> StepServiceResult<Vec<StepAuditResponse>> {
        let audits = self
            .query_service
            .get_step_audit_history(task_uuid, step_uuid)
            .await?;

        Ok(StepQueryService::to_audit_responses(&audits))
    }

    /// Manually resolve or complete a step.
    ///
    /// Supports three action types:
    /// - `ResetForRetry`: Reset the attempt counter
    /// - `ResolveManually`: Mark as resolved without results
    /// - `CompleteManually`: Complete with execution results
    pub async fn resolve_step_manually(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        action: StepManualAction,
    ) -> StepServiceResult<StepResponse> {
        // Find the step
        let step = WorkflowStep::find_by_id(&self.write_pool, step_uuid)
            .await
            .map_err(|e| StepServiceError::Database(e.to_string()))?
            .ok_or(StepServiceError::NotFound(step_uuid))?;

        // Verify ownership
        if step.task_uuid != task_uuid {
            return Err(StepServiceError::OwnershipMismatch);
        }

        // Initialize state machine
        let mut step_state_machine =
            StepStateMachine::new(step.clone(), self.system_context.clone());

        // Build the event based on action type
        let event = self.build_step_event(&action, step_uuid)?;

        // Execute transition
        match step_state_machine.transition(event).await {
            Ok(new_state) => {
                info!(
                    step_uuid = %step_uuid,
                    new_state = %new_state,
                    "Step action completed successfully"
                );

                // Get updated step with readiness
                let sql_executor = SqlFunctionExecutor::new(self.write_pool.clone());
                let updated_step = WorkflowStep::find_by_id(&self.write_pool, step_uuid)
                    .await
                    .map_err(|e| StepServiceError::Database(e.to_string()))?
                    .ok_or(StepServiceError::NotFound(step_uuid))?;

                let readiness_statuses = sql_executor
                    .get_step_readiness_status(task_uuid, Some(vec![step_uuid]))
                    .await
                    .map_err(|e| StepServiceError::Database(e.to_string()))?;

                let swr = StepWithReadiness {
                    step: updated_step,
                    readiness: readiness_statuses.into_iter().next(),
                };

                Ok(StepQueryService::to_step_response(&swr))
            }
            Err(state_machine_error) => {
                error!(
                    error = %state_machine_error,
                    step_uuid = %step_uuid,
                    "Failed to manually resolve step"
                );

                let error_message = match state_machine_error {
                    StateMachineError::InvalidTransition { from, to } => {
                        format!(
                            "Cannot manually resolve step: invalid transition from {} to {to}",
                            from.unwrap_or("unknown".to_string())
                        )
                    }
                    StateMachineError::GuardFailed { reason } => {
                        format!("Cannot manually resolve step: {reason}")
                    }
                    StateMachineError::Database(db_error) => {
                        format!("Database error during manual resolution: {db_error}")
                    }
                    _ => format!("Manual resolution failed: {state_machine_error}"),
                };

                Err(StepServiceError::InvalidTransition(error_message))
            }
        }
    }

    /// Build a StepEvent from a StepManualAction.
    fn build_step_event(
        &self,
        action: &StepManualAction,
        step_uuid: Uuid,
    ) -> StepServiceResult<StepEvent> {
        match action {
            StepManualAction::ResetForRetry { .. } => {
                info!(step_uuid = %step_uuid, "Using ResetForRetry event");
                Ok(StepEvent::ResetForRetry)
            }
            StepManualAction::ResolveManually { .. } => {
                info!(step_uuid = %step_uuid, "Using ResolveManually event");
                Ok(StepEvent::ResolveManually)
            }
            StepManualAction::CompleteManually {
                completion_data,
                reason,
                completed_by,
            } => {
                info!(step_uuid = %step_uuid, "Using CompleteManually event with execution results");

                // Build custom metadata
                let mut custom_metadata: HashMap<String, serde_json::Value> =
                    if let Some(metadata_value) = &completion_data.metadata {
                        if let Some(metadata_obj) = metadata_value.as_object() {
                            metadata_obj
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect()
                        } else {
                            let mut map = HashMap::new();
                            map.insert("operator_metadata".to_string(), metadata_value.clone());
                            map
                        }
                    } else {
                        HashMap::new()
                    };

                // Add manual completion tracking
                custom_metadata.insert("manually_completed".to_string(), serde_json::json!(true));
                custom_metadata.insert("completed_by".to_string(), serde_json::json!(completed_by));
                custom_metadata.insert("completion_reason".to_string(), serde_json::json!(reason));

                // Construct execution result
                let execution_result = StepExecutionResult::success(
                    step_uuid,
                    completion_data.result.clone(),
                    0,
                    Some(custom_metadata),
                );

                // Serialize to JSON
                let execution_result_json =
                    serde_json::to_value(&execution_result).map_err(|e| {
                        StepServiceError::Internal(format!(
                            "Failed to serialize execution result: {}",
                            e
                        ))
                    })?;

                Ok(StepEvent::CompleteManually(Some(execution_result_json)))
            }
        }
    }
}
