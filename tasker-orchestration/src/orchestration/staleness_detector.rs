//! # Staleness Detector Background Service (TAS-49 Phase 2, TAS-59 Phase 3)
//!
//! Automatic background service that periodically detects and processes stale tasks.
//!
//! ## Architecture
//!
//! - Runs on configurable interval (default: 5 minutes)
//! - Calls SQL function `detect_and_transition_stale_tasks()`
//! - Integrates with OpenTelemetry metrics for observability
//! - Supports dry-run mode for testing
//! - **TAS-59**: Adds checkpoint-based health checks for batch workers
//!
//! ## Staleness Detection Flow
//!
//! 1. Timer tick triggers detection cycle
//! 2. SQL function identifies tasks exceeding state thresholds
//! 3. Per-template lifecycle config takes precedence over global defaults
//! 4. **TAS-59**: Post-filter batch workers using checkpoint health checks
//! 5. Tasks transitioned to Error state and/or moved to DLQ
//! 6. Metrics recorded for monitoring and alerting
//!
//! ## Configuration
//!
//! Configured via `config/tasker/base/orchestration.toml`:
//! ```toml
//! [staleness_detection]
//! enabled = true
//! batch_size = 100
//! detection_interval_seconds = 300  # 5 minutes
//!
//! [batch_processing]
//! checkpoint_stall_minutes = 15  # Batch worker checkpoint health
//! ```

use chrono::{DateTime, Utc};
use opentelemetry::KeyValue;
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::config::tasker::{BatchProcessingConfig, StalenessDetectionConfig};
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::errors::TaskerResult;
use tasker_shared::metrics::orchestration;
use tasker_shared::models::orchestration::StalenessAction;

/// Staleness detection result from SQL function
///
/// Maps to the output of `detect_and_transition_stale_tasks()` SQL function.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StalenessResult {
    /// Task UUID that was detected as stale
    pub task_uuid: Uuid,

    /// Task namespace name
    pub namespace_name: String,

    /// Task template name
    pub task_name: String,

    /// Current task state when detected
    pub current_state: String,

    /// Minutes task has been in current state
    pub time_in_state_minutes: i32,

    /// Threshold that was exceeded (in minutes)
    pub staleness_threshold_minutes: i32,

    /// Action taken by SQL function (type-safe enum)
    pub action_taken: StalenessAction,

    /// Whether task was successfully moved to DLQ
    pub moved_to_dlq: bool,

    /// Whether task state transition completed successfully
    pub transition_success: bool,
}

/// Background service for automatic staleness detection
///
/// Runs on configurable interval and processes stale tasks using the
/// `detect_and_transition_stale_tasks()` SQL function.
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
/// use tasker_orchestration::orchestration::staleness_detector::StalenessDetector;
/// use tasker_shared::config::components::StalenessDetectionConfig;
///
/// let detector = StalenessDetector::new(pool.clone(), config.staleness_detection.clone());
///
/// // Start detector in background
/// let handle = tokio::spawn(async move {
///     if let Err(e) = detector.run().await {
///         error!("Staleness detector failed: {}", e);
///     }
/// });
/// ```
#[derive(Clone)]
pub struct StalenessDetector {
    executor: SqlFunctionExecutor,
    config: StalenessDetectionConfig,
    batch_config: BatchProcessingConfig,
}

// Manual Debug implementation because SqlFunctionExecutor contains PgPool
impl std::fmt::Debug for StalenessDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StalenessDetector")
            .field("config", &self.config)
            .field("batch_config", &self.batch_config)
            .finish_non_exhaustive()
    }
}

impl StalenessDetector {
    /// Create new staleness detector
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `config` - Staleness detection configuration
    /// * `batch_config` - Batch processing configuration (for checkpoint health checks)
    #[must_use]
    pub fn new(
        pool: PgPool,
        config: StalenessDetectionConfig,
        batch_config: BatchProcessingConfig,
    ) -> Self {
        let executor = SqlFunctionExecutor::new(pool);
        Self {
            executor,
            config,
            batch_config,
        }
    }

