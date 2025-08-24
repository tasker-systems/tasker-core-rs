//! # Base Executor Implementation
//!
//! This module provides the `BaseExecutor` which implements the `OrchestrationExecutor` trait
//! with common functionality that can be used by all executor types. It provides lifecycle
//! management, health monitoring, metrics collection, and base processing capabilities.

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::{Notify, RwLock};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::executor::{Executor, ExecutorProcessor, ProcessBatchResult};
use crate::resilience::{CircuitBreaker, CircuitBreakerConfig};
use crate::{TaskerError, TaskerResult};

use super::health::{ExecutorHealth, HealthMonitor};
use super::metrics::{ExecutorMetrics, MetricsCollector};
use crate::config::orchestration::ExecutorConfig;
use crate::coordinator::core::CoordinatorCore;

#[derive(Debug, Clone)]
pub struct ProcessingLoop {
    /// Weak reference to the executor to avoid circular references
    pub weak_executor: Weak<BaseExecutor>,
    /// Shared state for the processing loop
    pub state: Arc<ProcessingState>,
}

/// Processing loop encapsulation for BaseExecutor
///
/// Shared state for the processing loop to avoid circular references
#[derive(Debug)]
pub struct ProcessingState {
    /// Control flag for the processing loop
    pub running: Arc<AtomicBool>,
    /// Shutdown notification mechanism
    pub shutdown_notify: Arc<Notify>,
    /// Executor ID for logging
    pub executor_id: Uuid,
    /// Executor type for logging
    pub executor_processor: String,
}

/// Base implementation of OrchestrationExecutor trait
///
/// This struct provides a foundation for all executor types with common functionality
/// including lifecycle management, health monitoring, metrics collection, and
/// circuit breaker integration.
#[derive(Debug)]
pub struct BaseExecutor {
    /// Unique identifier for this executor instance
    pub id: Uuid,
    /// Type of executor
    pub executor_processor: Box<dyn ExecutorProcessor>,
    /// Configuration for this executor instance
    pub config: Arc<RwLock<ExecutorConfig>>,
    /// Database connection pool
    pool: PgPool,
    /// Current configuration
    /// Health monitor
    pub health_monitor: Arc<HealthMonitor>,
    /// Metrics collector
    pub metrics_collector: Arc<MetricsCollector>,
    /// Circuit breaker for fault tolerance
    pub circuit_breaker: Arc<CircuitBreaker>,
    /// Processing loop for this executor
    pub processing_loop: Arc<RwLock<Option<ProcessingLoop>>>,
    /// Processing task handle
    pub processing_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Optional orchestration core for real processing (None for simple executors)
    core: Option<Arc<CoordinatorCore>>,
}

impl BaseExecutor {
    /// Create a new BaseExecutor
    pub fn new(executor_processor: Box<dyn ExecutorProcessor>, pool: PgPool) -> Self {
        let id = Uuid::new_v4();
        let health_monitor = Arc::new(HealthMonitor::new(id));
        let metrics_collector = Arc::new(MetricsCollector::new(
            id,
            executor_processor.name().to_string(),
        ));

        // Create circuit breaker with executor-specific configuration
        let cb_config = CircuitBreakerConfig {
            failure_threshold: executor_processor.config().circuit_breaker_threshold as u32,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        };
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("{}_executor", executor_processor.name()),
            cb_config,
        ));

        let config = Arc::new(RwLock::new(executor_processor.config().clone()));

        Self {
            id,
            executor_processor,
            config,
            pool,
            health_monitor,
            metrics_collector,
            circuit_breaker,
            processing_loop: Arc::new(RwLock::new(None)),
            processing_handle: Arc::new(RwLock::new(None)),
            core: None,
        }
    }

    /// Create a new BaseExecutor with CoordinatorCore access for real processing
    pub fn with_core(
        executor_processor: Box<dyn ExecutorProcessor>,
        core: Arc<CoordinatorCore>,
    ) -> TaskerResult<Self> {
        let id = Uuid::new_v4();
        let health_monitor = Arc::new(HealthMonitor::new(id));
        let metrics_collector = Arc::new(MetricsCollector::new(
            id,
            executor_processor.name().to_string(),
        ));

        // Create circuit breaker with provided configuration
        let cb_config = CircuitBreakerConfig {
            failure_threshold: executor_processor.config().circuit_breaker_threshold as u32,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        };
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("{}_executor", executor_processor.name()),
            cb_config,
        ));

        let config = Arc::new(RwLock::new(executor_processor.config().clone()));

        Ok(Self {
            id,
            executor_processor,
            config,
            pool: core.database_pool().clone(),
            health_monitor,
            metrics_collector,
            circuit_breaker,
            processing_loop: Arc::new(RwLock::new(None)),
            processing_handle: Arc::new(RwLock::new(None)),
            core: Some(core),
        })
    }

    /// Get database pool reference
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get health monitor reference
    pub fn health_monitor(&self) -> &Arc<HealthMonitor> {
        &self.health_monitor
    }

    /// Get metrics collector reference
    pub fn metrics_collector(&self) -> &Arc<MetricsCollector> {
        &self.metrics_collector
    }

    /// Get circuit breaker reference
    pub fn circuit_breaker(&self) -> &Arc<CircuitBreaker> {
        &self.circuit_breaker
    }
}

