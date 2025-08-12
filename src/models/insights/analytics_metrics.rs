//! # System Analytics & Performance Metrics
//!
//! **CRITICAL**: This is NOT a database table - it's a computed view via SQL functions.
//!
//! ## Overview
//!
//! The `AnalyticsMetrics` represents dynamically computed analytics and performance
//! metrics for the entire Tasker system. This data is **never stored** - it's calculated
//! on-demand using sophisticated SQL functions that analyze system-wide performance.
//!
//! ## Human-Readable Explanation
//!
//! Think of this as your "system health dashboard" - a comprehensive view of how your entire
//! workflow orchestration system is performing. It's like checking the vital signs of your
//! system to understand:
//!
//! - **"Is my system healthy?"** (Overall health score and system status)
//! - **"How busy is the system?"** (Active tasks and throughput rates)
//! - **"Are things running smoothly?"** (Error rates and completion rates)
//! - **"How fast are workflows completing?"** (Average durations and performance trends)
//! - **"Do I need to scale up?"** (Task and step throughput analysis)
//!
//! ### Real-World Example
//!
//! For a production system processing customer orders:
//! ```text
//! System Health Score: 87/100 (Good)
//! Active Tasks: 1,247 currently running
//! Task Throughput: 156 tasks/hour
//! Completion Rate: 94.2% (excellent)
//! Error Rate: 2.1% (acceptable)
//! Avg Task Duration: 8.5 minutes
//! Total Namespaces: 12 different workflows
//! Analysis Period: Last 24 hours
//! ```
//!
//! This immediately tells you the system is performing well with low error rates,
//! but you might want to investigate the 2.1% error rate to make it even better.
//!
//! ## SQL Function Integration
//!
//! This module integrates with the PostgreSQL function:
//!
//! ### `get_analytics_metrics(since_timestamp timestamp with time zone)`
//! - Computes comprehensive system analytics and performance metrics
//! - Includes task throughput, completion rates, error rates, and health scores
//! - Analyzes recent activity for trend analysis
//!
//! ## Function Return Schema
//!
//! The function returns:
//! ```sql
//! RETURNS TABLE(
//!   active_tasks_count bigint,
//!   total_namespaces_count bigint,
//!   unique_task_types_count bigint,
//!   system_health_score numeric,
//!   task_throughput bigint,
//!   completion_count bigint,
//!   error_count bigint,
//!   completion_rate numeric,
//!   error_rate numeric,
//!   avg_task_duration numeric,
//!   avg_step_duration numeric,
//!   step_throughput bigint,
//!   analysis_period_start timestamp with time zone,
//!   calculated_at timestamp with time zone
//! )
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::BigDecimal, FromRow, PgPool};

/// Represents computed system analytics and performance metrics.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_analytics_metrics()` SQL function.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Active task counts and states
/// - Task and step throughput rates
/// - Completion and error statistics
/// - Average durations and performance metrics
/// - System health indicators
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
pub struct AnalyticsMetrics {
    pub active_tasks_count: i64,
    pub total_namespaces_count: i64,
    pub unique_task_types_count: i64,
    pub system_health_score: BigDecimal,
    pub task_throughput: i64,
    pub completion_count: i64,
    pub error_count: i64,
    pub completion_rate: BigDecimal,
    pub error_rate: BigDecimal,
    pub avg_task_duration: BigDecimal,
    pub avg_step_duration: BigDecimal,
    pub step_throughput: i64,
    pub analysis_period_start: DateTime<Utc>,
    pub calculated_at: DateTime<Utc>,
}

