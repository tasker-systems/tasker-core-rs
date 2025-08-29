//! Fallback poller for task readiness reliability in TAS-43
//!
//! This module provides a safety net polling mechanism that uses the existing
//! tasker_ready_tasks view to catch any tasks missed by the event-driven system.
//! It operates much slower than event-driven notifications but ensures zero
//! missed tasks in production environments.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::{system_context::SystemContext, TaskerError, TaskerResult};

use super::events::{ReadinessTrigger, TaskReadyEvent};

// TAS-43: Import command pattern types for direct command sending
use crate::orchestration::command_processor::{OrchestrationCommand, TaskReadinessResult};

/// Configuration for readiness fallback polling
///
/// Controls the behavior of the fallback safety net, including polling intervals,
/// age thresholds, and batch sizes for processing stale ready tasks.
#[derive(Debug, Clone)]
pub struct ReadinessFallbackConfig {
    /// Enable fallback polling (should always be true for production reliability)
    pub enabled: bool,
    /// Polling interval (much slower than event-driven, e.g., 30 seconds)
    pub polling_interval: Duration,
    /// Batch size for querying ready tasks
    pub batch_size: u32,
    /// Age threshold - only poll for tasks older than this (avoids race with events)
    pub age_threshold: Duration,
    /// Maximum age to poll for (prevents infinite old task processing)
    pub max_age: Duration,
}

impl Default for ReadinessFallbackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            polling_interval: Duration::from_secs(30), // Much slower than event-driven
            batch_size: 50,
            age_threshold: Duration::from_secs(5), // Only poll for tasks >5 seconds old
            max_age: Duration::from_secs(24 * 60 * 60), // Don't poll for tasks >24 hours old
        }
    }
}

/// Fallback poller that uses tasker_ready_tasks view for reliability (TAS-43)
///
/// This component provides a critical safety net for the event-driven system by
/// periodically checking the tasker_ready_tasks view for any tasks that may have
/// been missed by PostgreSQL LISTEN/NOTIFY events. It sends ProcessTaskReadiness commands
/// directly to the orchestration processor, using the same command pattern as events.
pub struct ReadinessFallbackPoller {
    config: ReadinessFallbackConfig,
    context: Arc<SystemContext>,
    command_sender: mpsc::Sender<OrchestrationCommand>, // TAS-43: Send commands instead of events
    poller_id: Uuid,
    is_running: bool,
    stats: Arc<FallbackPollerStats>,
    background_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Statistics for fallback poller monitoring
#[derive(Debug, Default)]
pub struct FallbackPollerStats {
    pub polls_executed: std::sync::atomic::AtomicU64,
    pub tasks_found: std::sync::atomic::AtomicU64,
    pub events_sent: std::sync::atomic::AtomicU64,
    pub errors: std::sync::atomic::AtomicU64,
    pub sql_queries_executed: std::sync::atomic::AtomicU64,
    pub polling_time_ms: std::sync::atomic::AtomicU64,
}

impl ReadinessFallbackPoller {
    /// Create new fallback poller (TAS-43)
    ///
    /// The poller sends ProcessTaskReadiness commands directly to the orchestration processor
    /// when it finds stale ready tasks in the tasker_ready_tasks view.
    pub fn new(
        config: ReadinessFallbackConfig,
        context: Arc<SystemContext>,
        command_sender: mpsc::Sender<OrchestrationCommand>, // TAS-43: Send commands instead of events
    ) -> Self {
        let poller_id = Uuid::new_v4();

        info!(
            poller_id = %poller_id,
            enabled = %config.enabled,
            interval_ms = %config.polling_interval.as_millis(),
            age_threshold_s = %config.age_threshold.as_secs(),
            "Creating ReadinessFallbackPoller"
        );

        Self {
            config,
            context,
            command_sender, // TAS-43: Store command sender instead of event sender
            poller_id,
            is_running: false,
            stats: Arc::new(FallbackPollerStats::default()),
            background_handle: None,
        }
    }

    /// Start fallback polling
    ///
    /// Begins the background polling loop that periodically checks for stale ready tasks.
    /// If polling is disabled, this method returns immediately without starting the loop.
    pub async fn start(&mut self) -> TaskerResult<()> {
        if !self.config.enabled {
            info!(poller_id = %self.poller_id, "Fallback polling disabled");
            return Ok(());
        }

        info!(poller_id = %self.poller_id, "Starting ReadinessFallbackPoller");

        self.is_running = true;
        self.start_polling_loop().await?;

        Ok(())
    }

