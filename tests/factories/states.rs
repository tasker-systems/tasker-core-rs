//! # State Factories
//!
//! Factories for creating state transitions and managing entity states.
//!
//! This module provides factories for creating WorkflowStepTransition records
//! with proper state machine adherence and audit trail functionality.

#![allow(dead_code)]
#![allow(clippy::wrong_self_convention)] // Builder pattern methods use `to_*` and `from_*` appropriately

use super::base::*;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;
use tasker_core::models::core::workflow_step_transition::NewWorkflowStepTransition;
use tasker_core::models::WorkflowStepTransition;
use tasker_core::state_machine::WorkflowStepState;

/// Factory for creating WorkflowStepTransition instances
///
/// Following Rails pattern from ComplexWorkflowFactoryHelpers, this factory creates
/// state transitions with simple direct state setting (no FSM layer) as requested.
///
/// # Examples
///
/// ```rust
/// // Create a simple transition to 'complete' state
/// let transition = WorkflowStepTransitionFactory::new()
///     .for_workflow_step(step_id)
///     .to_state("complete")
///     .create(&pool)
///     .await?;
///
/// // Create a transition with error metadata
/// let error_transition = WorkflowStepTransitionFactory::new()
///     .for_workflow_step(step_id)
///     .to_state("error")
///     .with_error("Network timeout")
///     .create(&pool)
///     .await?;
///
/// // Create a retry transition
/// let retry_transition = WorkflowStepTransitionFactory::new()
///     .for_workflow_step(step_id)
///     .from_state("error")
///     .to_state("pending")
///     .with_retry_attempt(2)
///     .create(&pool)
///     .await?;
/// ```
#[derive(Debug, Clone)]
pub struct WorkflowStepTransitionFactory {
    base: BaseFactory,
    workflow_step_id: Option<i64>,
    to_state: Option<String>,
    from_state: Option<String>,
    metadata: Option<Value>,
    execution_duration: Option<f64>,
    error_message: Option<String>,
    retry_attempt: Option<i32>,
    resolved_by: Option<String>,
    triggered_by: Option<String>,
}

impl Default for WorkflowStepTransitionFactory {
    fn default() -> Self {
        Self {
            base: BaseFactory::new(),
            workflow_step_id: None,
            to_state: Some("pending".to_string()), // Default to pending state
            from_state: None,
            metadata: None,
            execution_duration: None,
            error_message: None,
            retry_attempt: None,
            resolved_by: None,
            triggered_by: None,
        }
    }
}