    /// Run staleness detection loop
    ///
    /// Runs continuously until cancelled or error occurs. Each cycle:
    /// 1. Waits for interval tick
    /// 2. Calls SQL detection function
    /// 3. Records metrics
    /// 4. Logs results
    ///
    /// **Note**: This method assumes the detector has already been enabled via bootstrap.
    /// The `enabled` config flag is checked at startup (see `OrchestrationCore::start_background_services`),
    /// and this background task is only spawned when enabled. Runtime enable/disable is not supported.
    ///
    /// # Errors
    ///
    /// Returns error if critical failure occurs that prevents continued operation.
    /// Non-critical errors (e.g., single detection cycle failure) are logged but
    /// don't stop the loop.
    pub async fn run(&self) -> TaskerResult<()> {
        let interval_duration = Duration::from_secs(self.config.detection_interval_seconds as u64);
        let mut interval_timer = interval(interval_duration);

        info!(
            enabled = self.config.enabled,
            interval_seconds = self.config.detection_interval_seconds,
            batch_size = self.config.batch_size,
            "Starting staleness detector"
        );

        // Record that detector is starting
        orchestration::staleness_detection_runs_total()
            .add(1, &[KeyValue::new("dry_run", self.config.dry_run)]);

        loop {
            interval_timer.tick().await;

            let start = std::time::Instant::now();

            match self.detect_and_transition_stale_tasks().await {
                Ok(results) => {
                    let total = results.len();
                    let moved_to_dlq = results.iter().filter(|r| r.moved_to_dlq).count();
                    let transitioned = results.iter().filter(|r| r.transition_success).count();

                    // Record detection duration
                    let duration_ms = start.elapsed().as_millis() as f64;
                    orchestration::staleness_detection_duration().record(
                        duration_ms,
                        &[
                            KeyValue::new("dry_run", self.config.dry_run),
                            KeyValue::new("tasks_detected", total as i64),
                        ],
                    );

                    if total > 0 {
                        info!(
                            total = total,
                            moved_to_dlq = moved_to_dlq,
                            transitioned = transitioned,
                            dry_run = self.config.dry_run,
                            duration_ms = duration_ms,
                            "Staleness detection completed"
                        );

                        // Record metrics for each detected task
                        self.record_detection_metrics(&results);
                    } else {
                        debug!("No stale tasks detected this cycle");
                    }
                }
                Err(e) => {
                    error!(error = %e, "Staleness detection cycle failed");
                    // Don't stop the loop on error - just log and continue
                    continue;
                }
            }
        }
    }

