//! # CheckpointAndFailHandler - Batch Resumption Testing (TAS-64)
//!
//! A batch worker handler that processes items with checkpointing and fails at configurable point.
//! Used for E2E testing of cursor-based resumption after retry.
//!
//! ## Configuration (YAML handler.initialization)
//!
//! ```yaml
//! handler:
//!   callable: CheckpointAndFailHandler
//!   initialization:
//!     fail_after_items: 50      # Fail after processing this many items
//!     fail_on_attempt: 1        # Only fail on this attempt (1-indexed)
//!     checkpoint_interval: 25   # Update checkpoint every N items
//! ```
//!
//! ## Behavior
//!
//! 1. Reads cursor config from `workflow_step.inputs` (via BatchWorkerContext)
//! 2. Checks for existing checkpoint in `workflow_step.results.checkpoint_progress`
//! 3. Resumes from checkpoint if present (proves resumability)
//! 4. Processes items until reaching fail_after_items on fail_on_attempt
//! 5. On retry (attempt > fail_on_attempt), completes successfully
//!
//! ## Testing Pattern
//!
//! - First attempt: Processes 50 items, checkpoints at 25, fails at 50
//! - Second attempt: Resumes from checkpoint (25), completes remaining items
//! - Result shows `resumed_from: 25` proving no duplicate processing

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;

use crate::step_handlers::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tasker_worker::batch_processing::BatchWorkerContext;

/// Batch worker handler that checkpoints progress and fails at configurable point
#[derive(Debug)]
pub struct CheckpointAndFailHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for CheckpointAndFailHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get configuration from handler.initialization
        let fail_after_items = step_data
            .step_definition
            .handler
            .initialization
            .get("fail_after_items")
            .and_then(|v| v.as_u64())
            .unwrap_or(50);

        let fail_on_attempt = step_data
            .step_definition
            .handler
            .initialization
            .get("fail_on_attempt")
            .and_then(|v| v.as_i64())
            .unwrap_or(1) as i32;

        let checkpoint_interval = step_data
            .step_definition
            .handler
            .initialization
            .get("checkpoint_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(25);

        // Current attempt (1-indexed)
        let current_attempt = step_data.workflow_step.attempts.unwrap_or(1);

        // Extract batch worker context from inputs
        let context = BatchWorkerContext::from_step_data(step_data)?;

        // Check for no-op worker
        if context.is_no_op() {
            tracing::info!(
                step_uuid = %step_uuid,
                batch_id = %context.batch_id(),
                "CheckpointAndFailHandler: No-op worker, returning success"
            );
            return Ok(success_result(
                step_uuid,
                json!({
                    "no_op": true,
                    "reason": "NoBatches scenario"
                }),
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        // Check for existing checkpoint in results (preserved by ResetForRetry)
        let checkpoint_progress = step_data
            .workflow_step
            .results
            .as_ref()
            .and_then(|r| r.get("checkpoint_progress"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // Resume from checkpoint or start from beginning
        let resume_from = if checkpoint_progress > 0 {
            checkpoint_progress
        } else {
            context.start_position()
        };

        tracing::info!(
            step_uuid = %step_uuid,
            batch_id = %context.batch_id(),
            start_position = context.start_position(),
            end_position = context.end_position(),
            checkpoint_progress = checkpoint_progress,
            resume_from = resume_from,
            current_attempt = current_attempt,
            fail_after_items = fail_after_items,
            fail_on_attempt = fail_on_attempt,
            "CheckpointAndFailHandler executing"
        );

        // Process items
        let mut current_position = resume_from;
        let mut processed_count = 0u64;
        let mut last_checkpoint = resume_from;

        while current_position < context.end_position() {
            // Simulate processing an item
            current_position += 1;
            processed_count += 1;

            // Checkpoint at intervals
            if processed_count.is_multiple_of(checkpoint_interval) {
                last_checkpoint = current_position;
                tracing::debug!(
                    step_uuid = %step_uuid,
                    checkpoint_progress = current_position,
                    processed_count = processed_count,
                    "Checkpoint saved"
                );
            }

            // Check if we should fail (only on specific attempt)
            let total_processed = (resume_from - context.start_position()) + processed_count;
            if total_processed >= fail_after_items && current_attempt == fail_on_attempt {
                tracing::warn!(
                    step_uuid = %step_uuid,
                    attempt = current_attempt,
                    processed_count = processed_count,
                    checkpoint_progress = last_checkpoint,
                    "CheckpointAndFailHandler: Simulating failure at checkpoint"
                );

                let elapsed_ms = start_time.elapsed().as_millis() as i64;

                // Return error with checkpoint preserved in results
                // The results field is preserved by ResetForRetry
                return Ok(error_result(
                    step_uuid,
                    format!(
                        "Simulated failure after {} items on attempt {}",
                        total_processed, current_attempt
                    ),
                    Some("SIMULATED_BATCH_FAILURE".to_string()),
                    Some("RetryableError".to_string()),
                    true, // retryable
                    elapsed_ms,
                    Some(HashMap::from([
                        ("checkpoint_progress".to_string(), json!(last_checkpoint)),
                        (
                            "processed_before_failure".to_string(),
                            json!(processed_count),
                        ),
                        ("resumed_from".to_string(), json!(resume_from)),
                    ])),
                ));
            }
        }

        // Success - processed all items
        let elapsed_ms = start_time.elapsed().as_millis() as i64;

        tracing::info!(
            step_uuid = %step_uuid,
            total_processed = processed_count,
            resumed_from = resume_from,
            final_position = current_position,
            attempts = current_attempt,
            "CheckpointAndFailHandler: Completed successfully"
        );

        Ok(success_result(
            step_uuid,
            json!({
                "total_processed": processed_count,
                "resumed_from": resume_from,
                "final_position": current_position,
                "start_position": context.start_position(),
                "end_position": context.end_position(),
                "attempts": current_attempt,
                "message": "Batch processing completed successfully"
            }),
            elapsed_ms,
            Some(HashMap::from([
                ("checkpoint_progress".to_string(), json!(current_position)),
                ("total_processed".to_string(), json!(processed_count)),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "CheckpointAndFailHandler"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_name() {
        let config = StepHandlerConfig::empty();
        let handler = CheckpointAndFailHandler::new(config);
        assert_eq!(handler.name(), "CheckpointAndFailHandler");
    }
}
