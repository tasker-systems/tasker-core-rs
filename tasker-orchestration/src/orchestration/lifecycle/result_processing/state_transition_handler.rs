//! State Transition Handler
//!
//! TAS-41: Handles orchestration state transitions for EnqueuedForOrchestration steps.

use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::models::core::workflow_step::WorkflowStep;
use tasker_shared::state_machine::states::WorkflowStepState;
use tasker_shared::system_context::SystemContext;
use tasker_shared::errors::OrchestrationResult;

/// Handles orchestration state transitions for steps
#[derive(Clone)]
pub struct StateTransitionHandler {
    context: Arc<SystemContext>,
}

impl StateTransitionHandler {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// TAS-41: Process orchestration state transitions for EnqueuedForOrchestration steps
    ///
    /// This method handles the transition of steps from EnqueuedForOrchestration state
    /// to their final states (Complete or Error) after orchestration metadata processing.
    /// This is critical for fixing the race condition where workers bypass orchestration.
    pub async fn process_state_transition(
        &self,
        step_uuid: &Uuid,
        original_status: &String,
        correlation_id: Uuid,
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
                correlation_id = %correlation_id,
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
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    original_status = %original_status,
                    step_state = %step_state,
                    "Processing orchestration state transition for step in notification state"
                );

                // Create state machine for the step
                use tasker_shared::state_machine::StepStateMachine;
                let mut state_machine = StepStateMachine::new(step.clone(), self.context.clone());