#[async_trait]
impl Executor for BaseExecutor {
    fn id(&self) -> Uuid {
        self.id
    }

    #[instrument(skip(self), fields(executor_id = %self.id, executor_type = %self.executor_processor.name()))]
    async fn start(self: Arc<Self>) -> TaskerResult<()> {
        // Check if already running by looking for existing processing loop
        {
            let processing_loop_guard = self.processing_loop.read().await;
            if let Some(existing_loop) = processing_loop_guard.as_ref() {
                if existing_loop.should_continue() {
                    return Err(TaskerError::InvalidState(
                        "Executor is already running".to_string(),
                    ));
                }
            }
        }

        info!("Starting {} executor", self.executor_processor.name());

        // Mark as starting
        self.health_monitor.mark_starting("initializing").await?;

        // Create shared state for the processing loop
        let processing_state = Arc::new(ProcessingState {
            running: Arc::new(AtomicBool::new(false)),
            shutdown_notify: Arc::new(Notify::new()),
            executor_id: self.id,
            executor_processor: self.executor_processor.name().to_string(),
        });

        // Create processing loop with weak reference to avoid circular dependency
        let weak_self = Arc::downgrade(&self);
        let processing_loop = ProcessingLoop::new_with_weak(weak_self, processing_state.clone());

        // Start the processing loop (this sets the running flag)
        // Then spawn a task to run it
        processing_state.running.store(true, Ordering::Release);
        let processing_loop_for_spawn = processing_loop.clone();
        let processing_handle = tokio::spawn(async move {
            if let Err(e) = processing_loop_for_spawn.run().await {
                error!("Processing loop failed: {}", e);
            }
        });

        // Store the processing loop and handle
        *self.processing_loop.write().await = Some(processing_loop);
        *self.processing_handle.write().await = Some(processing_handle);

        // Give the processing loop a moment to start properly
        tokio::time::sleep(Duration::from_millis(10)).await;

        info!(
            "Started {} executor successfully",
            self.executor_processor.name()
        );
        Ok(())
    }

    #[instrument(skip(self), fields(executor_id = %self.id, executor_type = %self.executor_processor.name()))]
    async fn stop(&self, timeout: Duration) -> TaskerResult<()> {
        // Check if already stopped
        {
            let processing_loop_guard = self.processing_loop.read().await;
            if let Some(processing_loop) = processing_loop_guard.as_ref() {
                if !processing_loop.should_continue() {
                    return Ok(()); // Already stopped
                }
            } else {
                return Ok(()); // Never started
            }
        }

        info!("Stopping {} executor", self.executor_processor.name());

        // Mark as stopping
        self.health_monitor
            .mark_stopping(true, "graceful_shutdown")
            .await?;

        // Stop the processing loop
        let processing_loop = {
            let mut processing_loop_guard = self.processing_loop.write().await;
            processing_loop_guard.take()
        };

        if let Some(processing_loop) = processing_loop {
            if let Err(e) = processing_loop.stop(timeout).await {
                warn!("Failed to stop processing loop gracefully: {}", e);
            }
        }

        // Wait for processing task to finish
        if let Some(handle) = self.processing_handle.write().await.take() {
            match tokio::time::timeout(timeout, handle).await {
                Ok(Ok(())) => {
                    info!("Processing task stopped gracefully");
                }
                Ok(Err(e)) => {
                    warn!("Processing task ended with error: {}", e);
                }
                Err(_) => {
                    warn!("Processing task did not stop within timeout, aborting");
                    // Still clear the processing loop even if timeout occurred
                    *self.processing_loop.write().await = None;
                    return Err(TaskerError::Timeout(
                        "Executor stop timeout exceeded".to_string(),
                    ));
                }
            }
        }

        // Clear the processing loop after successful stop
        *self.processing_loop.write().await = None;

        info!(
            "Stopped {} executor successfully",
            self.executor_processor.name()
        );
        Ok(())
    }