    /// Detect and transition stale tasks using SQL function with batch worker post-filtering
    ///
    /// Calls `detect_and_transition_stale_tasks()` via `SqlFunctionExecutor` with configured parameters,
    /// then post-filters results for batch worker checkpoint health (TAS-59 Phase 3).
    ///
    /// # Errors
    ///
    /// Returns error if database query fails.
    async fn detect_and_transition_stale_tasks(&self) -> TaskerResult<Vec<StalenessResult>> {
        let dry_run = self.config.dry_run;
        let batch_size = self.config.batch_size;
        let waiting_deps_threshold = self.config.thresholds.waiting_for_dependencies_minutes;
        let waiting_retry_threshold = self.config.thresholds.waiting_for_retry_minutes;
        let steps_in_process_threshold = self.config.thresholds.steps_in_process_minutes;
        let max_lifetime_hours = self.config.thresholds.task_max_lifetime_hours;

        debug!(
            dry_run = dry_run,
            batch_size = batch_size,
            "Calling detect_and_transition_stale_tasks SQL function via SqlFunctionExecutor"
        );

        let db_results = self
            .executor
            .detect_and_transition_stale_tasks(
                dry_run,
                batch_size as i32,
                waiting_deps_threshold as i32,
                waiting_retry_threshold as i32,
                steps_in_process_threshold as i32,
                max_lifetime_hours as i32,
            )
            .await?;

        // Convert database results to internal StalenessResult format
        let mut results: Vec<StalenessResult> = db_results
            .into_iter()
            .map(|r| StalenessResult {
                task_uuid: r.task_uuid,
                namespace_name: r.namespace_name,
                task_name: r.task_name,
                current_state: r.current_state,
                time_in_state_minutes: r.time_in_state_minutes,
                staleness_threshold_minutes: r.staleness_threshold_minutes,
                action_taken: r.action_taken.parse().unwrap(), // Infallible: always returns Ok
                moved_to_dlq: r.moved_to_dlq,
                transition_success: r.transition_success,
            })
            .collect();

        // TAS-59 Phase 3: Post-filter for batch worker checkpoint health
        // Only filter if batch processing is enabled and checkpoint health checks are configured
        if self.batch_config.enabled && self.batch_config.checkpoint_stall_minutes > 0 {
            let initial_count = results.len();

            // Filter out tasks that fail checkpoint health check
            let mut filtered_results = Vec::with_capacity(results.len());
            for result in results {
                match self.should_task_be_stale(result.task_uuid).await {
                    Ok(is_stale) if is_stale => {
                        // Task is truly stale (checkpoint health check passed or no batch workers)
                        filtered_results.push(result);
                    }
                    Ok(_) => {
                        // Task has healthy batch workers, not truly stale
                        debug!(
                            task_uuid = %result.task_uuid,
                            "Task filtered out - batch workers have healthy checkpoints"
                        );
                    }
                    Err(e) => {
                        // Error checking batch health - log but include task (conservative approach)
                        warn!(
                            task_uuid = %result.task_uuid,
                            error = %e,
                            "Error checking batch worker health, including task in staleness results"
                        );
                        filtered_results.push(result);
                    }
                }
            }

            results = filtered_results;

            let filtered_count = initial_count - results.len();
            if filtered_count > 0 {
                info!(
                    initial_count = initial_count,
                    filtered_count = filtered_count,
                    final_count = results.len(),
                    "Batch worker checkpoint health filtering applied"
                );
            }
        }

        Ok(results)
    }

    /// Record detection metrics for monitoring
    ///
    /// Records OpenTelemetry metrics for each detected stale task and updates
    /// the pending investigations gauge at the end of the cycle.
    fn record_detection_metrics(&self, results: &[StalenessResult]) {
        for result in results {
            // Calculate time bucket for labeling
            let time_bucket = if result.time_in_state_minutes <= 120 {
                "60-120"
            } else if result.time_in_state_minutes <= 360 {
                "120-360"
            } else {
                ">360"
            };

            // Record stale task detection
            let detection_labels = &[
                KeyValue::new("state", result.current_state.clone()),
                KeyValue::new("time_in_state_minutes", time_bucket.to_string()),
            ];
            orchestration::stale_tasks_detected_total().add(1, detection_labels);

            // Record DLQ entries created
            if result.moved_to_dlq {
                let dlq_labels = &[
                    KeyValue::new("correlation_id", result.task_uuid.to_string()),
                    KeyValue::new("dlq_reason", "staleness_timeout"),
                    KeyValue::new("original_state", result.current_state.clone()),
                ];
                orchestration::dlq_entries_created_total().add(1, dlq_labels);
            }

            // Record error state transitions
            if result.transition_success {
                let transition_labels = &[
                    KeyValue::new("correlation_id", result.task_uuid.to_string()),
                    KeyValue::new("original_state", result.current_state.clone()),
                    KeyValue::new("reason", "staleness_timeout"),
                ];
                orchestration::tasks_transitioned_to_error_total().add(1, transition_labels);
            }

            // Log individual task details at debug level
            debug!(
                task_uuid = %result.task_uuid,
                namespace = %result.namespace_name,
                task_name = %result.task_name,
                state = %result.current_state,
                time_in_state_min = result.time_in_state_minutes,
                threshold_min = result.staleness_threshold_minutes,
                action = %result.action_taken,
                moved_to_dlq = result.moved_to_dlq,
                transition_success = result.transition_success,
                "Stale task detected and processed"
            );
        }

        if !results.is_empty() {
            // Count failures for alerting
            let failures = results
                .iter()
                .filter(|r| r.action_taken.is_failure())
                .count();

            if failures > 0 {
                warn!(
                    failures = failures,
                    total = results.len(),
                    "Some stale task transitions failed"
                );
            }
        }

        // Update pending investigations gauge
        // Note: This is a best-effort gauge update. If the query fails, we log it but don't fail the detection cycle.
        self.update_pending_investigations_gauge();
    }

