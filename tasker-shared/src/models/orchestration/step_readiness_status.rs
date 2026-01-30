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
//! ### `get_step_readiness_status(input_task_uuid bigint, step_uuids bigint[])`
//! - Computes readiness analysis for workflow steps
//! - Returns comprehensive dependency satisfaction and retry eligibility
//! - Uses CTEs and step transitions for efficient calculation
//!
//! ## Function Return Schema
//!
//! The function returns:
//! ```sql
//! RETURNS TABLE(
//!   workflow_step_uuid bigint,
//!   task_uuid bigint,
//!   named_step_uuid integer,
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
//!   max_attempts integer,
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
use sqlx::{types::Uuid, FromRow, PgPool};
use utoipa::ToSchema;

/// Represents computed step readiness analysis.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_step_readiness_status()` SQL function.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Current step state from `tasker.workflow_step_transitions`
/// - Step dependencies from `tasker.workflow_step_edges`
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct StepReadinessStatus {
    pub workflow_step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub named_step_uuid: Uuid,
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
    pub max_attempts: i32,
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

/// Create struct for compatibility with ActiveRecord patterns
///
/// Not used in production since step readiness is computed dynamically
/// via SQL functions rather than stored in database tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStepReadinessStatus {
    /// Private marker field to maintain compatibility without allowing instantiation
    _marker: std::marker::PhantomData<()>,
}

impl NewStepReadinessStatus {
    /// Constructor that enforces computed data pattern
    ///
    /// This struct exists for compatibility but step readiness data
    /// is computed dynamically via SQL functions and should not be instantiated.
    fn _private_constructor() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl StepReadinessStatus {
    /// Get step readiness status for all steps in a task using SQL function.
    ///
    /// This method calls the `get_step_readiness_status(input_task_uuid)` PostgreSQL function
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
        task_uuid: Uuid,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessStatus,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                named_step_uuid as "named_step_uuid!: Uuid",
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
                max_attempts as "max_attempts!: i32",
                backoff_request_seconds,
                last_attempted_at
            FROM get_step_readiness_status($1::uuid, NULL)
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get step readiness for specific steps within a task.
    ///
    /// This method allows filtering to specific step IDs for more targeted analysis.
    pub async fn get_blocked_steps(
        pool: &PgPool,
        task_uuid: Uuid,
        step_uuids: &[Uuid],
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = sqlx::query_as!(
            StepReadinessStatus,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                named_step_uuid as "named_step_uuid!: Uuid",
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
                max_attempts as "max_attempts!: i32",
                backoff_request_seconds,
                last_attempted_at
            FROM get_step_readiness_status($1::uuid, $2::uuid[])
            "#,
            task_uuid,
            step_uuids
        )
        .fetch_all(pool)
        .await?;

        Ok(statuses)
    }

    /// Get only steps that are ready for execution.
    pub async fn get_ready_for_task(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_uuid).await?;
        Ok(statuses
            .into_iter()
            .filter(|s| s.ready_for_execution)
            .collect())
    }

    /// Get steps blocked by dependencies.
    pub async fn get_blocked_by_dependencies(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_uuid).await?;
        Ok(statuses
            .into_iter()
            .filter(|s| !s.dependencies_satisfied)
            .collect())
    }

    /// Get steps eligible for retry.
    pub async fn get_retry_eligible_steps(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_uuid).await?;
        Ok(statuses.into_iter().filter(|s| s.retry_eligible).collect())
    }

    /// Check if all steps are complete for a task.
    pub async fn all_steps_complete(pool: &PgPool, task_uuid: Uuid) -> Result<bool, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_uuid).await?;
        Ok(!statuses.is_empty() && statuses.iter().all(|s| s.current_state == "complete"))
    }

    /// Get readiness summary for a task.
    pub async fn get_readiness_summary(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<StepReadinessResult, sqlx::Error> {
        let statuses = Self::get_for_task(pool, task_uuid).await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a default StepReadinessStatus for reuse in tests.
    fn default_readiness_status() -> StepReadinessStatus {
        StepReadinessStatus {
            workflow_step_uuid: Uuid::nil(),
            task_uuid: Uuid::nil(),
            named_step_uuid: Uuid::nil(),
            name: "test_step".to_string(),
            current_state: "pending".to_string(),
            dependencies_satisfied: true,
            retry_eligible: false,
            ready_for_execution: false,
            last_failure_at: None,
            next_retry_at: None,
            total_parents: 0,
            completed_parents: 0,
            attempts: 0,
            max_attempts: 3,
            backoff_request_seconds: None,
            last_attempted_at: None,
        }
    }

    // ---- is_ready ----

    #[test]
    fn test_is_ready_true() {
        let mut status = default_readiness_status();
        status.ready_for_execution = true;
        assert!(
            status.is_ready(),
            "Should be ready when ready_for_execution is true"
        );
    }

    #[test]
    fn test_is_ready_false() {
        let status = default_readiness_status();
        assert!(
            !status.is_ready(),
            "Should not be ready when ready_for_execution is false"
        );
    }

    // ---- is_blocked ----

    #[test]
    fn test_is_blocked_true() {
        let mut status = default_readiness_status();
        status.dependencies_satisfied = false;
        assert!(
            status.is_blocked(),
            "Should be blocked when dependencies_satisfied is false"
        );
    }

    #[test]
    fn test_is_blocked_false() {
        let status = default_readiness_status();
        assert!(
            !status.is_blocked(),
            "Should not be blocked when dependencies_satisfied is true"
        );
    }

    // ---- can_retry ----

    #[test]
    fn test_can_retry_true() {
        let mut status = default_readiness_status();
        status.retry_eligible = true;
        assert!(
            status.can_retry(),
            "Should be retryable when retry_eligible is true"
        );
    }

    #[test]
    fn test_can_retry_false() {
        let status = default_readiness_status();
        assert!(
            !status.can_retry(),
            "Should not be retryable when retry_eligible is false"
        );
    }

    // ---- is_processing ----

    #[test]
    fn test_is_processing_true() {
        let mut status = default_readiness_status();
        status.current_state = "in_progress".to_string();
        assert!(
            status.is_processing(),
            "Should be processing when current_state is in_progress"
        );
    }

    #[test]
    fn test_is_processing_false() {
        let status = default_readiness_status();
        assert!(
            !status.is_processing(),
            "Should not be processing when current_state is pending"
        );
    }

    // ---- is_complete ----

    #[test]
    fn test_is_complete_true() {
        let mut status = default_readiness_status();
        status.current_state = "complete".to_string();
        assert!(
            status.is_complete(),
            "Should be complete when current_state is complete"
        );
    }

    #[test]
    fn test_is_complete_false() {
        let mut status = default_readiness_status();
        status.current_state = "error".to_string();
        assert!(
            !status.is_complete(),
            "Should not be complete when current_state is error"
        );
    }

    // ---- has_failed ----

    #[test]
    fn test_has_failed_true() {
        let mut status = default_readiness_status();
        status.current_state = "error".to_string();
        assert!(
            status.has_failed(),
            "Should have failed when current_state is error"
        );
    }

    #[test]
    fn test_has_failed_false() {
        let mut status = default_readiness_status();
        status.current_state = "complete".to_string();
        assert!(
            !status.has_failed(),
            "Should not have failed when current_state is complete"
        );
    }
}
