//! Simplified fallback poller for catching missed ready tasks
//!
//! This module provides a simple background task that periodically runs
//! TaskClaimStepEnqueuer::process_batch() to catch any tasks that may have
//! been missed by the primary pgmq notification system.
//!
//! ## TAS-75 Phase 5b: Circuit Breaker Protection
//!
//! The poller is protected by a circuit breaker that opens when repeated failures
//! occur, preventing cascading failures and reducing load on unhealthy databases.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::circuit_breaker::{
    TaskReadinessCircuitBreaker, TaskReadinessCircuitBreakerConfig,
    TaskReadinessCircuitBreakerMetrics,
};
use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use tasker_shared::{SystemContext, TaskerResult};

/// Configuration for the fallback poller
#[derive(Debug, Clone)]
pub struct FallbackPollerConfig {
    /// Whether the fallback poller is enabled
    pub enabled: bool,
    /// Polling interval (e.g., 30 seconds)
    pub polling_interval: Duration,
    /// Circuit breaker configuration (TAS-75)
    pub circuit_breaker: TaskReadinessCircuitBreakerConfig,
}

impl Default for FallbackPollerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            polling_interval: Duration::from_secs(30),
            circuit_breaker: TaskReadinessCircuitBreakerConfig::default(),
        }
    }
}

/// Simple fallback poller that periodically processes ready tasks
pub struct FallbackPoller {
    config: FallbackPollerConfig,
    #[allow(dead_code)] // future need
    context: Arc<SystemContext>,
    task_claim_step_enqueuer: Arc<StepEnqueuerService>,
    /// TAS-75: Circuit breaker for polling operations
    circuit_breaker: Arc<TaskReadinessCircuitBreaker>,
    poller_id: Uuid,
    shutdown_handle: Option<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for FallbackPoller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FallbackPoller")
            .field("poller_id", &self.poller_id)
            .field("config", &self.config)
            .field(
                "circuit_breaker_state",
                &self.circuit_breaker.current_state(),
            )
            .field("has_shutdown_handle", &self.shutdown_handle.is_some())
            .finish()
    }
}

impl FallbackPoller {
    /// Create a new fallback poller
    pub async fn new(
        config: FallbackPollerConfig,
        context: Arc<SystemContext>,
    ) -> TaskerResult<Self> {
        let poller_id = Uuid::new_v4();

        info!(
            poller_id = %poller_id,
            enabled = config.enabled,
            interval_seconds = config.polling_interval.as_secs(),
            circuit_breaker_failure_threshold = config.circuit_breaker.failure_threshold,
            circuit_breaker_recovery_timeout = config.circuit_breaker.recovery_timeout_seconds,
            "Creating FallbackPoller with circuit breaker protection"
        );

        // Create a TaskClaimStepEnqueuer for processing batches
        let task_claim_step_enqueuer = Arc::new(StepEnqueuerService::new(context.clone()).await?);

        // TAS-75: Create circuit breaker for polling operations
        let circuit_breaker = Arc::new(TaskReadinessCircuitBreaker::new(
            config.circuit_breaker.clone(),
        ));

        Ok(Self {
            config,
            context,
            task_claim_step_enqueuer,
            circuit_breaker,
            poller_id,
            shutdown_handle: None,
        })
    }