impl WorkflowStepTransitionFactory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the workflow step this transition belongs to
    pub fn for_workflow_step(mut self, workflow_step_id: i64) -> Self {
        self.workflow_step_id = Some(workflow_step_id);
        self
    }

    /// Set the target state for this transition
    pub fn to_state(mut self, state: &str) -> Self {
        self.to_state = Some(state.to_string());
        self
    }

    /// Set the source state for this transition
    pub fn from_state(mut self, state: &str) -> Self {
        self.from_state = Some(state.to_string());
        self
    }

    /// Set custom metadata
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Create a transition to 'in_progress' state
    pub fn to_in_progress(mut self) -> Self {
        self.to_state = Some(WorkflowStepState::InProgress.to_string());
        self
    }

    /// Create a transition to 'complete' state with optional execution duration
    pub fn to_complete(mut self) -> Self {
        self.to_state = Some(WorkflowStepState::Complete.to_string());
        self
    }

    /// Create a transition to 'complete' state with execution duration
    pub fn to_complete_with_duration(mut self, duration_seconds: f64) -> Self {
        self.to_state = Some(WorkflowStepState::Complete.to_string());
        self.execution_duration = Some(duration_seconds);
        self
    }

    /// Create a transition to 'error' state with error message
    pub fn to_error(mut self) -> Self {
        self.to_state = Some(WorkflowStepState::Error.to_string());
        self
    }

    /// Create a transition to 'error' state with specific error message
    pub fn with_error(mut self, error_message: &str) -> Self {
        self.to_state = Some(WorkflowStepState::Error.to_string());
        self.error_message = Some(error_message.to_string());
        self
    }

    /// Create a transition to 'cancelled' state
    pub fn to_cancelled(mut self) -> Self {
        self.to_state = Some(WorkflowStepState::Cancelled.to_string());
        self
    }

    /// Create a transition to 'cancelled' state with trigger reason
    pub fn to_cancelled_by(mut self, triggered_by: &str) -> Self {
        self.to_state = Some(WorkflowStepState::Cancelled.to_string());
        self.triggered_by = Some(triggered_by.to_string());
        self
    }

    /// Create a transition to 'resolved_manually' state
    pub fn to_resolved_manually(mut self) -> Self {
        self.to_state = Some(WorkflowStepState::ResolvedManually.to_string());
        self
    }

    /// Create a transition to 'resolved_manually' state with resolver
    pub fn resolved_by(mut self, resolver: &str) -> Self {
        self.to_state = Some(WorkflowStepState::ResolvedManually.to_string());
        self.resolved_by = Some(resolver.to_string());
        self
    }

    /// Create a retry transition (error -> pending) with attempt number
    pub fn with_retry_attempt(mut self, attempt: i32) -> Self {
        self.from_state = Some(WorkflowStepState::Error.to_string());
        self.to_state = Some(WorkflowStepState::Pending.to_string());
        self.retry_attempt = Some(attempt);
        self
    }

    /// Build the metadata JSON from individual fields
    fn build_metadata(&self) -> Value {
        let mut metadata = self.metadata.clone().unwrap_or_else(|| json!({}));

        if let Value::Object(ref mut map) = metadata {
            // Add execution duration if specified
            if let Some(duration) = self.execution_duration {
                map.insert("execution_duration".to_string(), json!(duration));
            }

            // Add error message if specified
            if let Some(error) = &self.error_message {
                map.insert("error_message".to_string(), json!(error));
            }

            // Add retry attempt if specified
            if let Some(attempt) = self.retry_attempt {
                map.insert("retry_attempt".to_string(), json!(true));
                map.insert("attempt_number".to_string(), json!(attempt));
            }

            // Add resolved_by if specified
            if let Some(resolver) = &self.resolved_by {
                map.insert("resolved_by".to_string(), json!(resolver));
            }

            // Add triggered_by if specified
            if let Some(trigger) = &self.triggered_by {
                map.insert("triggered_by".to_string(), json!(trigger));
            }

            // Add factory metadata
            map.insert("factory_created".to_string(), json!(true));
            map.insert("created_at".to_string(), json!(Utc::now()));
        }

        metadata
    }
}

#[async_trait]
impl SqlxFactory<WorkflowStepTransition> for WorkflowStepTransitionFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<WorkflowStepTransition> {
        let workflow_step_id =
            self.workflow_step_id
                .ok_or_else(|| FactoryError::InvalidConfig {
                    details: "workflow_step_id is required".to_string(),
                })?;

        let to_state = self
            .to_state
            .clone()
            .ok_or_else(|| FactoryError::InvalidConfig {
                details: "to_state is required".to_string(),
            })?;

        // Build metadata with all specified fields
        let metadata = self.build_metadata();
        utils::validate_jsonb(&metadata)?;

        let new_transition = NewWorkflowStepTransition {
            workflow_step_id,
            to_state,
            from_state: self.from_state.clone(),
            metadata: Some(metadata),
        };

        // Use the model's create method which handles sort_key and most_recent automatically
        let transition = WorkflowStepTransition::create(pool, new_transition)
            .await
            .map_err(FactoryError::Database)?;

        Ok(transition)
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<WorkflowStepTransition> {
        // For transitions, we typically create new instances since each transition
        // represents a unique state change event in time
        self.create(pool).await
    }
}

