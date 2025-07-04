use super::errors::{PersistenceError, PersistenceResult};
use async_trait::async_trait;
use chrono::Utc;
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
        let sort_key = self.get_next_sort_key(task.task_id, pool).await?;

        let transition_metadata = metadata.unwrap_or_else(|| {
            serde_json::json!({
                "event": event,
                "timestamp": Utc::now(),
            })
        });

        // Start a transaction to ensure consistency
        let mut tx = pool.begin().await?;

        // Insert the new transition
        sqlx::query!(
            r#"
            INSERT INTO tasker_task_transitions 
            (task_id, from_state, to_state, sort_key, most_recent, metadata)
            VALUES ($1, $2, $3, $4, true, $5)
            "#,
            task.task_id,
            from_state,
            to_state,
            sort_key,
            transition_metadata
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| PersistenceError::TransitionSaveFailed {
            reason: format!("Failed to insert transition: {}", e),
        })?;

        // Update most_recent flags for previous transitions
        sqlx::query!(
            r#"
            UPDATE tasker_task_transitions 
            SET most_recent = false 
            WHERE task_id = $1 AND sort_key < $2
            "#,
            task.task_id,
            sort_key
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| PersistenceError::TransitionSaveFailed {
            reason: format!("Failed to update most_recent flags: {}", e),
        })?;

        tx.commit().await?;
        Ok(())
    }

    async fn resolve_current_state(
        &self,
        task_id: i64,
        pool: &PgPool,
    ) -> PersistenceResult<Option<String>> {
        let row = sqlx::query!(
            r#"
            SELECT to_state 
            FROM tasker_task_transitions 
            WHERE task_id = $1 AND most_recent = true
            ORDER BY sort_key DESC 
            LIMIT 1
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| r.to_state))
    }

    async fn get_next_sort_key(&self, task_id: i64, pool: &PgPool) -> PersistenceResult<i32> {
        let row = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(sort_key), 0) + 1 as next_key
            FROM tasker_task_transitions 
            WHERE task_id = $1
            "#,
            task_id
        )
        .fetch_one(pool)
        .await?;

        Ok(row.next_key.unwrap_or(1))
    }
}

/// Workflow step transition persistence implementation
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
        let sort_key = self.get_next_sort_key(step.workflow_step_id, pool).await?;

        let transition_metadata = metadata.unwrap_or_else(|| {
            serde_json::json!({
                "event": event,
                "timestamp": Utc::now(),
            })
        });

        // Start a transaction to ensure consistency
        let mut tx = pool.begin().await?;

        // Insert the new transition
        sqlx::query!(
            r#"
            INSERT INTO tasker_workflow_step_transitions 
            (workflow_step_id, from_state, to_state, sort_key, most_recent, metadata)
            VALUES ($1, $2, $3, $4, true, $5)
            "#,
            step.workflow_step_id,
            from_state,
            to_state,
            sort_key,
            transition_metadata
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| PersistenceError::TransitionSaveFailed {
            reason: format!("Failed to insert transition: {}", e),
        })?;

        // Update most_recent flags for previous transitions
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_step_transitions 
            SET most_recent = false 
            WHERE workflow_step_id = $1 AND sort_key < $2
            "#,
            step.workflow_step_id,
            sort_key
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| PersistenceError::TransitionSaveFailed {
            reason: format!("Failed to update most_recent flags: {}", e),
        })?;

        tx.commit().await?;
        Ok(())
    }

    async fn resolve_current_state(
        &self,
        step_id: i64,
        pool: &PgPool,
    ) -> PersistenceResult<Option<String>> {
        let row = sqlx::query!(
            r#"
            SELECT to_state 
            FROM tasker_workflow_step_transitions 
            WHERE workflow_step_id = $1 AND most_recent = true
            ORDER BY sort_key DESC 
            LIMIT 1
            "#,
            step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| r.to_state))
    }

    async fn get_next_sort_key(&self, step_id: i64, pool: &PgPool) -> PersistenceResult<i32> {
        let row = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(sort_key), 0) + 1 as next_key
            FROM tasker_workflow_step_transitions 
            WHERE workflow_step_id = $1
            "#,
            step_id
        )
        .fetch_one(pool)
        .await?;

        Ok(row.next_key.unwrap_or(1))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_creation() {
        let metadata = serde_json::json!({
            "event": "start",
            "timestamp": Utc::now(),
        });

        assert_eq!(metadata["event"], "start");
        assert!(metadata["timestamp"].is_string());
    }

    #[tokio::test]
    async fn test_idempotent_logic() {
        // Test that idempotent_transition properly detects when no transition is needed
        // This would require a test database setup to run properly
    }
}
