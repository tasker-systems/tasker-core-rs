//! # Step Execution Batch Model
//!
//! Main batch execution tracking for ZeroMQ dual result pattern architecture.
//!
//! ## Overview
//!
//! The `StepExecutionBatch` model represents a batch of workflow steps sent for
//! execution via ZeroMQ. It serves as the primary tracking unit for the dual
//! result pattern, correlating database records with ZeroMQ messages through
//! the batch_uuid field.
//!
//! ## Key Features
//!
//! - **UUID Correlation**: Links database records to ZeroMQ messages
//! - **Handler Tracking**: Records which handler class processes the batch
//! - **Timeout Management**: Configurable execution timeouts per batch
//! - **Metadata Storage**: JSONB field for flexible batch context
//! - **State Machine Integration**: Full lifecycle tracking via transitions
//!
//! ## Database Schema
//!
//! Maps to `tasker_step_execution_batches` table:
//! ```sql
//! CREATE TABLE tasker_step_execution_batches (
//!   batch_id BIGSERIAL PRIMARY KEY,
//!   task_id BIGINT NOT NULL,
//!   handler_class VARCHAR NOT NULL,
//!   batch_uuid VARCHAR NOT NULL UNIQUE,
//!   batch_size INTEGER DEFAULT 0,
//!   timeout_seconds INTEGER DEFAULT 30,
//!   metadata JSONB,
//!   -- ... timestamps
//! );
//! ```
//!
//! ## Relationships
//!
//! - **Belongs to Task**: Each batch executes steps for a single task
//! - **Has Many BatchSteps**: HABTM relationship with workflow steps
//! - **Has Many Transitions**: State machine tracking for batch lifecycle
//!
//! ## Performance Characteristics
//!
//! - **UUID Lookups**: O(1) via unique index on batch_uuid
//! - **Task Queries**: Indexed on task_id for batch listing
//! - **Time-based Queries**: Indexed on created_at for cleanup/monitoring

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Represents a batch of workflow steps sent for execution via ZeroMQ.
///
/// Each batch tracks multiple workflow steps executing together, with
/// correlation between database records and ZeroMQ messages via batch_uuid.
///
/// # Batch Lifecycle
///
/// 1. **Created**: Batch assembled with workflow steps
/// 2. **Published**: Sent to ZeroMQ for execution
/// 3. **Executing**: Workers processing individual steps
/// 4. **Completed**: All steps finished (success or failure)
///
/// # UUID Correlation
///
/// The `batch_uuid` field provides the critical link between:
/// - Database records (this model and related tables)
/// - ZeroMQ messages (StepBatchRequest/Response)
/// - Worker execution contexts
///
/// # Metadata Structure
///
/// The metadata field can contain:
/// ```json
/// {
///   "priority": "high",
///   "requested_by": "scheduler",
///   "execution_strategy": "parallel",
///   "max_workers": 5
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct StepExecutionBatch {
    pub batch_id: i64,
    pub task_id: i64,
    pub handler_class: String,
    pub batch_uuid: String,
    pub initiated_by: Option<String>,
    pub batch_size: i32,
    pub timeout_seconds: i32,
    pub metadata: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New batch for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStepExecutionBatch {
    pub task_id: i64,
    pub handler_class: String,
    pub batch_uuid: String,
    pub initiated_by: Option<String>,
    pub batch_size: i32,
    pub timeout_seconds: i32,
    pub metadata: Option<serde_json::Value>,
}

impl StepExecutionBatch {
    /// Find a batch by its ID
    pub async fn find_by_id(pool: &PgPool, batch_id: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT batch_id, task_id, handler_class, batch_uuid,
                   initiated_by, batch_size as "batch_size!", timeout_seconds as "timeout_seconds!",
                   metadata, created_at, updated_at
            FROM tasker_step_execution_batches
            WHERE batch_id = $1
            "#,
            batch_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Find a batch by its UUID (primary lookup for ZeroMQ correlation)
    pub async fn find_by_uuid(
        pool: &PgPool,
        batch_uuid: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT batch_id, task_id, handler_class, batch_uuid,
                   initiated_by, batch_size as "batch_size!", timeout_seconds as "timeout_seconds!",
                   metadata, created_at, updated_at
            FROM tasker_step_execution_batches
            WHERE batch_uuid = $1
            "#,
            batch_uuid
        )
        .fetch_optional(pool)
        .await
    }

    /// Create a new batch
    pub async fn create(
        pool: &PgPool,
        new_batch: NewStepExecutionBatch,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            INSERT INTO tasker_step_execution_batches 
            (task_id, handler_class, batch_uuid, initiated_by, batch_size, timeout_seconds, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING batch_id, task_id, handler_class, batch_uuid,
                      initiated_by, batch_size as "batch_size!", timeout_seconds as "timeout_seconds!",
                      metadata, created_at, updated_at
            "#,
            new_batch.task_id,
            new_batch.handler_class,
            new_batch.batch_uuid,
            new_batch.initiated_by,
            new_batch.batch_size,
            new_batch.timeout_seconds,
            new_batch.metadata
        )
        .fetch_one(pool)
        .await
    }

    /// Generate a new batch UUID for ZeroMQ correlation
    pub fn generate_batch_uuid() -> String {
        Uuid::new_v4().to_string()
    }

    /// Find all batches for a task
    pub async fn find_by_task(pool: &PgPool, task_id: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT batch_id, task_id, handler_class, batch_uuid,
                   initiated_by, batch_size as "batch_size!", timeout_seconds as "timeout_seconds!",
                   metadata, created_at, updated_at
            FROM tasker_step_execution_batches
            WHERE task_id = $1
            ORDER BY created_at DESC
            "#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find batches that may have timed out
    pub async fn find_potentially_timed_out(
        pool: &PgPool,
        default_timeout_seconds: i32,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT b.batch_id as "batch_id!", b.task_id as "task_id!", b.handler_class as "handler_class!", b.batch_uuid as "batch_uuid!",
                   b.initiated_by, b.batch_size as "batch_size!", b.timeout_seconds as "timeout_seconds!",
                   b.metadata, b.created_at as "created_at!", b.updated_at as "updated_at!"
            FROM tasker_step_execution_batches b
            LEFT JOIN tasker_step_execution_batch_transitions t
                ON t.batch_id = b.batch_id AND t.most_recent = true
            WHERE (t.to_state IS NULL OR t.to_state NOT IN ('completed', 'failed', 'cancelled'))
              AND b.created_at < NOW() - INTERVAL '1 second' * COALESCE(b.timeout_seconds, $1)
            "#,
            default_timeout_seconds
        )
        .fetch_all(pool)
        .await
    }

    /// Update batch metadata
    pub async fn update_metadata(
        &mut self,
        pool: &PgPool,
        metadata: serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let updated = sqlx::query!(
            r#"
            UPDATE tasker_step_execution_batches
            SET metadata = $1, updated_at = NOW()
            WHERE batch_id = $2
            RETURNING updated_at
            "#,
            metadata,
            self.batch_id
        )
        .fetch_one(pool)
        .await?;

        self.metadata = Some(metadata);
        self.updated_at = updated.updated_at;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_uuid_generation() {
        let uuid1 = StepExecutionBatch::generate_batch_uuid();
        let uuid2 = StepExecutionBatch::generate_batch_uuid();

        // Should generate unique UUIDs
        assert_ne!(uuid1, uuid2);

        // Should be valid UUID format
        assert!(Uuid::parse_str(&uuid1).is_ok());
        assert!(Uuid::parse_str(&uuid2).is_ok());
    }
}
