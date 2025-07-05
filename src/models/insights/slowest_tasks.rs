//! # Slowest Tasks Analytics
//!
//! **CRITICAL**: This is NOT a database table - it's a computed view via SQL functions.
//!
//! ## Overview
//!
//! The `SlowestTasks` represents dynamically computed analytics for identifying
//! the slowest performing tasks. This data is **never stored** - it's calculated
//! on-demand using sophisticated SQL functions that analyze task execution times.
//!
//! ## SQL Function Integration
//!
//! This module integrates with the PostgreSQL function:
//!
//! ### `get_slowest_tasks_v01(since_timestamp, limit_count, namespace_filter, task_name_filter, version_filter)`
//! - Identifies the slowest executing tasks
//! - Supports filtering by namespace, task name, and version
//! - Returns configurable number of results with comprehensive task analysis
//!
//! ## Function Return Schema
//!
//! The function returns:
//! ```sql
//! RETURNS TABLE(
//!   task_id bigint,
//!   task_name character varying,
//!   namespace_name character varying,
//!   version character varying,
//!   duration_seconds numeric,
//!   step_count bigint,
//!   completed_steps bigint,
//!   error_steps bigint,
//!   created_at timestamp with time zone,
//!   completed_at timestamp with time zone,
//!   initiator character varying,
//!   source_system character varying
//! )
//! ```

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::BigDecimal, FromRow, PgPool};

/// Represents computed slowest tasks analytics.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_slowest_tasks_v01()` SQL function.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Task execution durations and completion times
/// - Step counts and completion statistics
/// - Task context and execution metadata
/// - Task performance patterns across namespaces
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
pub struct SlowestTasks {
    pub task_id: i64,
    pub task_name: String,
    pub namespace_name: String,
    pub version: String,
    pub duration_seconds: BigDecimal,
    pub step_count: i64,
    pub completed_steps: i64,
    pub error_steps: i64,
    pub created_at: NaiveDateTime,
    pub completed_at: Option<NaiveDateTime>,
    pub initiator: Option<String>,
    pub source_system: Option<String>,
}

/// Filter parameters for slowest tasks analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowestTasksFilter {
    pub since_timestamp: Option<DateTime<Utc>>,
    pub limit_count: Option<i32>,
    pub namespace_filter: Option<String>,
    pub task_name_filter: Option<String>,
    pub version_filter: Option<String>,
}

impl Default for SlowestTasksFilter {
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

impl SlowestTasks {
    /// Get slowest tasks with default filters (top 10).
    pub async fn get_slowest(pool: &PgPool) -> Result<Vec<SlowestTasks>, sqlx::Error> {
        Self::get_with_filters(pool, SlowestTasksFilter::default()).await
    }

    /// Get slowest tasks since a specific timestamp.
    pub async fn get_since(
        pool: &PgPool,
        since_timestamp: DateTime<Utc>,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestTasks>, sqlx::Error> {
        let filter = SlowestTasksFilter {
            since_timestamp: Some(since_timestamp),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest tasks for a specific namespace.
    pub async fn get_by_namespace(
        pool: &PgPool,
        namespace: &str,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestTasks>, sqlx::Error> {
        let filter = SlowestTasksFilter {
            namespace_filter: Some(namespace.to_string()),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest tasks for a specific task name.
    pub async fn get_by_task_name(
        pool: &PgPool,
        task_name: &str,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestTasks>, sqlx::Error> {
        let filter = SlowestTasksFilter {
            task_name_filter: Some(task_name.to_string()),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest tasks with custom filters.
    pub async fn get_with_filters(
        pool: &PgPool,
        filter: SlowestTasksFilter,
    ) -> Result<Vec<SlowestTasks>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            SlowestTasks,
            r#"
            SELECT 
                task_id as "task_id!: i64",
                task_name as "task_name!: String",
                namespace_name as "namespace_name!: String",
                version as "version!: String",
                duration_seconds as "duration_seconds!: BigDecimal",
                step_count as "step_count!: i64",
                completed_steps as "completed_steps!: i64",
                error_steps as "error_steps!: i64",
                created_at as "created_at!: NaiveDateTime",
                completed_at as "completed_at?: NaiveDateTime",
                initiator,
                source_system
            FROM get_slowest_tasks_v01($1, $2, $3, $4, $5)
            "#,
            filter.since_timestamp,
            filter.limit_count.unwrap_or(10),
            filter.namespace_filter,
            filter.task_name_filter,
            filter.version_filter
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Get duration in seconds as a float.
    pub fn duration_as_seconds(&self) -> f64 {
        self.duration_seconds.to_string().parse().unwrap_or(0.0)
    }

    /// Calculate completion percentage (0.0 to 1.0).
    pub fn completion_ratio(&self) -> f64 {
        if self.step_count == 0 {
            0.0
        } else {
            self.completed_steps as f64 / self.step_count as f64
        }
    }

    /// Calculate error percentage (0.0 to 1.0).
    pub fn error_ratio(&self) -> f64 {
        if self.step_count == 0 {
            0.0
        } else {
            self.error_steps as f64 / self.step_count as f64
        }
    }

    /// Check if this task is still running.
    pub fn is_running(&self) -> bool {
        self.completed_at.is_none()
    }

    /// Check if this task completed successfully (all steps done, no errors).
    pub fn completed_successfully(&self) -> bool {
        self.completed_at.is_some()
            && self.completed_steps == self.step_count
            && self.error_steps == 0
    }

    /// Check if this task has errors.
    pub fn has_errors(&self) -> bool {
        self.error_steps > 0
    }

    /// Get pending steps count.
    pub fn pending_steps(&self) -> i64 {
        self.step_count - self.completed_steps - self.error_steps
    }

    /// Get human-readable duration string.
    pub fn duration_display(&self) -> String {
        let seconds = self.duration_as_seconds();

        if seconds < 60.0 {
            format!("{seconds:.1}s")
        } else if seconds < 3600.0 {
            format!("{:.1}m", seconds / 60.0)
        } else if seconds < 86400.0 {
            format!("{:.1}h", seconds / 3600.0)
        } else {
            format!("{:.1}d", seconds / 86400.0)
        }
    }

    /// Get completion percentage as a display string.
    pub fn completion_display(&self) -> String {
        format!("{:.1}%", self.completion_ratio() * 100.0)
    }

    /// Get a status summary string.
    pub fn status_summary(&self) -> String {
        if self.completed_successfully() {
            "Completed".to_string()
        } else if self.has_errors() {
            format!("Errors ({} failed)", self.error_steps)
        } else if self.is_running() {
            format!("Running ({:.1}%)", self.completion_ratio() * 100.0)
        } else {
            "Unknown".to_string()
        }
    }
}
