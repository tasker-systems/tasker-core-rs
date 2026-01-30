//! # Slowest Steps Analytics
//!
//! **CRITICAL**: This is NOT a database table - it's a computed view via SQL functions.
//!
//! ## Overview
//!
//! The `SlowestSteps` represents dynamically computed analytics for identifying
//! the slowest performing workflow steps. This data is **never stored** - it's calculated
//! on-demand using sophisticated SQL functions that analyze step execution times.
//!
//! ## SQL Function Integration
//!
//! This module integrates with the PostgreSQL function:
//!
//! ### `get_slowest_steps(since_timestamp, limit_count, namespace_filter, task_name_filter, version_filter)`
//! - Identifies the slowest executing workflow steps
//! - Supports filtering by namespace, task name, and version
//! - Returns configurable number of results with duration analysis
//!
//! ## Function Return Schema
//!
//! The function returns:
//! ```sql
//! RETURNS TABLE(
//!   workflow_step_uuid bigint,
//!   task_uuid bigint,
//!   step_name character varying,
//!   task_name character varying,
//!   namespace_name character varying,
//!   version character varying,
//!   duration_seconds numeric,
//!   attempts integer,
//!   created_at timestamp with time zone,
//!   completed_at timestamp with time zone,
//!   retryable boolean,
//!   step_status character varying
//! )
//! ```

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::BigDecimal, types::Uuid, FromRow, PgPool};

/// Represents computed slowest steps analytics.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_slowest_steps()` SQL function.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Step execution durations across tasks
/// - Step retry attempts and completion times
/// - Task and namespace context information
/// - Step performance patterns
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
pub struct SlowestSteps {
    pub workflow_step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub step_name: String,
    pub task_name: String,
    pub namespace_name: String,
    pub version: String,
    pub duration_seconds: BigDecimal,
    pub attempts: i32,
    pub created_at: NaiveDateTime,
    pub completed_at: Option<NaiveDateTime>,
    pub retryable: bool,
    pub step_status: String,
}

/// Filter parameters for slowest steps analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowestStepsFilter {
    pub since_timestamp: Option<DateTime<Utc>>,
    pub limit_count: Option<i32>,
    pub namespace_filter: Option<String>,
    pub task_name_filter: Option<String>,
    pub version_filter: Option<String>,
}

impl Default for SlowestStepsFilter {
    fn default() -> Self {
        Self {
            since_timestamp: None,
            limit_count: Some(10),
            namespace_filter: None,
            task_name_filter: None,
            version_filter: None,
        }
    }
}

