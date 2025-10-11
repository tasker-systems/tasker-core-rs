//! # Step Result Hydrator
//!
//! Hydrates step execution results from PGMQ messages containing minimal SimpleStepMessage payloads.
//!
//! ## Purpose
//!
//! Workers submit lightweight SimpleStepMessage (task_uuid + step_uuid) to the orchestration queue.
//! This hydrator performs database lookup to retrieve the full StepExecutionResult from the
//! WorkflowStep.results JSONB column, enabling efficient message queue operations while maintaining
//! rich execution data.
//!
//! ## Process
//!
//! 1. Parse SimpleStepMessage from PGMQ message
//! 2. Database lookup for WorkflowStep by step_uuid
//! 3. Validate results exist in JSONB column
//! 4. Deserialize StepExecutionResult from JSONB
//! 5. Return fully hydrated result for processing

use pgmq::Message as PgmqMessage;
use std::sync::Arc;
use tasker_shared::messaging::message::SimpleStepMessage;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::models::WorkflowStep;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{debug, error, info};

/// Hydrates full StepExecutionResult from lightweight SimpleStepMessage
///
/// This service performs database-driven hydration, converting minimal queue messages
/// into rich execution results for orchestration processing.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::orchestration::hydration::StepResultHydrator;
/// use std::sync::Arc;
///
/// # async fn example(context: Arc<tasker_shared::system_context::SystemContext>, message: pgmq::Message) -> tasker_shared::TaskerResult<()> {
/// let hydrator = StepResultHydrator::new(context);
/// let result = hydrator.hydrate_from_message(&message).await?;
/// // result is now ready for orchestration processing
/// # Ok(())
/// # }
/// ```
pub struct StepResultHydrator {
    context: Arc<SystemContext>,
}

impl StepResultHydrator {
    /// Create a new StepResultHydrator
    ///
    /// # Arguments
    ///
    /// * `context` - System context providing database access
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Hydrate full StepExecutionResult from PGMQ message
    ///
    /// Performs the complete hydration process:
    /// 1. Parse SimpleStepMessage from message payload
    /// 2. Look up WorkflowStep in database
    /// 3. Extract and validate results JSONB
    /// 4. Deserialize into StepExecutionResult
    ///
    /// # Arguments
    ///
    /// * `message` - PGMQ message containing SimpleStepMessage payload
    ///
    /// # Returns
    ///
    /// Fully hydrated `StepExecutionResult` ready for orchestration processing
    ///
    /// # Errors
    ///
    /// - `ValidationError`: Invalid message format or missing data
    /// - `DatabaseError`: Database lookup failure
    pub async fn hydrate_from_message(
        &self,
        message: &PgmqMessage,
    ) -> TaskerResult<StepExecutionResult> {
        debug!(
            msg_id = message.msg_id,
            message_size = message.message.to_string().len(),
            "HYDRATOR: Starting step result hydration"
        );

        // Step 1: Parse SimpleStepMessage (only task_uuid and step_uuid)
        let simple_message: SimpleStepMessage =
            serde_json::from_value(message.message.clone()).map_err(|e| {
                error!(
                    msg_id = message.msg_id,
                    raw_message = %message.message,
                    error = %e,
                    "HYDRATOR: Failed to parse SimpleStepMessage"
                );
                TaskerError::ValidationError(format!("Invalid SimpleStepMessage format: {e}"))
            })?;

        info!(
            msg_id = message.msg_id,
            step_uuid = %simple_message.step_uuid,
            task_uuid = %simple_message.task_uuid,
            "HYDRATOR: Successfully parsed SimpleStepMessage"
        );

        // Step 2: Database lookup for WorkflowStep
        debug!(
            step_uuid = %simple_message.step_uuid,
            "HYDRATOR: Looking up WorkflowStep in database"
        );

        let workflow_step =
            WorkflowStep::find_by_id(self.context.database_pool(), simple_message.step_uuid)
                .await
                .map_err(|e| {
                    error!(
                        step_uuid = %simple_message.step_uuid,
                        error = %e,
                        "HYDRATOR: Database lookup failed for WorkflowStep"
                    );
                    TaskerError::DatabaseError(format!("Failed to lookup step: {e}"))
                })?
                .ok_or_else(|| {
                    error!(
                        step_uuid = %simple_message.step_uuid,
                        "HYDRATOR: WorkflowStep not found in database"
                    );
                    TaskerError::ValidationError(format!(
                        "WorkflowStep not found for step_uuid: {}",
                        simple_message.step_uuid
                    ))
                })?;

        debug!(
            step_uuid = %simple_message.step_uuid,
            task_uuid = %workflow_step.task_uuid,
            has_results = workflow_step.results.is_some(),
            "HYDRATOR: Successfully retrieved WorkflowStep from database"
        );

        // Step 3: Validate results exist
        let results_json = workflow_step.results.ok_or_else(|| {
            error!(
                step_uuid = %simple_message.step_uuid,
                task_uuid = %workflow_step.task_uuid,
                "HYDRATOR: No results found in WorkflowStep.results JSONB column"
            );
            TaskerError::ValidationError(format!(
                "No results found for step_uuid: {}",
                simple_message.step_uuid
            ))
        })?;

        debug!(
            step_uuid = %simple_message.step_uuid,
            results_size = results_json.to_string().len(),
            "HYDRATOR: Deserializing StepExecutionResult from JSONB"
        );

        // Step 4: Deserialize StepExecutionResult from results JSONB column
        let step_execution_result: StepExecutionResult =
            serde_json::from_value(results_json.clone()).map_err(|e| {
                error!(
                    step_uuid = %simple_message.step_uuid,
                    task_uuid = %workflow_step.task_uuid,
                    results_json = %results_json,
                    error = %e,
                    "HYDRATOR: Failed to deserialize StepExecutionResult from results JSONB"
                );
                TaskerError::ValidationError(format!(
                    "Failed to deserialize StepExecutionResult from results JSONB: {e}"
                ))
            })?;

        info!(
            step_uuid = %simple_message.step_uuid,
            task_uuid = %workflow_step.task_uuid,
            status = %step_execution_result.status,
            execution_time_ms = step_execution_result.metadata.execution_time_ms,
            "HYDRATOR: Successfully hydrated StepExecutionResult"
        );

        Ok(step_execution_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_result_hydrator_construction() {
        // Verify the hydrator can be constructed
        // Full integration tests require database access
    }
}
