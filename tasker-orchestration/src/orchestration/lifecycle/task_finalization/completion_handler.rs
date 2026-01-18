//! Completion Handler
//!
//! Handles task completion and error state transitions with proper state machine integration.
//!
//! ## TAS-73: Atomic Finalization
//!
//! The `complete_task` method uses transaction-based locking to ensure graceful handling
//! when multiple orchestrators simultaneously attempt to finalize the same task:
//!
//! 1. Acquires exclusive row lock via `SELECT ... FOR UPDATE`
//! 2. Checks current state while holding lock
//! 3. If already complete, releases lock and returns gracefully (no error)
//! 4. If not complete, performs transition within same transaction
//! 5. Commits to release lock
//!
//! This prevents race conditions where two orchestrators could both see "EvaluatingResults"
//! and attempt to transition, with the second one getting an error.

use std::sync::Arc;

use chrono::Utc;
use tracing::{debug, error};
use uuid::Uuid;

use tasker_shared::models::core::task_transition::{NewTaskTransition, TaskTransition};
use tasker_shared::models::orchestration::TaskExecutionContext;
use tasker_shared::models::Task;
use tasker_shared::state_machine::{TaskEvent, TaskState, TaskStateMachine};
use tasker_shared::system_context::SystemContext;

use super::{FinalizationAction, FinalizationError, FinalizationResult};

/// Handles task completion and error state transitions
#[derive(Clone, Debug)]
pub struct CompletionHandler {
    context: Arc<SystemContext>,
}

