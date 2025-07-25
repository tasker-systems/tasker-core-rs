//! # Step Execution Batch Received Result Model
//!
//! Append-only audit ledger for all ZeroMQ messages in the dual result pattern.
//!
//! ## Overview
//!
//! The `StepExecutionBatchReceivedResult` model provides a complete audit trail
//! of all messages received from ZeroMQ workers and orchestrators. This append-only
//! ledger supports reconciliation, debugging, and advanced analytics of the
//! dual result pattern.
//!
//! ## Key Features
//!
//! - **Append-Only Design**: Immutable audit trail for compliance
//! - **Complete Message Storage**: Raw JSON preserves full message content
//! - **Dual Message Types**: Tracks both partial results and batch completions
//! - **Worker Attribution**: Identifies which worker sent each message
//! - **Sequence Tracking**: Detects out-of-order or missing messages
//! - **Error Context**: Records any processing errors for debugging
//!
//! ## Database Schema
//!
//! Maps to `tasker_step_execution_batch_received_results` table:
//! ```sql
//! CREATE TABLE tasker_step_execution_batch_received_results (
//!   id BIGSERIAL PRIMARY KEY,
//!   batch_step_id BIGINT NOT NULL,
//!   message_type VARCHAR NOT NULL,
//!   worker_id VARCHAR,
//!   sequence_number INTEGER,
//!   status VARCHAR,
//!   execution_time_ms BIGINT,
//!   raw_message_json JSONB NOT NULL,
//!   processed_at TIMESTAMP,
//!   processing_errors JSONB,
//!   -- ... timestamps
//! );
//! ```
//!
//! ## Message Types
//!
//! - **partial_result**: Individual step completion from worker
//! - **batch_completion**: Final batch summary from orchestrator
//!
//! ## Reconciliation Support
//!
//! This table enables powerful reconciliation queries:
//! - Compare partial results vs batch completion summaries
//! - Detect missing partial results
//! - Identify discrepancies in execution times or statuses
//! - Analyze worker performance patterns

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Represents a received message from ZeroMQ in the audit ledger.
///
/// Each record captures a complete message with full context for
/// debugging, reconciliation, and compliance requirements.
///
/// # Append-Only Design
///
/// Records are never updated or deleted, providing:
/// - Complete audit trail for compliance
/// - Forensic analysis capabilities
/// - Protection against data loss
/// - Historical performance analytics
///
/// # Message Type Discrimination
///
/// The `message_type` field identifies the message pattern:
/// - `"partial_result"`: Worker reporting individual step completion
/// - `"batch_completion"`: Orchestrator summarizing entire batch
///
/// # Raw Message Preservation
///
/// The `raw_message_json` field stores the complete ZeroMQ message:
/// ```json
/// {
///   "message_type": "partial_result",
///   "batch_id": "batch_abc123",
///   "step_id": 12345,
///   "status": "completed",
///   "output": {"result": "success"},
///   "worker_id": "worker_01",
///   "sequence": 1,
///   "timestamp": "2025-07-24T10:00:00Z",
///   "execution_time_ms": 1500
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct StepExecutionBatchReceivedResult {
    pub id: i64,
    pub batch_step_id: i64,
    pub message_type: String,
    pub worker_id: Option<String>,
    pub sequence_number: Option<i32>,
    pub status: Option<String>,
    pub execution_time_ms: Option<i64>,
    pub raw_message_json: serde_json::Value,
    pub processed_at: Option<NaiveDateTime>,
    pub processing_errors: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New received result for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStepExecutionBatchReceivedResult {
    pub batch_step_id: i64,
    pub message_type: String,
    pub worker_id: Option<String>,
    pub sequence_number: Option<i32>,
    pub status: Option<String>,
    pub execution_time_ms: Option<i64>,
    pub raw_message_json: serde_json::Value,
    pub processed_at: Option<NaiveDateTime>,
    pub processing_errors: Option<serde_json::Value>,
}

impl StepExecutionBatchReceivedResult {
    /// Find by primary key
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_step_id, message_type, worker_id, sequence_number,
                   status, execution_time_ms, raw_message_json, processed_at,
                   processing_errors, created_at, updated_at
            FROM tasker_step_execution_batch_received_results
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Create a new received result record
    pub async fn create(
        pool: &PgPool,
        new_result: NewStepExecutionBatchReceivedResult,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            INSERT INTO tasker_step_execution_batch_received_results
            (batch_step_id, message_type, worker_id, sequence_number, status,
             execution_time_ms, raw_message_json, processed_at, processing_errors)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, batch_step_id, message_type, worker_id, sequence_number,
                      status, execution_time_ms, raw_message_json, processed_at,
                      processing_errors, created_at, updated_at
            "#,
            new_result.batch_step_id,
            new_result.message_type,
            new_result.worker_id,
            new_result.sequence_number,
            new_result.status,
            new_result.execution_time_ms,
            new_result.raw_message_json,
            new_result.processed_at,
            new_result.processing_errors
        )
        .fetch_one(pool)
        .await
    }

    /// Find all results for a batch-step
    pub async fn find_by_batch_step(
        pool: &PgPool,
        batch_step_id: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_step_id, message_type, worker_id, sequence_number,
                   status, execution_time_ms, raw_message_json, processed_at,
                   processing_errors, created_at, updated_at
            FROM tasker_step_execution_batch_received_results
            WHERE batch_step_id = $1
            ORDER BY sequence_number, created_at
            "#,
            batch_step_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find all results for a batch (via join)
    pub async fn find_by_batch(pool: &PgPool, batch_id: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT r.id, r.batch_step_id, r.message_type, r.worker_id, r.sequence_number,
                   r.status, r.execution_time_ms, r.raw_message_json, r.processed_at,
                   r.processing_errors, r.created_at, r.updated_at
            FROM tasker_step_execution_batch_received_results r
            JOIN tasker_step_execution_batch_steps bs ON bs.id = r.batch_step_id
            WHERE bs.batch_id = $1
            ORDER BY bs.sequence_order, r.sequence_number, r.created_at
            "#,
            batch_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find partial results without corresponding batch completion
    pub async fn find_orphaned_partial_results(
        pool: &PgPool,
        older_than_minutes: i32,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT DISTINCT r1.id, r1.batch_step_id, r1.message_type, r1.worker_id, 
                   r1.sequence_number, r1.status, r1.execution_time_ms, 
                   r1.raw_message_json, r1.processed_at, r1.processing_errors, 
                   r1.created_at, r1.updated_at
            FROM tasker_step_execution_batch_received_results r1
            JOIN tasker_step_execution_batch_steps bs ON bs.id = r1.batch_step_id
            WHERE r1.message_type = 'partial_result'
              AND r1.created_at < NOW() - INTERVAL '1 minute' * $1
              AND NOT EXISTS (
                  SELECT 1
                  FROM tasker_step_execution_batch_received_results r2
                  JOIN tasker_step_execution_batch_steps bs2 ON bs2.id = r2.batch_step_id
                  WHERE bs2.batch_id = bs.batch_id
                    AND r2.message_type = 'batch_completion'
              )
            ORDER BY r1.created_at
            "#,
            older_than_minutes as f64
        )
        .fetch_all(pool)
        .await
    }

    /// Count results by message type for a batch
    pub async fn count_by_type_for_batch(
        pool: &PgPool,
        batch_id: i64,
    ) -> Result<Vec<MessageTypeCount>, sqlx::Error> {
        sqlx::query_as!(
            MessageTypeCount,
            r#"
            SELECT r.message_type, COUNT(*) as count
            FROM tasker_step_execution_batch_received_results r
            JOIN tasker_step_execution_batch_steps bs ON bs.id = r.batch_step_id
            WHERE bs.batch_id = $1
            GROUP BY r.message_type
            "#,
            batch_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find discrepancies between partial results and batch completion
    pub async fn find_reconciliation_discrepancies(
        pool: &PgPool,
        batch_id: i64,
    ) -> Result<Vec<ReconciliationDiscrepancy>, sqlx::Error> {
        sqlx::query_as!(
            ReconciliationDiscrepancy,
            r#"
            SELECT 
                bs.workflow_step_id,
                partial.status as partial_status,
                partial.execution_time_ms as partial_execution_time,
                completion.status as completion_status,
                completion.execution_time_ms as completion_execution_time
            FROM tasker_step_execution_batch_steps bs
            LEFT JOIN LATERAL (
                SELECT status, execution_time_ms
                FROM tasker_step_execution_batch_received_results
                WHERE batch_step_id = bs.id 
                  AND message_type = 'partial_result'
                ORDER BY sequence_number DESC
                LIMIT 1
            ) partial ON true
            LEFT JOIN LATERAL (
                SELECT 
                    raw_message_json->'step_summaries'->0->>'final_status' as status,
                    (raw_message_json->'step_summaries'->0->>'execution_time_ms')::bigint as execution_time_ms
                FROM tasker_step_execution_batch_received_results r2
                JOIN tasker_step_execution_batch_steps bs2 ON bs2.id = r2.batch_step_id
                WHERE bs2.batch_id = $1
                  AND r2.message_type = 'batch_completion'
                  AND raw_message_json->'step_summaries' @> jsonb_build_array(
                      jsonb_build_object('step_id', bs.workflow_step_id)
                  )
                LIMIT 1
            ) completion ON true
            WHERE bs.batch_id = $1
              AND (partial.status IS DISTINCT FROM completion.status
                   OR partial.execution_time_ms IS DISTINCT FROM completion.execution_time_ms)
            "#,
            batch_id
        )
        .fetch_all(pool)
        .await
    }
}

/// Helper struct for message type counts
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessageTypeCount {
    pub message_type: String,
    pub count: Option<i64>,
}

/// Helper struct for reconciliation discrepancies
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReconciliationDiscrepancy {
    pub workflow_step_id: i64,
    pub partial_status: Option<String>,
    pub partial_execution_time: Option<i64>,
    pub completion_status: Option<String>,
    pub completion_execution_time: Option<i64>,
}