    /// Stop fallback polling
    ///
    /// Gracefully stops the background polling loop and cleans up resources.
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(poller_id = %self.poller_id, "Stopping ReadinessFallbackPoller");

        self.is_running = false;

        // Cancel background polling task
        if let Some(handle) = self.background_handle.take() {
            handle.abort();
        }

        Ok(())
    }

    /// Start polling loop in background
    ///
    /// Creates a background task that executes the polling logic at regular intervals.
    /// Uses tokio::time::interval for consistent scheduling with missed tick skipping.
    async fn start_polling_loop(&mut self) -> TaskerResult<()> {
        let config = self.config.clone();
        let context = Arc::clone(&self.context);
        let command_sender = self.command_sender.clone(); // TAS-43: Use command sender instead of event sender
        let stats = Arc::clone(&self.stats);
        let poller_id = self.poller_id;

        let handle = tokio::spawn(async move {
            info!(
                poller_id = %poller_id,
                interval_seconds = %config.polling_interval.as_secs(),
                "Starting fallback polling loop"
            );

            let mut interval = tokio::time::interval(config.polling_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                debug!(poller_id = %poller_id, "Executing fallback poll for ready tasks");

                let poll_start = std::time::Instant::now();
                stats
                    .polls_executed
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                match Self::poll_for_ready_tasks(
                    &config,
                    &context,
                    &command_sender,
                    &stats,
                    poller_id,
                )
                .await
                {
                    Ok(found_count) => {
                        let poll_duration = poll_start.elapsed().as_millis() as u64;
                        stats
                            .polling_time_ms
                            .fetch_add(poll_duration, std::sync::atomic::Ordering::Relaxed);

                        debug!(
                            poller_id = %poller_id,
                            found_tasks = found_count,
                            duration_ms = poll_duration,
                            "Fallback polling completed successfully"
                        );
                    }
                    Err(e) => {
                        error!(
                            poller_id = %poller_id,
                            error = %e,
                            "Fallback polling failed"
                        );
                        stats
                            .errors
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        });

        self.background_handle = Some(handle);
        Ok(())
    }

    /// Poll for ready tasks using tasker_ready_tasks view (TAS-43)
    ///
    /// This method queries the existing tasker_ready_tasks view to find tasks that
    /// may have been missed by the event-driven system. It sends ProcessTaskReadiness commands
    /// directly to the orchestration processor, applying age filters to avoid race conditions.
    async fn poll_for_ready_tasks(
        config: &ReadinessFallbackConfig,
        context: &SystemContext,
        command_sender: &mpsc::Sender<OrchestrationCommand>, // TAS-43: Send commands instead of events
        stats: &FallbackPollerStats,
        poller_id: Uuid,
    ) -> TaskerResult<u32> {
        let age_threshold_seconds = config.age_threshold.as_secs() as f64;
        let max_age_seconds = config.max_age.as_secs() as f64;
        let batch_size = config.batch_size as i64;

        debug!(
            poller_id = %poller_id,
            age_threshold_seconds = age_threshold_seconds,
            max_age_seconds = max_age_seconds,
            batch_size = batch_size,
            "Querying tasker_ready_tasks view for stale ready tasks"
        );

        stats
            .sql_queries_executed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Query tasker_ready_tasks view for stale ready tasks
        // This uses the existing view that incorporates all the proven readiness logic
        let mut ready_tasks = sqlx::query!(
            r#"
            SELECT
                rt.task_uuid,
                rt.namespace_name,
                rt.priority,
                rt.ready_steps_count,
                rt.age_hours,
                rt.created_at,
                rt.computed_priority
            FROM tasker_ready_tasks rt
            WHERE rt.created_at < NOW() - INTERVAL '1 seconds' * $1
              AND rt.created_at > NOW() - INTERVAL '1 seconds' * $2
              AND (rt.claimed_by IS NULL)
              AND rt.execution_status = 'has_ready_steps'
              AND rt.ready_steps_count > 0
            ORDER BY rt.computed_priority DESC, rt.created_at ASC
            LIMIT $3
            "#,
            age_threshold_seconds,
            max_age_seconds,
            batch_size
        )
        .fetch_all(context.database_pool())
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Fallback query failed: {}", e)))?;

        let found_count = ready_tasks.len() as u32;
        stats
            .tasks_found
            .fetch_add(found_count as u64, std::sync::atomic::Ordering::Relaxed);

        if found_count > 0 {
            info!(
                poller_id = %poller_id,
                found_count = found_count,
                "Found ready tasks via fallback polling that may have been missed by events"
            );
        }

        // We are only going to send ProcessTaskReadiness as a command once
        // the underlying implementation already checks a batch of ready tasks
        if !ready_tasks.is_empty() {
            let (command_tx, command_rx): (
                _,
                tokio::sync::oneshot::Receiver<TaskerResult<TaskReadinessResult>>,
            ) = tokio::sync::oneshot::channel();

            let task = ready_tasks.pop().unwrap();

            let command = OrchestrationCommand::ProcessTaskReadiness {
                task_uuid: task.task_uuid.unwrap_or_else(uuid::Uuid::new_v4),
                namespace: task
                    .namespace_name
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                priority: task.priority.unwrap_or(0),
                ready_steps: task.ready_steps_count.unwrap_or(0) as i32,
                triggered_by: "fallback_polling".to_string(),
                step_uuid: None,
                step_state: None,
                task_state: None,
                resp: command_tx,
            };

            match command_sender.send(command).await {
                Ok(_) => {
                    stats
                        .events_sent
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    // Wait for command response to ensure processing (no timeout for now)
                    match command_rx.await {
                        Ok(Ok(result)) => {
                            debug!(
                                poller_id = %poller_id,
                                task_uuid = ?task.task_uuid,
                                namespace = %task.namespace_name.as_deref().unwrap_or("default"),
                                priority = task.priority.unwrap_or(0),
                                computed_priority = task.computed_priority.unwrap_or(0.0),
                                age_hours = task.age_hours.unwrap_or(0.0),
                                steps_enqueued = %result.steps_enqueued,
                                processing_time_ms = %result.processing_time_ms,
                                "Successfully processed task readiness from fallback polling"
                            );
                        }
                        Ok(Err(e)) => {
                            warn!(
                                poller_id = %poller_id,
                                task_uuid = ?task.task_uuid,
                                error = %e,
                                "Command processing failed for fallback polling task"
                            );
                            stats
                                .errors
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Err(e) => {
                            warn!(
                                poller_id = %poller_id,
                                task_uuid = ?task.task_uuid,
                                error = %e,
                                "Failed to receive command response for fallback polling task"
                            );
                            stats
                                .errors
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        poller_id = %poller_id,
                        task_uuid = ?task.task_uuid,
                        error = %e,
                        "Failed to send ProcessTaskReadiness command from fallback polling"
                    );
                    stats
                        .errors
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }

        Ok(found_count)
    }

    /// Get poller statistics
    pub fn get_stats(&self) -> FallbackPollerStatsSnapshot {
        FallbackPollerStatsSnapshot {
            poller_id: self.poller_id,
            is_running: self.is_running,
            enabled: self.config.enabled,
            polling_interval: self.config.polling_interval,
            age_threshold: self.config.age_threshold,
            max_age: self.config.max_age,
            batch_size: self.config.batch_size,
            polls_executed: self
                .stats
                .polls_executed
                .load(std::sync::atomic::Ordering::Relaxed),
            tasks_found: self
                .stats
                .tasks_found
                .load(std::sync::atomic::Ordering::Relaxed),
            events_sent: self
                .stats
                .events_sent
                .load(std::sync::atomic::Ordering::Relaxed),
            errors: self.stats.errors.load(std::sync::atomic::Ordering::Relaxed),
            sql_queries_executed: self
                .stats
                .sql_queries_executed
                .load(std::sync::atomic::Ordering::Relaxed),
            polling_time_ms: self
                .stats
                .polling_time_ms
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Check if poller is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Get poller ID
    pub fn poller_id(&self) -> Uuid {
        self.poller_id
    }

    /// Get configuration
    pub fn config(&self) -> &ReadinessFallbackConfig {
        &self.config
    }
}

/// Snapshot of fallback poller statistics
#[derive(Debug, Clone)]
pub struct FallbackPollerStatsSnapshot {
    pub poller_id: Uuid,
    pub is_running: bool,
    pub enabled: bool,
    pub polling_interval: Duration,
    pub age_threshold: Duration,
    pub max_age: Duration,
    pub batch_size: u32,
    pub polls_executed: u64,
    pub tasks_found: u64,
    pub events_sent: u64,
    pub errors: u64,
    pub sql_queries_executed: u64,
    pub polling_time_ms: u64,
}

impl FallbackPollerStatsSnapshot {
    /// Calculate average polling time in milliseconds
    pub fn avg_polling_time_ms(&self) -> f64 {
        if self.polls_executed == 0 {
            0.0
        } else {
            self.polling_time_ms as f64 / self.polls_executed as f64
        }
    }

    /// Calculate tasks found per poll
    pub fn avg_tasks_per_poll(&self) -> f64 {
        if self.polls_executed == 0 {
            0.0
        } else {
            self.tasks_found as f64 / self.polls_executed as f64
        }
    }

    /// Calculate error rate
    pub fn error_rate(&self) -> f64 {
        if self.polls_executed == 0 {
            0.0
        } else {
            self.errors as f64 / self.polls_executed as f64
        }
    }

    /// Calculate success rate for event sending
    pub fn event_send_success_rate(&self) -> f64 {
        if self.tasks_found == 0 {
            1.0 // No tasks to send, so 100% success
        } else {
            self.events_sent as f64 / self.tasks_found as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, timeout};

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_fallback_poller_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let (tx, _rx) = mpsc::channel(100);

        let config = ReadinessFallbackConfig::default();
        let poller = ReadinessFallbackPoller::new(config, context, tx);

        let stats = poller.get_stats();
        assert!(!stats.is_running);
        assert!(stats.enabled);
        assert_eq!(stats.polls_executed, 0);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_fallback_polling_disabled(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let (tx, _rx) = mpsc::channel(100);

        let config = ReadinessFallbackConfig {
            enabled: false,
            ..Default::default()
        };

        let mut poller = ReadinessFallbackPoller::new(config, context, tx);
        poller.start().await?;

        let stats = poller.get_stats();
        assert!(!stats.is_running); // Should not start when disabled
        assert!(!stats.enabled);

        Ok(())
    }

    #[tokio::test]
    async fn test_stats_calculations() {
        let stats = FallbackPollerStatsSnapshot {
            poller_id: Uuid::new_v4(),
            is_running: true,
            enabled: true,
            polling_interval: Duration::from_secs(30),
            age_threshold: Duration::from_secs(5),
            max_age: Duration::from_secs(24 * 3600),
            batch_size: 50,
            polls_executed: 10,
            tasks_found: 15,
            events_sent: 14,
            errors: 1,
            sql_queries_executed: 10,
            polling_time_ms: 1500, // 1.5 seconds total
        };

        // Test average polling time
        assert_eq!(stats.avg_polling_time_ms(), 150.0); // 1500ms / 10 polls = 150ms

        // Test average tasks per poll
        assert_eq!(stats.avg_tasks_per_poll(), 1.5); // 15 tasks / 10 polls = 1.5

        // Test error rate
        assert_eq!(stats.error_rate(), 0.1); // 1 error / 10 polls = 0.1 (10%)

        // Test event send success rate
        let success_rate = 14.0 / 15.0; // 14 sent / 15 found ≈ 0.933
        assert!((stats.event_send_success_rate() - success_rate).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_stats_calculations_edge_cases() {
        // Test with zero polls
        let empty_stats = FallbackPollerStatsSnapshot {
            poller_id: Uuid::new_v4(),
            is_running: false,
            enabled: true,
            polling_interval: Duration::from_secs(30),
            age_threshold: Duration::from_secs(5),
            max_age: Duration::from_secs(24 * 3600),
            batch_size: 50,
            polls_executed: 0,
            tasks_found: 0,
            events_sent: 0,
            errors: 0,
            sql_queries_executed: 0,
            polling_time_ms: 0,
        };

        assert_eq!(empty_stats.avg_polling_time_ms(), 0.0);
        assert_eq!(empty_stats.avg_tasks_per_poll(), 0.0);
        assert_eq!(empty_stats.error_rate(), 0.0);
        assert_eq!(empty_stats.event_send_success_rate(), 1.0); // No tasks = 100% success
    }

    #[test]
    fn test_fallback_config() {
        let config = ReadinessFallbackConfig::default();
        assert!(config.enabled);
        assert_eq!(config.polling_interval, Duration::from_secs(30));
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.age_threshold, Duration::from_secs(5));
        assert_eq!(config.max_age, Duration::from_secs(24 * 3600));

        let custom_config = ReadinessFallbackConfig {
            enabled: false,
            polling_interval: Duration::from_secs(60),
            batch_size: 100,
            age_threshold: Duration::from_secs(10),
            max_age: Duration::from_secs(12 * 3600),
        };

        assert!(!custom_config.enabled);
        assert_eq!(custom_config.polling_interval, Duration::from_secs(60));
        assert_eq!(custom_config.batch_size, 100);
        assert_eq!(custom_config.age_threshold, Duration::from_secs(10));
        assert_eq!(custom_config.max_age, Duration::from_secs(12 * 3600));
    }

    // Note: Integration test with actual database would require setting up
    // test data in tasker_ready_tasks view, which is complex due to the view's
    // dependencies on multiple tables and state. This would be better tested
    // in the full integration test suite.
}