    /// Update the pending investigations gauge
    ///
    /// Queries the database for current pending DLQ count and updates the gauge metric.
    /// This provides real-time visibility into DLQ backlog for monitoring and alerting.
    fn update_pending_investigations_gauge(&self) {
        // Spawn a non-blocking task to update the gauge
        let executor = self.executor.clone();
        tokio::spawn(async move {
            match executor.pool().acquire().await {
                Ok(mut conn) => {
                    match sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM tasker_tasks_dlq WHERE resolution_status = 'pending'",
                    )
                    .fetch_one(&mut *conn)
                    .await
                    {
                        Ok(pending_count) => {
                            orchestration::dlq_pending_investigations()
                                .record(pending_count as u64, &[]);
                            debug!(
                                pending_investigations = pending_count,
                                "Updated DLQ pending investigations gauge"
                            );
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                "Failed to query pending DLQ count for gauge update"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        "Failed to acquire database connection for pending DLQ gauge update"
                    );
                }
            }
        });
    }

    /// Get current configuration
    #[must_use]
    pub const fn config(&self) -> &StalenessDetectionConfig {
        &self.config
    }

    /// Check if a workflow step is a batch worker based on results structure
    ///
    /// TAS-59: Batch workers store cursor data in `results.batch_cursor`
    fn is_batch_worker(&self, results: &serde_json::Value) -> bool {
        results
            .as_object()
            .and_then(|obj| obj.get("batch_cursor"))
            .is_some()
    }

    /// Check if a batch worker is healthy based on checkpoint timestamps
    ///
    /// TAS-59: Batch workers must update `batch_cursor.last_checkpoint` periodically.
    /// If the checkpoint timestamp exceeds the stall threshold, the worker is unhealthy.
    ///
    /// Returns `true` if healthy (recent checkpoint), `false` if stalled.
    fn is_batch_worker_checkpoint_healthy(&self, results: &serde_json::Value) -> bool {
        // Extract batch_cursor from results
        let batch_cursor = match results.as_object().and_then(|obj| obj.get("batch_cursor")) {
            Some(cursor) => cursor,
            None => return true, // Not a batch worker, consider healthy
        };

        // Get last_checkpoint timestamp
        let last_checkpoint_str = match batch_cursor
            .as_object()
            .and_then(|cursor| cursor.get("last_checkpoint"))
            .and_then(|v| v.as_str())
        {
            Some(ts) => ts,
            None => {
                debug!("Batch worker missing last_checkpoint timestamp");
                return false; // Missing checkpoint data = unhealthy
            }
        };

        // Parse timestamp
        let last_checkpoint = match DateTime::parse_from_rfc3339(last_checkpoint_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                warn!(error = %e, "Failed to parse batch worker checkpoint timestamp");
                return false; // Invalid timestamp = unhealthy
            }
        };

        // Calculate elapsed time since last checkpoint
        let elapsed = Utc::now() - last_checkpoint;
        let stall_threshold =
            chrono::Duration::minutes(self.batch_config.checkpoint_stall_minutes as i64);

        if elapsed > stall_threshold {
            warn!(
                elapsed_minutes = elapsed.num_minutes(),
                threshold_minutes = stall_threshold.num_minutes(),
                "Batch worker stalled - no recent checkpoint"
            );
            return false; // Stalled
        }

