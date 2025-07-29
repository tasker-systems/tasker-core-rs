//! Result Aggregation Handler (Corrected Architecture)
//!
//! CORRECTED ARCHITECTURE: This handler RECEIVES result commands FROM Ruby workers
//! and delegates orchestration logic to OrchestrationResultProcessor. It implements
//! the thin transport adapter pattern where commands are received and processing
//! is delegated to existing orchestration components.

use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::execution::command::{
    Command, CommandPayload, CommandSource, CommandType, StepResult, StepSummary,
};
use crate::execution::command_router::CommandHandler;
use crate::execution::worker_pool::WorkerPool;
use crate::orchestration::{
    OrchestrationResultProcessor, StepError, StepSummary as OrchStepSummary,
};

/// Result aggregation handler that RECEIVES results FROM workers and delegates to orchestration
///
/// This implements the corrected architecture where Ruby executes and Rust orchestrates:
/// - Ruby workers send ReportPartialResult and ReportBatchCompletion commands
/// - ResultAggregationHandler receives these commands via transport layer
/// - Handler delegates to OrchestrationResultProcessor for actual orchestration logic
/// - OrchestrationResultProcessor updates StateManager and calls TaskFinalizer
pub struct ResultAggregationHandler {
    /// Worker pool for load management
    worker_pool: Arc<WorkerPool>,

    /// Orchestration result processor for handling orchestration logic
    result_processor: Arc<OrchestrationResultProcessor>,

    /// Configuration for result handling
    config: ResultHandlerConfig,
}

/// Configuration for result handling
#[derive(Debug, Clone)]
pub struct ResultHandlerConfig {
    /// Enable result tracking for monitoring
    pub enable_tracking: bool,

    /// Enable worker load management
    pub enable_load_management: bool,
}

impl Default for ResultHandlerConfig {
    fn default() -> Self {
        Self {
            enable_tracking: true,
            enable_load_management: true,
        }
    }
}

impl ResultAggregationHandler {
    /// Create a new result aggregation handler
    pub fn new(
        worker_pool: Arc<WorkerPool>,
        result_processor: Arc<OrchestrationResultProcessor>,
        config: ResultHandlerConfig,
    ) -> Self {
        Self {
            worker_pool,
            result_processor,
            config,
        }
    }
}

#[async_trait]
impl CommandHandler for ResultAggregationHandler {
    async fn handle_command(
        &self,
        command: Command,
    ) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
        match &command.payload {
            CommandPayload::ReportPartialResult {
                batch_id,
                step_id,
                result,
                execution_time_ms,
                worker_id,
            } => {
                match self
                    .handle_partial_result(
                        batch_id.clone(),
                        *step_id,
                        result.clone(),
                        *execution_time_ms,
                        worker_id.clone(),
                    )
                    .await
                {
                    Ok(response_command) => Ok(Some(response_command)),
                    Err(e) => Err(e),
                }
            }
            CommandPayload::ReportBatchCompletion {
                batch_id,
                step_summaries,
                total_execution_time_ms,
            } => {
                match self
                    .handle_batch_completion(
                        batch_id.clone(),
                        step_summaries.clone(),
                        *total_execution_time_ms,
                    )
                    .await
                {
                    Ok(response_command) => Ok(Some(response_command)),
                    Err(e) => Err(e),
                }
            }
            _ => Err(format!("Unsupported command type: {:?}", command.command_type).into()),
        }
    }

    fn handler_name(&self) -> &str {
        "ResultAggregationHandler"
    }

    fn supported_commands(&self) -> Vec<CommandType> {
        vec![
            CommandType::ReportPartialResult,
            CommandType::ReportBatchCompletion,
        ]
    }
}

impl ResultAggregationHandler {
    /// Handle partial result from worker - delegate to orchestration
    async fn handle_partial_result(
        &self,
        batch_id: String,
        step_id: i64,
        result: StepResult,
        execution_time_ms: u64,
        worker_id: String,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Received partial result for step {} in batch {} from worker {}",
            step_id, batch_id, worker_id
        );

        // Convert command types to orchestration types
        let status = match result.status {
            crate::execution::command::StepStatus::Completed => "completed".to_string(),
            crate::execution::command::StepStatus::Failed => "failed".to_string(),
            crate::execution::command::StepStatus::Timeout => "timeout".to_string(),
            crate::execution::command::StepStatus::Cancelled => "cancelled".to_string(),
        };

        let error = result.error.map(|e| StepError {
            message: e.message,
            error_type: Some(e.error_type),
            retryable: e.retryable,
        });

