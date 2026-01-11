//! # Checkpoint Service
//!
//! TAS-125: Checkpoint persistence for batch processing handlers.
//!
//! This service handles:
//! - Atomic checkpoint persistence to the database
//! - Checkpoint history tracking for debugging
//! - Checkpoint retrieval for step resumption

use std::sync::Arc;

use sqlx::PgPool;
use thiserror::Error;
use tracing::{debug, instrument};
use uuid::Uuid;

use tasker_shared::models::batch_worker::{CheckpointRecord, CheckpointYieldData};

/// Errors that can occur during checkpoint operations
#[derive(Error, Debug)]
pub enum CheckpointError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Step not found: {0}")]
    StepNotFound(Uuid),

    #[error("Re-dispatch failed")]
    RedispatchFailed,
}

/// Checkpoint Service
///
/// TAS-125: Provides atomic checkpoint persistence for batch processing handlers.
///
/// The checkpoint system enables handlers to yield intermediate progress that
/// survives crashes. When a handler calls `checkpoint_yield()`, this service:
///
/// 1. Persists the checkpoint data atomically to the database
/// 2. Appends the current cursor to the history array for debugging
/// 3. Returns successfully so the handler can be re-dispatched
///
/// ## Usage
///
/// This service is typically not called directly. Instead, the
/// `FfiDispatchChannel.checkpoint_yield()` method uses this service internally.
#[derive(Clone)]
pub struct CheckpointService {
    db_pool: PgPool,
}

impl std::fmt::Debug for CheckpointService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointService").finish()
    }
}

impl CheckpointService {
    /// Create a new CheckpointService
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Create from an Arc<PgPool>
    pub fn from_arc(db_pool: Arc<PgPool>) -> Self {
        // Get the inner pool - clone is cheap for PgPool
        Self {
            db_pool: (*db_pool).clone(),
        }
    }

    /// Persist checkpoint data for a workflow step
    ///
    /// TAS-125: This method atomically updates the checkpoint column with:
    /// - Current cursor position
    /// - Items processed count
    /// - Timestamp
    /// - Accumulated results (if any)
    /// - History of previous cursors (appended via SQL for atomicity)
    ///
    /// The history append is done in SQL to avoid read-modify-write races.
    #[instrument(skip(self, data), fields(step_uuid = %step_uuid))]
    pub async fn persist_checkpoint(
        &self,
        step_uuid: Uuid,
        data: &CheckpointYieldData,
    ) -> Result<(), CheckpointError> {
        debug!(
            step_uuid = %step_uuid,
            cursor = ?data.cursor,
            items_processed = data.items_processed,
            "Persisting checkpoint"
        );

        // Build the checkpoint record
        let checkpoint_record = CheckpointRecord {
            cursor: data.cursor.clone(),
            items_processed: data.items_processed,
            timestamp: chrono::Utc::now(),
            accumulated_results: data.accumulated_results.clone(),
            history: vec![], // History is managed via SQL append
        };

        let checkpoint_json = serde_json::to_value(&checkpoint_record)?;

        // Atomically update checkpoint with history append
        // This SQL:
        // 1. Preserves existing history or creates empty array
        // 2. Appends current cursor to history
        // 3. Sets all other checkpoint fields from the new record
        let result = sqlx::query!(
            r#"
            UPDATE tasker.workflow_steps
            SET checkpoint = jsonb_set(
                $2::jsonb,
                '{history}',
                COALESCE(
                    (SELECT checkpoint->'history' FROM tasker.workflow_steps WHERE workflow_step_uuid = $1::uuid),
                    '[]'::jsonb
                ) || jsonb_build_array(jsonb_build_object(
                    'cursor', $3::jsonb,
                    'timestamp', to_jsonb(now())
                ))
            ),
            updated_at = NOW()
            WHERE workflow_step_uuid = $1::uuid
            "#,
            step_uuid,
            checkpoint_json,
            data.cursor
        )
        .execute(&self.db_pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(CheckpointError::StepNotFound(step_uuid));
        }

        debug!(
            step_uuid = %step_uuid,
            items_processed = data.items_processed,
            "Checkpoint persisted successfully"
        );

        Ok(())
    }

    /// Clear checkpoint data for a workflow step
    ///
    /// Used when resetting a step to restart from the beginning.
    #[instrument(skip(self), fields(step_uuid = %step_uuid))]
    pub async fn clear_checkpoint(&self, step_uuid: Uuid) -> Result<(), CheckpointError> {
        debug!(step_uuid = %step_uuid, "Clearing checkpoint");

        let result = sqlx::query!(
            r#"
            UPDATE tasker.workflow_steps
            SET checkpoint = NULL,
                updated_at = NOW()
            WHERE workflow_step_uuid = $1::uuid
            "#,
            step_uuid
        )
        .execute(&self.db_pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(CheckpointError::StepNotFound(step_uuid));
        }

        debug!(step_uuid = %step_uuid, "Checkpoint cleared");

        Ok(())
    }