    /// Start the fallback polling loop
    pub async fn start(&mut self) -> TaskerResult<()> {
        if !self.config.enabled {
            info!(poller_id = %self.poller_id, "Fallback polling disabled");
            return Ok(());
        }

        if self.shutdown_handle.is_some() {
            warn!(poller_id = %self.poller_id, "Fallback poller already running");
            return Ok(());
        }

        info!(
            poller_id = %self.poller_id,
            interval_seconds = self.config.polling_interval.as_secs(),
            "Starting fallback polling loop"
        );

        let config = self.config.clone();
        let enqueuer = Arc::clone(&self.task_claim_step_enqueuer);
        let circuit_breaker = Arc::clone(&self.circuit_breaker);
        let poller_id = self.poller_id;

        let handle = tokio::spawn(async move {
            let mut ticker = interval(config.polling_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;

                // TAS-75: Check circuit breaker before polling
                if circuit_breaker.is_circuit_open() {
                    warn!(
                        poller_id = %poller_id,
                        circuit_state = ?circuit_breaker.current_state(),
                        failures = circuit_breaker.current_failures(),
                        "Fallback poller: skipping cycle (circuit breaker open)"
                    );
                    continue;
                }

                debug!(
                    poller_id = %poller_id,
                    circuit_state = ?circuit_breaker.current_state(),
                    "Running fallback polling cycle"
                );

                match enqueuer.process_batch().await {
                    Ok(result) => {
                        // TAS-75: Record success
                        circuit_breaker.record_success();

                        if result.tasks_processed > 0 {
                            info!(
                                poller_id = %poller_id,
                                tasks_processed = result.tasks_processed,
                                tasks_failed = result.tasks_failed,
                                "Fallback poller found and processed ready tasks"
                            );
                        } else {
                            debug!(
                                poller_id = %poller_id,
                                "No ready tasks found in fallback polling cycle"
                            );
                        }
                    }
                    Err(e) => {
                        // TAS-75: Record failure
                        circuit_breaker.record_failure();

                        error!(
                            poller_id = %poller_id,
                            error = %e,
                            circuit_state = ?circuit_breaker.current_state(),
                            failures = circuit_breaker.current_failures(),
                            "Fallback polling cycle failed"
                        );
                    }
                }
            }
        });

        self.shutdown_handle = Some(handle);
        Ok(())
    }

    /// Stop the fallback polling loop
    pub async fn stop(&mut self) -> TaskerResult<()> {
        if let Some(handle) = self.shutdown_handle.take() {
            info!(poller_id = %self.poller_id, "Stopping fallback poller");
            handle.abort();
        }
        Ok(())
    }

    /// Check if the poller is running
    pub fn is_running(&self) -> bool {
        self.shutdown_handle.is_some()
    }

    /// Get the poller configuration
    pub fn config(&self) -> &FallbackPollerConfig {
        &self.config
    }

    /// Get the poller ID
    pub fn poller_id(&self) -> Uuid {
        self.poller_id
    }

    /// TAS-75: Get circuit breaker reference for health monitoring
    pub fn circuit_breaker(&self) -> &TaskReadinessCircuitBreaker {
        &self.circuit_breaker
    }

    /// TAS-75: Get circuit breaker metrics for health endpoints
    pub fn circuit_breaker_metrics(&self) -> TaskReadinessCircuitBreakerMetrics {
        self.circuit_breaker.metrics()
    }

    /// TAS-75: Check if circuit breaker is healthy
    pub fn is_circuit_breaker_healthy(&self) -> bool {
        self.circuit_breaker.is_healthy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fallback_poller_config() {
        let config = FallbackPollerConfig::default();
        assert!(config.enabled);
        assert_eq!(config.polling_interval, Duration::from_secs(30));

        let custom_config = FallbackPollerConfig {
            enabled: false,
            polling_interval: Duration::from_secs(60),
            circuit_breaker: TaskReadinessCircuitBreakerConfig::default(),
        };
        assert!(!custom_config.enabled);
        assert_eq!(custom_config.polling_interval, Duration::from_secs(60));
        assert_eq!(custom_config.circuit_breaker.failure_threshold, 10);
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_fallback_poller_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let config = FallbackPollerConfig::default();
        let poller = FallbackPoller::new(config, context).await?;

        assert!(!poller.is_running());
        assert!(poller.config().enabled);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_fallback_poller_disabled(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let config = FallbackPollerConfig {
            enabled: false,
            ..Default::default()
        };

        let mut poller = FallbackPoller::new(config, context).await?;
        poller.start().await?;

        // Should not actually start when disabled
        assert!(!poller.is_running());

        Ok(())
    }
}
