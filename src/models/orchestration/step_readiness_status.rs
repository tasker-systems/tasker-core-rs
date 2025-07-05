//! # Step Readiness Status
//!
//! **CRITICAL**: This is NOT a database table - it's a computed view via SQL functions.
//!
//! ## Overview
//!
//! The `StepReadinessStatus` represents dynamically computed readiness analysis
//! for workflow steps. Like TaskExecutionContext, this data is **never stored** - it's calculated
//! on-demand using sophisticated SQL functions that analyze step states and dependencies.
//!
//! ## SQL Function Integration
//!
//! This module integrates with the PostgreSQL function:
//!
//! ### `get_step_readiness_status(input_task_id bigint, step_ids bigint[])`
//! - Computes readiness analysis for workflow steps
//! - Returns comprehensive dependency satisfaction and retry eligibility
//! - Uses CTEs and step transitions for efficient calculation
//!
//! ## Function Return Schema
//!
//! The function returns:
//! ```sql
//! RETURNS TABLE(
//!   workflow_step_id bigint,
//!   task_id bigint,
//!   named_step_id integer,
//!   name text,
//!   current_state text,
//!   dependencies_satisfied boolean,
//!   retry_eligible boolean,
//!   ready_for_execution boolean,
//!   last_failure_at timestamp without time zone,
//!   next_retry_at timestamp without time zone,
//!   total_parents integer,
//!   completed_parents integer,
//!   attempts integer,
//!   retry_limit integer,
//!   backoff_request_seconds integer,
//!   last_attempted_at timestamp without time zone
//! )
//! ```
//!
//! ## Performance Characteristics
//!
//! - **No Storage Overhead**: No tables to maintain or indexes to update
//! - **Always Current**: Real-time calculation ensures data is never stale
//! - **Efficient Computation**: Leverages existing indexes on steps and transitions

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Represents computed step readiness analysis.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_step_readiness_status()` SQL function.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Current step state from `tasker_workflow_step_transitions`
/// - Step dependencies from `tasker_workflow_step_edges`  
/// - Step retry state and configuration
/// - Parent/child completion status
///
/// # No CRUD Operations
///
/// Unlike other models, this struct does NOT support:
/// - `create()` - Cannot insert computed data
/// - `update()` - Cannot modify computed data
/// - `delete()` - Cannot delete computed data
///
/// Only read operations are available via the SQL function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct StepReadinessStatus {
    pub workflow_step_id: i64,
    pub task_id: i64,
    pub named_step_id: i32,
    pub name: String,
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub last_failure_at: Option<NaiveDateTime>,
    pub next_retry_at: Option<NaiveDateTime>,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub attempts: i32,
    pub retry_limit: i32,
    pub backoff_request_seconds: Option<i32>,
    pub last_attempted_at: Option<NaiveDateTime>,
}

/// Query result for steps with their readiness analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepReadinessWithStep {
    pub step: StepReadinessStatus,
    pub step_name: String,
    pub step_description: Option<String>,
}

/// Summary of step readiness results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepReadinessResult {
    pub total_steps: usize,
    pub ready_steps: usize,
    pub blocked_steps: usize,
    pub failed_steps: usize,
    pub processing_steps: usize,
}

/// Stub for compatibility (not used in function-based approach)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStepReadinessStatus {
    // Placeholder - not used since this is computed data
}

impl StepReadinessStatus {
    /// Get step readiness status for all steps in a task using SQL function.
    ///
    /// This method calls the `get_step_readiness_status(input_task_id)` PostgreSQL function
    /// to compute real-time readiness analysis for all steps in the specified task.
    ///
    /// # SQL Function Details
    ///
    /// The underlying function performs sophisticated analysis:
    ///
    /// ```sql
    /// SELECT * FROM get_step_readiness_status($1, NULL)
    /// ```
    ///
    /// **Function Implementation Overview:**
    /// 1. **Step Data Collection**: Gathers step instances and their current state
    /// 2. **Dependency Analysis**: Counts parent/child relationships and completion status
    /// 3. **Retry Eligibility**: Determines if steps can be retried based on limits and attempts
    /// 4. **Ready for Execution**: Identifies steps that can be processed immediately
    /// 5. **Comprehensive Status**: Provides complete readiness picture for orchestration
    ///
    /// # Performance Characteristics
    ///
    /// - **Complexity**: O(S + E) where S = steps, E = edges (dependencies)
    /// - **Typical Performance**: <30ms for tasks with 100 steps
    /// - **Index Dependencies**: Leverages existing indexes on step/edge tables
    /// - **Memory Usage**: Linear with number of steps in task
    pub async fn get_for_task(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessStatus,
            r#"
            SELECT 
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                named_step_id as "named_step_id!: i32",
                name as "name!: String",
                current_state as "current_state!: String",
                dependencies_satisfied as "dependencies_satisfied!: bool",
                retry_eligible as "retry_eligible!: bool",
                ready_for_execution as "ready_for_execution!: bool",
                last_failure_at,
                next_retry_at,
                total_parents as "total_parents!: i32",
                completed_parents as "completed_parents!: i32",
                attempts as "attempts!: i32",
                retry_limit as "retry_limit!: i32",
                backoff_request_seconds,
                last_attempted_at
            FROM get_step_readiness_status($1, NULL)
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get step readiness for specific steps within a task.
    ///
    /// This method allows filtering to specific step IDs for more targeted analysis.
    pub async fn get_for_steps(
        pool: &PgPool,
        task_id: i64,
        step_ids: &[i64],
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessStatus,
            r#"
            SELECT 
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                named_step_id as "named_step_id!: i32",
                name as "name!: String",
                current_state as "current_state!: String",
                dependencies_satisfied as "dependencies_satisfied!: bool",
                retry_eligible as "retry_eligible!: bool",
                ready_for_execution as "ready_for_execution!: bool",
                last_failure_at,
                next_retry_at,
                total_parents as "total_parents!: i32",
                completed_parents as "completed_parents!: i32",
                attempts as "attempts!: i32",
                retry_limit as "retry_limit!: i32",
                backoff_request_seconds,
                last_attempted_at
            FROM get_step_readiness_status($1, $2)
            "#,
            task_id,
            step_ids
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get only steps that are ready for execution.
    pub async fn get_ready_for_task(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_id).await?;
        Ok(statuses
            .into_iter()
            .filter(|s| s.ready_for_execution)
            .collect())
    }

    /// Get steps blocked by dependencies.
    pub async fn get_blocked_by_dependencies(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_id).await?;
        Ok(statuses
            .into_iter()
            .filter(|s| !s.dependencies_satisfied)
            .collect())
    }

    /// Get steps eligible for retry.
    pub async fn get_retry_eligible(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_id).await?;
        Ok(statuses.into_iter().filter(|s| s.retry_eligible).collect())
    }

    /// Check if all steps are complete for a task.
    pub async fn all_steps_complete(pool: &PgPool, task_id: i64) -> Result<bool, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_id).await?;
        Ok(!statuses.is_empty() && statuses.iter().all(|s| s.current_state == "complete"))
    }

    /// Get readiness summary for a task.
    pub async fn get_readiness_summary(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<StepReadinessResult, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_id).await?;

        let total_steps = statuses.len();
        let ready_steps = statuses.iter().filter(|s| s.ready_for_execution).count();
        let blocked_steps = statuses
            .iter()
            .filter(|s| !s.dependencies_satisfied)
            .count();
        let failed_steps = statuses
            .iter()
            .filter(|s| s.current_state == "error")
            .count();
        let processing_steps = statuses
            .iter()
            .filter(|s| s.current_state == "in_progress")
            .count();

        Ok(StepReadinessResult {
            total_steps,
            ready_steps,
            blocked_steps,
            failed_steps,
            processing_steps,
        })
    }

    /// Check if this step is ready for execution.
    pub fn is_ready(&self) -> bool {
        self.ready_for_execution
    }

    /// Check if this step is blocked by dependencies.
    pub fn is_blocked(&self) -> bool {
        !self.dependencies_satisfied
    }

    /// Check if this step can be retried.
    pub fn can_retry(&self) -> bool {
        self.retry_eligible
    }

    /// Check if this step is currently processing.
    pub fn is_processing(&self) -> bool {
        self.current_state == "in_progress"
    }

    /// Check if this step has completed successfully.
    pub fn is_complete(&self) -> bool {
        self.current_state == "complete"
    }

    /// Check if this step has failed.
    pub fn has_failed(&self) -> bool {
        self.current_state == "error"
    }
}