    async fn health(&self) -> ExecutorHealth {
        self.health_monitor
            .current_health()
            .await
            .unwrap_or_else(|_| ExecutorHealth::Unhealthy {
                last_seen: 0,
                reason: "Unable to read health state".to_string(),
                unhealthy_for_seconds: 0,
            })
    }

    async fn metrics(&self) -> ExecutorMetrics {
        self.metrics_collector
            .current_metrics()
            .await
            .unwrap_or_else(|_| {
                // Return basic metrics if we can't read from collector
                ExecutorMetrics {
                    executor_id: self.id,
                    executor_type: self.executor_processor.name().to_string(),
                    collected_at: 0,
                    uptime_seconds: 0,
                    total_items_processed: 0,
                    total_items_failed: 0,
                    total_items_skipped: 0,
                    items_per_second: 0.0,
                    avg_processing_time_ms: 0.0,
                    p95_processing_time_ms: 0.0,
                    p99_processing_time_ms: 0.0,
                    error_rate: 0.0,
                    total_retries: 0,
                    error_counts: std::collections::HashMap::new(),
                    utilization: 0.0,
                    avg_batch_size: 0.0,
                    backpressure_factor: 1.0,
                    active_connections: 0,
                    queue_depth: 0,
                    batches_completed: 0,
                    empty_polls: 0,
                    polling_interval_ms: 100,
                    circuit_breaker_state: "unknown".to_string(),
                    circuit_breaker_trips: 0,
                    circuit_breaker_retry_seconds: None,
                    recent_metrics: Default::default(),
                }
            })
    }

    async fn process_batch(&self) -> TaskerResult<ProcessBatchResult> {
        let config = self.config.read().await;
        todo!("Implement batch processing logic");
    }

    async fn heartbeat(&self) -> TaskerResult<()> {
        self.health_monitor.heartbeat().await
    }

    fn should_continue(&self) -> bool {
        // Check if we have a processing loop that is actually running
        if let Ok(processing_loop_guard) = self.processing_loop.try_read() {
            if let Some(processing_loop) = processing_loop_guard.as_ref() {
                processing_loop.should_continue()
            } else {
                false // No processing loop means not running
            }
        } else {
            false // Can't acquire lock, assume not running
        }
    }

    async fn apply_backpressure(&self, factor: f64) -> TaskerResult<()> {
        if !(0.0..=1.0).contains(&factor) {
            return Err(TaskerError::InvalidParameter(
                "Backpressure factor must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Update configuration with new backpressure factor
        {
            let mut config = self.config.write().await;
            config.backpressure_factor = factor;
        }

        // Update metrics
        self.metrics_collector
            .set_backpressure_factor(factor)
            .await?;

        // Update polling interval in metrics
        let config = self.config.read().await;
        self.metrics_collector
            .set_polling_interval_ms(config.effective_polling_interval_ms())
            .await?;

        debug!(
            "Applied backpressure factor {} to {} executor",
            factor,
            self.executor_processor.name()
        );

        Ok(())
    }

    async fn get_config(&self) -> ExecutorConfig {
        self.config.read().await.clone()
    }

    async fn update_config(&self, new_config: ExecutorConfig) -> TaskerResult<()> {
        new_config.validate()?;

        {
            let mut config = self.config.write().await;
            *config = new_config;
        }

        // Update metrics with new configuration
        let config = self.config.read().await;
        self.metrics_collector
            .set_polling_interval_ms(config.effective_polling_interval_ms())
            .await?;
        self.metrics_collector
            .set_backpressure_factor(config.backpressure_factor)
            .await?;

        info!(
            "Updated configuration for {} executor",
            self.executor_processor.name()
        );

        Ok(())
    }
}

// Arc<BaseExecutor> already implements Clone automatically

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn create_test_database_pool() -> TaskerResult<PgPool> {
        let database_url = crate::test_utils::get_test_database_url();

        PgPool::connect(&database_url)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to connect to database: {e}")))
    }
}
