//! TAS-32: Orchestration Coordination Logic (No State Management)
//!
//! **ARCHITECTURE CHANGE**: This module has been updated for TAS-32 queue state management
//! improvements where Ruby workers handle all step execution, result saving, and state transitions.
//!
//! Rust orchestration now focuses solely on:
//! - Task-level coordination and finalization
//! - Processing orchestration metadata from workers
//! - Intelligent backoff calculations for retry coordination
//! - Triggering task completion when all steps are done
//!
//! **What Rust orchestration NO LONGER does**:
//! - Step state transitions (handled by Ruby workers with Statesman)
//! - Saving step results to database (handled by Ruby MessageManager)
//! - Creating workflow step transitions (handled by Ruby state machines)
//!
//! This enables autonomous Ruby workers with database-driven coordination.

use std::sync::Arc;
use tasker_shared::errors::OrchestrationResult;
use tasker_shared::system_context::SystemContext;
use uuid::Uuid;

use crate::orchestration::{
    backoff_calculator::{BackoffCalculator, BackoffContext},
    lifecycle::task_finalizer::TaskFinalizer,
    task_claim::finalization_claimer::FinalizationClaimer,
    BackoffCalculatorConfig,
};
use tasker_shared::messaging::message::OrchestrationMetadata;
use tasker_shared::models::core::workflow_step::WorkflowStep;

use tracing::{debug, error, info, trace, warn};

/// TAS-32: Orchestration coordination processor (coordination only, no state management)
///
/// **ARCHITECTURE CHANGE**: This component now handles only task-level coordination
/// and orchestration metadata processing. Ruby workers manage all step-level operations.
///
/// **Coordination Responsibilities**:
/// - Task finalization when steps complete
/// - Processing backoff metadata from workers
/// - Orchestration-level retry timing decisions
/// - Cross-cutting orchestration concerns
///
/// **No Longer Handles**:
/// - Step state transitions (Ruby workers + Statesman)
/// - Step result persistence (Ruby MessageManager)
/// - Workflow step transition creation (Ruby state machines)
#[derive(Clone)]
pub struct OrchestrationResultProcessor {
    task_finalizer: TaskFinalizer,
    backoff_calculator: BackoffCalculator,
    finalization_claimer: FinalizationClaimer,
    context: Arc<SystemContext>,
    processor_id: String,
}

impl OrchestrationResultProcessor {
    /// Create a new orchestration result processor
    pub fn new(task_finalizer: TaskFinalizer, context: Arc<SystemContext>) -> Self {
        let backoff_config =
            BackoffCalculatorConfig::from_config_manager(context.config_manager.clone());
        let backoff_calculator =
            BackoffCalculator::new(backoff_config, context.database_pool().clone());
        let processor_id = FinalizationClaimer::generate_processor_id("orchestration");
        let finalization_claimer = FinalizationClaimer::new(context.clone(), processor_id.clone());

        Self {
            task_finalizer,
            backoff_calculator,
            finalization_claimer,
            context,
            processor_id,
        }
    }

