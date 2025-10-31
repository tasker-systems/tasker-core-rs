//! # Archival Service Background Service (TAS-49 Phase 2)
//!
//! Automatic background service that periodically archives completed tasks.
//!
//! ## Architecture
//!
//! - Runs on configurable interval (default: 24 hours)
//! - Calls SQL function `archive_completed_tasks()`
//! - Integrates with OpenTelemetry metrics for observability
//! - Supports dry-run mode for testing
//!
//! ## Archival Flow
//!
//! 1. Timer tick triggers archival cycle
//! 2. SQL function identifies terminal tasks past retention period
//! 3. Tasks moved to archive tables with complete audit trail
//! 4. Main table records deleted to reduce table growth
//! 5. Metrics recorded for monitoring and alerting
//!
//! ## Configuration
//!
//! Configured via `config/tasker/base/orchestration.toml`:
//! ```toml
//! [archive]
//! enabled = true
//! retention_days = 30
//! archive_batch_size = 1000
//! archive_interval_hours = 24
//!
//! [archive.policies]
//! archive_completed = true
//! archive_failed = true
//! archive_cancelled = false
//! archive_dlq_resolved = true
//! ```

use opentelemetry::KeyValue;
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use tasker_shared::config::components::ArchiveConfig;
use tasker_shared::errors::TaskerResult;
use tasker_shared::metrics::orchestration;

/// Archival result from SQL function
///
/// Maps to the output of `archive_completed_tasks()` SQL function.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArchivalStats {
    /// Number of tasks archived
    pub tasks_archived: i32,

    /// Number of workflow steps archived
    pub steps_archived: i32,

    /// Number of state transitions archived
    pub transitions_archived: i32,

    /// Execution time in milliseconds
    pub execution_time_ms: i32,
}

/// Background service for automatic task archival
///
/// Runs on configurable interval and archives completed tasks using the
/// `archive_completed_tasks()` SQL function.
///
/// ## Lifecycle
///
/// - Created during `OrchestrationCore::bootstrap()`
/// - Started via `run()` which spawns a tokio task
/// - Stops when tokio task is cancelled or receives shutdown signal
///
/// ## Example Usage
///
/// ```rust,ignore
/// use tasker_orchestration::orchestration::archival_service::ArchivalService;
/// use tasker_shared::config::components::ArchiveConfig;
///
/// let service = ArchivalService::new(pool.clone(), config.archive.clone());
///
/// // Start service in background
/// let handle = tokio::spawn(async move {
///     if let Err(e) = service.run().await {
///         error!("Archival service failed: {}", e);
///     }
/// });
/// ```
#[derive(Debug, Clone)]
pub struct ArchivalService {
    pool: PgPool,
    config: ArchiveConfig,
}

impl ArchivalService {
    /// Create new archival service
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `config` - Archive configuration
    #[must_use]
    pub const fn new(pool: PgPool, config: ArchiveConfig) -> Self {
        Self { pool, config }
    }

    /// Run archival loop
    ///
    /// Runs continuously until cancelled or error occurs. Each cycle:
    /// 1. Waits for interval tick
    /// 2. Checks if archival is enabled
    /// 3. Calls SQL archival function
    /// 4. Records metrics
    /// 5. Logs results
    ///
    /// # Errors
    ///
    /// Returns error if critical failure occurs that prevents continued operation.
    /// Non-critical errors (e.g., single archival cycle failure) are logged but
    /// don't stop the loop.
    pub async fn run(&self) -> TaskerResult<()> {
        let interval_duration = Duration::from_secs(self.config.archive_interval_hours * 3600);
        let mut interval_timer = interval(interval_duration);

        info!(
            enabled = self.config.enabled,
            interval_hours = self.config.archive_interval_hours,
            retention_days = self.config.retention_days,
            batch_size = self.config.archive_batch_size,
            "Starting archival service"
        );

        // Record that service is starting
        orchestration::archival_runs_total().add(1, &[KeyValue::new("status", "starting")]);

        loop {
            interval_timer.tick().await;

            if !self.config.enabled {
                debug!("Archival disabled, skipping cycle");
                continue;
            }

            let start = std::time::Instant::now();

            match self.archive_tasks().await {
                Ok(stats) => {
                    // Record execution duration
                    let duration_ms = start.elapsed().as_millis() as f64;
                    orchestration::archival_execution_duration().record(
                        duration_ms,
                        &[
                            KeyValue::new("status", "success"),
                            KeyValue::new("tasks_archived", stats.tasks_archived as i64),
                        ],
                    );

                    if stats.tasks_archived > 0 {
                        info!(
                            tasks_archived = stats.tasks_archived,
                            steps_archived = stats.steps_archived,
                            transitions_archived = stats.transitions_archived,
                            execution_time_ms = stats.execution_time_ms,
                            duration_ms = duration_ms,
                            "Archival cycle completed"
                        );

                        // Record successful archival metrics
                        self.record_archival_metrics(&stats);
                    } else {
                        debug!("No tasks to archive this cycle");
                    }

                    // Record successful run
                    orchestration::archival_runs_total().add(
                        1,
                        &[
                            KeyValue::new("status", "success"),
                            KeyValue::new("tasks_archived", stats.tasks_archived as i64),
                        ],
                    );
                }
                Err(e) => {
                    error!(error = %e, "Archival cycle failed");

                    // Record failed run
                    orchestration::archival_runs_total()
                        .add(1, &[KeyValue::new("status", "error")]);

                    // Don't stop the loop on error - just log and continue
                    continue;
                }
            }
        }
    }

