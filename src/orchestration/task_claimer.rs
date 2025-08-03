//! # Task Claimer
//!
//! ## Architecture: Distributed Task Claiming with Priority Fairness
//!
//! The TaskClaimer component uses the `tasker_ready_tasks` view and `claim_ready_tasks()` SQL function
//! to atomically claim tasks for processing. This ensures only one orchestrator processes a task's
//! ready steps at a time while maintaining the priority fairness solution.
//!
//! ## Key Features
//!
//! - **Atomic Task Claiming**: Uses `FOR UPDATE SKIP LOCKED` for distributed safety
//! - **Priority Fairness**: Respects time-weighted computed priority to prevent starvation
//! - **Configurable Claiming**: Supports namespace filtering and batch claiming
//! - **Heartbeat Support**: Can extend claims during long-running operations
//! - **Monitoring Integration**: Returns computed priority and age metrics
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::orchestration::task_claimer::TaskClaimer;
//! use sqlx::PgPool;
//!
//! # async fn example(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! let claimer = TaskClaimer::new(pool, "orchestrator-host123-uuid".to_string());
//!
//! // Claim up to 5 tasks with priority fairness
//! let claimed_tasks = claimer.claim_ready_tasks(5, None).await?;
//!
//! for task in claimed_tasks {
//!     println!("Claimed task {} with computed priority {}",
//!              task.task_id, task.computed_priority);
//!     
//!     // Process the task...
//!     
//!     // Release the claim when done
//!     claimer.release_task_claim(task.task_id).await?;
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, TaskerError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

/// Represents a claimed task with priority fairness metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedTask {
    /// Task ID that was claimed
    pub task_id: i64,
    /// Namespace of the claimed task
    pub namespace_name: String,
    /// Base priority level (1=Low, 2=Normal, 3=High, 4=Urgent)
    pub priority: i32,
    /// Computed priority including time-weighted escalation
    pub computed_priority: f64,
    /// Task age in hours for monitoring
    pub age_hours: f64,
    /// Number of ready steps to be processed
    pub ready_steps_count: i64,
    /// Claim timeout in seconds
    pub claim_timeout_seconds: i32,
    /// When this task was claimed
    pub claimed_at: DateTime<Utc>,
}

/// Configuration for task claiming behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimerConfig {
    /// Maximum number of tasks to claim in a single batch
    pub max_batch_size: i32,
    /// Default claim timeout in seconds
    pub default_claim_timeout: i32,
    /// Heartbeat interval for extending claims
    pub heartbeat_interval: Duration,
    /// Enable claim extension during processing
    pub enable_heartbeat: bool,
}

impl Default for TaskClaimerConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 10,
            default_claim_timeout: 300,                  // 5 minutes
            heartbeat_interval: Duration::from_secs(60), // 1 minute
            enable_heartbeat: true,
        }
    }
}

/// Task claiming component for distributed orchestration
pub struct TaskClaimer {
    pool: PgPool,
    orchestrator_id: String,
    config: TaskClaimerConfig,
}

impl TaskClaimer {
    /// Create a new task claimer instance
    pub fn new(pool: PgPool, orchestrator_id: String) -> Self {
        Self {
            pool,
            orchestrator_id,
            config: TaskClaimerConfig::default(),
        }
    }

    /// Create a new task claimer with custom configuration
    pub fn with_config(pool: PgPool, orchestrator_id: String, config: TaskClaimerConfig) -> Self {
        Self {
            pool,
            orchestrator_id,
            config,
        }
    }