/// Factory helper for creating common transition sequences
///
/// This follows the Rails pattern of creating sensible defaults that put the system
/// into states directly rather than trying to move through the FSM layer.
impl WorkflowStepTransitionFactory {
    /// Create a complete workflow step lifecycle (pending -> in_progress -> complete)
    pub async fn create_complete_lifecycle(
        workflow_step_id: i64,
        pool: &PgPool,
    ) -> FactoryResult<Vec<WorkflowStepTransition>> {
        let mut transitions = Vec::new();

        // Initial transition to pending (usually created by system)
        let pending = Self::new()
            .for_workflow_step(workflow_step_id)
            .to_state("pending")
            .create(pool)
            .await?;
        transitions.push(pending);

        // Transition to in_progress
        let in_progress = Self::new()
            .for_workflow_step(workflow_step_id)
            .from_state("pending")
            .to_in_progress()
            .create(pool)
            .await?;
        transitions.push(in_progress);

        // Transition to complete with duration
        let complete = Self::new()
            .for_workflow_step(workflow_step_id)
            .from_state("in_progress")
            .to_complete_with_duration(2.5) // 2.5 seconds execution time
            .create(pool)
            .await?;
        transitions.push(complete);

        Ok(transitions)
    }

    /// Create a failed workflow step lifecycle (pending -> in_progress -> error)
    pub async fn create_failed_lifecycle(
        workflow_step_id: i64,
        error_message: &str,
        pool: &PgPool,
    ) -> FactoryResult<Vec<WorkflowStepTransition>> {
        let mut transitions = Vec::new();

        // Initial transition to pending
        let pending = Self::new()
            .for_workflow_step(workflow_step_id)
            .to_state("pending")
            .create(pool)
            .await?;
        transitions.push(pending);

        // Transition to in_progress
        let in_progress = Self::new()
            .for_workflow_step(workflow_step_id)
            .from_state("pending")
            .to_in_progress()
            .create(pool)
            .await?;
        transitions.push(in_progress);

        // Transition to error
        let error = Self::new()
            .for_workflow_step(workflow_step_id)
            .from_state("in_progress")
            .with_error(error_message)
            .create(pool)
            .await?;
        transitions.push(error);

        Ok(transitions)
    }

    /// Create a retry lifecycle (error -> pending -> in_progress -> complete)
    pub async fn create_retry_lifecycle(
        workflow_step_id: i64,
        attempt_number: i32,
        pool: &PgPool,
    ) -> FactoryResult<Vec<WorkflowStepTransition>> {
        let mut transitions = Vec::new();

        // Retry transition (error -> pending)
        let retry = Self::new()
            .for_workflow_step(workflow_step_id)
            .with_retry_attempt(attempt_number)
            .create(pool)
            .await?;
        transitions.push(retry);

        // Transition to in_progress
        let in_progress = Self::new()
            .for_workflow_step(workflow_step_id)
            .from_state("pending")
            .to_in_progress()
            .create(pool)
            .await?;
        transitions.push(in_progress);

        // Transition to complete (successful retry)
        let complete = Self::new()
            .for_workflow_step(workflow_step_id)
            .from_state("in_progress")
            .to_complete_with_duration(1.8) // Faster on retry
            .create(pool)
            .await?;
        transitions.push(complete);

        Ok(transitions)
    }

    /// Create a manually resolved lifecycle (error -> resolved_manually)
    pub async fn create_manual_resolution_lifecycle(
        workflow_step_id: i64,
        resolver: &str,
        pool: &PgPool,
    ) -> FactoryResult<Vec<WorkflowStepTransition>> {
        let mut transitions = Vec::new();

        // Transition to resolved_manually
        let resolved = Self::new()
            .for_workflow_step(workflow_step_id)
            .from_state("error")
            .resolved_by(resolver)
            .create(pool)
            .await?;
        transitions.push(resolved);

        Ok(transitions)
    }
}
