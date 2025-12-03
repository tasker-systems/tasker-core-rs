//! # Workflow Step Result Audit
//!
//! Lightweight audit trail for workflow step execution results (TAS-62).
//!
//! ## Overview
//!
//! The `WorkflowStepResultAudit` model provides SOC2-compliant audit trails for workflow
//! step execution results. It stores references and attribution only—full result data
//! is retrieved via JOIN to `tasker_workflow_step_transitions` which already captures
//! the complete `StepExecutionResult` in its metadata.
//!
//! ## Design Principles
//!
//! 1. **No data duplication**: Results already exist in transition metadata
//! 2. **Attribution capture**: NEW data (worker_uuid, correlation_id) not in transitions
//! 3. **Indexed scalars**: Extract `success` and `execution_time_ms` for efficient filtering
//! 4. **SQL trigger**: Guaranteed audit record creation (SOC2 compliance)
//! 5. **Lightweight records**: ~100 bytes per audit entry vs ~1KB+ for duplicated results
//!
//! ## Database Schema
//!
//! Maps to `tasker_workflow_step_result_audit` table:
//! ```sql
//! CREATE TABLE tasker_workflow_step_result_audit (
//!     workflow_step_result_audit_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
//!     workflow_step_uuid UUID NOT NULL REFERENCES tasker_workflow_steps,
//!     workflow_step_transition_uuid UUID NOT NULL REFERENCES tasker_workflow_step_transitions,
//!     task_uuid UUID NOT NULL REFERENCES tasker_tasks,
//!     recorded_at TIMESTAMP NOT NULL DEFAULT NOW(),
//!     worker_uuid UUID,
//!     correlation_id UUID,
//!     success BOOLEAN NOT NULL,
//!     execution_time_ms BIGINT,
//!     created_at TIMESTAMP NOT NULL DEFAULT NOW(),
//!     updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
//!     UNIQUE (workflow_step_uuid, workflow_step_transition_uuid)
//! );
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_shared::models::core::WorkflowStepResultAudit;
//!
//! // Get audit history for a step with full transition details
//! let history = WorkflowStepResultAudit::get_audit_history(&pool, step_uuid).await?;
//!
//! // Get all audit records for a task
//! let task_history = WorkflowStepResultAudit::get_task_audit_history(&pool, task_uuid).await?;
//! ```

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Lightweight audit record for workflow step execution results.
///
/// This model stores references and attribution only. Full execution results
/// are retrieved via JOIN to the transitions table when needed.
///
/// # SOC2 Compliance
///
/// Provides complete attribution for audit trails:
/// - **worker_uuid**: Which worker instance processed the step
/// - **correlation_id**: Distributed tracing identifier for request correlation
/// - **recorded_at**: When the audit record was created
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkflowStepResultAudit {
    /// Primary key using UUIDv7 for time-ordered, distributed-friendly IDs
    pub workflow_step_result_audit_uuid: Uuid,

    /// Reference to the workflow step
    pub workflow_step_uuid: Uuid,

    /// Reference to the specific transition that recorded the result
    pub workflow_step_transition_uuid: Uuid,

    /// Reference to the parent task (denormalized for query efficiency)
    pub task_uuid: Uuid,

    /// Timestamp when audit record was created
    pub recorded_at: NaiveDateTime,

    /// UUID of the worker instance that processed this step (SOC2 attribution)
    pub worker_uuid: Option<Uuid>,

    /// Correlation ID for distributed tracing (SOC2 attribution)
    pub correlation_id: Option<Uuid>,

    /// Whether the step execution succeeded (extracted for indexing)
    pub success: bool,

    /// Step execution time in milliseconds (extracted for filtering)
    pub execution_time_ms: Option<i64>,

    /// Standard audit timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Audit record with full transition details via JOIN
///
/// Used when clients need both audit attribution and full execution results.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StepAuditWithTransition {
    // Audit fields
    pub workflow_step_result_audit_uuid: Uuid,
    pub workflow_step_uuid: Uuid,
    pub workflow_step_transition_uuid: Uuid,
    pub task_uuid: Uuid,
    pub recorded_at: NaiveDateTime,
    pub worker_uuid: Option<Uuid>,
    pub correlation_id: Option<Uuid>,
    pub success: bool,
    pub execution_time_ms: Option<i64>,

    // Transition fields (via JOIN)
    pub from_state: Option<String>,
    pub to_state: String,
    pub transition_metadata: Option<serde_json::Value>,

    // Step context (via JOIN)
    pub step_name: String,
}

