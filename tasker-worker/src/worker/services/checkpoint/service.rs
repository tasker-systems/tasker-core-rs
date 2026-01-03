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
            UPDATE tasker_workflow_steps
            SET checkpoint = jsonb_set(
                $2::jsonb,
                '{history}',
                COALESCE(
                    (SELECT checkpoint->'history' FROM tasker_workflow_steps WHERE workflow_step_uuid = $1::uuid),
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
            UPDATE tasker_workflow_steps
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
            FROM tasker_workflow_steps
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
}