    /// Archive completed tasks using SQL function
    ///
    /// Calls `archive_completed_tasks()` with configured parameters.
    ///
    /// # Errors
    ///
    /// Returns error if database query fails.
    async fn archive_tasks(&self) -> TaskerResult<ArchivalStats> {
        let retention_days = self.config.retention_days;
        let batch_size = self.config.archive_batch_size;
        let dry_run = false; // Always run in production mode

        debug!(
            retention_days = retention_days,
            batch_size = batch_size,
            "Calling archive_completed_tasks SQL function"
        );

        let result = sqlx::query_as::<_, ArchivalStats>(
            r#"
            SELECT
                tasks_archived,
                steps_archived,
                transitions_archived,
                execution_time_ms
            FROM archive_completed_tasks(
                $1::INTEGER,
                $2::INTEGER,
                $3::BOOLEAN
            )
            "#,
        )
        .bind(retention_days)
        .bind(batch_size)
        .bind(dry_run)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    /// Record archival metrics for monitoring
    ///
    /// Records OpenTelemetry metrics for archival operations.
    fn record_archival_metrics(&self, stats: &ArchivalStats) {
        // Record total tasks archived
        orchestration::tasks_archived_total().add(
            stats.tasks_archived as u64,
            &[KeyValue::new(
                "retention_days",
                self.config.retention_days as i64,
            )],
        );

        // Record steps archived
        debug!(
            steps_archived = stats.steps_archived,
            "Workflow steps archived"
        );

        // Record transitions archived
        debug!(
            transitions_archived = stats.transitions_archived,
            "State transitions archived"
        );

        // Record SQL execution time
        orchestration::archival_execution_duration().record(
            f64::from(stats.execution_time_ms),
            &[
                KeyValue::new("status", "success"),
                KeyValue::new("tasks_archived", stats.tasks_archived as i64),
            ],
        );

        // Log individual archival details at debug level
        debug!(
            tasks_archived = stats.tasks_archived,
            steps_archived = stats.steps_archived,
            transitions_archived = stats.transitions_archived,
            execution_time_ms = stats.execution_time_ms,
            retention_days = self.config.retention_days,
            "Archival cycle metrics recorded"
        );

        // Warn if archival is taking too long
        if stats.execution_time_ms > 60000 {
            // > 1 minute
            warn!(
                execution_time_ms = stats.execution_time_ms,
                tasks_archived = stats.tasks_archived,
                "Archival cycle taking longer than expected - consider reducing batch size"
            );
        }
    }

    /// Get current configuration
    #[must_use]
    pub const fn config(&self) -> &ArchiveConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_archival_service_creation() {
        let pool = PgPool::connect_lazy("postgresql://test").expect("Should create lazy pool");
        let config = ArchiveConfig::default();

        let service = ArchivalService::new(pool, config.clone());

        assert_eq!(service.config().enabled, config.enabled);
        assert_eq!(
            service.config().archive_interval_hours,
            config.archive_interval_hours
        );
        assert_eq!(service.config().retention_days, config.retention_days);
    }

    #[test]
    fn test_archival_stats_structure() {
        let stats = ArchivalStats {
            tasks_archived: 100,
            steps_archived: 450,
            transitions_archived: 300,
            execution_time_ms: 1250,
        };

        assert_eq!(stats.tasks_archived, 100);
        assert_eq!(stats.steps_archived, 450);
        assert_eq!(stats.transitions_archived, 300);
        assert_eq!(stats.execution_time_ms, 1250);
    }

    #[tokio::test]
    async fn test_archival_service_config_getter() {
        let pool = PgPool::connect_lazy("postgresql://test").expect("Should create lazy pool");
        let config = ArchiveConfig::default();

        let service = ArchivalService::new(pool, config.clone());

        assert_eq!(
            service.config().archive_batch_size,
            config.archive_batch_size
        );
    }
}
