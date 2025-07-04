//! # System Health & Capacity Monitoring
//!
//! **CRITICAL**: This is NOT a database table - it's a computed view via SQL functions.
//!
//! ## Overview
//!
//! The `SystemHealthCounts` represents dynamically computed system-wide health
//! and capacity metrics. This data is **never stored** - it's calculated
//! on-demand using sophisticated SQL functions that analyze current system state.
//!
//! ## Human-Readable Explanation
//!
//! Think of this as your "mission control dashboard" - a real-time count of everything
//! happening in your workflow system right now. It's like looking at air traffic control
//! screens that show every plane in the sky and their status:
//!
//! - **"How much work is in the system?"** (Total tasks and steps by status)
//! - **"What's actively running?"** (In-progress counts)
//! - **"What's waiting to start?"** (Pending and ready counts)
//! - **"What needs attention?"** (Error and retry exhausted counts)
//! - **"Am I running out of capacity?"** (Connection pool utilization)
//! - **"Is the system stressed?"** (Backoff and retry patterns)
//!
//! ### Real-World Example
//!
//! For a busy e-commerce system during Black Friday:
//! ```text
//! Tasks:  Total: 5,429 | Running: 891 | Pending: 2,103 | Complete: 2,401 | Failed: 34
//! Steps:  Total: 48,672 | Running: 3,847 | Pending: 12,890 | Complete: 31,203 | Failed: 732
//! Retries: 245 retryable | 89 exhausted | 156 in backoff
//! Connections: 127/150 active (85% utilization - getting close to limit!)
//! Health Score: 78/100 (Fair - high load but managing)
//! ```
//!
//! This shows a system under heavy load but still functioning, though the high connection
//! utilization suggests you might need to scale up database connections soon.
//!
//! ## SQL Function Integration
//!
//! This module integrates with the PostgreSQL function:
//!
//! ### `get_system_health_counts_v01()`
//! - Computes comprehensive system health and capacity metrics
//! - Provides task and step counts by state
//! - Includes connection pool and capacity information
//! - Essential for monitoring and alerting
//!
//! ## Function Return Schema
//!
//! The function returns:
//! ```sql
//! RETURNS TABLE(
//!   total_tasks bigint,
//!   pending_tasks bigint,
//!   in_progress_tasks bigint,
//!   complete_tasks bigint,
//!   error_tasks bigint,
//!   cancelled_tasks bigint,
//!   total_steps bigint,
//!   pending_steps bigint,
//!   in_progress_steps bigint,
//!   complete_steps bigint,
//!   error_steps bigint,
//!   retryable_error_steps bigint,
//!   exhausted_retry_steps bigint,
//!   in_backoff_steps bigint,
//!   active_connections bigint,
//!   max_connections bigint
//! )
//! ```

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Represents computed system health and capacity metrics.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_system_health_counts_v01()` SQL function.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Current task states and counts across the system
/// - Step states, retry status, and backoff conditions
/// - Database connection pool utilization
/// - System capacity and resource usage
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
pub struct SystemHealthCounts {
    // Task counts by state
    pub total_tasks: i64,
    pub pending_tasks: i64,
    pub in_progress_tasks: i64,
    pub complete_tasks: i64,
    pub error_tasks: i64,
    pub cancelled_tasks: i64,

    // Step counts by state
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub complete_steps: i64,
    pub error_steps: i64,
    pub retryable_error_steps: i64,
    pub exhausted_retry_steps: i64,
    pub in_backoff_steps: i64,

    // System capacity metrics
    pub active_connections: i64,
    pub max_connections: i64,
}

/// System health summary with computed health indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthSummary {
    pub counts: SystemHealthCounts,
    pub task_completion_rate: f64,
    pub task_error_rate: f64,
    pub step_completion_rate: f64,
    pub step_error_rate: f64,
    pub connection_utilization: f64,
    pub overall_health_score: f64,
    pub health_status: String,
}

impl SystemHealthCounts {
    /// Get current system health counts using SQL function.
    ///
    /// This method calls the `get_system_health_counts_v01()` PostgreSQL function
    /// to compute real-time system health and capacity metrics.
    ///
    /// # Example Usage
    ///
    /// ```rust,ignore
    /// let health = SystemHealthCounts::get_current(&pool).await?;
    ///
    /// println!("Total tasks: {}", health.total_tasks);
    /// println!("Error tasks: {}", health.error_tasks);
    /// println!("Connection usage: {}/{}", health.active_connections, health.max_connections);
    /// ```
    pub async fn get_current(pool: &PgPool) -> Result<Option<SystemHealthCounts>, sqlx::Error> {
        let counts = sqlx::query_as!(
            SystemHealthCounts,
            r#"
            SELECT 
                total_tasks as "total_tasks!: i64",
                pending_tasks as "pending_tasks!: i64",
                in_progress_tasks as "in_progress_tasks!: i64",
                complete_tasks as "complete_tasks!: i64",
                error_tasks as "error_tasks!: i64",
                cancelled_tasks as "cancelled_tasks!: i64",
                total_steps as "total_steps!: i64",
                pending_steps as "pending_steps!: i64",
                in_progress_steps as "in_progress_steps!: i64",
                complete_steps as "complete_steps!: i64",
                error_steps as "error_steps!: i64",
                retryable_error_steps as "retryable_error_steps!: i64",
                exhausted_retry_steps as "exhausted_retry_steps!: i64",
                in_backoff_steps as "in_backoff_steps!: i64",
                active_connections as "active_connections!: i64",
                max_connections as "max_connections!: i64"
            FROM get_system_health_counts_v01()
            "#
        )
        .fetch_optional(pool)
        .await?;

        Ok(counts)
    }

    /// Get a comprehensive health summary with computed metrics.
    pub async fn get_health_summary(
        pool: &PgPool,
    ) -> Result<Option<SystemHealthSummary>, sqlx::Error> {
        if let Some(counts) = Self::get_current(pool).await? {
            let summary = SystemHealthSummary {
                task_completion_rate: counts.task_completion_rate(),
                task_error_rate: counts.task_error_rate(),
                step_completion_rate: counts.step_completion_rate(),
                step_error_rate: counts.step_error_rate(),
                connection_utilization: counts.connection_utilization(),
                overall_health_score: counts.overall_health_score(),
                health_status: counts.health_status(),
                counts,
            };
            Ok(Some(summary))
        } else {
            Ok(None)
        }
    }

    /// Calculate task completion rate (0.0 to 1.0).
    pub fn task_completion_rate(&self) -> f64 {
        if self.total_tasks == 0 {
            0.0
        } else {
            self.complete_tasks as f64 / self.total_tasks as f64
        }
    }

    /// Calculate task error rate (0.0 to 1.0).
    pub fn task_error_rate(&self) -> f64 {
        if self.total_tasks == 0 {
            0.0
        } else {
            self.error_tasks as f64 / self.total_tasks as f64
        }
    }

    /// Calculate step completion rate (0.0 to 1.0).
    pub fn step_completion_rate(&self) -> f64 {
        if self.total_steps == 0 {
            0.0
        } else {
            self.complete_steps as f64 / self.total_steps as f64
        }
    }

    /// Calculate step error rate (0.0 to 1.0).
    pub fn step_error_rate(&self) -> f64 {
        if self.total_steps == 0 {
            0.0
        } else {
            self.error_steps as f64 / self.total_steps as f64
        }
    }

    /// Calculate connection pool utilization (0.0 to 1.0).
    pub fn connection_utilization(&self) -> f64 {
        if self.max_connections == 0 {
            0.0
        } else {
            self.active_connections as f64 / self.max_connections as f64
        }
    }

    /// Calculate overall health score (0.0 to 100.0).
    ///
    /// This composite score considers:
    /// - Task and step completion rates (higher is better)
    /// - Error rates (lower is better)
    /// - Connection utilization (moderate is best)
    /// - Retry exhaustion (lower is better)
    pub fn overall_health_score(&self) -> f64 {
        let completion_score = (self.task_completion_rate() + self.step_completion_rate()) * 50.0;
        let error_penalty = (self.task_error_rate() + self.step_error_rate()) * 25.0;
        let connection_penalty = if self.connection_utilization() > 0.9 {
            10.0
        } else {
            0.0
        };
        let retry_penalty = if self.total_steps > 0 {
            (self.exhausted_retry_steps as f64 / self.total_steps as f64) * 15.0
        } else {
            0.0
        };

        (completion_score - error_penalty - connection_penalty - retry_penalty)
            .clamp(0.0, 100.0)
    }

    /// Get health status string based on health score.
    pub fn health_status(&self) -> String {
        let score = self.overall_health_score();

        if score >= 90.0 {
            "Excellent".to_string()
        } else if score >= 75.0 {
            "Good".to_string()
        } else if score >= 50.0 {
            "Fair".to_string()
        } else if score >= 25.0 {
            "Poor".to_string()
        } else {
            "Critical".to_string()
        }
    }

    /// Check if the system is considered healthy.
    pub fn is_healthy(&self) -> bool {
        self.overall_health_score() >= 75.0
    }

    /// Check if there are concerning error rates.
    pub fn has_high_error_rate(&self) -> bool {
        self.task_error_rate() > 0.1 || self.step_error_rate() > 0.1 // More than 10%
    }

    /// Check if connection pool is under stress.
    pub fn connection_pool_stressed(&self) -> bool {
        self.connection_utilization() > 0.8 // More than 80% utilization
    }

    /// Get count of active work (in-progress tasks and steps).
    pub fn active_work_count(&self) -> i64 {
        self.in_progress_tasks + self.in_progress_steps
    }

    /// Get count of blocked work (pending + backoff).
    pub fn blocked_work_count(&self) -> i64 {
        self.pending_tasks + self.pending_steps + self.in_backoff_steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_get_system_health_counts() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Test getting current system health counts
        let health = SystemHealthCounts::get_current(pool)
            .await
            .expect("Failed to get system health counts");

        // Should return some counts (might be zero for empty system)
        if let Some(h) = health {
            // All counts should be non-negative by definition

            // Test computed metrics
            let _task_completion = h.task_completion_rate();
            let _task_error = h.task_error_rate();
            let _step_completion = h.step_completion_rate();
            let _step_error = h.step_error_rate();
            let _connection_util = h.connection_utilization();
            let _health_score = h.overall_health_score();
            let _health_status = h.health_status();
            let _is_healthy = h.is_healthy();
            let _high_errors = h.has_high_error_rate();
            let _pool_stressed = h.connection_pool_stressed();
            let _active_work = h.active_work_count();
            let _blocked_work = h.blocked_work_count();
        }

        db.close().await;
    }

    #[tokio::test]
    async fn test_get_health_summary() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Test getting health summary
        let summary = SystemHealthCounts::get_health_summary(pool)
            .await
            .expect("Failed to get health summary");

        // Should return without error (might be None for empty system)
        if let Some(s) = summary {
            assert!(s.overall_health_score >= 0.0);
            assert!(s.overall_health_score <= 100.0);
            assert!(!s.health_status.is_empty());
        }

        db.close().await;
    }

    #[test]
    fn test_health_calculations() {
        let health = SystemHealthCounts {
            total_tasks: 100,
            pending_tasks: 10,
            in_progress_tasks: 20,
            complete_tasks: 60,
            error_tasks: 10,
            cancelled_tasks: 0,
            total_steps: 500,
            pending_steps: 50,
            in_progress_steps: 100,
            complete_steps: 300,
            error_steps: 40,
            retryable_error_steps: 30,
            exhausted_retry_steps: 10,
            in_backoff_steps: 10,
            active_connections: 8,
            max_connections: 10,
        };

        assert_eq!(health.task_completion_rate(), 0.6);
        assert_eq!(health.task_error_rate(), 0.1);
        assert_eq!(health.step_completion_rate(), 0.6);
        assert_eq!(health.step_error_rate(), 0.08);
        assert_eq!(health.connection_utilization(), 0.8);

        let health_score = health.overall_health_score();
        assert!(health_score > 0.0 && health_score <= 100.0);

        assert!(!health.health_status().is_empty());
        assert_eq!(health.active_work_count(), 120);
        assert_eq!(health.blocked_work_count(), 70);
    }
}
