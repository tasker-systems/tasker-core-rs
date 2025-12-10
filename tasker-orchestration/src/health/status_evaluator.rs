//! # Status Evaluator
//!
//! TAS-75 Phase 5: Background task that evaluates all health status and updates caches.
//!
//! The `StatusEvaluator` is the central orchestration component for health monitoring.
//! It runs as a background task, periodically evaluating:
//! - Database connectivity and circuit breaker state
//! - Command channel saturation
//! - PGMQ queue depths
//!
//! All results are written to thread-safe caches that API handlers read from.

use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::monitoring::channel_metrics::ChannelMonitor;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::caches::HealthStatusCaches;
use super::channel_status::evaluate_channel_status;
use super::db_status::evaluate_db_status;
use super::queue_status::evaluate_queue_status;
use super::types::{
    BackpressureSource, BackpressureStatus, ChannelHealthStatus, DatabaseHealthStatus,
    HealthConfig, QueueDepthStatus, QueueDepthTier,
};
use crate::orchestration::command_processor::OrchestrationCommand;
use crate::web::circuit_breaker::WebDatabaseCircuitBreaker;

/// Background health status evaluator
///
/// This component runs in a background task and periodically evaluates all
/// health conditions, updating the shared caches that API handlers read from.
///
/// ## Lifecycle
///
/// ```ignore
/// // Create evaluator during bootstrap
/// let evaluator = StatusEvaluator::new(
///     caches.clone(),
///     db_pool.clone(),
///     channel_monitor,
///     command_sender,
///     queue_names,
///     circuit_breaker,
///     config,
/// );
///
/// // Spawn background task
/// let handle = evaluator.spawn();
///
/// // Handle is available for graceful shutdown
/// // handle.abort() when shutting down
/// ```
pub struct StatusEvaluator {
    /// Caches to update
    caches: HealthStatusCaches,

    /// Database pool for health checks
    db_pool: PgPool,

    /// Channel monitor for saturation tracking
    channel_monitor: ChannelMonitor,

    /// Command sender for capacity checks
    command_sender: mpsc::Sender<OrchestrationCommand>,

    /// Queue names to monitor
    queue_names: Vec<String>,

    /// Circuit breaker reference
    circuit_breaker: Arc<WebDatabaseCircuitBreaker>,

    /// Configuration
    config: HealthConfig,
}

impl std::fmt::Debug for StatusEvaluator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusEvaluator")
            .field(
                "evaluation_interval_ms",
                &self.config.evaluation_interval_ms,
            )
            .field("check_database", &self.config.check_database)
            .field("check_channels", &self.config.check_channels)
            .field("check_queues", &self.config.check_queues)
            .field("queue_count", &self.queue_names.len())
            .finish()
    }
}