    /// Claim ready tasks atomically using priority fairness ordering
    ///
    /// This uses the `claim_ready_tasks()` SQL function which implements the priority fairness
    /// solution with time-weighted computed priority to prevent starvation.
    #[instrument(skip(self), fields(orchestrator_id = %self.orchestrator_id))]
    pub async fn claim_ready_tasks(
        &self,
        limit: i32,
        namespace_filter: Option<&str>,
    ) -> Result<Vec<ClaimedTask>> {
        let actual_limit = std::cmp::min(limit, self.config.max_batch_size);

        debug!(
            limit = actual_limit,
            namespace_filter = namespace_filter,
            "Claiming ready tasks with priority fairness"
        );

        let query = r#"
            SELECT task_id, namespace_name, priority, computed_priority, age_hours, 
                   ready_steps_count, claim_timeout_seconds
            FROM claim_ready_tasks($1::VARCHAR, $2::INTEGER, $3::VARCHAR)
        "#;

        let rows = sqlx::query_as::<_, ClaimedTaskRow>(query)
            .bind(&self.orchestrator_id)
            .bind(actual_limit)
            .bind(namespace_filter)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to claim ready tasks: {}", e);
                TaskerError::DatabaseError(format!("Task claiming failed: {}", e))
            })?;

        let claimed_tasks: Vec<ClaimedTask> = rows
            .into_iter()
            .map(|row| ClaimedTask {
                task_id: row.task_id,
                namespace_name: row.namespace_name,
                priority: row.priority,
                computed_priority: row.computed_priority,
                age_hours: row.age_hours,
                ready_steps_count: row.ready_steps_count,
                claim_timeout_seconds: row.claim_timeout_seconds,
                claimed_at: Utc::now(),
            })
            .collect();

        if !claimed_tasks.is_empty() {
            info!(
                claimed_count = claimed_tasks.len(),
                orchestrator_id = %self.orchestrator_id,
                "Successfully claimed tasks with priority fairness"
            );

            // Log priority distribution for monitoring
            let priority_summary = self.summarize_claimed_priorities(&claimed_tasks);
            debug!(priority_summary = ?priority_summary, "Claimed task priority distribution");
        } else {
            debug!("No ready tasks available for claiming");
        }

        Ok(claimed_tasks)
    }

    /// Release a task claim when processing is complete or on error
    #[instrument(skip(self), fields(orchestrator_id = %self.orchestrator_id))]
    pub async fn release_task_claim(&self, task_id: i64) -> Result<bool> {
        debug!(task_id = task_id, "Releasing task claim");

        let query = "SELECT release_task_claim($1::BIGINT, $2::VARCHAR) as released";

        let row: (bool,) = sqlx::query_as(query)
            .bind(task_id)
            .bind(&self.orchestrator_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to release task claim {}: {}", task_id, e);
                TaskerError::DatabaseError(format!("Task claim release failed: {}", e))
            })?;

        let released = row.0;

        if released {
            debug!(task_id = task_id, "Task claim released successfully");
        } else {
            warn!(
                task_id = task_id,
                orchestrator_id = %self.orchestrator_id,
                "Task claim was not released (not owned by this orchestrator or already released)"
            );
        }

        Ok(released)
    }

    /// Extend a task claim to prevent timeout during long-running operations (heartbeat)
    #[instrument(skip(self), fields(orchestrator_id = %self.orchestrator_id))]
    pub async fn extend_task_claim(&self, task_id: i64) -> Result<bool> {
        debug!(task_id = task_id, "Extending task claim (heartbeat)");

        let query = "SELECT extend_task_claim($1::BIGINT, $2::VARCHAR) as extended";

        let row: (bool,) = sqlx::query_as(query)
            .bind(task_id)
            .bind(&self.orchestrator_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to extend task claim {}: {}", task_id, e);
                TaskerError::DatabaseError(format!("Task claim extension failed: {}", e))
            })?;

        let extended = row.0;

        if extended {
            debug!(task_id = task_id, "Task claim extended successfully");
        } else {
            warn!(
                task_id = task_id,
                orchestrator_id = %self.orchestrator_id,
                "Task claim was not extended (not owned by this orchestrator or not found)"
            );
        }

        Ok(extended)
    }

    /// Get current orchestrator ID
    pub fn orchestrator_id(&self) -> &str {
        &self.orchestrator_id
    }

    /// Get current configuration
    pub fn config(&self) -> &TaskClaimerConfig {
        &self.config
    }

    /// Create a priority summary for monitoring and debugging
    fn summarize_claimed_priorities(&self, tasks: &[ClaimedTask]) -> PrioritySummary {
        let mut summary = PrioritySummary::default();

        for task in tasks {
            match task.priority {
                4 => {
                    summary.urgent_count += 1;
                    summary.urgent_avg_computed_priority += task.computed_priority;
                }
                3 => {
                    summary.high_count += 1;
                    summary.high_avg_computed_priority += task.computed_priority;
                }
                2 => {
                    summary.normal_count += 1;
                    summary.normal_avg_computed_priority += task.computed_priority;
                }
                1 => {
                    summary.low_count += 1;
                    summary.low_avg_computed_priority += task.computed_priority;
                }
                _ => {
                    summary.invalid_count += 1;
                    summary.invalid_avg_computed_priority += task.computed_priority;
                }
            }

            if task.computed_priority > task.priority as f64 {
                summary.escalated_count += 1;
            }
        }

        // Calculate averages
        if summary.urgent_count > 0 {
            summary.urgent_avg_computed_priority /= summary.urgent_count as f64;
        }
        if summary.high_count > 0 {
            summary.high_avg_computed_priority /= summary.high_count as f64;
        }
        if summary.normal_count > 0 {
            summary.normal_avg_computed_priority /= summary.normal_count as f64;
        }
        if summary.low_count > 0 {
            summary.low_avg_computed_priority /= summary.low_count as f64;
        }
        if summary.invalid_count > 0 {
            summary.invalid_avg_computed_priority /= summary.invalid_count as f64;
        }

        summary
    }
}

/// Internal struct for SQL query results
#[derive(sqlx::FromRow)]
struct ClaimedTaskRow {
    task_id: i64,
    namespace_name: String,
    priority: i32,
    computed_priority: f64,
    age_hours: f64,
    ready_steps_count: i64,
    claim_timeout_seconds: i32,
}

/// Priority distribution summary for monitoring
#[derive(Debug, Default)]
struct PrioritySummary {
    urgent_count: i32,
    high_count: i32,
    normal_count: i32,
    low_count: i32,
    invalid_count: i32,
    escalated_count: i32,
    urgent_avg_computed_priority: f64,
    high_avg_computed_priority: f64,
    normal_avg_computed_priority: f64,
    low_avg_computed_priority: f64,
    invalid_avg_computed_priority: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_claimer_config_defaults() {
        let config = TaskClaimerConfig::default();
        assert_eq!(config.max_batch_size, 10);
        assert_eq!(config.default_claim_timeout, 300);
        assert_eq!(config.heartbeat_interval, Duration::from_secs(60));
        assert!(config.enable_heartbeat);
    }

    #[test]
    fn test_claimed_task_creation() {
        let task = ClaimedTask {
            task_id: 123,
            namespace_name: "fulfillment".to_string(),
            priority: 2,
            computed_priority: 4.5,
            age_hours: 0.1,
            ready_steps_count: 3,
            claim_timeout_seconds: 300,
            claimed_at: Utc::now(),
        };

        assert_eq!(task.task_id, 123);
        assert_eq!(task.namespace_name, "fulfillment");
        assert_eq!(task.priority, 2);
        assert_eq!(task.computed_priority, 4.5);
        assert_eq!(task.ready_steps_count, 3);
    }
}