    /// TAS-32: Handle step result notification with orchestration metadata (coordination only)
    ///
    /// TAS-32 ARCHITECTURE CHANGE: This method now only processes orchestration metadata
    /// for backoff calculations and task-level coordination. Ruby workers handle all
    /// step state transitions and result saving.
    ///
    /// The Rust orchestration focuses on:
    /// - Processing backoff metadata from workers
    /// - Task-level finalization coordination
    /// - Orchestration-level retry decisions
    pub async fn handle_step_result_with_metadata(
        &self,
        step_uuid: Uuid,
        status: String,
        execution_time_ms: u64,
        orchestration_metadata: Option<OrchestrationMetadata>,
    ) -> OrchestrationResult<()> {
        tracing::info!(
            step_uuid = %step_uuid,
            status = %status,
            execution_time_ms = execution_time_ms,
            has_orchestration_metadata = orchestration_metadata.is_some(),
            "🔍 ORCHESTRATION_RESULT_PROCESSOR: Starting step result notification processing"
        );

        // Process orchestration metadata for backoff decisions (coordinating retry timing)
        if let Some(metadata) = &orchestration_metadata {
            debug!(
                step_uuid = %step_uuid,
                headers_count = metadata.headers.len(),
                has_error_context = metadata.error_context.is_some(),
                has_backoff_hint = metadata.backoff_hint.is_some(),
                custom_fields_count = metadata.custom.len(),
                "🔍 ORCHESTRATION_RESULT_PROCESSOR: Processing orchestration metadata"
            );

            if let Err(e) = self
                .process_orchestration_metadata(step_uuid, metadata)
                .await
            {
                error!(
                    step_uuid = %step_uuid,
                    error = %e,
                    "❌ ORCHESTRATION_RESULT_PROCESSOR: Failed to process orchestration metadata"
                );
            } else {
                debug!(
                    step_uuid = %step_uuid,
                    "✅ ORCHESTRATION_RESULT_PROCESSOR: Successfully processed orchestration metadata"
                );
            }
        } else {
            debug!(
                step_uuid = %step_uuid,
                "🔍 ORCHESTRATION_RESULT_PROCESSOR: No orchestration metadata to process"
            );
        }

        // Delegate to coordination-only result handling (no state updates)
        debug!(
            step_uuid = %step_uuid,
            status = %status,
            "🔍 ORCHESTRATION_RESULT_PROCESSOR: Delegating to handle_step_result for coordination-only processing"
        );

        match self
            .handle_step_result(step_uuid, status.clone(), execution_time_ms)
            .await
        {
            Ok(()) => {
                info!(
                    step_uuid = %step_uuid,
                    status = %status,
                    "✅ ORCHESTRATION_RESULT_PROCESSOR: Step result notification processing completed successfully"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    step_uuid = %step_uuid,
                    status = %status,
                    error = %e,
                    "❌ ORCHESTRATION_RESULT_PROCESSOR: Step result notification processing failed"
                );
                Err(e)
            }
        }
    }