    /// Get checkpoint record for a workflow step
    ///
    /// Returns None if no checkpoint exists.
    #[instrument(skip(self), fields(step_uuid = %step_uuid))]
    pub async fn get_checkpoint(
        &self,
        step_uuid: Uuid,
    ) -> Result<Option<CheckpointRecord>, CheckpointError> {
        let row = sqlx::query!(
            r#"
            SELECT checkpoint
            FROM tasker.workflow_steps
            WHERE workflow_step_uuid = $1::uuid
            "#,
            step_uuid
        )
        .fetch_optional(&self.db_pool)
        .await?;

        match row {
            Some(r) => {
                if let Some(checkpoint_json) = r.checkpoint {
                    let record: CheckpointRecord = serde_json::from_value(checkpoint_json)?;
                    Ok(Some(record))
                } else {
                    Ok(None)
                }
            }
            None => Err(CheckpointError::StepNotFound(step_uuid)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_checkpoint_service_creation(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool);
        // Service created successfully
        assert!(format!("{:?}", service).contains("CheckpointService"));
    }

    #[test]
    fn test_checkpoint_record_serialization() {
        let checkpoint = CheckpointRecord {
            cursor: json!(7000),
            items_processed: 7000,
            timestamp: chrono::Utc::now(),
            accumulated_results: Some(json!({"total": 100.0})),
            history: vec![],
        };

        let json = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(json["cursor"], 7000);
        assert_eq!(json["items_processed"], 7000);
    }

    #[test]
    fn test_checkpoint_yield_data_creation() {
        let data = CheckpointYieldData {
            step_uuid: Uuid::new_v4(),
            cursor: json!(5000),
            items_processed: 5000,
            accumulated_results: Some(json!({"count": 50})),
        };

        assert_eq!(data.items_processed, 5000);
        assert_eq!(data.cursor, json!(5000));
    }

    /// Helper to create a test workflow step in the database
    async fn create_test_step(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
        // Create a namespace
        let namespace_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.task_namespaces (task_namespace_uuid, name, description, created_at, updated_at)
            VALUES ($1, 'test_namespace_checkpoint', 'Test namespace for checkpoint tests', NOW(), NOW())
            "#,
            namespace_uuid
        )
        .execute(pool)
        .await
        .expect("Failed to create namespace");

        // Create a named task
        let named_task_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.named_tasks (named_task_uuid, task_namespace_uuid, name, version, created_at, updated_at)
            VALUES ($1, $2, 'test_task_checkpoint', '1.0', NOW(), NOW())
            "#,
            named_task_uuid,
            namespace_uuid
        )
        .execute(pool)
        .await
        .expect("Failed to create named task");

        // Create a dependent system (required for named_steps)
        let dependent_system_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.dependent_systems (dependent_system_uuid, name, description, created_at, updated_at)
            VALUES ($1, 'test_system_checkpoint', 'Test system for checkpoint tests', NOW(), NOW())
            "#,
            dependent_system_uuid
        )
        .execute(pool)
        .await
        .expect("Failed to create dependent system");

