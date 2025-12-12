//! # Task Execution Context - Real-Time Task Status (UNIFIED VERSION)
//!
//! **CRITICAL**: This is NOT a database table - it's a computed view via SQL functions.
//!
//! ## Overview
//!
//! The `TaskExecutionContext` represents dynamically computed execution state and statistics
//! for workflow tasks. Unlike other models, this data is **never stored** - it's calculated
//! on-demand using sophisticated SQL functions that analyze step states and dependencies.
//!
//! ## Unified Structure
//!
//! This version unifies the 3 different TaskExecutionContext definitions that existed across:
//! - task_finalizer.rs (canonical enums)
//! - sql_functions.rs (FromRow, ToSchema)
//! - task_execution_context.rs (this file)
//!
//! Now uses proper `ExecutionStatus` and `RecommendedAction` enums instead of raw strings.

use crate::models::orchestration::execution_status::{ExecutionStatus, RecommendedAction};
use serde::{Deserialize, Serialize};
use sqlx::types::{BigDecimal, Uuid};
use sqlx::{FromRow, PgPool};

#[cfg(feature = "web-api")]
use utoipa::ToSchema;

/// Represents computed task execution context and statistics.
///
/// **UNIFIED VERSION**: Uses proper ExecutionStatus and RecommendedAction enums
/// instead of raw strings for type safety.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_task_execution_context()` or `get_task_execution_contexts_batch()` SQL functions.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TaskExecutionContext {
    pub task_uuid: Uuid,
    pub named_task_uuid: Uuid,
    pub status: String,
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub completed_steps: i64,
    pub failed_steps: i64,
    pub ready_steps: i64,
    /// Proper typed execution status (was raw string)
    pub execution_status: ExecutionStatus,
    /// Proper typed recommended action (was raw string)
    pub recommended_action: Option<RecommendedAction>,
    #[cfg_attr(feature = "web-api", schema(value_type = String))]
    pub completion_percentage: BigDecimal,
    pub health_status: String,
    pub enqueued_steps: i64,
}

impl TaskExecutionContext {
    /// Get execution context for a single task using SQL function.
    ///
    /// This method calls the `get_task_execution_context(input_task_uuid)` PostgreSQL function
    /// to compute real-time execution statistics and status for the specified task.
    pub async fn get_for_task(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Option<TaskExecutionContext>, sqlx::Error> {
        let context = sqlx::query_as!(
            RawTaskExecutionContext,
            r#"
            SELECT
                task_uuid as "task_uuid!: Uuid",
                named_task_uuid as "named_task_uuid!: Uuid",
                status as "status!: String",
                total_steps as "total_steps!: i64",
                pending_steps as "pending_steps!: i64",
                in_progress_steps as "in_progress_steps!: i64",
                completed_steps as "completed_steps!: i64",
                failed_steps as "failed_steps!: i64",
                ready_steps as "ready_steps!: i64",
                execution_status as "execution_status!: String",
                recommended_action,
                completion_percentage as "completion_percentage!: BigDecimal",
                health_status as "health_status!: String",
                enqueued_steps as "enqueued_steps!: i64"
            FROM get_task_execution_context($1::uuid)
            "#,
            task_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(context.map(|raw| raw.into_typed()))
    }

    /// Get execution contexts for multiple tasks using batch SQL function.
    ///
    /// This method calls the `get_task_execution_contexts_batch(input_task_uuids)` PostgreSQL
    /// function to efficiently compute execution statistics for multiple tasks simultaneously.
    pub async fn get_for_tasks(
        pool: &PgPool,
        task_uuids: &[Uuid],
    ) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let contexts = sqlx::query_as!(
            RawTaskExecutionContext,
            r#"
            SELECT
                task_uuid as "task_uuid!: Uuid",
                named_task_uuid as "named_task_uuid!: Uuid",
                status as "status!: String",
                total_steps as "total_steps!: i64",
                pending_steps as "pending_steps!: i64",
                in_progress_steps as "in_progress_steps!: i64",
                completed_steps as "completed_steps!: i64",
                failed_steps as "failed_steps!: i64",
                ready_steps as "ready_steps!: i64",
                execution_status as "execution_status!: String",
                recommended_action,
                completion_percentage as "completion_percentage!: BigDecimal",
                health_status as "health_status!: String",
                enqueued_steps as "enqueued_steps!: i64"
            FROM get_task_execution_contexts_batch($1::uuid[])
            "#,
            task_uuids
        )
        .fetch_all(pool)
        .await?;

        Ok(contexts.into_iter().map(|raw| raw.into_typed()).collect())
    }

    /// Check if the task has any steps ready for execution.
    pub fn has_ready_steps(&self) -> bool {
        self.ready_steps > 0
    }

    /// Check if the task is actively being processed.
    pub fn is_processing(&self) -> bool {
        self.in_progress_steps > 0 || self.enqueued_steps > 0
    }

    /// Check if the task has steps enqueued for processing.
    pub fn has_enqueued_steps(&self) -> bool {
        self.enqueued_steps > 0
    }

    /// Check if the task is complete (all steps finished).
    pub fn is_complete(&self) -> bool {
        self.total_steps > 0 && self.completed_steps == self.total_steps
    }

    /// Check if the task has failed steps.
    pub fn has_failures(&self) -> bool {
        self.failed_steps > 0
    }

    /// Get completion percentage as a float between 0.0 and 1.0.
    pub fn completion_ratio(&self) -> f64 {
        if self.total_steps == 0 {
            0.0
        } else {
            self.completed_steps as f64 / self.total_steps as f64
        }
    }

    /// Get a human-readable status summary.
    pub fn status_summary(&self) -> String {
        if self.is_complete() {
            "Complete".to_string()
        } else if self.has_failures() && self.ready_steps == 0 {
            "Blocked".to_string()
        } else if self.is_processing() {
            "Processing".to_string()
        } else if self.has_ready_steps() {
            "Ready".to_string()
        } else {
            "Waiting".to_string()
        }
    }
}

/// Raw TaskExecutionContext from SQL (with string types) - internal use only
#[derive(Debug, FromRow)]
struct RawTaskExecutionContext {
    pub task_uuid: Uuid,
    pub named_task_uuid: Uuid,
    pub status: String,
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub completed_steps: i64,
    pub failed_steps: i64,
    pub ready_steps: i64,
    pub execution_status: String,
    pub recommended_action: Option<String>,
    pub completion_percentage: BigDecimal,
    pub health_status: String,
    pub enqueued_steps: i64,
}

impl RawTaskExecutionContext {
    /// Convert raw SQL result to properly typed TaskExecutionContext
    fn into_typed(self) -> TaskExecutionContext {
        TaskExecutionContext {
            task_uuid: self.task_uuid,
            named_task_uuid: self.named_task_uuid,
            status: self.status,
            total_steps: self.total_steps,
            pending_steps: self.pending_steps,
            in_progress_steps: self.in_progress_steps,
            completed_steps: self.completed_steps,
            failed_steps: self.failed_steps,
            ready_steps: self.ready_steps,
            execution_status: ExecutionStatus::from(self.execution_status),
            recommended_action: self.recommended_action.map(RecommendedAction::from),
            completion_percentage: self.completion_percentage,
            health_status: self.health_status,
            enqueued_steps: self.enqueued_steps,
        }
    }
}