impl WorkflowStepResultAudit {
    /// Get audit history for a specific step with full transition details via JOIN.
    ///
    /// Returns audit records enriched with transition metadata containing the
    /// complete `StepExecutionResult`.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `step_uuid` - The workflow step UUID to get audit history for
    ///
    /// # Returns
    ///
    /// Vec of audit records with full transition details, ordered by recorded_at DESC
    pub async fn get_audit_history(
        pool: &PgPool,
        step_uuid: Uuid,
    ) -> Result<Vec<StepAuditWithTransition>, sqlx::Error> {
        let records = sqlx::query_as!(
            StepAuditWithTransition,
            r#"
            SELECT
                a.workflow_step_result_audit_uuid,
                a.workflow_step_uuid,
                a.workflow_step_transition_uuid,
                a.task_uuid,
                a.recorded_at,
                a.worker_uuid,
                a.correlation_id,
                a.success,
                a.execution_time_ms,
                t.from_state,
                t.to_state,
                t.metadata as transition_metadata,
                ns.name as step_name
            FROM tasker_workflow_step_result_audit a
            INNER JOIN tasker_workflow_step_transitions t
                ON t.workflow_step_transition_uuid = a.workflow_step_transition_uuid
            INNER JOIN tasker_workflow_steps ws
                ON ws.workflow_step_uuid = a.workflow_step_uuid
            INNER JOIN tasker_named_steps ns
                ON ns.named_step_uuid = ws.named_step_uuid
            WHERE a.workflow_step_uuid = $1
            ORDER BY a.recorded_at DESC
            "#,
            step_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Get audit history for all steps in a task with full transition details.
    ///
    /// Returns audit records for all steps in a task, enriched with transition
    /// metadata containing complete `StepExecutionResult` data.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `task_uuid` - The task UUID to get audit history for
    ///
    /// # Returns
    ///
    /// Vec of audit records with full transition details, ordered by recorded_at DESC
    pub async fn get_task_audit_history(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<StepAuditWithTransition>, sqlx::Error> {
        let records = sqlx::query_as!(
            StepAuditWithTransition,
            r#"
            SELECT
                a.workflow_step_result_audit_uuid,
                a.workflow_step_uuid,
                a.workflow_step_transition_uuid,
                a.task_uuid,
                a.recorded_at,
                a.worker_uuid,
                a.correlation_id,
                a.success,
                a.execution_time_ms,
                t.from_state,
                t.to_state,
                t.metadata as transition_metadata,
                ns.name as step_name
            FROM tasker_workflow_step_result_audit a
            INNER JOIN tasker_workflow_step_transitions t
                ON t.workflow_step_transition_uuid = a.workflow_step_transition_uuid
            INNER JOIN tasker_workflow_steps ws
                ON ws.workflow_step_uuid = a.workflow_step_uuid
            INNER JOIN tasker_named_steps ns
                ON ns.named_step_uuid = ws.named_step_uuid
            WHERE a.task_uuid = $1
            ORDER BY a.recorded_at DESC
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Find a specific audit record by UUID.
    pub async fn find_by_uuid(
        pool: &PgPool,
        uuid: Uuid,
    ) -> Result<Option<WorkflowStepResultAudit>, sqlx::Error> {
        let record = sqlx::query_as!(
            WorkflowStepResultAudit,
            r#"
            SELECT
                workflow_step_result_audit_uuid,
                workflow_step_uuid,
                workflow_step_transition_uuid,
                task_uuid,
                recorded_at,
                worker_uuid,
                correlation_id,
                success,
                execution_time_ms,
                created_at,
                updated_at
            FROM tasker_workflow_step_result_audit
            WHERE workflow_step_result_audit_uuid = $1
            "#,
            uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    /// Get audit records by worker UUID for attribution investigation.
    ///
    /// Useful for SOC2 compliance when investigating which steps
    /// a specific worker instance processed.
    pub async fn get_by_worker(
        pool: &PgPool,
        worker_uuid: Uuid,
        limit: Option<i64>,
    ) -> Result<Vec<WorkflowStepResultAudit>, sqlx::Error> {
        let limit = limit.unwrap_or(100);

        let records = sqlx::query_as!(
            WorkflowStepResultAudit,
            r#"
            SELECT
                workflow_step_result_audit_uuid,
                workflow_step_uuid,
                workflow_step_transition_uuid,
                task_uuid,
                recorded_at,
                worker_uuid,
                correlation_id,
                success,
                execution_time_ms,
                created_at,
                updated_at
            FROM tasker_workflow_step_result_audit
            WHERE worker_uuid = $1
            ORDER BY recorded_at DESC
            LIMIT $2
            "#,
            worker_uuid,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Get audit records by correlation ID for distributed tracing.
    ///
    /// Useful for tracking a request across service boundaries.
    pub async fn get_by_correlation_id(
        pool: &PgPool,
        correlation_id: Uuid,
    ) -> Result<Vec<WorkflowStepResultAudit>, sqlx::Error> {
        let records = sqlx::query_as!(
            WorkflowStepResultAudit,
            r#"
            SELECT
                workflow_step_result_audit_uuid,
                workflow_step_uuid,
                workflow_step_transition_uuid,
                task_uuid,
                recorded_at,
                worker_uuid,
                correlation_id,
                success,
                execution_time_ms,
                created_at,
                updated_at
            FROM tasker_workflow_step_result_audit
            WHERE correlation_id = $1
            ORDER BY recorded_at DESC
            "#,
            correlation_id
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Get audit records in a time range for SOC2 audit reports.
    pub async fn get_in_time_range(
        pool: &PgPool,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
        limit: Option<i64>,
    ) -> Result<Vec<WorkflowStepResultAudit>, sqlx::Error> {
        let limit = limit.unwrap_or(1000);

        let records = sqlx::query_as!(
            WorkflowStepResultAudit,
            r#"
            SELECT
                workflow_step_result_audit_uuid,
                workflow_step_uuid,
                workflow_step_transition_uuid,
                task_uuid,
                recorded_at,
                worker_uuid,
                correlation_id,
                success,
                execution_time_ms,
                created_at,
                updated_at
            FROM tasker_workflow_step_result_audit
            WHERE recorded_at BETWEEN $1 AND $2
            ORDER BY recorded_at DESC
            LIMIT $3
            "#,
            start_time,
            end_time,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Count audit records by success/failure for reporting.
    pub async fn count_by_success(pool: &PgPool, success: bool) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_workflow_step_result_audit
            WHERE success = $1
            "#,
            success
        )
        .fetch_one(pool)
        .await?
        .count
        .unwrap_or(0);

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_struct_serialization() {
        let audit = WorkflowStepResultAudit {
            workflow_step_result_audit_uuid: Uuid::new_v4(),
            workflow_step_uuid: Uuid::new_v4(),
            workflow_step_transition_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            recorded_at: chrono::Utc::now().naive_utc(),
            worker_uuid: Some(Uuid::new_v4()),
            correlation_id: Some(Uuid::new_v4()),
            success: true,
            execution_time_ms: Some(150),
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        let json = serde_json::to_string(&audit).expect("serialization should work");
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"execution_time_ms\":150"));
    }

    #[test]
    fn test_audit_with_null_attribution() {
        let audit = WorkflowStepResultAudit {
            workflow_step_result_audit_uuid: Uuid::new_v4(),
            workflow_step_uuid: Uuid::new_v4(),
            workflow_step_transition_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            recorded_at: chrono::Utc::now().naive_utc(),
            worker_uuid: None,
            correlation_id: None,
            success: false,
            execution_time_ms: None,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        // Should serialize without attribution fields when None
        let json = serde_json::to_string(&audit).expect("serialization should work");
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"worker_uuid\":null"));
    }
}