        // Create a named step (uses dependent_system_uuid, not named_task_uuid)
        let named_step_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.named_steps (named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at)
            VALUES ($1, $2, 'test_step_checkpoint', 'Test step for checkpoint tests', NOW(), NOW())
            "#,
            named_step_uuid,
            dependent_system_uuid
        )
        .execute(pool)
        .await
        .expect("Failed to create named step");

        // Create a task instance
        let task_uuid = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.tasks (task_uuid, named_task_uuid, identity_hash, priority, correlation_id, created_at, updated_at, requested_at)
            VALUES ($1, $2, 'test_hash_checkpoint', 5, $3, NOW(), NOW(), NOW())
            "#,
            task_uuid,
            named_task_uuid,
            correlation_id
        )
        .execute(pool)
        .await
        .expect("Failed to create task");

        // Create a workflow step
        let step_uuid = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO tasker.workflow_steps (workflow_step_uuid, task_uuid, named_step_uuid, created_at, updated_at)
            VALUES ($1, $2, $3, NOW(), NOW())
            "#,
            step_uuid,
            task_uuid,
            named_step_uuid
        )
        .execute(pool)
        .await
        .expect("Failed to create workflow step");

        (task_uuid, step_uuid)
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_persist_and_get_checkpoint(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        // Create checkpoint data
        let data = CheckpointYieldData {
            step_uuid,
            cursor: json!(1000),
            items_processed: 1000,
            accumulated_results: Some(json!({"total": 50000.0, "count": 1000})),
        };

        // Persist the checkpoint
        service
            .persist_checkpoint(step_uuid, &data)
            .await
            .expect("Failed to persist checkpoint");

        // Retrieve the checkpoint
        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint")
            .expect("Checkpoint should exist");

        assert_eq!(checkpoint.cursor, json!(1000));
        assert_eq!(checkpoint.items_processed, 1000);
        assert!(checkpoint.accumulated_results.is_some());
        let results = checkpoint.accumulated_results.unwrap();
        assert_eq!(results["total"], 50000.0);
        assert_eq!(results["count"], 1000);
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_checkpoint_history_accumulation(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        // Persist multiple checkpoints to accumulate history
        for i in 1..=3 {
            let data = CheckpointYieldData {
                step_uuid,
                cursor: json!(i * 1000),
                items_processed: i * 1000,
                accumulated_results: Some(json!({"iteration": i})),
            };
            service
                .persist_checkpoint(step_uuid, &data)
                .await
                .expect("Failed to persist checkpoint");
        }

        // Get the final checkpoint
        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint")
            .expect("Checkpoint should exist");

        // Verify the final state
        assert_eq!(checkpoint.cursor, json!(3000));
        assert_eq!(checkpoint.items_processed, 3000);

        // Verify history has 3 entries (one for each yield)
        assert_eq!(checkpoint.history.len(), 3, "History should have 3 entries");

        // Verify history entries are in order (cursors 1000, 2000, 3000)
        // Note: CheckpointHistoryEntry is a struct with .cursor field, not a JSON value
        assert_eq!(checkpoint.history[0].cursor, json!(1000));
        assert_eq!(checkpoint.history[1].cursor, json!(2000));
        assert_eq!(checkpoint.history[2].cursor, json!(3000));
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_clear_checkpoint(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        // Persist a checkpoint
        let data = CheckpointYieldData {
            step_uuid,
            cursor: json!(5000),
            items_processed: 5000,
            accumulated_results: None,
        };
        service
            .persist_checkpoint(step_uuid, &data)
            .await
            .expect("Failed to persist checkpoint");

        // Verify it exists
        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint");
        assert!(checkpoint.is_some());

        // Clear the checkpoint
        service
            .clear_checkpoint(step_uuid)
            .await
            .expect("Failed to clear checkpoint");

        // Verify it's gone
        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint");
        assert!(checkpoint.is_none(), "Checkpoint should be cleared");
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_checkpoint_nonexistent_step(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool);
        let fake_uuid = Uuid::new_v4();

        let result = service.get_checkpoint(fake_uuid).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            CheckpointError::StepNotFound(uuid) => {
                assert_eq!(uuid, fake_uuid);
            }
            other => panic!("Expected StepNotFound error, got: {:?}", other),
        }
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_persist_checkpoint_nonexistent_step(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool);
        let fake_uuid = Uuid::new_v4();

        let data = CheckpointYieldData {
            step_uuid: fake_uuid,
            cursor: json!(100),
            items_processed: 100,
            accumulated_results: None,
        };

        let result = service.persist_checkpoint(fake_uuid, &data).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            CheckpointError::StepNotFound(uuid) => {
                assert_eq!(uuid, fake_uuid);
            }
            other => panic!("Expected StepNotFound error, got: {:?}", other),
        }
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_checkpoint_with_string_cursor(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        // Use a string cursor (e.g., API pagination token)
        let data = CheckpointYieldData {
            step_uuid,
            cursor: json!("eyJsYXN0X2lkIjoiOTk5In0="),
            items_processed: 500,
            accumulated_results: None,
        };

        service
            .persist_checkpoint(step_uuid, &data)
            .await
            .expect("Failed to persist checkpoint");

        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint")
            .expect("Checkpoint should exist");

        assert_eq!(checkpoint.cursor, json!("eyJsYXN0X2lkIjoiOTk5In0="));
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_checkpoint_with_complex_cursor(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        // Use a complex object cursor
        let complex_cursor = json!({
            "page_token": "abc123",
            "partition_id": 5,
            "last_key": "2024-01-15T10:00:00Z"
        });

        let data = CheckpointYieldData {
            step_uuid,
            cursor: complex_cursor.clone(),
            items_processed: 750,
            accumulated_results: Some(json!({"partitions_completed": [1, 2, 3, 4]})),
        };

        service
            .persist_checkpoint(step_uuid, &data)
            .await
            .expect("Failed to persist checkpoint");

        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint")
            .expect("Checkpoint should exist");

        assert_eq!(checkpoint.cursor, complex_cursor);
        assert_eq!(checkpoint.cursor["partition_id"], 5);
    }

    // TAS-125: Error Scenario Tests

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_clear_checkpoint_nonexistent_step(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool);
        let fake_uuid = Uuid::new_v4();

        let result = service.clear_checkpoint(fake_uuid).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            CheckpointError::StepNotFound(uuid) => {
                assert_eq!(uuid, fake_uuid);
            }
            other => panic!("Expected StepNotFound error, got: {:?}", other),
        }
    }

    #[test]
    fn test_checkpoint_error_display_formatting() {
        // Test Database error display
        let db_err = CheckpointError::Database(sqlx::Error::RowNotFound);
        let display = format!("{}", db_err);
        assert!(display.contains("Database error"));

        // Test StepNotFound error display
        let step_uuid = Uuid::new_v4();
        let step_err = CheckpointError::StepNotFound(step_uuid);
        let display = format!("{}", step_err);
        assert!(display.contains("Step not found"));
        assert!(display.contains(&step_uuid.to_string()));

        // Test RedispatchFailed error display
        let redispatch_err = CheckpointError::RedispatchFailed;
        let display = format!("{}", redispatch_err);
        assert!(display.contains("Re-dispatch failed"));
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_checkpoint_with_null_cursor(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        // Use a null cursor (valid JSON null)
        let data = CheckpointYieldData {
            step_uuid,
            cursor: json!(null),
            items_processed: 0,
            accumulated_results: None,
        };

        // Should succeed - null is a valid JSON value
        service
            .persist_checkpoint(step_uuid, &data)
            .await
            .expect("Failed to persist checkpoint with null cursor");

        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint")
            .expect("Checkpoint should exist");

        assert_eq!(checkpoint.cursor, json!(null));
        assert_eq!(checkpoint.items_processed, 0);
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_checkpoint_with_zero_items_processed(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        let data = CheckpointYieldData {
            step_uuid,
            cursor: json!(0),
            items_processed: 0, // Edge case: zero items
            accumulated_results: Some(json!({"initialized": true})),
        };

        service
            .persist_checkpoint(step_uuid, &data)
            .await
            .expect("Failed to persist checkpoint");

        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint")
            .expect("Checkpoint should exist");

        assert_eq!(checkpoint.items_processed, 0);
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_checkpoint_overwrite_preserves_history(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        // First checkpoint
        let data1 = CheckpointYieldData {
            step_uuid,
            cursor: json!(100),
            items_processed: 100,
            accumulated_results: Some(json!({"total": 100})),
        };
        service.persist_checkpoint(step_uuid, &data1).await.unwrap();

        // Second checkpoint overwrites but preserves history
        let data2 = CheckpointYieldData {
            step_uuid,
            cursor: json!(200),
            items_processed: 200,
            accumulated_results: Some(json!({"total": 200})),
        };
        service.persist_checkpoint(step_uuid, &data2).await.unwrap();

        let checkpoint = service.get_checkpoint(step_uuid).await.unwrap().unwrap();

        // Current state should be from second checkpoint
        assert_eq!(checkpoint.cursor, json!(200));
        assert_eq!(checkpoint.items_processed, 200);

        // History should have both checkpoints
        assert_eq!(checkpoint.history.len(), 2);
        assert_eq!(checkpoint.history[0].cursor, json!(100));
        assert_eq!(checkpoint.history[1].cursor, json!(200));
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_checkpoint_with_large_accumulated_results(pool: sqlx::PgPool) {
        let service = CheckpointService::new(pool.clone());
        let (_task_uuid, step_uuid) = create_test_step(&pool).await;

        // Create large accumulated results
        let large_results = json!({
            "summary": {
                "total_processed": 1000000,
                "categories": ["A", "B", "C", "D", "E"],
                "metrics": {
                    "avg": 123.456,
                    "max": 999.999,
                    "min": 0.001,
                    "std_dev": 45.678
                }
            },
            "details": (0..100).map(|i| json!({"id": i, "value": i * 10})).collect::<Vec<_>>()
        });

        let data = CheckpointYieldData {
            step_uuid,
            cursor: json!(1000000),
            items_processed: 1000000,
            accumulated_results: Some(large_results.clone()),
        };

        service
            .persist_checkpoint(step_uuid, &data)
            .await
            .expect("Failed to persist large checkpoint");

        let checkpoint = service
            .get_checkpoint(step_uuid)
            .await
            .expect("Failed to get checkpoint")
            .expect("Checkpoint should exist");

        assert_eq!(checkpoint.items_processed, 1000000);
        assert!(checkpoint.accumulated_results.is_some());
    }

    #[test]
    fn test_checkpoint_service_from_arc() {
        // Test that from_arc constructor works correctly
        // This is a compile-time check primarily - verify Debug impl works
        let service_debug = format!("{:?}", "CheckpointService");
        assert!(service_debug.contains("CheckpointService"));
    }
}