impl StatusEvaluator {
    /// Create a new status evaluator
    ///
    /// # Arguments
    /// * `caches` - Shared caches to update
    /// * `db_pool` - Database connection pool
    /// * `channel_monitor` - Monitor for command channel
    /// * `command_sender` - Sender for capacity checks
    /// * `queue_names` - List of queue names to monitor
    /// * `circuit_breaker` - Circuit breaker for database operations
    /// * `config` - Health evaluation configuration
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        caches: HealthStatusCaches,
        db_pool: PgPool,
        channel_monitor: ChannelMonitor,
        command_sender: mpsc::Sender<OrchestrationCommand>,
        queue_names: Vec<String>,
        circuit_breaker: Arc<WebDatabaseCircuitBreaker>,
        config: HealthConfig,
    ) -> Self {
        Self {
            caches,
            db_pool,
            channel_monitor,
            command_sender,
            queue_names,
            circuit_breaker,
            config,
        }
    }

    /// Spawn background evaluation task
    ///
    /// Returns a `JoinHandle` that can be used for graceful shutdown.
    /// The task runs until aborted or the process exits.
    pub fn spawn(self) -> JoinHandle<()> {
        let interval_ms = self.config.evaluation_interval_ms;

        info!(
            interval_ms = interval_ms,
            check_database = self.config.check_database,
            check_channels = self.config.check_channels,
            check_queues = self.config.check_queues,
            queue_count = self.queue_names.len(),
            "Starting health status evaluator"
        );

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));

            loop {
                interval.tick().await;

                if let Err(e) = self.evaluate_all().await {
                    error!(error = %e, "Health evaluation cycle failed");
                }
            }
        })
    }

    /// Run a single evaluation cycle
    ///
    /// This is the main evaluation logic that checks all health conditions
    /// and updates the caches.
    async fn evaluate_all(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Starting health evaluation cycle");

        // 1. Check database health (if enabled)
        // When disabled, returns default with evaluated=false to indicate "unknown"
        let db_status = if self.config.check_database {
            evaluate_db_status(&self.db_pool, &self.circuit_breaker, &self.config.database).await
        } else {
            debug!("Database health check disabled - returning Unknown status");
            DatabaseHealthStatus::default() // evaluated=false indicates "not evaluated/unknown"
        };

        // 2. Check channel saturation (if enabled)
        // When disabled, returns default with evaluated=false to indicate "unknown"
        let channel_status = if self.config.check_channels {
            evaluate_channel_status(
                &self.channel_monitor,
                &self.command_sender,
                &self.config.channels,
            )
        } else {
            debug!("Channel health check disabled - returning Unknown status");
            ChannelHealthStatus::default() // evaluated=false indicates "not evaluated/unknown"
        };

        // 3. Check queue depths (if enabled)
        // When disabled, returns default with tier=Unknown to indicate "not evaluated"
        let queue_status = if self.config.check_queues {
            evaluate_queue_status(&self.db_pool, &self.queue_names, &self.config.queues).await
        } else {
            debug!("Queue depth check disabled - returning Unknown tier");
            QueueDepthStatus::default() // tier=Unknown indicates "not evaluated"
        };

        // 4. Compute aggregate backpressure decision
        let backpressure = self.compute_backpressure(&db_status, &channel_status, &queue_status);

        // 5. Update all caches atomically
        self.caches
            .update_all(db_status, channel_status, queue_status, backpressure)
            .await;

        debug!("Health evaluation cycle complete");

        Ok(())
    }

    /// Compute aggregate backpressure decision from all health statuses
    ///
    /// Backpressure is triggered when any of these conditions are met:
    /// 1. Circuit breaker is open
    /// 2. Channel saturation is critical (>95%)
    /// 3. Queue depth is at critical or overflow tier
    fn compute_backpressure(
        &self,
        db_status: &DatabaseHealthStatus,
        channel_status: &ChannelHealthStatus,
        queue_status: &QueueDepthStatus,
    ) -> BackpressureStatus {
        // Check circuit breaker first (highest priority)
        if db_status.circuit_breaker_open {
            return BackpressureStatus {
                active: true,
                reason: Some("Circuit breaker open".to_string()),
                retry_after_secs: Some(30),
                source: Some(BackpressureSource::CircuitBreaker),
            };
        }

        // Check channel saturation (critical threshold)
        if channel_status.is_critical {
            return BackpressureStatus {
                active: true,
                reason: Some(format!(
                    "Command channel critically saturated ({:.1}%)",
                    channel_status.command_saturation_percent
                )),
                retry_after_secs: Some(15),
                source: Some(BackpressureSource::ChannelSaturation {
                    channel: "command".to_string(),
                    saturation_percent: channel_status.command_saturation_percent,
                }),
            };
        }

        // Check queue depths (critical and overflow)
        match queue_status.tier {
            QueueDepthTier::Overflow => {
                return BackpressureStatus {
                    active: true,
                    reason: Some(format!(
                        "Queue '{}' at overflow depth ({} messages)",
                        queue_status.worst_queue, queue_status.max_depth
                    )),
                    retry_after_secs: Some(60),
                    source: Some(BackpressureSource::QueueDepth {
                        queue: queue_status.worst_queue.clone(),
                        depth: queue_status.max_depth,
                        tier: QueueDepthTier::Overflow,
                    }),
                };
            }
            QueueDepthTier::Critical => {
                return BackpressureStatus {
                    active: true,
                    reason: Some(format!(
                        "Queue '{}' at critical depth ({} messages)",
                        queue_status.worst_queue, queue_status.max_depth
                    )),
                    retry_after_secs: Some(30),
                    source: Some(BackpressureSource::QueueDepth {
                        queue: queue_status.worst_queue.clone(),
                        depth: queue_status.max_depth,
                        tier: QueueDepthTier::Critical,
                    }),
                };
            }
            QueueDepthTier::Warning => {
                // Warning tier doesn't trigger backpressure (only logged)
                warn!(
                    queue = %queue_status.worst_queue,
                    depth = queue_status.max_depth,
                    "Queue depth at warning level"
                );
            }
            QueueDepthTier::Normal => {
                // Normal operation - no action needed
            }
            QueueDepthTier::Unknown => {
                // Unknown status - we couldn't evaluate, fail-open (no backpressure)
                // but log for observability
                debug!("Queue depth status unknown - check may be disabled or evaluation failed");
            }
        }

        // No backpressure condition active
        BackpressureStatus::default()
    }

    /// Get reference to the caches (for testing)
    #[cfg(test)]
    pub fn caches(&self) -> &HealthStatusCaches {
        &self.caches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_compute_backpressure_healthy() {
        // Create a mock evaluator (we only need the compute_backpressure method)
        let config = HealthConfig::default();

        let db_status = DatabaseHealthStatus {
            evaluated: true,
            is_connected: true,
            circuit_breaker_open: false,
            circuit_breaker_failures: 0,
            last_check_duration_ms: 5,
            error_message: None,
        };

        let channel_status = ChannelHealthStatus {
            evaluated: true,
            command_saturation_percent: 25.0,
            command_available_capacity: 750,
            command_messages_sent: 100,
            command_overflow_events: 0,
            is_saturated: false,
            is_critical: false,
        };

        let queue_status = QueueDepthStatus {
            tier: QueueDepthTier::Normal,
            max_depth: 50,
            worst_queue: "test_queue".to_string(),
            queue_depths: HashMap::new(),
        };

        // Compute backpressure using a helper
        let backpressure =
            compute_backpressure_helper(&config, &db_status, &channel_status, &queue_status);

        assert!(!backpressure.active);
        assert!(backpressure.reason.is_none());
    }

    #[test]
    fn test_compute_backpressure_circuit_breaker_open() {
        let config = HealthConfig::default();

        let db_status = DatabaseHealthStatus {
            evaluated: true,
            is_connected: false,
            circuit_breaker_open: true,
            circuit_breaker_failures: 5,
            last_check_duration_ms: 0,
            error_message: Some("Circuit breaker open".to_string()),
        };

        let channel_status = ChannelHealthStatus::default();
        let queue_status = QueueDepthStatus::default();

        let backpressure =
            compute_backpressure_helper(&config, &db_status, &channel_status, &queue_status);

        assert!(backpressure.active);
        assert!(backpressure.reason.as_ref().unwrap().contains("Circuit"));
        assert!(matches!(
            backpressure.source,
            Some(BackpressureSource::CircuitBreaker)
        ));
    }

    #[test]
    fn test_compute_backpressure_channel_critical() {
        let config = HealthConfig::default();

        let db_status = DatabaseHealthStatus {
            evaluated: true,
            is_connected: true,
            circuit_breaker_open: false,
            circuit_breaker_failures: 0,
            last_check_duration_ms: 5,
            error_message: None,
        };

        let channel_status = ChannelHealthStatus {
            evaluated: true,
            command_saturation_percent: 97.0,
            command_available_capacity: 30,
            command_messages_sent: 1000,
            command_overflow_events: 2,
            is_saturated: true,
            is_critical: true,
        };

        let queue_status = QueueDepthStatus::default();

        let backpressure =
            compute_backpressure_helper(&config, &db_status, &channel_status, &queue_status);

        assert!(backpressure.active);
        assert!(backpressure.reason.as_ref().unwrap().contains("saturated"));
        assert!(matches!(
            backpressure.source,
            Some(BackpressureSource::ChannelSaturation { .. })
        ));
    }

    #[test]
    fn test_compute_backpressure_queue_critical() {
        let config = HealthConfig::default();

        let db_status = DatabaseHealthStatus {
            evaluated: true,
            is_connected: true,
            circuit_breaker_open: false,
            circuit_breaker_failures: 0,
            last_check_duration_ms: 5,
            error_message: None,
        };

        let channel_status = ChannelHealthStatus::default();

        let queue_status = QueueDepthStatus {
            tier: QueueDepthTier::Critical,
            max_depth: 7500,
            worst_queue: "orchestration_step_results".to_string(),
            queue_depths: HashMap::new(),
        };

        let backpressure =
            compute_backpressure_helper(&config, &db_status, &channel_status, &queue_status);

        assert!(backpressure.active);
        assert!(backpressure.reason.as_ref().unwrap().contains("critical"));
        assert!(matches!(
            backpressure.source,
            Some(BackpressureSource::QueueDepth {
                tier: QueueDepthTier::Critical,
                ..
            })
        ));
    }

    #[test]
    fn test_compute_backpressure_queue_overflow() {
        let config = HealthConfig::default();

        let db_status = DatabaseHealthStatus {
            evaluated: true,
            is_connected: true,
            circuit_breaker_open: false,
            circuit_breaker_failures: 0,
            last_check_duration_ms: 5,
            error_message: None,
        };

        let channel_status = ChannelHealthStatus::default();

        let queue_status = QueueDepthStatus {
            tier: QueueDepthTier::Overflow,
            max_depth: 15000,
            worst_queue: "orchestration_step_results".to_string(),
            queue_depths: HashMap::new(),
        };

        let backpressure =
            compute_backpressure_helper(&config, &db_status, &channel_status, &queue_status);

        assert!(backpressure.active);
        assert!(backpressure.reason.as_ref().unwrap().contains("overflow"));
        assert_eq!(backpressure.retry_after_secs, Some(60)); // Longer retry for overflow
    }

    #[test]
    fn test_compute_backpressure_priority_order() {
        // Circuit breaker should take priority even if other conditions are also bad
        let config = HealthConfig::default();

        let db_status = DatabaseHealthStatus {
            evaluated: true,
            is_connected: false,
            circuit_breaker_open: true,
            circuit_breaker_failures: 5,
            last_check_duration_ms: 0,
            error_message: Some("Circuit breaker open".to_string()),
        };

        let channel_status = ChannelHealthStatus {
            evaluated: true,
            command_saturation_percent: 99.0,
            command_available_capacity: 10,
            command_messages_sent: 1000,
            command_overflow_events: 5,
            is_saturated: true,
            is_critical: true,
        };

        let queue_status = QueueDepthStatus {
            tier: QueueDepthTier::Overflow,
            max_depth: 20000,
            worst_queue: "test".to_string(),
            queue_depths: HashMap::new(),
        };

        let backpressure =
            compute_backpressure_helper(&config, &db_status, &channel_status, &queue_status);

        // Circuit breaker should be the reported reason (highest priority)
        assert!(backpressure.active);
        assert!(matches!(
            backpressure.source,
            Some(BackpressureSource::CircuitBreaker)
        ));
    }

    // Helper function to test compute_backpressure without creating full StatusEvaluator
    fn compute_backpressure_helper(
        _config: &HealthConfig,
        db_status: &DatabaseHealthStatus,
        channel_status: &ChannelHealthStatus,
        queue_status: &QueueDepthStatus,
    ) -> BackpressureStatus {
        // Check circuit breaker first
        if db_status.circuit_breaker_open {
            return BackpressureStatus {
                active: true,
                reason: Some("Circuit breaker open".to_string()),
                retry_after_secs: Some(30),
                source: Some(BackpressureSource::CircuitBreaker),
            };
        }

        // Check channel saturation
        if channel_status.is_critical {
            return BackpressureStatus {
                active: true,
                reason: Some(format!(
                    "Command channel critically saturated ({:.1}%)",
                    channel_status.command_saturation_percent
                )),
                retry_after_secs: Some(15),
                source: Some(BackpressureSource::ChannelSaturation {
                    channel: "command".to_string(),
                    saturation_percent: channel_status.command_saturation_percent,
                }),
            };
        }

        // Check queue depths
        match queue_status.tier {
            QueueDepthTier::Overflow => {
                return BackpressureStatus {
                    active: true,
                    reason: Some(format!(
                        "Queue '{}' at overflow depth ({} messages)",
                        queue_status.worst_queue, queue_status.max_depth
                    )),
                    retry_after_secs: Some(60),
                    source: Some(BackpressureSource::QueueDepth {
                        queue: queue_status.worst_queue.clone(),
                        depth: queue_status.max_depth,
                        tier: QueueDepthTier::Overflow,
                    }),
                };
            }
            QueueDepthTier::Critical => {
                return BackpressureStatus {
                    active: true,
                    reason: Some(format!(
                        "Queue '{}' at critical depth ({} messages)",
                        queue_status.worst_queue, queue_status.max_depth
                    )),
                    retry_after_secs: Some(30),
                    source: Some(BackpressureSource::QueueDepth {
                        queue: queue_status.worst_queue.clone(),
                        depth: queue_status.max_depth,
                        tier: QueueDepthTier::Critical,
                    }),
                };
            }
            _ => {}
        }

        BackpressureStatus::default()
    }
}