        // Delegate to OrchestrationResultProcessor (preserving existing orchestration logic)
        match self
            .result_processor
            .handle_partial_result(
                batch_id.clone(),
                step_id,
                status,
                result.output,
                error,
                execution_time_ms,
                worker_id.clone(),
            )
            .await
        {
            Ok(_) => {
                debug!("Successfully processed partial result for step {}", step_id);

                // Update worker load if enabled
                if self.config.enable_load_management {
                    // Decrement worker load for completed/failed steps
                    if matches!(
                        result.status,
                        crate::execution::command::StepStatus::Completed
                            | crate::execution::command::StepStatus::Failed
                    ) {
                        if let Err(e) = self.worker_pool.decrement_worker_load(&worker_id, 1).await
                        {
                            warn!("Failed to decrement worker load for {}: {}", worker_id, e);
                        }
                    }
                }

                Ok(Command::new(
                    CommandType::Success,
                    CommandPayload::Success {
                        message: format!(
                            "Partial result processed for step {} in batch {}",
                            step_id, batch_id
                        ),
                        data: None,
                    },
                    CommandSource::RustOrchestrator {
                        id: "result_aggregation_handler".to_string(),
                    },
                ))
            }
            Err(e) => {
                error!(
                    "Failed to process partial result for step {}: {}",
                    step_id, e
                );
                Ok(Command::new(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "PartialResultProcessingFailed".to_string(),
                        message: format!("Failed to process partial result: {}", e),
                        retryable: true,
                        details: None,
                    },
                    CommandSource::RustOrchestrator {
                        id: "result_aggregation_handler".to_string(),
                    },
                ))
            }
        }
    }

    /// Handle batch completion from worker - delegate to orchestration
    async fn handle_batch_completion(
        &self,
        batch_id: String,
        step_summaries: Vec<StepSummary>,
        total_execution_time_ms: u64,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Received batch completion for batch {} with {} step summaries",
            batch_id,
            step_summaries.len()
        );

        // Convert command types to orchestration types
        let orch_summaries: Vec<OrchStepSummary> = step_summaries
            .iter()
            .map(|s| OrchStepSummary {
                step_id: s.step_id,
                final_status: match s.final_status {
                    crate::execution::command::StepStatus::Completed => "completed".to_string(),
                    crate::execution::command::StepStatus::Failed => "failed".to_string(),
                    crate::execution::command::StepStatus::Timeout => "timeout".to_string(),
                    crate::execution::command::StepStatus::Cancelled => "cancelled".to_string(),
                },
                execution_time_ms: s.execution_time_ms,
                worker_id: s.worker_id.clone(),
            })
            .collect();

        // Delegate to OrchestrationResultProcessor (preserving existing orchestration logic)
        match self
            .result_processor
            .handle_batch_completion(batch_id.clone(), orch_summaries, total_execution_time_ms)
            .await
        {
            Ok(_) => {
                debug!(
                    "Successfully processed batch completion for batch {}",
                    batch_id
                );

                // Update worker loads for all workers in batch if enabled
                if self.config.enable_load_management {
                    for summary in &step_summaries {
                        // Decrement worker load for each completed step
                        if let Err(e) = self
                            .worker_pool
                            .decrement_worker_load(&summary.worker_id, 1)
                            .await
                        {
                            warn!(
                                "Failed to decrement worker load for {}: {}",
                                summary.worker_id, e
                            );
                        }
                    }
                }

                // Calculate success statistics
                let total_steps = step_summaries.len();
                let successful_steps = step_summaries
                    .iter()
                    .filter(|s| {
                        matches!(
                            s.final_status,
                            crate::execution::command::StepStatus::Completed
                        )
                    })
                    .count();
                let failed_steps = total_steps - successful_steps;

                let response_data = serde_json::json!({
                    "batch_id": batch_id,
                    "total_steps": total_steps,
                    "successful_steps": successful_steps,
                    "failed_steps": failed_steps,
                    "total_execution_time_ms": total_execution_time_ms
                });

                Ok(Command::new(
                    CommandType::Success,
                    CommandPayload::Success {
                        message: format!(
                            "Batch {} completed: {}/{} steps successful",
                            batch_id, successful_steps, total_steps
                        ),
                        data: Some(response_data),
                    },
                    CommandSource::RustOrchestrator {
                        id: "result_aggregation_handler".to_string(),
                    },
                ))
            }
            Err(e) => {
                error!(
                    "Failed to process batch completion for batch {}: {}",
                    batch_id, e
                );
                Ok(Command::new(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "BatchCompletionProcessingFailed".to_string(),
                        message: format!("Failed to process batch completion: {}", e),
                        retryable: true,
                        details: None,
                    },
                    CommandSource::RustOrchestrator {
                        id: "result_aggregation_handler".to_string(),
                    },
                ))
            }
        }
    }
}