impl AnalyticsMetrics {
    /// Get current system analytics metrics using SQL function.
    ///
    /// This method calls the `get_analytics_metrics()` PostgreSQL function
    /// to compute real-time analytics and performance statistics for the entire system.
    ///
    /// # Parameters
    ///
    /// - `since_timestamp`: Optional timestamp to analyze metrics from a specific point in time.
    ///   If None, uses system default (typically last 24 hours).
    ///
    /// # Example Usage
    ///
    /// ```rust,no_run
    /// use chrono::{Duration, Utc};
    /// use sqlx::PgPool;
    /// use tasker_core::models::insights::AnalyticsMetrics;
    ///
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// // Get current metrics (last 24 hours)
    /// let metrics = AnalyticsMetrics::get_current(&pool).await?;
    ///
    /// if let Some(metrics) = metrics {
    ///     println!("Active tasks: {}", metrics.active_tasks_count);
    ///     println!("Completed tasks: {}", metrics.completion_count);
    /// }
    ///
    /// // Get metrics since a specific time
    /// let since = Utc::now() - Duration::hours(6);
    /// let recent_metrics = AnalyticsMetrics::get_since(&pool, Some(since)).await?;
    ///
    /// if let Some(recent) = recent_metrics {
    ///     println!("Tasks completed in last 6 hours: {}", recent.completion_count);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For complete working examples with database setup, see the integration tests in
    /// `tests/models/analytics_metrics.rs`.
    pub async fn get_current(pool: &PgPool) -> Result<Option<AnalyticsMetrics>, sqlx::Error> {
        Self::get_since(pool, None).await
    }

    /// Get analytics metrics since a specific timestamp.
    pub async fn get_since(
        pool: &PgPool,
        since_timestamp: Option<DateTime<Utc>>,
    ) -> Result<Option<AnalyticsMetrics>, sqlx::Error> {
        let metrics = sqlx::query_as!(
            AnalyticsMetrics,
            r#"
            SELECT
                active_tasks_count as "active_tasks_count!: i64",
                total_namespaces_count as "total_namespaces_count!: i64",
                unique_task_types_count as "unique_task_types_count!: i64",
                system_health_score as "system_health_score!: BigDecimal",
                task_throughput as "task_throughput!: i64",
                completion_count as "completion_count!: i64",
                error_count as "error_count!: i64",
                completion_rate as "completion_rate!: BigDecimal",
                error_rate as "error_rate!: BigDecimal",
                avg_task_duration as "avg_task_duration!: BigDecimal",
                avg_step_duration as "avg_step_duration!: BigDecimal",
                step_throughput as "step_throughput!: i64",
                analysis_period_start as "analysis_period_start!: DateTime<Utc>",
                calculated_at as "calculated_at!: DateTime<Utc>"
            FROM get_analytics_metrics($1)
            "#,
            since_timestamp
        )
        .fetch_optional(pool)
        .await?;

        Ok(metrics)
    }

    /// Check if the system is healthy based on health score.
    pub fn is_healthy(&self) -> bool {
        self.system_health_score >= BigDecimal::from(80)
    }

    /// Check if error rate is concerning (>5%).
    pub fn has_high_error_rate(&self) -> bool {
        self.error_rate > BigDecimal::from(5)
    }

    /// Get completion rate as a percentage (0.0 to 100.0).
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_core::models::insights::analytics_metrics::AnalyticsMetrics;
    /// use sqlx::types::BigDecimal;
    /// use std::str::FromStr;
    /// use chrono::{DateTime, Utc};
    ///
    /// let metrics = AnalyticsMetrics {
    ///     active_tasks_count: 100,
    ///     total_namespaces_count: 5,
    ///     unique_task_types_count: 12,
    ///     system_health_score: BigDecimal::from_str("85.5").unwrap(),
    ///     task_throughput: 156,
    ///     completion_count: 94,
    ///     error_count: 6,
    ///     completion_rate: BigDecimal::from_str("94.2").unwrap(),
    ///     error_rate: BigDecimal::from_str("6.0").unwrap(),
    ///     avg_task_duration: BigDecimal::from_str("512.5").unwrap(),
    ///     avg_step_duration: BigDecimal::from_str("45.2").unwrap(),
    ///     step_throughput: 320,
    ///     analysis_period_start: "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
    ///     calculated_at: "2024-01-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap(),
    /// };
    ///
    /// assert_eq!(metrics.completion_percentage(), 94.2);
    /// ```
    pub fn completion_percentage(&self) -> f64 {
        self.completion_rate.to_string().parse().unwrap_or(0.0)
    }

