//! Shared result processing logic extracted from ZmqPubSubExecutor
//!
//! This module contains the orchestration logic for handling step results and batch completion,
//! enabling reuse across different transport layers (ZeroMQ, TCP commands, etc.).
//!
//! Enhanced in Phase 5.2 to handle orchestration metadata from workers and integrate with
//! backoff calculations for intelligent retry coordination.

use serde_json::Value;
use sqlx::PgPool;

use crate::messaging::message::OrchestrationMetadata;
use crate::models::core::workflow_step::WorkflowStep;
use crate::orchestration::{
    backoff_calculator::{BackoffCalculator, BackoffContext},
    task_finalizer::TaskFinalizer,
    StateManager,
};

/// Shared orchestration result processor that handles step results and batch completion
///
/// This component extracts the orchestration logic from ZmqPubSubExecutor to enable
/// reuse across different transport layers (ZeroMQ, TCP commands, etc.).
///
/// Enhanced in Phase 5.2 with orchestration metadata processing and intelligent backoff
/// calculations based on worker feedback (HTTP headers, error context, backoff hints).
#[derive(Clone)]
pub struct OrchestrationResultProcessor {
    state_manager: StateManager,
    task_finalizer: TaskFinalizer,
    backoff_calculator: BackoffCalculator,
    pool: PgPool,
}

impl OrchestrationResultProcessor {
    /// Create a new orchestration result processor
    pub fn new(state_manager: StateManager, task_finalizer: TaskFinalizer, pool: PgPool) -> Self {
        let backoff_calculator = BackoffCalculator::with_defaults(pool.clone());
        Self {
            state_manager,
            task_finalizer,
            backoff_calculator,
            pool,
        }
    }

    /// Create a new orchestration result processor with custom backoff calculator
    pub fn with_backoff_calculator(
        state_manager: StateManager,
        task_finalizer: TaskFinalizer,
        backoff_calculator: BackoffCalculator,
        pool: PgPool,
    ) -> Self {
        Self {
            state_manager,
            task_finalizer,
            backoff_calculator,
            pool,
        }
    }

    /// NEW Phase 5.2: Handle enhanced step result with orchestration metadata
    ///
    /// This method processes step results from pgmq workers including orchestration metadata
    /// for intelligent backoff calculations and retry coordination.
    pub async fn handle_step_result_with_metadata(
        &self,
        step_id: i64,
        status: String,
        output: Option<Value>,
        error: Option<StepError>,
        execution_time_ms: u64,
        orchestration_metadata: Option<OrchestrationMetadata>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            "Processing step result with metadata - step_id: {}, status: {}, exec_time: {}ms, has_metadata: {}",
            step_id,
            status,
            execution_time_ms,
            orchestration_metadata.is_some()
        );

        // Process orchestration metadata for backoff decisions
        if let Some(metadata) = &orchestration_metadata {
            if let Err(e) = self.process_orchestration_metadata(step_id, metadata).await {
                tracing::warn!(
                    "Failed to process orchestration metadata for step {}: {}",
                    step_id,
                    e
                );
            }
        }

