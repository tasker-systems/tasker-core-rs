//! Shared result processing logic extracted from ZmqPubSubExecutor
//!
//! This module contains the orchestration logic for handling step results and batch completion,
//! enabling reuse across different transport layers (ZeroMQ, TCP commands, etc.).

use serde_json::Value;
use sqlx::PgPool;

use crate::execution::message_protocols::ResultMessage;
use crate::models::core::{step_execution_batch::StepExecutionBatch, workflow_step::WorkflowStep};
use crate::orchestration::{task_finalizer::TaskFinalizer, StateManager};

/// Shared orchestration result processor that handles step results and batch completion
///
/// This component extracts the orchestration logic from ZmqPubSubExecutor to enable
/// reuse across different transport layers (ZeroMQ, TCP commands, etc.).
pub struct OrchestrationResultProcessor {
    state_manager: StateManager,
    task_finalizer: TaskFinalizer,
    pool: PgPool,
}

impl OrchestrationResultProcessor {
    /// Create a new orchestration result processor
    pub fn new(state_manager: StateManager, task_finalizer: TaskFinalizer, pool: PgPool) -> Self {
        Self {
            state_manager,
            task_finalizer,
            pool,
        }
    }

    /// Handle a partial result message (immediate step state update)
    ///
    /// This delegates to StateManager for step state updates and maintains audit trails.
    /// Extracted from ZmqPubSubExecutor::handle_partial_result() lines 402-418.
    pub async fn handle_partial_result(
        &self,
        batch_id: String,
        step_id: i64,
        status: String,
        output: Option<Value>,
        error: Option<StepError>,
        execution_time_ms: u64,
        worker_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            "Processing partial result for step {} in batch {}: status={} worker={} exec_time={}ms",
            step_id,
            batch_id,
            status,
            worker_id,
            execution_time_ms
        );

        // Delegate to StateManager for step state updates (preserving existing orchestration logic)
        let state_update_result = match status.as_str() {
            "completed" => {
                self.state_manager
                    .complete_step_with_results(step_id, output.clone())
                    .await
            }
            "failed" => {
                let error_message = error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "Step execution failed".to_string());
                self.state_manager
                    .fail_step_with_error(step_id, error_message)
                    .await
            }
            "in_progress" => self.state_manager.mark_step_in_progress(step_id).await,
            _ => {
                tracing::warn!("Unknown step status: {}", status);
                return Err(format!("Unknown step status: {}", status).into());
            }
        };

        // Handle state update result and trigger finalization check if needed
        match state_update_result {
            Ok(_) => {
                tracing::debug!(
                    "Successfully updated step {} to status '{}'",
                    step_id,
                    status
                );

                // If step completed or failed, check if task should be finalized
                if matches!(status.as_str(), "completed" | "failed") {
                    if let Ok(Some(workflow_step)) =
                        WorkflowStep::find_by_id(&self.pool, step_id).await
                    {
                        match self
                            .task_finalizer
                            .handle_no_viable_steps(workflow_step.task_id)
                            .await
                        {
                            Ok(finalization_result) => {
                                tracing::info!(
                                    "Task {} finalization result: action={:?}, reason={:?}",
                                    workflow_step.task_id,
                                    finalization_result.action,
                                    finalization_result.reason
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Task finalization check failed for task {}: {}",
                                    workflow_step.task_id,
                                    e
                                );
                            }
                        }
                    } else {
                        tracing::error!(
                            "Failed to lookup WorkflowStep for step {} during finalization check",
                            step_id
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to update step {} state to '{}': {}",
                    step_id,
                    status,
                    e
                );
                return Err(e.into());
            }
        }

        Ok(())
    }

    /// Handle batch completion message (reconciliation check and task finalization)
    ///
    /// This delegates to TaskFinalizer for task completion logic and maintains audit trails.
    /// Extracted from ZmqPubSubExecutor::handle_batch_completion() lines 575+.
    pub async fn handle_batch_completion(
        &self,
        batch_id: String,
        step_summaries: Vec<StepSummary>,
        total_execution_time_ms: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            "Processing batch completion for batch {} with {} step summaries",
            batch_id,
            step_summaries.len()
        );

        // Get task_id from batch and check for task finalization
        if let Ok(Some(step_execution_batch)) =
            StepExecutionBatch::find_by_uuid(&self.pool, &batch_id).await
        {
            let task_id = step_execution_batch.task_id;

            tracing::info!(
                "Checking if batch {} completion for task {} triggers task finalization",
                batch_id,
                task_id
            );

            // Delegate to TaskFinalizer for task completion logic (preserving existing orchestration)
            match self.task_finalizer.handle_no_viable_steps(task_id).await {
                Ok(finalization_result) => {
                    tracing::info!(
                        "Task {} finalization result: action={:?}, reason={:?}",
                        task_id,
                        finalization_result.action,
                        finalization_result.reason
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Task finalization failed for task {} after batch {} completion: {}",
                        task_id,
                        batch_id,
                        e
                    );
                    return Err(e.into());
                }
            }

            // TODO: Perform batch reconciliation logic
            tracing::debug!(
                "TODO: Should perform reconciliation between step_summaries and tracked partial results for batch {}",
                batch_id
            );

            tracing::info!("Batch {} marked as complete for task {}", batch_id, task_id);
            Ok(())
        } else {
            let error_msg = format!(
                "Failed to find StepExecutionBatch for batch_id: {}",
                batch_id
            );
            tracing::error!("{}", error_msg);
            Err(error_msg.into())
        }
    }
}

/// Step error information for partial results
#[derive(Debug, Clone)]
pub struct StepError {
    pub message: String,
    pub error_type: Option<String>,
    pub retryable: bool,
}

/// Step summary for batch completion
#[derive(Debug, Clone)]
pub struct StepSummary {
    pub step_id: i64,
    pub final_status: String,
    pub execution_time_ms: u64,
    pub worker_id: String,
}
