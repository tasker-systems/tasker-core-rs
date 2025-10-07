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

use std::str::FromStr;
use std::sync::Arc;
use tasker_shared::messaging::StepExecutionStatus;
use tasker_shared::state_machine::states::WorkflowStepState;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{errors::OrchestrationResult, OrchestrationError};
use uuid::Uuid;

use crate::orchestration::{
    backoff_calculator::{BackoffCalculator, BackoffContext},
    lifecycle::task_finalizer::TaskFinalizer,
    BackoffCalculatorConfig,
};
use tasker_shared::messaging::{
    message::OrchestrationMetadata, StepExecutionResult, StepResultMessage,
};
use tasker_shared::models::core::workflow_step::WorkflowStep;
use tasker_shared::state_machine::{TaskEvent, TaskState, TaskStateMachine};

use tracing::{debug, error, info, warn};

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
    context: Arc<SystemContext>,
}

impl OrchestrationResultProcessor {
    /// Create a new orchestration result processor
    pub fn new(task_finalizer: TaskFinalizer, context: Arc<SystemContext>) -> Self {
        let backoff_config: BackoffCalculatorConfig = context.tasker_config.clone().into();
        let backoff_calculator =
            BackoffCalculator::new(backoff_config, context.database_pool().clone());

        Self {
            task_finalizer,
            backoff_calculator,
            context,
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
        step_result: StepResultMessage,
    ) -> OrchestrationResult<()> {
        let step_uuid = &step_result.step_uuid;
        let status = &step_result.status;
        let execution_time_ms = &step_result.execution_time_ms;
        let correlation_id = &step_result.correlation_id;
        let orchestration_metadata = &step_result.orchestration_metadata;

        info!(
            step_uuid = %step_uuid,
            correlation_id = %correlation_id,
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

            match step_result.status {
                StepExecutionStatus::Failed => {
                    self.process_orchestration_metadata(step_uuid, metadata)
                        .await?;
                }
                StepExecutionStatus::Timeout => {
                    self.process_orchestration_metadata(step_uuid, metadata)
                        .await?;
                }
                _ => {}
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
            .handle_step_result(
                &step_result.step_uuid,
                &step_result.status.clone().into(),
                &(step_result.execution_time_ms as i64),
            )
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

    pub async fn handle_step_execution_result(
        &self,
        step_result: &StepExecutionResult,
    ) -> OrchestrationResult<()> {
        let step_uuid = &step_result.step_uuid;
        let status = &step_result.status;
        let execution_time_ms = &step_result.metadata.execution_time_ms;
        let orchestration_metadata = &step_result.orchestration_metadata;

        info!(
            step_uuid = %step_uuid,
            status = %status,
            execution_time_ms = execution_time_ms,
            processor_uuid = %self.context.processor_uuid(),
            "✅ ORCHESTRATION_RESULT_PROCESSOR: Step execution result notification processing completed successfully"
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
            .handle_step_result(
                &step_result.step_uuid,
                &step_result.status,
                &step_result.metadata.execution_time_ms,
            )
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
        step_uuid: &Uuid,
        status: &String,
        execution_time_ms: &i64,
    ) -> OrchestrationResult<()> {
        info!(
            step_uuid = %step_uuid,
            status = %status,
            execution_time_ms = execution_time_ms,
            processor_uuid = %self.context.processor_uuid(),
            "🔍 ORCHESTRATION_PROCESSING: Processing step result for orchestration and task coordination"
        );

        // TAS-41: First handle orchestration state transitions if needed
        if let Err(e) = self
            .process_orchestration_state_transition(step_uuid, status)
            .await
        {
            error!(
                step_uuid = %step_uuid,
                error = %e,
                "❌ ORCHESTRATION_PROCESSING: Failed to process orchestration state transition"
            );
            return Err(e);
        }

        debug!(
            step_uuid = %step_uuid,
            status = %status,
            "🔍 TASK_COORDINATION: Status qualifies for finalization check - looking up WorkflowStep"
        );

        let workflow_step =
            WorkflowStep::find_by_id(self.context.database_pool(), *step_uuid).await?;

        match workflow_step {
            Some(workflow_step) => {
                // Create state machine for this task
                let mut task_state_machine = TaskStateMachine::for_task(
                    workflow_step.task_uuid,
                    self.context.database_pool().clone(),
                    self.context.processor_uuid(),
                )
                .await?;

                // Check the current state of the task
                let current_state = task_state_machine.current_state().await?;

                debug!(
                    task_uuid = %workflow_step.task_uuid,
                    current_state = ?current_state,
                    step_uuid = %step_uuid,
                    "🔍 TASK_COORDINATION: Current task state before attempting transition"
                );

                // Only transition with StepCompleted if we're in StepsInProcess state
                // If we're already in EvaluatingResults, we need to check what the next transition should be
                let should_finalize = if current_state == TaskState::StepsInProcess {
                    // We can transition with StepCompleted
                    task_state_machine
                        .transition(TaskEvent::StepCompleted(*step_uuid))
                        .await?
                } else if current_state == TaskState::EvaluatingResults {
                    // We're already evaluating results, need to check if we should finalize
                    // This happens when multiple steps complete in quick succession
                    debug!(
                        task_uuid = %workflow_step.task_uuid,
                        "Task already in EvaluatingResults state, checking if finalization is needed"
                    );
                    true // Check if finalization is needed
                } else {
                    debug!(
                        task_uuid = %workflow_step.task_uuid,
                        current_state = ?current_state,
                        "Task in state that doesn't require immediate finalization check"
                    );
                    false
                };

                if should_finalize {
                    match self
                        .task_finalizer
                        .finalize_task(workflow_step.task_uuid)
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
                        }
                        Err(err) => {
                            error!(
                                task_uuid = %workflow_step.task_uuid,
                                step_uuid = %step_uuid,
                                "❌ TASK_COORDINATION: Failed to finalize task"
                            );
                            return Err(OrchestrationError::DatabaseError {
                                operation: format!("TaskFinalizer.finalize_task for {step_uuid}"),
                                reason: format!("Failed to finalize task: {err}"),
                            });
                        }
                    }
                } else {
                    error!(
                        task_uuid = %workflow_step.task_uuid,
                        step_uuid = %step_uuid,
                        "❌ TASK_COORDINATION: Failed to transition state machine"
                    );
                    return Err(OrchestrationError::DatabaseError {
                        operation: format!("TaskStateMachine.transition for {step_uuid}"),
                        reason: "Failed to transition state machine".to_string(),
                    });
                }
            }
            None => {
                error!(
                    step_uuid = %step_uuid,
                    "❌ TASK_COORDINATION: Failed to find WorkflowStep"
                );
                return Err(OrchestrationError::DatabaseError {
                    operation: format!("WorkflowStep.find for {step_uuid}"),
                    reason: format!("Failed to find WorkflowStep for step UUID: {step_uuid}"),
                });
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
        step_uuid: &Uuid,
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

    /// TAS-41: Process orchestration state transitions for EnqueuedForOrchestration steps
    ///
    /// This method handles the transition of steps from EnqueuedForOrchestration state
    /// to their final states (Complete or Error) after orchestration metadata processing.
    /// This is critical for fixing the race condition where workers bypass orchestration.
    async fn process_orchestration_state_transition(
        &self,
        step_uuid: &Uuid,
        original_status: &String,
    ) -> OrchestrationResult<()> {
        // Load the current step to check its state
        let step = WorkflowStep::find_by_id(self.context.database_pool(), *step_uuid)
            .await
            .map_err(
                |e| tasker_shared::errors::OrchestrationError::DatabaseError {
                    operation: "load_step".to_string(),
                    reason: format!("Failed to load step {}: {}", step_uuid, e),
                },
            )?;

        let Some(step) = step else {
            warn!(
                step_uuid = %step_uuid,
                "Step not found - may have been processed by another processor"
            );
            return Ok(());
        };

        // Get the current state using the step's state machine
        let current_state = step
            .get_current_state(self.context.database_pool())
            .await
            .map_err(
                |e| tasker_shared::errors::OrchestrationError::DatabaseError {
                    operation: "get_current_state".to_string(),
                    reason: format!("Failed to get current state for step {}: {}", step_uuid, e),
                },
            )?;

        // Only process if step is in EnqueuedForOrchestration state
        if let Some(state_str) = current_state {
            let step_state = WorkflowStepState::from_str(&state_str).map_err(|e| {
                tasker_shared::errors::OrchestrationError::from(format!(
                    "Invalid workflow step state: {}",
                    e
                ))
            })?;

            if matches!(
                step_state,
                WorkflowStepState::EnqueuedForOrchestration
                    | WorkflowStepState::EnqueuedAsErrorForOrchestration
            ) {
                info!(
                    step_uuid = %step_uuid,
                    original_status = %original_status,
                    step_state = %step_state,
                    "Processing orchestration state transition for step in notification state"
                );

                // Create state machine for the step
                use tasker_shared::state_machine::StepStateMachine;
                let mut state_machine = StepStateMachine::new(step.clone(), self.context.clone());

                // Determine the final state based on step notification state and execution result
                use tasker_shared::state_machine::events::StepEvent;
                let final_event = match step_state {
                    WorkflowStepState::EnqueuedForOrchestration => {
                        // Success pathway - deserialize StepExecutionResult to determine final state
                        if let Some(results_json) = &step.results {
                            match serde_json::from_value::<StepExecutionResult>(
                                results_json.clone(),
                            ) {
                                Ok(step_execution_result) => {
                                    if step_execution_result.success {
                                        StepEvent::Complete(step.results.clone())
                                    } else {
                                        // Handle case where success path contains failure
                                        let error_message = step_execution_result
                                            .error
                                            .map(|e| e.message)
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        StepEvent::Fail(format!("Step failed: {}", error_message))
                                    }
                                }
                                Err(_) => {
                                    // Fallback to original status parsing for backward compatibility
                                    if original_status.to_lowercase().contains("success")
                                        || original_status.to_lowercase() == "complete"
                                        || original_status.to_lowercase() == "completed"
                                    {
                                        StepEvent::Complete(step.results.clone())
                                    } else {
                                        StepEvent::Fail(format!(
                                            "Step failed with status: {}",
                                            original_status
                                        ))
                                    }
                                }
                            }
                        } else {
                            // No results available - use status
                            if original_status.to_lowercase().contains("success")
                                || original_status.to_lowercase() == "complete"
                                || original_status.to_lowercase() == "completed"
                            {
                                StepEvent::Complete(None)
                            } else {
                                StepEvent::Fail(format!(
                                    "Step failed with status: {}",
                                    original_status
                                ))
                            }
                        }
                    }
                    WorkflowStepState::EnqueuedAsErrorForOrchestration => {
                        // Error pathway - determine if step should retry or move to permanent error
                        // Check retryability from step results metadata
                        let should_retry = if let Some(results_json) = &step.results {
                            match serde_json::from_value::<StepExecutionResult>(
                                results_json.clone(),
                            ) {
                                Ok(step_execution_result) => {
                                    // Check if error is marked as non-retryable in metadata
                                    // metadata.retryable is set by the Ruby worker's error classifier
                                    let retryable_from_metadata =
                                        step_execution_result.metadata.retryable;

                                    if !retryable_from_metadata {
                                        info!(
                                            step_uuid = %step_uuid,
                                            "Error marked as non-retryable by worker"
                                        );
                                        false
                                    } else {
                                        // Check retry limits from template
                                        let max_attempts = step.max_attempts.unwrap_or(0);
                                        let current_attempts = step.attempts.unwrap_or(0);

                                        if current_attempts >= max_attempts {
                                            info!(
                                                step_uuid = %step_uuid,
                                                current_attempts = current_attempts,
                                                max_attempts = max_attempts,
                                                "Step has exceeded retry limit from template"
                                            );
                                            false
                                        } else {
                                            info!(
                                                step_uuid = %step_uuid,
                                                current_attempts = current_attempts,
                                                max_attempts = max_attempts,
                                                "Step is retryable with attempts remaining"
                                            );
                                            true
                                        }
                                    }
                                }
                                Err(_) => {
                                    // Can't deserialize results - default to checking retry limit only
                                    let max_attempts = step.max_attempts.unwrap_or(0);
                                    let current_attempts = step.attempts.unwrap_or(0);
                                    current_attempts < max_attempts
                                }
                            }
                        } else {
                            // No results - check retry limit only
                            let max_attempts = step.max_attempts.unwrap_or(0);
                            let current_attempts = step.attempts.unwrap_or(0);
                            current_attempts < max_attempts
                        };

                        // Determine which event to use based on retryability
                        if should_retry {
                            // Transition to WaitingForRetry (backoff already calculated)
                            info!(
                                step_uuid = %step_uuid,
                                "Transitioning to WaitingForRetry state"
                            );
                            let error_message = if let Some(results_json) = &step.results {
                                match serde_json::from_value::<StepExecutionResult>(
                                    results_json.clone(),
                                ) {
                                    Ok(step_execution_result) => step_execution_result
                                        .error
                                        .map(|e| e.message)
                                        .unwrap_or_else(|| {
                                            "Step execution failed - retryable".to_string()
                                        }),
                                    Err(_) => format!(
                                        "Step failed with status: {} - retryable",
                                        original_status
                                    ),
                                }
                            } else {
                                format!("Step failed with status: {} - retryable", original_status)
                            };
                            StepEvent::WaitForRetry(error_message)
                        } else {
                            // Transition to Error (permanent failure or max retries)
                            info!(
                                step_uuid = %step_uuid,
                                "Transitioning to Error state (permanent or max retries)"
                            );
                            let error_message = if let Some(results_json) = &step.results {
                                match serde_json::from_value::<StepExecutionResult>(
                                    results_json.clone(),
                                ) {
                                    Ok(step_execution_result) => step_execution_result
                                        .error
                                        .map(|e| e.message)
                                        .unwrap_or_else(|| "Step execution failed".to_string()),
                                    Err(_) => {
                                        format!("Step failed with status: {}", original_status)
                                    }
                                }
                            } else {
                                format!("Step failed with status: {}", original_status)
                            };
                            StepEvent::Fail(error_message)
                        }
                    }
                    _ => unreachable!("Already matched above"),
                };

                // Execute the state transition
                let final_state = state_machine.transition(final_event).await.map_err(|e| {
                    tasker_shared::errors::OrchestrationError::StateTransitionFailed {
                        entity_type: "WorkflowStep".to_string(),
                        entity_uuid: *step_uuid,
                        reason: format!("Failed to transition step to final state: {}", e),
                    }
                })?;

                info!(
                    step_uuid = %step_uuid,
                    final_state = %final_state,
                    "Successfully transitioned step from notification state to final state"
                );
            } else {
                debug!(
                    step_uuid = %step_uuid,
                    current_state = %step_state,
                    "Step not in EnqueuedForOrchestration or EnqueuedAsErrorForOrchestration state - skipping orchestration transition"
                );
            }
        } else {
            warn!(
                step_uuid = %step_uuid,
                "Step has no current state - may be in inconsistent state"
            );
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
