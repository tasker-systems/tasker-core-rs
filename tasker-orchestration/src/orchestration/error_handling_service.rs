//! # Error Handling Service
//!
//! Service that integrates error classification with state transitions and backoff logic.
//! This service bridges the gap between the StandardErrorClassifier and the actual
//! step state management, implementing the logic to transition steps to appropriate
//! states based on error classification.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::orchestration::{
    BackoffCalculator, BackoffContext, ErrorClassifier, ErrorContext, StandardErrorClassifier,
};
use tasker_shared::{
    errors::OrchestrationError,
    models::WorkflowStep,
    state_machine::{StepEvent, StepStateMachine, WorkflowStepState},
    system_context::SystemContext,
    TaskerError,
};

/// Result of error handling operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandlingResult {
    /// Step UUID that was processed
    pub step_uuid: Uuid,
    /// Action taken based on error classification
    pub action: ErrorHandlingAction,
    /// Final state of the step after handling
    pub final_state: WorkflowStepState,
    /// Whether backoff was applied
    pub backoff_applied: bool,
    /// Next retry time if applicable
    pub next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Error classification details
    pub classification_summary: String,
}

/// Actions taken during error handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorHandlingAction {
    /// Step marked as permanently failed
    MarkedAsPermanentFailure,
    /// Step transitioned to waiting for retry with backoff
    TransitionedToWaitingForRetry,
    /// Step marked as error (retry limit exceeded)
    MarkedAsError,
    /// No action taken (already in appropriate state)
    NoActionTaken,
}

/// Configuration for error handling behavior
#[derive(Debug, Clone)]
pub struct ErrorHandlingConfig {
    /// Whether to use intelligent error classification
    pub use_error_classification: bool,
    /// Whether to transition to WaitingForRetry state
    pub use_waiting_for_retry_state: bool,
    /// Default retry limit if not specified on step
    pub default_max_attempts: u32,
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        Self {
            use_error_classification: true,
            use_waiting_for_retry_state: true,
            default_max_attempts: 3,
        }
    }
}

/// Error handling service that integrates classification with state management
pub struct ErrorHandlingService {
    config: ErrorHandlingConfig,
    error_classifier: Arc<dyn ErrorClassifier + Send + Sync>,
    backoff_calculator: BackoffCalculator,
    system_context: Arc<SystemContext>,
}

impl std::fmt::Debug for ErrorHandlingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErrorHandlingService")
            .field("config", &self.config)
            .field("has_error_classifier", &true)
            .field("backoff_calculator", &self.backoff_calculator)
            .finish()
    }
}

impl ErrorHandlingService {
    /// Create a new error handling service
    pub fn new(
        config: ErrorHandlingConfig,
        backoff_calculator: BackoffCalculator,
        system_context: Arc<SystemContext>,
    ) -> Self {
        let error_classifier = Arc::new(StandardErrorClassifier::new());

        Self {
            config,
            error_classifier,
            backoff_calculator,
            system_context,
        }
    }

    /// Create a new error handling service with custom classifier
    pub fn with_classifier(
        config: ErrorHandlingConfig,
        error_classifier: Arc<dyn ErrorClassifier + Send + Sync>,
        backoff_calculator: BackoffCalculator,
        system_context: Arc<SystemContext>,
    ) -> Self {
        Self {
            config,
            error_classifier,
            backoff_calculator,
            system_context,
        }
    }

    /// Handle a step error with intelligent classification and state management
    pub async fn handle_step_error(
        &self,
        step: &WorkflowStep,
        error: &OrchestrationError,
        _error_message: Option<String>,
    ) -> Result<ErrorHandlingResult, TaskerError> {
        // Create error context for classification
        let error_context = self.create_error_context(step, _error_message).await?;

        // Classify the error
        let classification = self.error_classifier.classify_error(error, &error_context);

        // Determine appropriate action based on classification
        if classification.is_retryable && self.config.use_error_classification {
            self.handle_retryable_error(step, &classification, &error_context)
                .await
        } else {
            self.handle_permanent_error(step, &classification).await
        }
    }

    /// Handle a retryable error by transitioning to WaitingForRetry or applying backoff
    async fn handle_retryable_error(
        &self,
        step: &WorkflowStep,
        classification: &crate::orchestration::ErrorClassification,
        error_context: &ErrorContext,
    ) -> Result<ErrorHandlingResult, TaskerError> {
        // Check if we should use the new WaitingForRetry state
        if self.config.use_waiting_for_retry_state {
            self.transition_to_waiting_for_retry(step, classification, error_context)
                .await
        } else {
            // Legacy behavior: apply backoff and keep in current state
            self.apply_backoff_legacy(step, classification).await
        }
    }

    /// Transition step to WaitingForRetry state with appropriate backoff
    async fn transition_to_waiting_for_retry(
        &self,
        step: &WorkflowStep,
        classification: &crate::orchestration::ErrorClassification,
        _error_context: &ErrorContext,
    ) -> Result<ErrorHandlingResult, TaskerError> {
        // Apply backoff calculation
        let backoff_context =
            BackoffContext::new().with_error(classification.error_message.clone());

        let backoff_result = self
            .backoff_calculator
            .calculate_and_apply_backoff(&step.workflow_step_uuid, backoff_context)
            .await
            .map_err(|e| TaskerError::DatabaseError(e.to_string()))?;

        // Create state machine for transition
        let mut state_machine = StepStateMachine::new(step.clone(), self.system_context.clone());

        // Transition to WaitingForRetry state
        let event = StepEvent::wait_for_retry(format!(
            "Error classified as retryable: {}",
            classification.error_message
        ));
        state_machine
            .transition(event)
            .await
            .map_err(|e| TaskerError::StateMachineError(e.to_string()))?;

        Ok(ErrorHandlingResult {
            step_uuid: step.workflow_step_uuid,
            action: ErrorHandlingAction::TransitionedToWaitingForRetry,
            final_state: WorkflowStepState::WaitingForRetry,
            backoff_applied: true,
            next_retry_at: Some(backoff_result.next_retry_at),
            classification_summary: format!(
                "Retryable error ({}): {}",
                classification.error_category, classification.error_message
            ),
        })
    }

