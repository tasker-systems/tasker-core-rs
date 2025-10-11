//! # Step Result Processing Service
//!
//! Manages step result processing by delegating to ResultProcessorActor.
//!
//! ## Purpose
//!
//! This service encapsulates the actor delegation pattern for step result processing,
//! handling the complexity of result validation, task finalization coordination,
//! error handling, and retry logic.
//!
//! ## Responsibilities
//!
//! - Result validation and orchestration metadata processing
//! - Task finalization coordination with atomic claiming (TAS-37)
//! - Error handling, retry logic, and failure state management
//! - Backoff calculations for intelligent retry coordination

use std::sync::Arc;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{error, info};

use crate::actors::{ActorRegistry, Handler, ProcessStepResultMessage};

/// Result of step result processing
#[derive(Debug)]
pub enum StepProcessingResult {
    /// Processing succeeded
    Success { message: String },
    /// Processing failed with error
    Failed { error: String },
    /// Step was skipped
    Skipped { reason: String },
}

/// Service for managing step result processing lifecycle
///
/// This service wraps the ResultProcessorActor delegation pattern, providing
/// a clean interface for step result processing operations.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::orchestration::lifecycle_services::StepResultProcessingService;
/// use std::sync::Arc;
///
/// # async fn example(
/// #     actors: Arc<crate::actors::ActorRegistry>,
/// #     result: tasker_shared::messaging::StepExecutionResult
/// # ) -> tasker_shared::TaskerResult<()> {
/// let service = StepResultProcessingService::new(actors);
/// let processing_result = service.process_step_result(result).await?;
/// # Ok(())
/// # }
/// ```
pub struct StepResultProcessingService {
    actors: Arc<ActorRegistry>,
}

impl StepResultProcessingService {
    /// Create a new StepResultProcessingService
    ///
    /// # Arguments
    ///
    /// * `actors` - Actor registry providing access to ResultProcessorActor
    pub fn new(actors: Arc<ActorRegistry>) -> Self {
        Self { actors }
    }

    /// Process a step execution result
    ///
    /// Delegates to ResultProcessorActor which handles:
    /// - Result validation and orchestration metadata processing
    /// - Task finalization coordination with atomic claiming (TAS-37)
    /// - Error handling, retry logic, and failure state management
    /// - Backoff calculations for intelligent retry coordination
    ///
    /// # Arguments
    ///
    /// * `step_result` - Step execution result from worker
    ///
    /// # Returns
    ///
    /// Processing result indicating success or failure
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failure
    /// - `OrchestrationError`: Actor processing failure
    pub async fn process_step_result(
        &self,
        step_result: StepExecutionResult,
    ) -> TaskerResult<StepProcessingResult> {
        info!(
            step_uuid = %step_result.step_uuid,
            status = %step_result.status,
            execution_time_ms = step_result.metadata.execution_time_ms,
            has_orchestration_metadata = step_result.orchestration_metadata.is_some(),
            "STEP_PROCESSING_SERVICE: Processing step result via ResultProcessorActor"
        );

        // TAS-46 Phase 2 - ResultProcessorActor message-based processing
        let msg = ProcessStepResultMessage {
            result: step_result.clone(),
        };

        match self.actors.result_processor_actor.handle(msg).await {
            Ok(()) => {
                info!(
                    step_uuid = %step_result.step_uuid,
                    status = %step_result.status,
                    "STEP_PROCESSING_SERVICE: ResultProcessorActor processing succeeded"
                );
                Ok(StepProcessingResult::Success {
                    message: format!(
                        "Step {} result processed successfully via ResultProcessorActor - includes TAS-37 atomic finalization claiming",
                        step_result.step_uuid
                    ),
                })
            }
            Err(e) => {
                error!(
                    step_uuid = %step_result.step_uuid,
                    status = %step_result.status,
                    error = %e,
                    "STEP_PROCESSING_SERVICE: ResultProcessorActor processing failed"
                );

                match step_result.status.as_str() {
                    "failed" => {
                        // Worker reported failure - return Failed result
                        Ok(StepProcessingResult::Failed {
                            error: format!("Step result processing failed: {e}"),
                        })
                    }
                    "skipped" => {
                        // Worker reported skipped - return Skipped result
                        Ok(StepProcessingResult::Skipped {
                            reason: format!("Step result processing skipped: {e}"),
                        })
                    }
                    _ => {
                        // Unexpected error during success case processing
                        Err(TaskerError::OrchestrationError(format!(
                            "Step result processing error: {e}"
                        )))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_result_processing_service_construction() {
        // Verify the service can be constructed
        // Full integration tests require actor registry
    }
}