        true // Healthy
    }

    /// Filter batch workers for checkpoint health (TAS-59 Phase 3)
    ///
    /// Post-processes SQL staleness detection results to add checkpoint-based health checks
    /// for batch workers. Regular steps pass through unchanged.
    ///
    /// # Arguments
    ///
    /// * `task_uuid` - Task UUID to check batch workers for
    ///
    /// # Returns
    ///
    /// `true` if task should be considered stale (either regular step exceeds time threshold,
    /// or batch worker has stalled checkpoints), `false` if task is healthy.
    ///
    /// # Implementation Notes
    ///
    /// This method queries workflow steps in 'in_progress' state by joining with the
    /// `tasker_workflow_step_transitions` table (state machine pattern). The current state
    /// is determined by the transition with `most_recent = true`.
    async fn should_task_be_stale(&self, task_uuid: Uuid) -> TaskerResult<bool> {
        // Query workflow steps for this task to check batch worker health
        // Join with transitions to get current state (state machine pattern)
        let steps = sqlx::query!(
            r#"
            SELECT ws.workflow_step_uuid, ws.results
            FROM tasker_workflow_steps ws
            INNER JOIN tasker_workflow_step_transitions wst
                ON ws.workflow_step_uuid = wst.workflow_step_uuid
            WHERE ws.task_uuid = $1
                AND wst.most_recent = true
                AND wst.to_state = 'in_progress'
            "#,
            task_uuid
        )
        .fetch_all(self.executor.pool())
        .await?;

        // If no in-progress steps, defer to SQL function decision
        if steps.is_empty() {
            return Ok(true);
        }

        // Check each step for batch worker health
        for step in steps {
            let results = step.results.unwrap_or_default();

            // If this is a batch worker, check checkpoint health
            if self.is_batch_worker(&results) && !self.is_batch_worker_checkpoint_healthy(&results)
            {
                debug!(
                    task_uuid = %task_uuid,
                    step_uuid = %step.workflow_step_uuid,
                    "Batch worker failed checkpoint health check"
                );
                return Ok(true); // Batch worker is unhealthy = task is stale
            }
        }

        // All batch workers have healthy checkpoints, defer to SQL decision
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_staleness_detector_creation() {
        let pool = PgPool::connect_lazy("postgresql://test").expect("Should create lazy pool");
        let config = StalenessDetectionConfig::default();
        let batch_config = BatchProcessingConfig::default();

        let detector = StalenessDetector::new(pool, config.clone(), batch_config);

        assert_eq!(detector.config().enabled, config.enabled);
        assert_eq!(
            detector.config().detection_interval_seconds,
            config.detection_interval_seconds
        );
    }

    #[test]
    fn test_staleness_result_structure() {
        let result = StalenessResult {
            task_uuid: Uuid::new_v4(),
            namespace_name: "test_namespace".to_string(),
            task_name: "test_task".to_string(),
            current_state: "waiting_for_dependencies".to_string(),
            time_in_state_minutes: 120,
            staleness_threshold_minutes: 60,
            action_taken: StalenessAction::TransitionedToDlqAndError,
            moved_to_dlq: true,
            transition_success: true,
        };

        assert!(result.moved_to_dlq);
        assert!(result.transition_success);
        assert_eq!(result.time_in_state_minutes, 120);
        assert_eq!(
            result.action_taken,
            StalenessAction::TransitionedToDlqAndError
        );
        assert!(!result.action_taken.is_failure());
        assert!(result.action_taken.dlq_created());
        assert!(result.action_taken.transition_succeeded());
    }

    #[tokio::test]
    async fn test_staleness_detector_config_getter() {
        let pool = PgPool::connect_lazy("postgresql://test").expect("Should create lazy pool");
        let config = StalenessDetectionConfig::default();
        let batch_config = BatchProcessingConfig::default();

        let detector = StalenessDetector::new(pool, config.clone(), batch_config);

        assert_eq!(detector.config().batch_size, config.batch_size);
    }
}