    /// Handle a partial result message (TAS-32: coordination only, no state updates)
    ///
    /// TAS-32 ARCHITECTURE CHANGE: Ruby workers now handle all step execution, result saving,
    /// and state transitions. Rust orchestration focuses only on task-level coordination.
    ///
    /// This method now only handles task finalization checks when steps complete,
    /// since Ruby workers manage step state transitions directly.
    pub async fn handle_step_result(
        &self,
        step_uuid: Uuid,
        status: String,
        execution_time_ms: u64,
    ) -> OrchestrationResult<()> {
        info!(
            step_uuid = %step_uuid,
            status = %status,
            execution_time_ms = execution_time_ms,
            processor_id = %self.processor_id,
            "🔍 TASK_COORDINATION: Processing step result notification for task coordination"
        );

        debug!(
            step_uuid = %step_uuid,
            status = %status,
            "🔍 TASK_COORDINATION: Status qualifies for finalization check - looking up WorkflowStep"
        );

        match WorkflowStep::find_by_id(self.context.database_pool(), step_uuid).await {
            Ok(Some(workflow_step)) => {
                info!(
                    step_uuid = %step_uuid,
                    task_uuid = %workflow_step.task_uuid,
                    "🔍 TASK_COORDINATION: Found WorkflowStep - attempting finalization claim"
                );

                // Try to claim the task for finalization
                match self
                    .finalization_claimer
                    .claim_task(workflow_step.task_uuid)
                    .await
                {
                    Ok(claim_result) => {
                        if claim_result.claimed {
                            // We got the claim - proceed with finalization
                            info!(
                                task_uuid = %workflow_step.task_uuid,
                                processor_id = %self.processor_id,
                                step_uuid = %step_uuid,
                                "✅ TASK_COORDINATION: Successfully claimed task for finalization"
                            );

                            // Perform finalization with the claim
                            debug!(
                                task_uuid = %workflow_step.task_uuid,
                                step_uuid = %step_uuid,
                                "🔍 TASK_COORDINATION: Calling TaskFinalizer.finalize_task"
                            );

                            match self
                                .task_finalizer
                                .finalize_task(workflow_step.task_uuid, false)
                                .await
                            {
                                Ok(result) => {
                                    info!(
                                        task_uuid = %workflow_step.task_uuid,
                                        step_uuid = %step_uuid,
                                        action = ?result.action,
                                        reason = ?result.reason,
                                        "✅ TASK_COORDINATION: Task finalization completed successfully"
                                    );

                                    // Release the claim after successful finalization
                                    if let Err(e) = self
                                        .finalization_claimer
                                        .release_claim(workflow_step.task_uuid)
                                        .await
                                    {
                                        warn!(
                                            task_uuid = %workflow_step.task_uuid,
                                            step_uuid = %step_uuid,
                                            error = %e,
                                            "⚠️ TASK_COORDINATION: Failed to release finalization claim after successful finalization"
                                        );
                                    } else {
                                        debug!(
                                            task_uuid = %workflow_step.task_uuid,
                                            step_uuid = %step_uuid,
                                            "🔍 TASK_COORDINATION: Successfully released finalization claim"
                                        );
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        task_uuid = %workflow_step.task_uuid,
                                        step_uuid = %step_uuid,
                                        error = %e,
                                        "❌ TASK_COORDINATION: Task finalization failed"
                                    );
                                    // Release claim on error
                                    if let Err(release_err) = self
                                        .finalization_claimer
                                        .release_claim(workflow_step.task_uuid)
                                        .await
                                    {
                                        warn!(
                                            task_uuid = %workflow_step.task_uuid,
                                            step_uuid = %step_uuid,
                                            release_error = %release_err,
                                            "⚠️ TASK_COORDINATION: Failed to release finalization claim after finalization error"
                                        );
                                    } else {
                                        debug!(
                                            task_uuid = %workflow_step.task_uuid,
                                            step_uuid = %step_uuid,
                                            "🔍 TASK_COORDINATION: Released finalization claim after finalization error"
                                        );
                                    }
                                    return Err(e.into());
                                }
                            };
                        } else {
                            // Another processor is handling or will handle finalization
                            debug!(
                                task_uuid = %workflow_step.task_uuid,
                                step_uuid = %step_uuid,
                                already_claimed_by = ?claim_result.already_claimed_by,
                                reason = ?claim_result.message,
                                "🔍 TASK_COORDINATION: Task finalization not needed or already claimed by another processor"
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            task_uuid = %workflow_step.task_uuid,
                            step_uuid = %step_uuid,
                            error = %e,
                            "❌ TASK_COORDINATION: Failed to attempt finalization claim"
                        );
                    }
                }
            }
            Ok(None) => {
                error!(
                    step_uuid = %step_uuid,
                    "❌ TASK_COORDINATION: WorkflowStep not found for step UUID during finalization check"
                );
            }
            Err(e) => {
                error!(
                    step_uuid = %step_uuid,
                    error = %e,
                    "❌ TASK_COORDINATION: Failed to lookup WorkflowStep during finalization check"
                );
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
        step_uuid: Uuid,
        metadata: &OrchestrationMetadata,
    ) -> OrchestrationResult<()> {
        tracing::debug!(
            "Processing orchestration metadata for step {}: headers={}, error_context={:?}, backoff_hint={:?}",
            step_uuid,
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
                tasker_shared::messaging::message::BackoffHintType::ServerRequested => {
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
                tasker_shared::messaging::message::BackoffHintType::RateLimit => {
                    // Add rate limit context for exponential backoff calculation
                    backoff_context = backoff_context.with_metadata(
                        "rate_limit_detected".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    if let Some(context) = &backoff_hint.context {
                        backoff_context = backoff_context.with_error(context.clone());
                    }
                    tracing::info!("Handler detected rate limit for step {}", step_uuid);
                }
                tasker_shared::messaging::message::BackoffHintType::ServiceUnavailable => {
                    // Service unavailable - use longer backoff
                    backoff_context = backoff_context.with_metadata(
                        "service_unavailable".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    backoff_context = backoff_context.with_metadata(
                        "handler_delay_seconds".to_string(),
                        serde_json::Value::Number(backoff_hint.delay_seconds.into()),
                    );
                    tracing::info!(
                        "Handler reported service unavailable for step {}",
                        step_uuid
                    );
                }
                tasker_shared::messaging::message::BackoffHintType::Custom => {
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
                        step_uuid
                    );
                }
            }
        }

        // Apply backoff calculation with enhanced context
        match self
            .backoff_calculator
            .calculate_and_apply_backoff(step_uuid, backoff_context)
            .await
        {
            Ok(backoff_result) => {
                tracing::info!(
                    "Applied {:?} backoff to step {}: delay={}s, next_retry={}",
                    backoff_result.backoff_type,
                    step_uuid,
                    backoff_result.delay_seconds,
                    backoff_result.next_retry_at
                );
            }
            Err(e) => {
                tracing::error!(
                    "Failed to calculate backoff for step {} with metadata: {}",
                    step_uuid,
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