impl CompletionHandler {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Get state machine for a task
    pub async fn get_state_machine_for_task(
        &self,
        task: &Task,
    ) -> Result<TaskStateMachine, FinalizationError> {
        TaskStateMachine::for_task(
            task.task_uuid,
            self.context.database_pool().clone(),
            self.context.processor_uuid(),
        )
        .await
        .map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to create state machine: {e}"),
            task_uuid: task.task_uuid,
        })
    }

    // =========================================================================
    // TAS-73: Transaction-based atomic finalization helpers
    // =========================================================================

    /// Get current task state within an existing transaction
    async fn get_current_state_with_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
    ) -> Result<TaskState, FinalizationError> {
        let row = sqlx::query!(
            r#"
            SELECT to_state
            FROM tasker.task_transitions
            WHERE task_uuid = $1 AND most_recent = true
            "#,
            task_uuid
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to query current state: {e}"),
            task_uuid,
        })?;

        match row {
            Some(r) => r.to_state.parse().map_err(|_| FinalizationError::StateMachine {
                error: format!("Invalid state in database: {}", r.to_state),
                task_uuid,
            }),
            None => Ok(TaskState::Pending),
        }
    }

    /// Create a state transition within an existing transaction
    async fn create_transition_with_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        from_state: TaskState,
        to_state: TaskState,
        event: &TaskEvent,
        processor_uuid: Uuid,
    ) -> Result<(), FinalizationError> {
        let new_transition = NewTaskTransition {
            task_uuid,
            to_state: to_state.to_string(),
            from_state: Some(from_state.to_string()),
            processor_uuid: Some(processor_uuid),
            metadata: Some(serde_json::json!({
                "event": format!("{:?}", event),
                "timestamp": Utc::now().to_rfc3339(),
                "atomic_finalization": true,
            })),
        };

        TaskTransition::create_with_transaction(tx, new_transition)
            .await
            .map_err(|e| FinalizationError::StateMachine {
                error: format!("Failed to create transition: {e}"),
                task_uuid,
            })?;

        Ok(())
    }

    /// Mark task as complete within an existing transaction
    async fn mark_complete_with_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
    ) -> Result<(), FinalizationError> {
        sqlx::query!(
            r#"
            UPDATE tasker.tasks
            SET complete = true, updated_at = NOW()
            WHERE task_uuid = $1::uuid
            "#,
            task_uuid
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to mark task complete: {e}"),
            task_uuid,
        })?;

        Ok(())
    }

    /// Complete a task successfully using atomic transaction-based locking.
    ///
    /// TAS-73: This method uses SELECT FOR UPDATE to acquire an exclusive lock on the
    /// task row before checking state and transitioning. This ensures that concurrent
    /// finalization attempts from multiple orchestrators are handled gracefully:
    ///
    /// - First orchestrator: acquires lock, transitions to Complete, commits
    /// - Second orchestrator: waits for lock, sees Complete state, returns OK (no error)
    ///
    /// This prevents the "inelegant" behavior where the second orchestrator would get
    /// a state machine error even though the task completed successfully.
    pub async fn complete_task(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;
        let pool = self.context.database_pool();
        let processor_uuid = self.context.processor_uuid();

        // TAS-73: Start transaction and acquire exclusive lock on task row.
        // Other orchestrators will WAIT here instead of racing.
        let mut tx = pool.begin().await.map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to begin transaction: {e}"),
            task_uuid,
        })?;

        // Acquire exclusive row lock - this is the key to atomic finalization.
        // The SELECT FOR UPDATE locks the row until the transaction commits/rollbacks.
        // Other orchestrators attempting the same will WAIT here instead of racing.
        sqlx::query_scalar!(
            r#"SELECT task_uuid as "task_uuid!" FROM tasker.tasks WHERE task_uuid = $1 FOR UPDATE"#,
            task_uuid
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to acquire task lock: {e}"),
            task_uuid,
        })?;

        // Get current state while holding the lock
        let current_state = Self::get_current_state_with_tx(&mut tx, task_uuid).await?;

        debug!(
            task_uuid = %task_uuid,
            current_state = %current_state,
            correlation_id = %correlation_id,
            "Atomic finalization: acquired lock, checking state"
        );

        // If task is already complete, release lock and return gracefully (no error!)
        if current_state == TaskState::Complete {
            // Explicit rollback to release lock immediately (though drop would do it too)
            tx.rollback().await.ok();

            debug!(
                task_uuid = %task_uuid,
                correlation_id = %correlation_id,
                "Atomic finalization: task already complete, returning gracefully"
            );

            return Ok(FinalizationResult {
                task_uuid,
                action: FinalizationAction::Completed,
                completion_percentage: context
                    .as_ref()
                    .and_then(|c| c.completion_percentage.to_string().parse().ok()),
                total_steps: context.as_ref().map(|c| c.total_steps as i32),
                health_status: context.as_ref().map(|c| c.health_status.clone()),
                enqueued_steps: None,
                reason: None,
            });
        }

        // Handle proper state transitions based on current state
        match current_state {
            TaskState::EvaluatingResults => {
                // Transition from EvaluatingResults to Complete
                Self::create_transition_with_tx(
                    &mut tx,
                    task_uuid,
                    current_state,
                    TaskState::Complete,
                    &TaskEvent::AllStepsSuccessful,
                    processor_uuid,
                )
                .await?;
            }
            TaskState::StepsInProcess => {
                // Need to go through: StepsInProcess -> EvaluatingResults -> Complete
                Self::create_transition_with_tx(
                    &mut tx,
                    task_uuid,
                    current_state,
                    TaskState::EvaluatingResults,
                    &TaskEvent::AllStepsCompleted,
                    processor_uuid,
                )
                .await?;

                // Now transition from EvaluatingResults to Complete
                Self::create_transition_with_tx(
                    &mut tx,
                    task_uuid,
                    TaskState::EvaluatingResults,
                    TaskState::Complete,
                    &TaskEvent::AllStepsSuccessful,
                    processor_uuid,
                )
                .await?;
            }
            TaskState::Pending => {
                // This should not happen anymore with the proper initialization fix
                tx.rollback().await.ok();
                error!(
                    correlation_id = %correlation_id,
                    task_uuid = %task_uuid,
                    current_state = %current_state,
                    "Task is still in Pending state when trying to complete - this indicates an initialization issue"
                );
                return Err(FinalizationError::StateMachine {
                    error: "Task should not be in Pending state when trying to complete. This indicates the task was not properly initialized.".to_string(),
                    task_uuid,
                });
            }
            TaskState::Initializing => {
                // This means task has no steps and can be completed directly
                Self::create_transition_with_tx(
                    &mut tx,
                    task_uuid,
                    current_state,
                    TaskState::Complete,
                    &TaskEvent::NoStepsFound,
                    processor_uuid,
                )
                .await?;
            }
            _ => {
                // For other states, try the legacy Complete event as a fallback
                Self::create_transition_with_tx(
                    &mut tx,
                    task_uuid,
                    current_state,
                    TaskState::Complete,
                    &TaskEvent::Complete,
                    processor_uuid,
                )
                .await?;
            }
        }

        // Update the task complete flag within the same transaction
        Self::mark_complete_with_tx(&mut tx, task_uuid).await?;

        // Commit the transaction - this releases the lock
        tx.commit().await.map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to commit finalization transaction: {e}"),
            task_uuid,
        })?;

        debug!(
            task_uuid = %task_uuid,
            correlation_id = %correlation_id,
            "Atomic finalization: committed successfully"
        );

        Ok(FinalizationResult {
            task_uuid,
            action: FinalizationAction::Completed,
            completion_percentage: context
                .as_ref()
                .and_then(|c| c.completion_percentage.to_string().parse().ok()),
            total_steps: context.as_ref().map(|c| c.total_steps as i32),
            health_status: context.as_ref().map(|c| c.health_status.clone()),
            enqueued_steps: None,
            reason: None,
        })
    }

    /// Mark a task as failed due to errors using atomic transaction-based locking.
    ///
    /// TAS-73: Uses the same atomic pattern as `complete_task` to handle concurrent
    /// error finalization attempts gracefully.
    pub async fn error_task(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
        correlation_id: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task_uuid = task.task_uuid;
        let pool = self.context.database_pool();
        let processor_uuid = self.context.processor_uuid();

        // TAS-73: Start transaction and acquire exclusive lock on task row
        let mut tx = pool.begin().await.map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to begin transaction: {e}"),
            task_uuid,
        })?;

        // Acquire exclusive row lock
        sqlx::query_scalar!(
            r#"SELECT task_uuid as "task_uuid!" FROM tasker.tasks WHERE task_uuid = $1 FOR UPDATE"#,
            task_uuid
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to acquire task lock: {e}"),
            task_uuid,
        })?;

        // Get current state while holding the lock
        let current_state = Self::get_current_state_with_tx(&mut tx, task_uuid).await?;

        debug!(
            task_uuid = %task_uuid,
            current_state = %current_state,
            correlation_id = %correlation_id,
            "Atomic error finalization: acquired lock, checking state"
        );

        // If task is already in Error terminal state, return gracefully
        if current_state == TaskState::Error {
            tx.rollback().await.ok();

            debug!(
                task_uuid = %task_uuid,
                correlation_id = %correlation_id,
                "Atomic error finalization: task already in Error state, returning gracefully"
            );

            return Ok(FinalizationResult {
                task_uuid,
                action: FinalizationAction::Failed,
                completion_percentage: context
                    .as_ref()
                    .and_then(|c| c.completion_percentage.to_string().parse().ok()),
                total_steps: context.as_ref().map(|c| c.total_steps as i32),
                health_status: context.as_ref().map(|c| c.health_status.clone()),
                enqueued_steps: None,
                reason: Some("Steps in error state".to_string()),
            });
        }

        // TAS-67: Different events for different states
        // - From EvaluatingResults: PermanentFailure -> BlockedByFailures
        // - From BlockedByFailures: GiveUp -> Error (terminal state)
        let error_message = "Steps in error state".to_string();

        if current_state == TaskState::BlockedByFailures {
            // Already in BlockedByFailures - transition to Error terminal state
            Self::create_transition_with_tx(
                &mut tx,
                task_uuid,
                current_state,
                TaskState::Error,
                &TaskEvent::GiveUp,
                processor_uuid,
            )
            .await?;
        } else {
            // From other states - transition to BlockedByFailures first
            Self::create_transition_with_tx(
                &mut tx,
                task_uuid,
                current_state,
                TaskState::BlockedByFailures,
                &TaskEvent::PermanentFailure(error_message.clone()),
                processor_uuid,
            )
            .await?;
        }

        // Commit the transaction
        tx.commit().await.map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to commit error finalization transaction: {e}"),
            task_uuid,
        })?;

        debug!(
            task_uuid = %task_uuid,
            correlation_id = %correlation_id,
            "Atomic error finalization: committed successfully"
        );

        Ok(FinalizationResult {
            task_uuid,
            action: FinalizationAction::Failed,
            completion_percentage: context
                .as_ref()
                .and_then(|c| c.completion_percentage.to_string().parse().ok()),
            total_steps: context.as_ref().map(|c| c.total_steps as i32),
            health_status: context.as_ref().map(|c| c.health_status.clone()),
            enqueued_steps: None,
            reason: Some("Steps in error state".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_completion_handler_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test that we can create CompletionHandler
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let handler = CompletionHandler::new(context);

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&handler.context) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_completion_handler_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test that CompletionHandler implements Clone
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let handler = CompletionHandler::new(context.clone());

        let cloned = handler.clone();

        // Verify both share the same Arc
        assert_eq!(Arc::as_ptr(&handler.context), Arc::as_ptr(&cloned.context));
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_state_machine_for_task_initialization(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let _handler = CompletionHandler::new(context);

        // Test that we can create state machines for tasks
        // State machines can be created even for tasks without transitions
        // They will default to Pending state
        let nonexistent_uuid = Uuid::new_v4();

        let result = tasker_shared::state_machine::TaskStateMachine::for_task(
            nonexistent_uuid,
            pool.clone(),
            Uuid::new_v4(),
        )
        .await;

        // State machine creation succeeds even without existing transitions
        // This is expected behavior - state machine starts in Pending
        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_finalization_result_structure_for_completion() {
        // Test that FinalizationResult has correct structure for successful completion
        let task_uuid = Uuid::new_v4();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::Completed,
            completion_percentage: Some(100.0),
            total_steps: Some(5),
            enqueued_steps: None,
            health_status: Some("healthy".to_string()),
            reason: None,
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::Completed));
        assert_eq!(result.completion_percentage, Some(100.0));
        assert_eq!(result.total_steps, Some(5));
        assert!(result.reason.is_none());
    }

    #[test]
    fn test_finalization_result_structure_for_error() {
        // Test that FinalizationResult has correct structure for error state
        let task_uuid = Uuid::new_v4();
        let result = FinalizationResult {
            task_uuid,
            action: FinalizationAction::Failed,
            completion_percentage: Some(75.0),
            total_steps: Some(10),
            enqueued_steps: None,
            health_status: Some("degraded".to_string()),
            reason: Some("Steps in error state".to_string()),
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert!(matches!(result.action, FinalizationAction::Failed));
        assert_eq!(result.completion_percentage, Some(75.0));
        assert_eq!(result.total_steps, Some(10));
        assert_eq!(result.reason, Some("Steps in error state".to_string()));
    }

    /// TAS-73: Test that concurrent finalization attempts are handled gracefully.
    ///
    /// This test verifies the atomic finalization behavior by:
    /// 1. Creating a task in EvaluatingResults state
    /// 2. Running two concurrent complete_task calls
    /// 3. Verifying both succeed (no errors)
    /// 4. Verifying only one transition to Complete exists
    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_concurrent_finalization_is_graceful(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use tasker_shared::models::core::task_transition::{NewTaskTransition, TaskTransition};

        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let handler = CompletionHandler::new(context.clone());

        // Create a task in the database
        let task_uuid = Uuid::now_v7();
        let named_task_uuid = Uuid::now_v7();

        // First, create the namespace and named_task (required for foreign keys)
        sqlx::query!(
            r#"
            INSERT INTO tasker.task_namespaces (name, description, created_at, updated_at)
            VALUES ('test_namespace', 'Test namespace', NOW(), NOW())
            ON CONFLICT (name) DO NOTHING
            "#
        )
        .execute(&pool)
        .await?;

        let namespace_id = sqlx::query_scalar!(
            "SELECT task_namespace_uuid FROM tasker.task_namespaces WHERE name = 'test_namespace'"
        )
        .fetch_one(&pool)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO tasker.named_tasks (named_task_uuid, task_namespace_uuid, name, description, version, created_at, updated_at)
            VALUES ($1, $2, 'test_task', 'Test task', 1, NOW(), NOW())
            ON CONFLICT (task_namespace_uuid, name, version) DO UPDATE SET named_task_uuid = $1
            "#,
            named_task_uuid,
            namespace_id
        )
        .execute(&pool)
        .await?;

        // Create the task
        sqlx::query!(
            r#"
            INSERT INTO tasker.tasks (task_uuid, named_task_uuid, complete, requested_at, identity_hash, priority, created_at, updated_at, correlation_id)
            VALUES ($1, $2, false, NOW(), $3, 0, NOW(), NOW(), $4)
            "#,
            task_uuid,
            named_task_uuid,
            format!("test_identity_hash_{}", task_uuid),
            Uuid::now_v7()
        )
        .execute(&pool)
        .await?;

        // Create a transition to put the task in EvaluatingResults state
        TaskTransition::create(
            &pool,
            NewTaskTransition {
                task_uuid,
                to_state: "evaluating_results".to_string(),
                from_state: Some("steps_in_process".to_string()),
                processor_uuid: Some(Uuid::new_v4()),
                metadata: Some(serde_json::json!({"setup": "test"})),
            },
        )
        .await?;

        // Verify initial state
        let initial_transition = TaskTransition::get_current(&pool, task_uuid).await?;
        assert!(initial_transition.is_some());
        assert_eq!(initial_transition.unwrap().to_state, "evaluating_results");

        // Create a Task struct for the handler
        let task = Task {
            task_uuid,
            named_task_uuid,
            complete: false,
            requested_at: chrono::Utc::now().naive_utc(),
            initiator: None,
            source_system: None,
            reason: None,
            tags: None,
            context: None,
            identity_hash: format!("test_identity_hash_{}", task_uuid),
            priority: 0,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
        };

        // Run two concurrent complete_task calls
        let handler1 = handler.clone();
        let handler2 = handler.clone();
        let task1 = task.clone();
        let task2 = task.clone();

        let (result1, result2) = tokio::join!(
            handler1.complete_task(task1, None, Uuid::new_v4()),
            handler2.complete_task(task2, None, Uuid::new_v4()),
        );

        // Both should succeed - no errors!
        assert!(
            result1.is_ok(),
            "First finalization should succeed: {:?}",
            result1.err()
        );
        assert!(
            result2.is_ok(),
            "Second finalization should succeed (gracefully): {:?}",
            result2.err()
        );

        // Both results should indicate completion
        let result1 = result1.unwrap();
        let result2 = result2.unwrap();
        assert!(matches!(result1.action, FinalizationAction::Completed));
        assert!(matches!(result2.action, FinalizationAction::Completed));

        // Verify final state is Complete
        let final_transition = TaskTransition::get_current(&pool, task_uuid).await?;
        assert!(final_transition.is_some());
        assert_eq!(
            final_transition.unwrap().to_state,
            "complete",
            "Task should be in Complete state"
        );

        // Verify only ONE transition to Complete exists (not two)
        let all_transitions = TaskTransition::list_by_task(&pool, task_uuid).await?;
        let complete_transitions: Vec<_> = all_transitions
            .iter()
            .filter(|t| t.to_state == "complete")
            .collect();
        assert_eq!(
            complete_transitions.len(),
            1,
            "Should have exactly one transition to Complete, not two"
        );

        Ok(())
    }
}