        // Delegate to existing state management logic
        self.handle_partial_result(
            step_id,
            status,
            output,
            error,
            execution_time_ms,
            "pgmq_worker".to_string(),
        )
        .await
    }

    /// Handle a partial result message (immediate step state update)
    ///
    /// This delegates to StateManager for step state updates and maintains audit trails.
    /// Extracted from ZmqPubSubExecutor::handle_partial_result() lines 402-418.
    pub async fn handle_partial_result(
        &self,
        step_id: i64,
        status: String,
        output: Option<Value>,
        error: Option<StepError>,
        execution_time_ms: u64,
        worker_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            "Processing partial result for step {} status={} worker={} exec_time={}ms",
            step_id,
            status,
            worker_id,
            execution_time_ms
        );

        // Delegate to StateManager for step state updates (preserving existing orchestration logic)
        let state_update_result = match status.as_str() {
            "success" => {
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
                    .handle_step_failure_with_retry(step_id, error_message)
                    .await
            }
            "in_progress" => self.state_manager.mark_step_in_progress(step_id).await,
            _ => {
                tracing::warn!("Unknown step status: {}", status);
                return Err(format!("Unknown step status: {status}").into());
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
                if matches!(status.as_str(), "success" | "failed") {
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

    /// Process orchestration metadata for backoff and retry coordination
    ///
    /// This method analyzes worker-provided metadata to make intelligent backoff decisions:
    /// - HTTP headers (Retry-After, Rate-Limit headers)
    /// - Error context for domain-specific retry logic
    /// - Explicit backoff hints from handlers
    async fn process_orchestration_metadata(
        &self,
        step_id: i64,
        metadata: &OrchestrationMetadata,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!(
            "Processing orchestration metadata for step {}: headers={}, error_context={:?}, backoff_hint={:?}",
            step_id,
            metadata.headers.len(),
            metadata.error_context,
            metadata.backoff_hint
        );

        // Create backoff context from orchestration metadata
        let mut backoff_context = BackoffContext::new();

        // Add HTTP headers (e.g., Retry-After, X-RateLimit-Reset)
        for (key, value) in &metadata.headers {
            backoff_context = backoff_context.with_header(key.clone(), value.clone());
        }

        // Add error context if present
        if let Some(error_context) = &metadata.error_context {
            backoff_context = backoff_context.with_error(error_context.clone());
        }

        // Add custom metadata
        for (key, value) in &metadata.custom {
            backoff_context = backoff_context.with_metadata(key.clone(), value.clone());
        }

        // Process explicit backoff hint if provided
        if let Some(backoff_hint) = &metadata.backoff_hint {
            match backoff_hint.backoff_type {
                crate::messaging::message::BackoffHintType::ServerRequested => {
                    // Add server-requested delay from hint to backoff context
                    backoff_context = backoff_context.with_metadata(
                        "handler_delay_seconds".to_string(),
                        serde_json::Value::Number(backoff_hint.delay_seconds.into()),
                    );
                    tracing::info!(
                        "Handler provided server-requested backoff: {}s",
                        backoff_hint.delay_seconds
                    );
                }
                crate::messaging::message::BackoffHintType::RateLimit => {
                    // Add rate limit context for exponential backoff calculation
                    backoff_context = backoff_context.with_metadata(
                        "rate_limit_detected".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    if let Some(context) = &backoff_hint.context {
                        backoff_context = backoff_context.with_error(context.clone());
                    }
                    tracing::info!("Handler detected rate limit for step {}", step_id);
                }
                crate::messaging::message::BackoffHintType::ServiceUnavailable => {
                    // Service unavailable - use longer backoff
                    backoff_context = backoff_context.with_metadata(
                        "service_unavailable".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    backoff_context = backoff_context.with_metadata(
                        "handler_delay_seconds".to_string(),
                        serde_json::Value::Number(backoff_hint.delay_seconds.into()),
                    );
                    tracing::info!("Handler reported service unavailable for step {}", step_id);
                }
                crate::messaging::message::BackoffHintType::Custom => {
                    // Custom backoff strategy
                    backoff_context = backoff_context
                        .with_metadata("custom_backoff".to_string(), serde_json::Value::Bool(true));
                    backoff_context = backoff_context.with_metadata(
                        "handler_delay_seconds".to_string(),
                        serde_json::Value::Number(backoff_hint.delay_seconds.into()),
                    );
                    if let Some(context) = &backoff_hint.context {
                        backoff_context = backoff_context.with_error(context.clone());
                    }
                    tracing::info!(
                        "Handler provided custom backoff strategy for step {}",
                        step_id
                    );
                }
            }
        }

        // Apply backoff calculation with enhanced context
        match self
            .backoff_calculator
            .calculate_and_apply_backoff(step_id, backoff_context)
            .await
        {
            Ok(backoff_result) => {
                tracing::info!(
                    "Applied {:?} backoff to step {}: delay={}s, next_retry={}",
                    backoff_result.backoff_type,
                    step_id,
                    backoff_result.delay_seconds,
                    backoff_result.next_retry_at
                );
            }
            Err(e) => {
                tracing::error!(
                    "Failed to calculate backoff for step {} with metadata: {}",
                    step_id,
                    e
                );
                return Err(e.into());
            }
        }

        Ok(())
    }
}

/// Step error information for partial results
#[derive(Debug, Clone)]
pub struct StepError {
    pub message: String,
    pub error_type: Option<String>,
    pub retryable: bool,
}