    /// Get error rate as a percentage (0.0 to 100.0).
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_core::models::insights::analytics_metrics::AnalyticsMetrics;
    /// use sqlx::types::BigDecimal;
    /// use std::str::FromStr;
    /// use chrono::{DateTime, Utc};
    ///
    /// let metrics = AnalyticsMetrics {
    ///     active_tasks_count: 100,
    ///     total_namespaces_count: 5,
    ///     unique_task_types_count: 12,
    ///     system_health_score: BigDecimal::from_str("85.5").unwrap(),
    ///     task_throughput: 156,
    ///     completion_count: 94,
    ///     error_count: 6,
    ///     completion_rate: BigDecimal::from_str("94.2").unwrap(),
    ///     error_rate: BigDecimal::from_str("2.1").unwrap(),
    ///     avg_task_duration: BigDecimal::from_str("512.5").unwrap(),
    ///     avg_step_duration: BigDecimal::from_str("45.2").unwrap(),
    ///     step_throughput: 320,
    ///     analysis_period_start: "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
    ///     calculated_at: "2024-01-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap(),
    /// };
    ///
    /// assert_eq!(metrics.error_percentage(), 2.1);
    /// ```
    pub fn error_percentage(&self) -> f64 {
        self.error_rate.to_string().parse().unwrap_or(0.0)
    }

    /// Get average task duration in seconds.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_core::models::insights::analytics_metrics::AnalyticsMetrics;
    /// use sqlx::types::BigDecimal;
    /// use std::str::FromStr;
    /// use chrono::{DateTime, Utc};
    ///
    /// let metrics = AnalyticsMetrics {
    ///     active_tasks_count: 100,
    ///     total_namespaces_count: 5,
    ///     unique_task_types_count: 12,
    ///     system_health_score: BigDecimal::from_str("85.5").unwrap(),
    ///     task_throughput: 156,
    ///     completion_count: 94,
    ///     error_count: 6,
    ///     completion_rate: BigDecimal::from_str("94.2").unwrap(),
    ///     error_rate: BigDecimal::from_str("2.1").unwrap(),
    ///     avg_task_duration: BigDecimal::from_str("312.75").unwrap(), // 5 minutes and 12.75 seconds
    ///     avg_step_duration: BigDecimal::from_str("45.2").unwrap(),
    ///     step_throughput: 320,
    ///     analysis_period_start: "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
    ///     calculated_at: "2024-01-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap(),
    /// };
    ///
    /// assert_eq!(metrics.avg_task_duration_seconds(), 312.75);
    /// ```
    pub fn avg_task_duration_seconds(&self) -> f64 {
        self.avg_task_duration.to_string().parse().unwrap_or(0.0)
    }

    /// Get average step duration in seconds.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_core::models::insights::analytics_metrics::AnalyticsMetrics;
    /// use sqlx::types::BigDecimal;
    /// use std::str::FromStr;
    /// use chrono::{DateTime, Utc};
    ///
    /// let metrics = AnalyticsMetrics {
    ///     active_tasks_count: 100,
    ///     total_namespaces_count: 5,
    ///     unique_task_types_count: 12,
    ///     system_health_score: BigDecimal::from_str("85.5").unwrap(),
    ///     task_throughput: 156,
    ///     completion_count: 94,
    ///     error_count: 6,
    ///     completion_rate: BigDecimal::from_str("94.2").unwrap(),
    ///     error_rate: BigDecimal::from_str("2.1").unwrap(),
    ///     avg_task_duration: BigDecimal::from_str("312.75").unwrap(),
    ///     avg_step_duration: BigDecimal::from_str("45.8").unwrap(), // About 45.8 seconds per step
    ///     step_throughput: 320,
    ///     analysis_period_start: "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
    ///     calculated_at: "2024-01-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap(),
    /// };
    ///
    /// assert_eq!(metrics.avg_step_duration_seconds(), 45.8);
    /// ```
    pub fn avg_step_duration_seconds(&self) -> f64 {
        self.avg_step_duration.to_string().parse().unwrap_or(0.0)
    }
}