                // Determine the final state based on step notification state and execution result
                let final_event = match step_state {
                    WorkflowStepState::EnqueuedForOrchestration => {
                        self.determine_success_event(&step, original_status)
                    }
                    WorkflowStepState::EnqueuedAsErrorForOrchestration => {
                        self.determine_error_event(&step, original_status, correlation_id)
                            .await
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
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    final_state = %final_state,
                    "Successfully transitioned step from notification state to final state"
                );
            } else {
                debug!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    current_state = %step_state,
                    "Step not in EnqueuedForOrchestration or EnqueuedAsErrorForOrchestration state - skipping orchestration transition"
                );
            }
        } else {
            warn!(
                correlation_id = %correlation_id,
                step_uuid = %step_uuid,
                "Step has no current state - may be in inconsistent state"
            );
        }

        Ok(())
    }

    /// Determine the event for success pathway (EnqueuedForOrchestration)
    fn determine_success_event(
        &self,
        step: &WorkflowStep,
        original_status: &String,
    ) -> tasker_shared::state_machine::events::StepEvent {
        use tasker_shared::state_machine::events::StepEvent;

        // Deserialize StepExecutionResult to determine final state
        if let Some(results_json) = &step.results {
            match serde_json::from_value::<StepExecutionResult>(results_json.clone()) {
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
                    if Self::is_success_status(original_status) {
                        StepEvent::Complete(step.results.clone())
                    } else {
                        StepEvent::Fail(format!("Step failed with status: {}", original_status))
                    }
                }
            }
        } else {
            // No results available - use status
            if Self::is_success_status(original_status) {
                StepEvent::Complete(None)
            } else {
                StepEvent::Fail(format!("Step failed with status: {}", original_status))
            }
        }
    }

    /// Determine the event for error pathway (EnqueuedAsErrorForOrchestration)
    async fn determine_error_event(
        &self,
        step: &WorkflowStep,
        original_status: &String,
        correlation_id: Uuid,
    ) -> tasker_shared::state_machine::events::StepEvent {
        use tasker_shared::state_machine::events::StepEvent;

        // Determine if step should retry or move to permanent error
        let should_retry = self.should_retry_step(step, correlation_id).await;

        if should_retry {
            // Transition to WaitingForRetry (backoff already calculated)
            info!(
                correlation_id = %correlation_id,
                step_uuid = %step.workflow_step_uuid,
                "Transitioning to WaitingForRetry state"
            );
            let error_message = self.extract_error_message(step, original_status);
            StepEvent::WaitForRetry(format!("{} - retryable", error_message))
        } else {
            // Transition to Error (permanent failure or max retries)
            info!(
                correlation_id = %correlation_id,
                step_uuid = %step.workflow_step_uuid,
                "Transitioning to Error state (permanent or max retries)"
            );
            let error_message = self.extract_error_message(step, original_status);
            StepEvent::Fail(error_message)
        }
    }

    /// Check if step should retry based on metadata and retry limits
    async fn should_retry_step(&self, step: &WorkflowStep, correlation_id: Uuid) -> bool {
        if let Some(results_json) = &step.results {
            match serde_json::from_value::<StepExecutionResult>(results_json.clone()) {
                Ok(step_execution_result) => {
                    // Check if error is marked as non-retryable in metadata
                    let retryable_from_metadata = step_execution_result.metadata.retryable;

                    if !retryable_from_metadata {
                        info!(
                            correlation_id = %correlation_id,
                            step_uuid = %step.workflow_step_uuid,
                            "Error marked as non-retryable by worker"
                        );
                        return false;
                    }

                    // Check retry limits from template
                    let max_attempts = step.max_attempts.unwrap_or(0);
                    let current_attempts = step.attempts.unwrap_or(0);

                    if current_attempts >= max_attempts {
                        info!(
                            correlation_id = %correlation_id,
                            step_uuid = %step.workflow_step_uuid,
                            current_attempts = current_attempts,
                            max_attempts = max_attempts,
                            "Step has exceeded retry limit from template"
                        );
                        false
                    } else {
                        info!(
                            correlation_id = %correlation_id,
                            step_uuid = %step.workflow_step_uuid,
                            current_attempts = current_attempts,
                            max_attempts = max_attempts,
                            "Step is retryable with attempts remaining"
                        );
                        true
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
        }
    }

    /// Extract error message from step results or status
    fn extract_error_message(&self, step: &WorkflowStep, original_status: &String) -> String {
        if let Some(results_json) = &step.results {
            match serde_json::from_value::<StepExecutionResult>(results_json.clone()) {
                Ok(step_execution_result) => step_execution_result
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Step execution failed".to_string()),
                Err(_) => format!("Step failed with status: {}", original_status),
            }
        } else {
            format!("Step failed with status: {}", original_status)
        }
    }

    /// Check if status indicates success
    fn is_success_status(status: &str) -> bool {
        let lower = status.to_lowercase();
        lower.contains("success") || lower == "complete" || lower == "completed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_state_transition_handler_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let handler = StateTransitionHandler::new(context);

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&handler.context) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_state_transition_handler_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let handler = StateTransitionHandler::new(context.clone());

        let cloned = handler.clone();

        // Verify both share the same Arc
        assert_eq!(
            Arc::as_ptr(&handler.context),
            Arc::as_ptr(&cloned.context)
        );
        Ok(())
    }

    #[test]
    fn test_is_success_status() {
        // Test success status detection
        assert!(StateTransitionHandler::is_success_status("success"));
        assert!(StateTransitionHandler::is_success_status("SUCCESS"));
        assert!(StateTransitionHandler::is_success_status("complete"));
        assert!(StateTransitionHandler::is_success_status("completed"));
        assert!(StateTransitionHandler::is_success_status("Success"));

        // Test non-success statuses
        assert!(!StateTransitionHandler::is_success_status("error"));
        assert!(!StateTransitionHandler::is_success_status("failed"));
        assert!(!StateTransitionHandler::is_success_status("timeout"));
    }

    #[test]
    fn test_is_success_status_partial_match() {
        // Test that "success" is matched even in longer strings
        assert!(StateTransitionHandler::is_success_status(
            "successful_execution"
        ));
        assert!(StateTransitionHandler::is_success_status(
            "operation_success"
        ));
    }
}

