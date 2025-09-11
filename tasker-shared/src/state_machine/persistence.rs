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
use super::states::TaskState;
use crate::models::core::task_transition::{NewTaskTransition, TaskTransition};
use crate::models::core::workflow_step_transition::{
    NewWorkflowStepTransition, WorkflowStepTransition,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

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
        entity_id: Uuid,
        pool: &PgPool,
    ) -> PersistenceResult<Option<String>>;

    /// Get the next sort key for ordering transitions
    async fn get_next_sort_key(&self, entity_id: Uuid, pool: &PgPool) -> PersistenceResult<i32>;
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
            task_uuid: task.task_uuid,
            to_state,
            from_state,
            processor_uuid: None, // No processor tracking in base persist_transition
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
        task_uuid: Uuid,
        pool: &PgPool,
    ) -> PersistenceResult<Option<String>> {
        let current_transition =
            TaskTransition::get_current(pool, task_uuid)
                .await
                .map_err(|_e| PersistenceError::StateResolutionFailed {
                    entity_id: task_uuid.to_string(),
                })?;

        Ok(current_transition.map(|t| t.to_state))
    }

    async fn get_next_sort_key(&self, task_uuid: Uuid, pool: &PgPool) -> PersistenceResult<i32> {
        // Note: This is now handled internally by TaskTransition::create()
        // This method is kept for trait compliance but not used in persist_transition
        let transitions = TaskTransition::list_by_task(pool, task_uuid)
            .await
            .map_err(|_e| PersistenceError::StateResolutionFailed {
                entity_id: task_uuid.to_string(),
            })?;

        Ok(transitions.len() as i32 + 1)
    }
}

impl TaskTransitionPersistence {
    /// TAS-41: Atomic transition with ownership validation
    /// Returns true if transition succeeded, false if ownership conflict occurred
    pub async fn transition_with_ownership(
        &self,
        task_uuid: Uuid,
        from_state: TaskState,
        to_state: TaskState,
        processor_uuid: Uuid,
        metadata: Option<Value>,
        pool: &PgPool,
    ) -> PersistenceResult<bool> {
        // Enhanced metadata if provided (but processor_uuid goes in its own column)
        let transition_metadata = metadata.unwrap_or_else(|| {
            serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })
        });

        // Create transition with processor_uuid in dedicated column
        let new_transition = NewTaskTransition {
            task_uuid,
            to_state: to_state.to_string(),
            from_state: Some(from_state.to_string()),
            processor_uuid: Some(processor_uuid), // TAS-41: Use dedicated column
            metadata: Some(transition_metadata),
        };

        TaskTransition::create(pool, new_transition)
            .await
            .map_err(|e| PersistenceError::TransitionSaveFailed {
                reason: format!("Ownership transition failed: {e}"),
            })?;

        // If we reach here, the transition succeeded
        Ok(true)
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
            workflow_step_uuid: step.workflow_step_uuid,
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
        step_uuid: Uuid,
        pool: &PgPool,
    ) -> PersistenceResult<Option<String>> {
        let current_transition = WorkflowStepTransition::get_current(pool, step_uuid)
            .await
            .map_err(|_e| PersistenceError::StateResolutionFailed {
                entity_id: step_uuid.to_string(),
            })?;

        Ok(current_transition.map(|t| t.to_state))
    }

    async fn get_next_sort_key(&self, step_uuid: Uuid, pool: &PgPool) -> PersistenceResult<i32> {
        // Note: This is now handled internally by WorkflowStepTransition::create()
        // This method is kept for trait compliance but not used in persist_transition
        let transitions = WorkflowStepTransition::list_by_workflow_step(pool, step_uuid)
            .await
            .map_err(|_e| PersistenceError::StateResolutionFailed {
                entity_id: step_uuid.to_string(),
            })?;

        Ok(transitions.len() as i32 + 1)
    }
}

/// Helper function to create idempotent transitions
pub async fn idempotent_transition<T>(
    persistence: &impl TransitionPersistence<T>,
    entity: &T,
    entity_id: Uuid,
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
    entity_id: Uuid,
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
                    entity_id = %entity_id,
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
