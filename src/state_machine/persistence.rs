//! # State Machine Persistence Layer
//!
//! ## Architecture Decision: Delegation to Model Layer
//!
//! This persistence layer acts as a bridge between the state machine system and the data models,
//! following the principle of separation of concerns. Rather than duplicating database logic,
//! it delegates to the appropriate model methods:
//!
//! - **TaskTransitionPersistence** → delegates to `TaskTransition::create()` and `TaskTransition::get_current()`
//! - **StepTransitionPersistence** → delegates to `WorkflowStepTransition::create()` and `WorkflowStepTransition::get_current()`
//!
//! This approach provides several benefits:
//! 1. **No SQL Duplication**: Model methods handle all database operations
//! 2. **Atomic Transactions**: Models provide proper transaction handling
//! 3. **Consistency**: Single source of truth for database operations
//! 4. **Maintainability**: Changes to database schema only require model updates
//! 5. **Testability**: Model methods can be tested independently

use super::errors::{PersistenceError, PersistenceResult};
use crate::models::core::task_transition::{NewTaskTransition, TaskTransition};
use crate::models::core::workflow_step_transition::{
    NewWorkflowStepTransition, WorkflowStepTransition,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;

/// Trait for persisting state transitions
#[async_trait]
pub trait TransitionPersistence<T> {
    /// Persist a state transition
    async fn persist_transition(
        &self,
        entity: &T,
        from_state: Option<String>,
        to_state: String,
        event: &str,
        metadata: Option<Value>,
        pool: &PgPool,
    ) -> PersistenceResult<()>;

    /// Resolve the current state from persisted transitions
    async fn resolve_current_state(
        &self,
        entity_id: i64,
        pool: &PgPool,
    ) -> PersistenceResult<Option<String>>;

    /// Get the next sort key for ordering transitions
    async fn get_next_sort_key(&self, entity_id: i64, pool: &PgPool) -> PersistenceResult<i32>;
}

/// Task transition persistence implementation
#[derive(Clone)]
pub struct TaskTransitionPersistence;

#[async_trait]
impl TransitionPersistence<crate::models::Task> for TaskTransitionPersistence {
    async fn persist_transition(
        &self,
        task: &crate::models::Task,
        from_state: Option<String>,
        to_state: String,
        event: &str,
        metadata: Option<Value>,
        pool: &PgPool,
    ) -> PersistenceResult<()> {
        let transition_metadata = metadata.unwrap_or_else(|| {
            serde_json::json!({
                "event": event,
                "timestamp": chrono::Utc::now(),
            })
        });

        let new_transition = NewTaskTransition {
            task_id: task.task_id,
            to_state,
            from_state,
            metadata: Some(transition_metadata),
        };

        TaskTransition::create(pool, new_transition)
            .await
            .map_err(|e| PersistenceError::TransitionSaveFailed {
                reason: format!("Model delegation failed: {e}"),
            })?;

        Ok(())
    }

    async fn resolve_current_state(
        &self,
        task_id: i64,
        pool: &PgPool,
    ) -> PersistenceResult<Option<String>> {
        let current_transition = TaskTransition::get_current(pool, task_id)
            .await
            .map_err(|_e| PersistenceError::StateResolutionFailed { entity_id: task_id })?;

        Ok(current_transition.map(|t| t.to_state))
    }

    async fn get_next_sort_key(&self, task_id: i64, pool: &PgPool) -> PersistenceResult<i32> {
        // Note: This is now handled internally by TaskTransition::create()
        // This method is kept for trait compliance but not used in persist_transition
        let transitions = TaskTransition::list_by_task(pool, task_id)
            .await
            .map_err(|_e| PersistenceError::StateResolutionFailed { entity_id: task_id })?;

        Ok(transitions.len() as i32 + 1)
    }
}

/// Workflow step transition persistence implementation
#[derive(Clone)]
pub struct StepTransitionPersistence;

#[async_trait]
impl TransitionPersistence<crate::models::WorkflowStep> for StepTransitionPersistence {
    async fn persist_transition(
        &self,
        step: &crate::models::WorkflowStep,
        from_state: Option<String>,
        to_state: String,
        event: &str,
        metadata: Option<Value>,
        pool: &PgPool,
    ) -> PersistenceResult<()> {
        let transition_metadata = metadata.unwrap_or_else(|| {
            serde_json::json!({
                "event": event,
                "timestamp": chrono::Utc::now(),
            })
        });

        let new_transition = NewWorkflowStepTransition {
            workflow_step_id: step.workflow_step_id,
            to_state,
            from_state,
            metadata: Some(transition_metadata),
        };

        WorkflowStepTransition::create(pool, new_transition)
            .await
            .map_err(|e| PersistenceError::TransitionSaveFailed {
                reason: format!("Model delegation failed: {e}"),
            })?;

        Ok(())
    }

    async fn resolve_current_state(
        &self,
        step_id: i64,
        pool: &PgPool,
    ) -> PersistenceResult<Option<String>> {
        let current_transition = WorkflowStepTransition::get_current(pool, step_id)
            .await
            .map_err(|_e| PersistenceError::StateResolutionFailed { entity_id: step_id })?;

        Ok(current_transition.map(|t| t.to_state))
    }

    async fn get_next_sort_key(&self, step_id: i64, pool: &PgPool) -> PersistenceResult<i32> {
        // Note: This is now handled internally by WorkflowStepTransition::create()
        // This method is kept for trait compliance but not used in persist_transition
        let transitions = WorkflowStepTransition::list_by_workflow_step(pool, step_id)
            .await
            .map_err(|_e| PersistenceError::StateResolutionFailed { entity_id: step_id })?;

        Ok(transitions.len() as i32 + 1)
    }
}

/// Helper function to create idempotent transitions
pub async fn idempotent_transition<T>(
    persistence: &impl TransitionPersistence<T>,
    entity: &T,
    entity_id: i64,
    target_state: String,
    event: &str,
    metadata: Option<Value>,
    pool: &PgPool,
) -> PersistenceResult<bool>
where
    T: Send + Sync,
{
    // Check current state
    let current_state = persistence.resolve_current_state(entity_id, pool).await?;

    // If already in target state, no transition needed
    if current_state.as_ref() == Some(&target_state) {
        return Ok(false); // No transition occurred
    }

    // Perform the transition
    persistence
        .persist_transition(entity, current_state, target_state, event, metadata, pool)
        .await?;
    Ok(true) // Transition occurred
}

/// Transaction-safe state resolution with retry logic
pub async fn resolve_state_with_retry<T>(
    persistence: &impl TransitionPersistence<T>,
    entity_id: i64,
    pool: &PgPool,
    max_retries: u32,
) -> PersistenceResult<Option<String>> {
    let mut retries = 0;

    loop {
        match persistence.resolve_current_state(entity_id, pool).await {
            Ok(state) => return Ok(state),
            Err(e) if retries < max_retries => {
                retries += 1;
                tracing::warn!(
                    entity_id = entity_id,
                    retry = retries,
                    error = %e,
                    "Retrying state resolution"
                );

                // Exponential backoff
                let delay = std::time::Duration::from_millis(100 * (1 << retries));
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}
