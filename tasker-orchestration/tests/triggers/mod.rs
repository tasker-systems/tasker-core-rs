//! Database trigger testing and validation for TAS-43 task readiness system

use sqlx::PgPool;
use tracing::{debug, info};
use uuid::Uuid;

use tasker_shared::{TaskerError, TaskerResult};

/// Test utilities for creating database test contexts
pub struct DatabaseTestContext {
    pool: PgPool,
}

impl DatabaseTestContext {
    /// Setup a test database context
    pub async fn setup(pool: &PgPool) -> TaskerResult<Self> {
        Ok(Self { pool: pool.clone() })
    }

    /// Create a test task with namespace
    pub async fn create_test_task(&self, namespace: &str, task_name: &str) -> TaskerResult<Uuid> {
        // Create namespace if it doesn't exist
        let namespace_uuid = self.create_namespace(namespace).await?;

        // Create named task
        let named_task_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.named_tasks (named_task_uuid, task_namespace_uuid, name, version, description)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            named_task_uuid,
            namespace_uuid,
            task_name,
            "1.0.0",
            "Test task for trigger validation"
        )
        .execute(&self.pool)
        .await?;

        // Create task
        let task_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.tasks (task_uuid, named_task_uuid, priority, context, complete)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            task_uuid,
            named_task_uuid,
            2, // Normal priority
            serde_json::json!({"test": true}),
            false
        )
        .execute(&self.pool)
        .await?;

        Ok(task_uuid)
    }

    /// Create a workflow step for testing
    pub async fn create_workflow_step(&self, task_uuid: Uuid) -> TaskerResult<Uuid> {
        let step_uuid = Uuid::new_v4();

        // Create named step first
        let named_step_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.named_steps (named_step_uuid, name, description)
            VALUES ($1, $2, $3)
            "#,
            named_step_uuid,
            "test_step",
            "Test step for trigger validation"
        )
        .execute(&self.pool)
        .await?;

        // Create workflow step
        sqlx::query!(
            r#"
            INSERT INTO tasker.workflow_steps (workflow_step_uuid, task_uuid, named_step_uuid, step_payload, current_state)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            step_uuid,
            task_uuid,
            named_step_uuid,
            serde_json::json!({"test_data": "value"}),
            "pending"
        )
        .execute(&self.pool)
        .await?;

        Ok(step_uuid)
    }

    /// Transition a step to complete state to trigger notifications
    pub async fn transition_step_to_complete(&self, step_uuid: Uuid) -> TaskerResult<()> {
        // Insert step transition record
        sqlx::query!(
            r#"
            INSERT INTO tasker.workflow_step_transitions (workflow_step_uuid, from_state, to_state)
            VALUES ($1, $2, $3)
            "#,
            step_uuid,
            "pending",
            "complete"
        )
        .execute(&self.pool)
        .await?;

        // Update the workflow step state
        sqlx::query!(
            r#"
            UPDATE tasker.workflow_steps
            SET current_state = $2
            WHERE workflow_step_uuid = $1
            "#,
            step_uuid,
            "complete"
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Transition a task to complete state
    pub async fn transition_task_to_complete(&self, task_uuid: Uuid) -> TaskerResult<()> {
        // Insert task transition record
        sqlx::query!(
            r#"
            INSERT INTO tasker.task_transitions (task_uuid, from_state, to_state)
            VALUES ($1, $2, $3)
            "#,
            task_uuid,
            "in_progress",
            "complete"
        )
        .execute(&self.pool)
        .await?;

        // Update the task state
        sqlx::query!(
            r#"
            UPDATE tasker.tasks
            SET complete = true
            WHERE task_uuid = $1
            "#,
            task_uuid
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Create a test namespace
    pub async fn create_namespace(&self, name: &str) -> TaskerResult<Uuid> {
        let namespace_uuid = Uuid::new_v4();

        sqlx::query!(
            r#"
            INSERT INTO tasker.task_namespaces (task_namespace_uuid, name, description)
            VALUES ($1, $2, $3)
            ON CONFLICT (name) DO NOTHING
            "#,
            namespace_uuid,
            name,
            format!("Test namespace: {}", name)
        )
        .execute(&self.pool)
        .await?;

        // Get the actual UUID (in case of conflict)
        let result = sqlx::query!(
            "SELECT task_namespace_uuid FROM tasker.task_namespaces WHERE name = $1",
            name
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.task_namespace_uuid)
    }

    /// Create a ready task for fallback polling tests
    pub async fn create_ready_task(&self, namespace: &str, task_name: &str) -> TaskerResult<Uuid> {
        let task_uuid = self.create_test_task(namespace, task_name).await?;
        let step_uuid = self.create_workflow_step(task_uuid).await?;

        // Set task to in_progress so it shows up in tasker_ready_tasks view
        sqlx::query!(
            r#"
            INSERT INTO tasker.task_transitions (task_uuid, from_state, to_state)
            VALUES ($1, $2, $3)
            "#,
            task_uuid,
            "pending",
            "in_progress"
        )
        .execute(&self.pool)
        .await?;

        Ok(task_uuid)
    }
}

/// Test suite for database triggers
pub struct TriggerTestSuite {
    pool: PgPool,
}

impl TriggerTestSuite {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Test that step transition triggers fire correctly
    pub async fn test_step_transition_triggers(&self) -> TaskerResult<()> {
        info!("Testing step transition trigger notifications");

        // Create test task with dependencies
        let test_context = DatabaseTestContext::setup(&self.pool).await?;
        let task_uuid = test_context
            .create_test_task("test_namespace", "test_task")
            .await?;

        // Create test listener for pg_notify
        let mut listener = sqlx::postgres::PgListener::connect_with(&self.pool).await?;
        listener.listen("task_ready").await?;
        listener.listen("task_ready.test_namespace").await?;

        // Simulate step completion that should trigger readiness
        let step_uuid = test_context.create_workflow_step(task_uuid).await?;
        test_context.transition_step_to_complete(step_uuid).await?;

        // Check for notifications
        tokio::select! {
            notification = listener.recv() => {
                match notification {
                    Ok(notif) => {
                        debug!("Received notification on channel {}: {}", notif.channel(), notif.payload());

                        // Validate payload structure
                        let payload: serde_json::Value = serde_json::from_str(notif.payload())?;
                        assert!(payload.get("task_uuid").is_some());
                        assert!(payload.get("namespace").is_some());
                        assert!(payload.get("triggered_by").is_some());

                        info!("Step transition trigger test passed");
                        Ok(())
                    }
                    Err(e) => Err(TaskerError::DatabaseError(format!("Listener error: {}", e)))
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                Err(TaskerError::ValidationError("No trigger notification received within timeout".to_string()))
            }
        }
    }

    /// Test task state change triggers
    pub async fn test_task_state_change_triggers(&self) -> TaskerResult<()> {
        info!("Testing task state change trigger notifications");

        let test_context = DatabaseTestContext::setup(&self.pool).await?;
        let mut listener = sqlx::postgres::PgListener::connect_with(&self.pool).await?;
        listener.listen("task_state_change").await?;

        // Create and transition task to complete
        let task_uuid = test_context
            .create_test_task("test_namespace", "test_task")
            .await?;
        test_context.transition_task_to_complete(task_uuid).await?;

        // Check for state change notifications
        tokio::select! {
            notification = listener.recv() => {
                match notification {
                    Ok(notif) => {
                        let payload: serde_json::Value = serde_json::from_str(notif.payload())?;
                        assert_eq!(payload["task_state"], "complete");
                        assert_eq!(payload["action_needed"], "finalization");

                        info!("Task state change trigger test passed");
                        Ok(())
                    }
                    Err(e) => Err(TaskerError::DatabaseError(format!("Listener error: {}", e)))
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                Err(TaskerError::ValidationError("No state change notification received".to_string()))
            }
        }
    }

    /// Test namespace creation triggers
    pub async fn test_namespace_creation_triggers(&self) -> TaskerResult<()> {
        info!("Testing namespace creation trigger notifications");

        let mut listener = sqlx::postgres::PgListener::connect_with(&self.pool).await?;
        listener.listen("namespace_created").await?;

        // Create new namespace
        let namespace_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.task_namespaces (task_namespace_uuid, name, description)
            VALUES ($1, $2, $3)
            "#,
            namespace_uuid,
            "test_new_namespace",
            "Test namespace for trigger validation"
        )
        .execute(&self.pool)
        .await?;

        // Check for namespace creation notification
        tokio::select! {
            notification = listener.recv() => {
                match notification {
                    Ok(notif) => {
                        let payload: serde_json::Value = serde_json::from_str(notif.payload())?;
                        assert_eq!(payload["namespace_name"], "test_new_namespace");
                        assert_eq!(payload["triggered_by"], "namespace_creation");

                        info!("Namespace creation trigger test passed");
                        Ok(())
                    }
                    Err(e) => Err(TaskerError::DatabaseError(format!("Listener error: {}", e)))
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                Err(TaskerError::ValidationError("No namespace creation notification received".to_string()))
            }
        }
    }

    /// Test trigger performance under load
    pub async fn test_trigger_performance(&self) -> TaskerResult<()> {
        info!("Testing trigger performance under load");

        let test_context = DatabaseTestContext::setup(&self.pool).await?;
        let mut listener = sqlx::postgres::PgListener::connect_with(&self.pool).await?;
        listener.listen("task_ready").await?;

        let start_time = std::time::Instant::now();
        let num_tasks = 100;

        // Create multiple tasks and steps rapidly
        let mut tasks = Vec::new();
        for i in 0..num_tasks {
            let task_uuid = test_context
                .create_test_task("perf_test", &format!("task_{}", i))
                .await?;
            let step_uuid = test_context.create_workflow_step(task_uuid).await?;
            tasks.push((task_uuid, step_uuid));
        }

        // Transition all steps to complete rapidly
        for (_, step_uuid) in &tasks {
            test_context.transition_step_to_complete(*step_uuid).await?;
        }

        let trigger_time = start_time.elapsed();

        // Count notifications received
        let mut notifications_received = 0;
        let notification_start = std::time::Instant::now();

        while notifications_received < num_tasks && notification_start.elapsed().as_secs() < 10 {
            tokio::select! {
                notification = listener.recv() => {
                    match notification {
                        Ok(_) => notifications_received += 1,
                        Err(e) => {
                            debug!("Listener error: {}", e);
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    break;
                }
            }
        }

        let total_time = notification_start.elapsed();

        info!(
            "Performance test completed: {} tasks processed in {:?}, {} notifications received in {:?}",
            num_tasks, trigger_time, notifications_received, total_time
        );

        // Validate reasonable performance
        if trigger_time.as_millis() > 1000 {
            return Err(TaskerError::ValidationError(format!(
                "Trigger processing took too long: {:?}",
                trigger_time
            )));
        }

        if notifications_received < num_tasks / 2 {
            return Err(TaskerError::ValidationError(format!(
                "Too few notifications received: {}/{}",
                notifications_received, num_tasks
            )));
        }

        info!("Trigger performance test passed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_all_triggers(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let test_suite = TriggerTestSuite::new(pool);

        test_suite.test_step_transition_triggers().await?;
        test_suite.test_task_state_change_triggers().await?;
        test_suite.test_namespace_creation_triggers().await?;
        test_suite.test_trigger_performance().await?;

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_database_context_utilities(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let test_context = DatabaseTestContext::setup(&pool).await?;

        // Test namespace creation
        let namespace_uuid = test_context.create_namespace("test_context").await?;
        assert!(!namespace_uuid.is_nil());

        // Test task creation
        let task_uuid = test_context
            .create_test_task("test_context", "context_task")
            .await?;
        assert!(!task_uuid.is_nil());

        // Test step creation
        let step_uuid = test_context.create_workflow_step(task_uuid).await?;
        assert!(!step_uuid.is_nil());

        // Test step transition
        test_context.transition_step_to_complete(step_uuid).await?;

        // Verify step state was updated
        let result = sqlx::query!(
            "SELECT current_state FROM tasker.workflow_steps WHERE workflow_step_uuid = $1",
            step_uuid
        )
        .fetch_one(&pool)
        .await?;

        assert_eq!(result.current_state, Some("complete".to_string()));

        Ok(())
    }
}