impl SlowestSteps {
    /// Get slowest steps with default filters (top 10).
    pub async fn get_slowest(pool: &PgPool) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        Self::get_with_filters(pool, SlowestStepsFilter::default()).await
    }

    /// Get slowest steps since a specific timestamp.
    pub async fn get_since(
        pool: &PgPool,
        since_timestamp: DateTime<Utc>,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        let filter = SlowestStepsFilter {
            since_timestamp: Some(since_timestamp),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest steps for a specific namespace.
    pub async fn get_by_namespace(
        pool: &PgPool,
        namespace: &str,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        let filter = SlowestStepsFilter {
            namespace_filter: Some(namespace.to_string()),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest steps for a specific task name.
    pub async fn get_by_task_name(
        pool: &PgPool,
        task_name: &str,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        let filter = SlowestStepsFilter {
            task_name_filter: Some(task_name.to_string()),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest steps with custom filters.
    pub async fn get_with_filters(
        pool: &PgPool,
        filter: SlowestStepsFilter,
    ) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        let steps = sqlx::query_as!(
            SlowestSteps,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                step_name as "step_name!: String",
                task_name as "task_name!: String",
                namespace_name as "namespace_name!: String",
                version as "version!: String",
                duration_seconds as "duration_seconds!: BigDecimal",
                attempts as "attempts!: i32",
                created_at as "created_at!: NaiveDateTime",
                completed_at as "completed_at?: NaiveDateTime",
                retryable as "retryable!: bool",
                step_status as "step_status!: String"
            FROM get_slowest_steps($1, $2, $3, $4, $5)
            "#,
            filter.since_timestamp,
            filter.limit_count.unwrap_or(10),
            filter.namespace_filter,
            filter.task_name_filter,
            filter.version_filter
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Get duration in seconds as a float.
    pub fn duration_as_seconds(&self) -> f64 {
        self.duration_seconds.to_string().parse().unwrap_or(0.0)
    }

    /// Check if this step had multiple attempts (was retried).
    pub fn was_retried(&self) -> bool {
        self.attempts > 1
    }

    /// Check if this step is still running.
    pub fn is_running(&self) -> bool {
        self.completed_at.is_none() && self.step_status == "in_progress"
    }

    /// Check if this step completed successfully.
    pub fn completed_successfully(&self) -> bool {
        self.step_status == "complete"
    }

    /// Check if this step failed.
    pub fn failed(&self) -> bool {
        self.step_status == "error"
    }

    /// Get human-readable duration string.
    pub fn duration_display(&self) -> String {
        let seconds = self.duration_as_seconds();

        if seconds < 60.0 {
            format!("{seconds:.1}s")
        } else if seconds < 3600.0 {
            format!("{:.1}m", seconds / 60.0)
        } else {
            format!("{:.1}h", seconds / 3600.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn make_default_step() -> SlowestSteps {
        SlowestSteps {
            workflow_step_uuid: Uuid::nil(),
            task_uuid: Uuid::nil(),
            step_name: "test_step".to_string(),
            task_name: "test_task".to_string(),
            namespace_name: "default".to_string(),
            version: "1.0.0".to_string(),
            duration_seconds: BigDecimal::from_str("45.5").unwrap(),
            attempts: 1,
            created_at: NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            completed_at: Some(
                NaiveDateTime::parse_from_str("2024-01-01 00:00:45", "%Y-%m-%d %H:%M:%S").unwrap(),
            ),
            retryable: true,
            step_status: "complete".to_string(),
        }
    }

    #[test]
    fn test_filter_default_values() {
        let filter = SlowestStepsFilter::default();
        assert!(filter.since_timestamp.is_none());
        assert_eq!(filter.limit_count, Some(10));
        assert!(filter.namespace_filter.is_none());
        assert!(filter.task_name_filter.is_none());
        assert!(filter.version_filter.is_none());
    }

    #[test]
    fn test_duration_as_seconds() {
        let step = make_default_step();
        assert_eq!(step.duration_as_seconds(), 45.5);
    }

    #[test]
    fn test_was_retried_single_attempt() {
        let step = make_default_step();
        assert!(!step.was_retried());
    }

    #[test]
    fn test_was_retried_multiple_attempts() {
        let mut step = make_default_step();
        step.attempts = 3;
        assert!(step.was_retried());
    }

    #[test]
    fn test_is_running_true() {
        let mut step = make_default_step();
        step.completed_at = None;
        step.step_status = "in_progress".to_string();
        assert!(step.is_running());
    }

    #[test]
    fn test_is_running_false_with_completed_at() {
        let mut step = make_default_step();
        step.step_status = "in_progress".to_string();
        // completed_at is Some from default
        assert!(!step.is_running());
    }

    #[test]
    fn test_is_running_false_with_complete_status() {
        let mut step = make_default_step();
        step.completed_at = None;
        step.step_status = "complete".to_string();
        assert!(!step.is_running());
    }

    #[test]
    fn test_completed_successfully_true() {
        let mut step = make_default_step();
        step.step_status = "complete".to_string();
        assert!(step.completed_successfully());
    }

    #[test]
    fn test_completed_successfully_false() {
        let mut step = make_default_step();
        step.step_status = "error".to_string();
        assert!(!step.completed_successfully());
    }

    #[test]
    fn test_failed_true() {
        let mut step = make_default_step();
        step.step_status = "error".to_string();
        assert!(step.failed());
    }

    #[test]
    fn test_failed_false() {
        let mut step = make_default_step();
        step.step_status = "complete".to_string();
        assert!(!step.failed());
    }

    #[test]
    fn test_duration_display_seconds() {
        let mut step = make_default_step();
        step.duration_seconds = BigDecimal::from_str("30.5").unwrap();
        assert_eq!(step.duration_display(), "30.5s");
    }

    #[test]
    fn test_duration_display_minutes() {
        let mut step = make_default_step();
        step.duration_seconds = BigDecimal::from_str("150.0").unwrap();
        assert_eq!(step.duration_display(), "2.5m");
    }

    #[test]
    fn test_duration_display_hours() {
        let mut step = make_default_step();
        step.duration_seconds = BigDecimal::from_str("7200.0").unwrap();
        assert_eq!(step.duration_display(), "2.0h");
    }
}
