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
//! ### `get_system_health_counts()`
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
//!   max_connections bigint,
//!   enqueued_steps bigint
//! )
//! ```

use crate::database::sql_functions::{
    SqlFunctionExecutor, SystemHealthCounts as SqlSystemHealthCounts,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Represents computed system health and capacity metrics.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_system_health_counts()` SQL function.
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
    // Task counts by state - simplified to match SQL function
    pub total_tasks: i64,
    pub pending_tasks: i64,
    pub in_progress_tasks: i64,
    pub complete_tasks: i64,
    pub error_tasks: i64,
    pub cancelled_tasks: i64,

    // Step counts by state - updated to match sql_functions.rs schema
    pub total_steps: i64,
    pub pending_steps: i64,
    pub enqueued_steps: i64,
    pub in_progress_steps: i64,
    pub enqueued_for_orchestration_steps: i64,
    pub enqueued_as_error_for_orchestration_steps: i64,
    pub waiting_for_retry_steps: i64,
    pub complete_steps: i64,
    pub error_steps: i64,
    pub cancelled_steps: i64,
    pub resolved_manually_steps: i64,

    // Connection metrics
    pub active_connections: i64,
    pub max_connections: i64,

    // Computed retry metrics (for backward compatibility)
    pub retryable_error_steps: i64,
    pub exhausted_retry_steps: i64,
}

/// System health summary with computed health indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthSummary {
    pub counts: SystemHealthCounts,
    pub task_completion_rate: f64,
    pub task_error_rate: f64,
    pub step_completion_rate: f64,
    pub step_error_rate: f64,
    pub overall_health_score: f64,
    pub health_status: String,
}

impl SystemHealthCounts {
    /// Convert from sql_functions::SystemHealthCounts to insights::SystemHealthCounts
    fn from_sql_function_result(sql_counts: SqlSystemHealthCounts) -> Self {
        // Compute retry metrics from available data
        let retryable_error_steps = sql_counts.waiting_for_retry_steps;
        let exhausted_retry_steps = if sql_counts.error_steps >= sql_counts.waiting_for_retry_steps
        {
            sql_counts.error_steps - sql_counts.waiting_for_retry_steps
        } else {
            0
        };

        Self {
            total_tasks: sql_counts.total_tasks,
            pending_tasks: sql_counts.pending_tasks,
            in_progress_tasks: sql_counts.in_progress_tasks,
            complete_tasks: sql_counts.complete_tasks,
            error_tasks: sql_counts.error_tasks,
            cancelled_tasks: sql_counts.cancelled_tasks,
            total_steps: sql_counts.total_steps,
            pending_steps: sql_counts.pending_steps,
            enqueued_steps: sql_counts.enqueued_steps,
            in_progress_steps: sql_counts.in_progress_steps,
            enqueued_for_orchestration_steps: sql_counts.enqueued_for_orchestration_steps,
            enqueued_as_error_for_orchestration_steps: sql_counts
                .enqueued_as_error_for_orchestration_steps,
            waiting_for_retry_steps: sql_counts.waiting_for_retry_steps,
            complete_steps: sql_counts.complete_steps,
            error_steps: sql_counts.error_steps,
            cancelled_steps: sql_counts.cancelled_steps,
            resolved_manually_steps: sql_counts.resolved_manually_steps,
            active_connections: sql_counts.active_connections,
            max_connections: sql_counts.max_connections,
            retryable_error_steps,
            exhausted_retry_steps,
        }
    }
    /// Get current system health counts using SQL function (delegates to sql_functions.rs).
    ///
    /// This method uses the standardized `SqlFunctionExecutor` to call the
    /// `get_system_health_counts()` PostgreSQL function, ensuring consistency
    /// with the rest of the system.
    ///
    /// # Example Usage
    ///
    /// ```rust,no_run
    /// use sqlx::PgPool;
    /// use tasker_shared::models::insights::SystemHealthCounts;
    ///
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let health = SystemHealthCounts::get_current(&pool).await?;
    ///
    /// if let Some(health) = health {
    ///     println!("System Health Score: {:.1}", health.overall_health_score());
    ///     println!("Total tasks: {}", health.total_tasks);
    ///     println!("Error tasks: {}", health.error_tasks);
    ///     println!("Is healthy: {}", health.is_healthy());
    ///     println!("Steps in process: {}", health.in_progress_steps);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For complete examples with test data, see `tests/models/system_health_counts.rs`.
    pub async fn get_current(pool: &PgPool) -> Result<Option<SystemHealthCounts>, sqlx::Error> {
        // DELEGATION: Use standard sql_functions.rs approach for consistency
        let executor = SqlFunctionExecutor::new(pool.clone());
        let sql_counts = executor.get_system_health_counts().await?;
        let insights_counts = Self::from_sql_function_result(sql_counts);
        Ok(Some(insights_counts))
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

    /// Calculate overall health score (0.0 to 100.0).
    ///
    /// This composite score considers:
    /// - Task and step completion rates (higher is better)
    /// - Error rates (lower is better)
    /// - Connection utilization (moderate is best)
    /// - Retry exhaustion (lower is better)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::models::insights::SystemHealthCounts;
    ///
    /// // Example with healthy system metrics
    /// let healthy_system = SystemHealthCounts {
    ///     total_tasks: 100,
    ///     pending_tasks: 5,
    ///     in_progress_tasks: 10,
    ///     complete_tasks: 85,
    ///     error_tasks: 0,
    ///     cancelled_tasks: 0,
    ///     total_steps: 300,
    ///     pending_steps: 10,
    ///     enqueued_steps: 15,
    ///     in_progress_steps: 20,
    ///     enqueued_for_orchestration_steps: 5,
    ///     enqueued_as_error_for_orchestration_steps: 0,
    ///     waiting_for_retry_steps: 0,
    ///     complete_steps: 250,
    ///     error_steps: 0,
    ///     cancelled_steps: 0,
    ///     resolved_manually_steps: 5,
    ///     active_connections: 15,
    ///     max_connections: 100,
    ///     retryable_error_steps: 0,
    ///     exhausted_retry_steps: 0,
    /// };
    ///
    /// let score = healthy_system.overall_health_score();
    /// assert!(score > 75.0, "Healthy system should score above 75");
    /// assert!(healthy_system.is_healthy());
    /// ```
    pub fn overall_health_score(&self) -> f64 {
        let completion_score = (self.task_completion_rate() + self.step_completion_rate()) * 50.0;
        let error_penalty = (self.task_error_rate() + self.step_error_rate()) * 25.0;

        // Connection utilization penalty (higher utilization = higher penalty)
        let connection_utilization = if self.max_connections > 0 {
            self.active_connections as f64 / self.max_connections as f64
        } else {
            0.0
        };
        let connection_penalty = connection_utilization * 15.0;

        // Retry penalty based on steps waiting for retry
        let retry_ratio = if self.total_steps > 0 {
            self.waiting_for_retry_steps as f64 / self.total_steps as f64
        } else {
            0.0
        };
        let retry_penalty = retry_ratio * 10.0;

        (completion_score - error_penalty - connection_penalty - retry_penalty).clamp(0.0, 100.0)
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

    /// Get count of active work (in progress tasks and in progress steps).
    pub fn active_work_count(&self) -> i64 {
        self.in_progress_tasks + self.in_progress_steps
    }

    /// Check if there are steps enqueued for processing.
    pub fn has_enqueued_steps(&self) -> bool {
        self.enqueued_steps > 0
    }

    /// Get count of steps waiting for retry (new TAS-41 field).
    pub fn waiting_for_retry_count(&self) -> i64 {
        self.waiting_for_retry_steps
    }

    /// Check if connection pool is under pressure (> 80% utilization).
    pub fn has_connection_pressure(&self) -> bool {
        if self.max_connections > 0 {
            let utilization = self.active_connections as f64 / self.max_connections as f64;
            utilization > 0.8
        } else {
            false
        }
    }

    /// Get connection utilization percentage (0.0 to 1.0).
    pub fn connection_utilization(&self) -> f64 {
        if self.max_connections > 0 {
            self.active_connections as f64 / self.max_connections as f64
        } else {
            0.0
        }
    }
}
