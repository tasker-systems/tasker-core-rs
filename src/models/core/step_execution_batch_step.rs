//! # Step Execution Batch Step Model
//!
//! HABTM join table linking batches to workflow steps for ZeroMQ execution.
//!
//! ## Overview
//!
//! The `StepExecutionBatchStep` model implements the many-to-many relationship
//! between execution batches and workflow steps. This join table enables tracking
//! which steps are included in each batch, their execution order, and the
//! expected handler class for validation.
//!
//! ## Key Features
//!
//! - **HABTM Relationship**: Links batches and workflow steps
//! - **Sequence Ordering**: Defines execution order within batch
//! - **Handler Validation**: Records expected handler for consistency checks
//! - **Unique Constraint**: Prevents duplicate step entries per batch
//! - **Cascade Deletion**: Automatic cleanup when batch or step deleted
//!
//! ## Database Schema
//!
//! Maps to `tasker_step_execution_batch_steps` table:
//! ```sql
//! CREATE TABLE tasker_step_execution_batch_steps (
//!   id BIGSERIAL PRIMARY KEY,
//!   batch_id BIGINT NOT NULL,
//!   workflow_step_id BIGINT NOT NULL,
//!   sequence_order INTEGER DEFAULT 0,
//!   expected_handler_class VARCHAR NOT NULL,
//!   metadata JSONB,
//!   UNIQUE(batch_id, workflow_step_id)
//! );
//! ```
//!
//! ## Relationships
//!
//! - **Belongs to Batch**: References step_execution_batches
//! - **Belongs to WorkflowStep**: References workflow_steps
//! - **Has Many ReceivedResults**: Audit trail for this batch-step combination
//!
//! ## Advanced Querying
//!
//! This table enables sophisticated queries:
//! - Find all steps in a batch
//! - Find all batches containing a specific step
//! - Detect orphaned batch-step combinations
//! - Analyze step execution patterns across batches

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Represents the HABTM relationship between batches and workflow steps.
///
/// Each record links a single workflow step to a batch, with additional
/// metadata about execution order and expected handler.
///
/// # Uniqueness Guarantee
///
/// The database enforces that each workflow_step_id can only appear
/// once per batch_id, preventing duplicate executions.
///
/// # Sequence Ordering
///
/// The `sequence_order` field allows control over step execution order
/// within a batch, useful for:
/// - Priority-based execution
/// - Dependency-aware ordering
/// - Load balancing across workers
///
/// # Handler Validation
///
/// The `expected_handler_class` provides a consistency check:
/// - Validates the step is executed by the correct handler
/// - Detects handler mismatches or version conflicts
/// - Enables handler-specific batch optimization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct StepExecutionBatchStep {
    pub id: i64,
    pub batch_id: i64,
    pub workflow_step_id: i64,
    pub sequence_order: i32,
    pub expected_handler_class: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New batch-step relationship for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStepExecutionBatchStep {
    pub batch_id: i64,
    pub workflow_step_id: i64,
    pub sequence_order: i32,
    pub expected_handler_class: String,
    pub metadata: Option<serde_json::Value>,
}

impl StepExecutionBatchStep {
    /// Find by primary key
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_id, workflow_step_id, sequence_order,
                   expected_handler_class, metadata, created_at, updated_at
            FROM tasker_step_execution_batch_steps
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Find by batch and workflow step (unique combination)
    pub async fn find_by_batch_and_step(
        pool: &PgPool,
        batch_id: i64,
        workflow_step_id: i64,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_id, workflow_step_id, sequence_order,
                   expected_handler_class, metadata, created_at, updated_at
            FROM tasker_step_execution_batch_steps
            WHERE batch_id = $1 AND workflow_step_id = $2
            "#,
            batch_id,
            workflow_step_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Create a new batch-step relationship
    pub async fn create(pool: &PgPool, new_batch_step: NewStepExecutionBatchStep) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            INSERT INTO tasker_step_execution_batch_steps 
            (batch_id, workflow_step_id, sequence_order, expected_handler_class, metadata)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, batch_id, workflow_step_id, sequence_order,
                      expected_handler_class, metadata, created_at, updated_at
            "#,
            new_batch_step.batch_id,
            new_batch_step.workflow_step_id,
            new_batch_step.sequence_order,
            new_batch_step.expected_handler_class,
            new_batch_step.metadata
        )
        .fetch_one(pool)
        .await
    }

    /// Create multiple batch-step relationships in a single transaction
    pub async fn create_batch(
        pool: &PgPool,
        batch_steps: Vec<NewStepExecutionBatchStep>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let mut created_steps = Vec::new();

        for batch_step in batch_steps {
            let created = sqlx::query_as!(
                Self,
                r#"
                INSERT INTO tasker_step_execution_batch_steps 
                (batch_id, workflow_step_id, sequence_order, expected_handler_class, metadata)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id, batch_id, workflow_step_id, sequence_order,
                          expected_handler_class, metadata, created_at, updated_at
                "#,
                batch_step.batch_id,
                batch_step.workflow_step_id,
                batch_step.sequence_order,
                batch_step.expected_handler_class,
                batch_step.metadata
            )
            .fetch_one(&mut *tx)
            .await?;
            
            created_steps.push(created);
        }

        tx.commit().await?;
        Ok(created_steps)
    }

    /// Find all steps in a batch, ordered by sequence
    pub async fn find_by_batch(pool: &PgPool, batch_id: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_id, workflow_step_id, sequence_order,
                   expected_handler_class, metadata, created_at, updated_at
            FROM tasker_step_execution_batch_steps
            WHERE batch_id = $1
            ORDER BY sequence_order, id
            "#,
            batch_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find all batches containing a specific workflow step
    pub async fn find_by_workflow_step(
        pool: &PgPool,
        workflow_step_id: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_id, workflow_step_id, sequence_order,
                   expected_handler_class, metadata, created_at, updated_at
            FROM tasker_step_execution_batch_steps
            WHERE workflow_step_id = $1
            ORDER BY created_at DESC
            "#,
            workflow_step_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find batch-steps without any received results (potential orphans)
    pub async fn find_without_results(
        pool: &PgPool,
        older_than_minutes: i32,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT bs.id, bs.batch_id, bs.workflow_step_id, bs.sequence_order,
                   bs.expected_handler_class, bs.metadata, bs.created_at, bs.updated_at
            FROM tasker_step_execution_batch_steps bs
            LEFT JOIN tasker_step_execution_batch_received_results r
                ON r.batch_step_id = bs.id
            WHERE r.id IS NULL
              AND bs.created_at < NOW() - INTERVAL '1 minute' * $1
            ORDER BY bs.created_at
            "#,
            older_than_minutes as f64
        )
        .fetch_all(pool)
        .await
    }

    /// Count steps in a batch
    pub async fn count_by_batch(pool: &PgPool, batch_id: i64) -> Result<i64, sqlx::Error> {
        let record = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_step_execution_batch_steps
            WHERE batch_id = $1
            "#,
            batch_id
        )
        .fetch_one(pool)
        .await?;

        Ok(record.count.unwrap_or(0))
    }
}