    /// Apply backoff using legacy approach (no state transition)
    async fn apply_backoff_legacy(
        &self,
        step: &WorkflowStep,
        classification: &crate::orchestration::ErrorClassification,
    ) -> Result<ErrorHandlingResult, TaskerError> {
        let backoff_context =
            BackoffContext::new().with_error(classification.error_message.clone());

        let backoff_result = self
            .backoff_calculator
            .calculate_and_apply_backoff(&step.workflow_step_uuid, backoff_context)
            .await
            .map_err(|e| TaskerError::DatabaseError(e.to_string()))?;

        Ok(ErrorHandlingResult {
            step_uuid: step.workflow_step_uuid,
            action: ErrorHandlingAction::NoActionTaken,
            final_state: WorkflowStepState::Error, // Assume current state
            backoff_applied: true,
            next_retry_at: Some(backoff_result.next_retry_at),
            classification_summary: format!(
                "Retryable error with legacy backoff ({}): {}",
                classification.error_category, classification.error_message
            ),
        })
    }

    /// Handle a permanent error by transitioning to Error state
    async fn handle_permanent_error(
        &self,
        step: &WorkflowStep,
        classification: &crate::orchestration::ErrorClassification,
    ) -> Result<ErrorHandlingResult, TaskerError> {
        // Create state machine for transition
        let mut state_machine = StepStateMachine::new(step.clone(), self.system_context.clone());

        // Transition to Error state
        let event = StepEvent::fail_with_error(format!(
            "Permanent error: {}",
            classification.error_message
        ));
        state_machine
            .transition(event)
            .await
            .map_err(|e| TaskerError::StateMachineError(e.to_string()))?;

        let action = if classification.is_final_attempt {
            ErrorHandlingAction::MarkedAsError
        } else {
            ErrorHandlingAction::MarkedAsPermanentFailure
        };

        Ok(ErrorHandlingResult {
            step_uuid: step.workflow_step_uuid,
            action,
            final_state: WorkflowStepState::Error,
            backoff_applied: false,
            next_retry_at: None,
            classification_summary: format!(
                "Permanent error ({}): {}",
                classification.error_category, classification.error_message
            ),
        })
    }

    /// Create error context from step and error information
    async fn create_error_context(
        &self,
        step: &WorkflowStep,
        _error_message: Option<String>,
    ) -> Result<ErrorContext, TaskerError> {
        let attempts = step.attempts.unwrap_or(0) as u32;
        let max_attempts = step
            .max_attempts
            .unwrap_or(self.config.default_max_attempts as i32) as u32;

        Ok(ErrorContext {
            step_uuid: step.workflow_step_uuid,
            task_uuid: step.task_uuid,
            attempt_number: attempts + 1, // Current attempt (1-based)
            max_attempts,
            execution_duration: std::time::Duration::from_secs(0), // TODO: Calculate from timestamps
            step_name: self.get_step_name(step).await?,
            error_source: "orchestration".to_string(),
            metadata: std::collections::HashMap::new(),
        })
    }

    /// Get step name from named step
    async fn get_step_name(&self, step: &WorkflowStep) -> Result<String, TaskerError> {
        let result = sqlx::query!(
            "SELECT name FROM tasker_named_steps WHERE named_step_uuid = $1",
            step.named_step_uuid
        )
        .fetch_optional(self.system_context.database_pool())
        .await
        .map_err(|e| TaskerError::DatabaseError(e.to_string()))?;

        Ok(result
            .map(|r| r.name)
            .unwrap_or_else(|| "unknown_step".to_string()))
    }

    /// Check if a step should be transitioned from WaitingForRetry back to Pending
    /// This is called by the orchestration system to check if retry delays have expired
    pub async fn check_waiting_for_retry_readiness(
        &self,
        step_uuid: Uuid,
    ) -> Result<bool, TaskerError> {
        self.backoff_calculator
            .is_ready_to_retry(step_uuid)
            .await
            .map_err(|e| TaskerError::DatabaseError(e.to_string()))
    }

    /// Transition a step from WaitingForRetry back to Pending when ready
    pub async fn transition_from_waiting_to_pending(
        &self,
        step: &WorkflowStep,
    ) -> Result<(), TaskerError> {
        let mut state_machine = StepStateMachine::new(step.clone(), self.system_context.clone());

        // Transition back to pending when retry delay expires
        let event = StepEvent::Retry;
        state_machine
            .transition(event)
            .await
            .map_err(|e| TaskerError::StateMachineError(e.to_string()))?;

        // Clear backoff settings now that step is ready
        self.backoff_calculator
            .clear_backoff(step.workflow_step_uuid)
            .await
            .map_err(|e| TaskerError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

// Tests would require integration test setup with database
// TODO: Add comprehensive integration tests for error handling scenarios
