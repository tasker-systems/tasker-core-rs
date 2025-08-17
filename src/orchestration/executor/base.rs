//! # Base Executor Implementation
//!
//! This module provides the `BaseExecutor` which implements the `OrchestrationExecutor` trait
//! with common functionality that can be used by all executor types. It provides lifecycle
//! management, health monitoring, metrics collection, and base processing capabilities.

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, RwLock};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::error::{Result, TaskerError};
use crate::orchestration::OrchestrationCore;
use crate::resilience::{CircuitBreaker, CircuitBreakerConfig};

use super::health::{ExecutorHealth, HealthMonitor};
use super::metrics::{ExecutorMetrics, MetricsCollector};
use super::traits::{ExecutorConfig, ExecutorType, OrchestrationExecutor, ProcessBatchResult};

/// Processing loop encapsulation for BaseExecutor
///
/// Shared state for the processing loop to avoid circular references
#[derive(Debug)]
pub struct ProcessingState {
    /// Control flag for the processing loop
    running: Arc<AtomicBool>,
    /// Shutdown notification mechanism
    shutdown_notify: Arc<Notify>,
    /// Executor ID for logging
    executor_id: Uuid,
    /// Executor type for logging
    executor_type: ExecutorType,
}

/// This struct separates the processing loop concerns from the BaseExecutor,
/// providing cleaner architecture and better testability.
#[derive(Debug, Clone)]
pub struct ProcessingLoop {
    /// Weak reference to the executor to avoid circular references
    weak_executor: Weak<BaseExecutor>,
    /// Shared state for the processing loop
    state: Arc<ProcessingState>,
}

impl ProcessingLoop {
    /// Create a new processing loop for the given executor using weak reference
    pub fn new_with_weak(executor: Weak<BaseExecutor>, state: Arc<ProcessingState>) -> Self {
        Self {
            weak_executor: executor,
            state,
        }
    }

    /// Create a new processing loop for the given executor (legacy method for compatibility)
    pub fn new(executor: Arc<BaseExecutor>) -> Self {
        let state = Arc::new(ProcessingState {
            running: Arc::new(AtomicBool::new(false)),
            shutdown_notify: Arc::new(Notify::new()),
            executor_id: executor.id,
            executor_type: executor.executor_type,
        });

        Self {
            weak_executor: Arc::downgrade(&executor),
            state,
        }
    }

    /// Start the processing loop
    pub async fn start(&self) -> Result<()> {
        if self.state.running.load(Ordering::Acquire) {
            return Err(TaskerError::InvalidState(
                "Processing loop is already running".to_string(),
            ));
        }

        self.state.running.store(true, Ordering::Release);
        info!("ProcessingLoop started and running flag set to true");
        self.run().await
    }

    /// Stop the processing loop gracefully
    pub async fn stop(&self, timeout: Duration) -> Result<()> {
        if !self.state.running.load(Ordering::Acquire) {
            return Ok(());
        }

        self.state.running.store(false, Ordering::Release);
        self.state.shutdown_notify.notify_waiters();

        // Wait for the loop to actually stop (would need to be handled by the spawn)
        tokio::time::timeout(timeout, async {
            while self.state.running.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| TaskerError::Timeout("Processing loop stop timeout".to_string()))?;

        Ok(())
    }

    /// Check if the processing loop should continue running
    pub fn should_continue(&self) -> bool {
        self.state.running.load(Ordering::Acquire)
    }

    /// Helper method to process a batch with a strong executor reference
    async fn process_batch_with_executor(&self, executor: &Arc<BaseExecutor>) -> Result<()> {
        let loop_start = Instant::now();

        // Check circuit breaker state
        let cb_state = executor.circuit_breaker.state();

        // Calculate retry time for circuit breaker (estimated based on typical pattern)
        let retry_time_instant = match cb_state {
            crate::resilience::CircuitState::Open => Some(Instant::now() + Duration::from_secs(30)), // Typical open circuit timeout
            crate::resilience::CircuitState::HalfOpen => {
                Some(Instant::now() + Duration::from_secs(5))
            } // Shorter retry for half-open
            crate::resilience::CircuitState::Closed => None, // Normal operation
        };

        let state_str = match cb_state {
            crate::resilience::CircuitState::Closed => "closed",
            crate::resilience::CircuitState::Open => "open",
            crate::resilience::CircuitState::HalfOpen => "half_open",
        };

        executor
            .metrics_collector
            .set_circuit_breaker_state(state_str, retry_time_instant)
            .await?;

        if matches!(cb_state, crate::resilience::CircuitState::Open) {
            debug!("Circuit breaker is open, skipping batch processing");
            tokio::time::sleep(Duration::from_secs(1)).await;
            return Ok(());
        }

        // Process the batch through the executor
        let batch_result = executor.process_batch().await?;

        // Record batch metrics
        let processing_time_ms = loop_start.elapsed().as_millis() as u64;
        executor
            .metrics_collector
            .record_batch(
                batch_result.processed_count as u64,
                batch_result.failed_count as u64,
                processing_time_ms,
                batch_result.processed_count
                    + batch_result.failed_count
                    + batch_result.skipped_count,
            )
            .await?;

        // Update health based on result
        executor
            .health_monitor
            .record_performance(
                processing_time_ms,
                batch_result.failed_count == 0,
                batch_result.processed_count
                    + batch_result.failed_count
                    + batch_result.skipped_count,
            )
            .await?;

        Ok(())
    }

    /// Main processing loop implementation
    pub async fn run(&self) -> Result<()> {
        info!(
            executor_id = %self.state.executor_id,
            executor_type = %self.state.executor_type.name(),
            "Starting processing loop"
        );

        // Try to upgrade weak reference for initial setup
        if let Some(executor) = self.weak_executor.upgrade() {
            // Mark as starting
            executor
                .health_monitor
                .mark_starting("processing_loop")
                .await?;
        } else {
            warn!(
                executor_id = %self.state.executor_id,
                "Executor dropped before processing loop could start"
            );
            return Ok(());
        }

        while self.should_continue() {
            // Upgrade weak reference for each iteration
            if let Some(executor) = self.weak_executor.upgrade() {
                if let Err(e) = self.process_batch_with_executor(&executor).await {
                    error!(
                        executor_id = %self.state.executor_id,
                        error = %e,
                        "Batch processing failed"
                    );

                    // Wait a bit before retrying on error, but respect shutdown
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(1000)) => {},
                        _ = self.state.shutdown_notify.notified() => {
                            debug!("Shutdown notification received during error recovery wait");
                            break;
                        }
                    }
                    continue;
                }

                // Get wait time from config
                let current_config = executor.config.read().await;
                let wait_time =
                    Duration::from_millis(current_config.effective_polling_interval_ms());
                drop(current_config);

                // Wait with ability to be interrupted by shutdown
                tokio::select! {
                    _ = tokio::time::sleep(wait_time) => {},
                    _ = self.state.shutdown_notify.notified() => {
                        debug!("Shutdown notification received");
                        break;
                    }
                }
            } else {
                // Executor has been dropped, exit loop
                warn!(
                    executor_id = %self.state.executor_id,
                    "Executor dropped during processing loop"
                );
                break;
            }
        }

        // Mark as stopped
        self.state.running.store(false, Ordering::Release);

        info!(
            executor_id = %self.state.executor_id,
            executor_type = %self.state.executor_type.name(),
            "Processing loop ended"
        );
        Ok(())
    }
}

/// Base implementation of OrchestrationExecutor trait
///
/// This struct provides a foundation for all executor types with common functionality
/// including lifecycle management, health monitoring, metrics collection, and
/// circuit breaker integration.
#[derive(Debug)]
pub struct BaseExecutor {
    /// Unique identifier for this executor instance
    id: Uuid,
    /// Type of executor
    executor_type: ExecutorType,
    /// Database connection pool
    pool: PgPool,
    /// Current configuration
    config: Arc<RwLock<ExecutorConfig>>,
    /// Health monitor
    health_monitor: Arc<HealthMonitor>,
    /// Metrics collector
    metrics_collector: Arc<MetricsCollector>,
    /// Circuit breaker for fault tolerance
    circuit_breaker: Arc<CircuitBreaker>,
    /// Processing loop for this executor
    processing_loop: Arc<RwLock<Option<ProcessingLoop>>>,
    /// Processing task handle
    processing_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Optional orchestration core for real processing (None for simple executors)
    orchestration_core: Option<Arc<OrchestrationCore>>,
}

impl BaseExecutor {
    /// Create a new BaseExecutor
    pub fn new(executor_type: ExecutorType, pool: PgPool) -> Self {
        let id = Uuid::new_v4();
        let config = ExecutorConfig::default_for_type(executor_type);
        let health_monitor = Arc::new(HealthMonitor::new(id));
        let metrics_collector =
            Arc::new(MetricsCollector::new(id, executor_type.name().to_string()));

        // Create circuit breaker with executor-specific configuration
        let cb_config = CircuitBreakerConfig {
            failure_threshold: config.circuit_breaker_threshold as usize,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        };
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("{}_executor", executor_type.name()),
            cb_config,
        ));

        Self {
            id,
            executor_type,
            pool,
            config: Arc::new(RwLock::new(config)),
            health_monitor,
            metrics_collector,
            circuit_breaker,
            processing_loop: Arc::new(RwLock::new(None)),
            processing_handle: Arc::new(RwLock::new(None)),
            orchestration_core: None,
        }
    }

    /// Create a new BaseExecutor with custom configuration
    pub fn with_config(
        executor_type: ExecutorType,
        pool: PgPool,
        config: ExecutorConfig,
    ) -> Result<Self> {
        config.validate()?;

        let id = Uuid::new_v4();
        let health_monitor = Arc::new(HealthMonitor::new(id));
        let metrics_collector =
            Arc::new(MetricsCollector::new(id, executor_type.name().to_string()));

        // Create circuit breaker with provided configuration
        let cb_config = CircuitBreakerConfig {
            failure_threshold: config.circuit_breaker_threshold as usize,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        };
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("{}_executor", executor_type.name()),
            cb_config,
        ));

        Ok(Self {
            id,
            executor_type,
            pool,
            config: Arc::new(RwLock::new(config)),
            health_monitor,
            metrics_collector,
            circuit_breaker,
            processing_loop: Arc::new(RwLock::new(None)),
            processing_handle: Arc::new(RwLock::new(None)),
            orchestration_core: None,
        })
    }

    /// Create a new BaseExecutor with OrchestrationCore access for real processing
    pub fn with_orchestration_core(
        executor_type: ExecutorType,
        orchestration_core: Arc<OrchestrationCore>,
        config: ExecutorConfig,
    ) -> Result<Self> {
        config.validate()?;

        let id = Uuid::new_v4();
        let health_monitor = Arc::new(HealthMonitor::new(id));
        let metrics_collector =
            Arc::new(MetricsCollector::new(id, executor_type.name().to_string()));

        // Create circuit breaker with provided configuration
        let cb_config = CircuitBreakerConfig {
            failure_threshold: config.circuit_breaker_threshold as usize,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        };
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("{}_executor", executor_type.name()),
            cb_config,
        ));

        Ok(Self {
            id,
            executor_type,
            pool: orchestration_core.database_pool().clone(),
            config: Arc::new(RwLock::new(config)),
            health_monitor,
            metrics_collector,
            circuit_breaker,
            processing_loop: Arc::new(RwLock::new(None)),
            processing_handle: Arc::new(RwLock::new(None)),
            orchestration_core: Some(orchestration_core),
        })
    }

    /// Process a batch based on executor type
    async fn process_batch_for_type(
        executor_type: ExecutorType,
        _pool: &PgPool,
        _config: &ExecutorConfig,
        orchestration_core: &Option<Arc<OrchestrationCore>>,
    ) -> Result<ProcessBatchResult> {
        match executor_type {
            ExecutorType::TaskRequestProcessor => {
                Self::process_task_requests(orchestration_core).await
            }
            ExecutorType::OrchestrationLoop => {
                Self::process_orchestration_cycle(orchestration_core).await
            }
            ExecutorType::StepResultProcessor => {
                Self::process_step_results(orchestration_core).await
            }
        }
    }

    /// Process task requests from the task request queue
    async fn process_task_requests(
        orchestration_core: &Option<Arc<OrchestrationCore>>,
    ) -> Result<ProcessBatchResult> {
        if let Some(core) = orchestration_core {
            let start = Instant::now();

            // Delegate to TaskRequestProcessor's process_batch method
            debug!("Delegating to TaskRequestProcessor::process_batch");

            match core.task_request_processor.process_batch().await {
                Ok(processed_count) => {
                    let processing_time_ms = start.elapsed().as_millis() as u64;
                    debug!(
                        "TaskRequestProcessor processed {} tasks in {}ms",
                        processed_count, processing_time_ms
                    );

                    Ok(ProcessBatchResult {
                        processed_count,
                        failed_count: 0,
                        skipped_count: 0,
                        processing_time_ms,
                        avg_processing_time_ms: if processed_count > 0 {
                            processing_time_ms as f64 / (processed_count as f64).max(1.0)
                        } else {
                            0.0
                        },
                        warnings: vec![],
                        has_more_items: processed_count > 0,
                    })
                }
                Err(e) => {
                    error!("TaskRequestProcessor failed: {}", e);
                    Ok(ProcessBatchResult {
                        processed_count: 0,
                        failed_count: 1,
                        skipped_count: 0,
                        processing_time_ms: start.elapsed().as_millis() as u64,
                        avg_processing_time_ms: 0.0,
                        warnings: vec![format!("Task request processing error: {}", e)],
                        has_more_items: true, // Assume there might be more to retry
                    })
                }
            }
        } else {
            // No orchestration core available
            debug!("No OrchestrationCore available for TaskRequestProcessor");
            Ok(ProcessBatchResult::empty())
        }
    }

    /// Process orchestration cycle for task claiming and step enqueueing
    async fn process_orchestration_cycle(
        orchestration_core: &Option<Arc<OrchestrationCore>>,
    ) -> Result<ProcessBatchResult> {
        if let Some(core) = orchestration_core {
            let start = Instant::now();

            // Delegate to OrchestrationLoop's run_cycle method
            debug!("Delegating to OrchestrationLoop::run_cycle");

            match core.orchestration_loop.run_cycle().await {
                Ok(cycle_result) => {
                    let processing_time_ms = cycle_result.cycle_duration_ms;
                    debug!(
                        "OrchestrationLoop processed {} tasks, enqueued {} steps in {}ms",
                        cycle_result.tasks_processed,
                        cycle_result.total_steps_enqueued,
                        processing_time_ms
                    );

                    Ok(ProcessBatchResult {
                        processed_count: cycle_result.tasks_processed,
                        failed_count: cycle_result.tasks_failed,
                        skipped_count: 0,
                        processing_time_ms,
                        avg_processing_time_ms: if cycle_result.tasks_processed > 0 {
                            processing_time_ms as f64
                                / (cycle_result.tasks_processed as f64).max(1.0)
                        } else {
                            0.0
                        },
                        warnings: vec![],
                        has_more_items: cycle_result.tasks_claimed > 0
                            || cycle_result.tasks_processed > 0,
                    })
                }
                Err(e) => {
                    error!("OrchestrationLoop failed: {}", e);
                    Ok(ProcessBatchResult {
                        processed_count: 0,
                        failed_count: 1,
                        skipped_count: 0,
                        processing_time_ms: start.elapsed().as_millis() as u64,
                        avg_processing_time_ms: 0.0,
                        warnings: vec![format!("Orchestration cycle error: {}", e)],
                        has_more_items: true, // Assume there might be more tasks to process
                    })
                }
            }
        } else {
            // No orchestration core available
            debug!("No OrchestrationCore available for OrchestrationLoop");
            Ok(ProcessBatchResult::empty())
        }
    }

    /// Process step results from orchestration result queue
    async fn process_step_results(
        orchestration_core: &Option<Arc<OrchestrationCore>>,
    ) -> Result<ProcessBatchResult> {
        if let Some(core) = orchestration_core {
            let start = Instant::now();

            // Delegate to StepResultProcessor's process_step_result_batch method
            debug!("Delegating to StepResultProcessor::process_step_result_batch");

            match core.step_result_processor.process_step_result_batch().await {
                Ok(processed_count) => {
                    let processing_time_ms = start.elapsed().as_millis() as u64;
                    debug!(
                        "StepResultProcessor processed {} step results in {}ms",
                        processed_count, processing_time_ms
                    );

                    Ok(ProcessBatchResult {
                        processed_count,
                        failed_count: 0,
                        skipped_count: 0,
                        processing_time_ms,
                        avg_processing_time_ms: if processed_count > 0 {
                            processing_time_ms as f64 / (processed_count as f64).max(1.0)
                        } else {
                            0.0
                        },
                        warnings: vec![],
                        has_more_items: processed_count > 0,
                    })
                }
                Err(e) => {
                    error!("StepResultProcessor failed: {}", e);
                    Ok(ProcessBatchResult {
                        processed_count: 0,
                        failed_count: 1,
                        skipped_count: 0,
                        processing_time_ms: start.elapsed().as_millis() as u64,
                        avg_processing_time_ms: 0.0,
                        warnings: vec![format!("Step result processing error: {}", e)],
                        has_more_items: true, // Assume there might be more to retry
                    })
                }
            }
        } else {
            // No orchestration core available
            debug!("No OrchestrationCore available for StepResultProcessor");
            Ok(ProcessBatchResult::empty())
        }
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
impl OrchestrationExecutor for BaseExecutor {
    fn id(&self) -> Uuid {
        self.id
    }

    fn executor_type(&self) -> ExecutorType {
        self.executor_type
    }

    #[instrument(skip(self), fields(executor_id = %self.id, executor_type = %self.executor_type.name()))]
    async fn start(self: Arc<Self>) -> Result<()> {
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

        info!("Starting {} executor", self.executor_type.name());

        // Mark as starting
        self.health_monitor.mark_starting("initializing").await?;

        // Create shared state for the processing loop
        let processing_state = Arc::new(ProcessingState {
            running: Arc::new(AtomicBool::new(false)),
            shutdown_notify: Arc::new(Notify::new()),
            executor_id: self.id,
            executor_type: self.executor_type,
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
            self.executor_type.name()
        );
        Ok(())
    }

    #[instrument(skip(self), fields(executor_id = %self.id, executor_type = %self.executor_type.name()))]
    async fn stop(&self, timeout: Duration) -> Result<()> {
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

        info!("Stopping {} executor", self.executor_type.name());

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
            self.executor_type.name()
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
                    executor_type: self.executor_type.name().to_string(),
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

    async fn process_batch(&self) -> Result<ProcessBatchResult> {
        let config = self.config.read().await;
        Self::process_batch_for_type(
            self.executor_type,
            &self.pool,
            &config,
            &self.orchestration_core,
        )
        .await
    }

    async fn heartbeat(&self) -> Result<()> {
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

    async fn apply_backpressure(&self, factor: f64) -> Result<()> {
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
            self.executor_type.name()
        );

        Ok(())
    }

    async fn get_config(&self) -> ExecutorConfig {
        self.config.read().await.clone()
    }

    async fn update_config(&self, new_config: ExecutorConfig) -> Result<()> {
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
            self.executor_type.name()
        );

        Ok(())
    }
}

// Arc<BaseExecutor> already implements Clone automatically

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn create_test_database_pool() -> Result<PgPool> {
        let database_url = crate::test_utils::get_test_database_url();

        PgPool::connect(&database_url)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to connect to database: {e}")))
    }

    #[tokio::test]
    async fn test_base_executor_creation() {
        let pool = create_test_database_pool().await.unwrap();
        let executor = BaseExecutor::new(ExecutorType::TaskRequestProcessor, pool);

        assert_eq!(executor.executor_type(), ExecutorType::TaskRequestProcessor);
        assert!(!executor.should_continue()); // Should be false until started
    }

    #[tokio::test]
    async fn test_executor_lifecycle() {
        let pool = create_test_database_pool().await.unwrap();
        let executor = Arc::new(BaseExecutor::new(ExecutorType::OrchestrationLoop, pool));

        // Start executor
        assert!(executor.clone().start().await.is_ok());
        assert!(executor.should_continue());

        // Check health
        let health = executor.health().await;
        assert!(
            matches!(health, ExecutorHealth::Starting { .. })
                || matches!(health, ExecutorHealth::Healthy { .. })
        );

        // Stop executor - use minimal timeout, processing loop will eventually stop
        let _ = executor.stop(Duration::from_millis(500)).await;
        assert!(!executor.should_continue());
    }

    #[tokio::test]
    async fn test_executor_configuration() {
        let pool = create_test_database_pool().await.unwrap();
        let mut config = ExecutorConfig::default_for_type(ExecutorType::OrchestrationLoop);
        config.batch_size = 42;
        config.polling_interval_ms = 250;

        let executor =
            BaseExecutor::with_config(ExecutorType::OrchestrationLoop, pool, config).unwrap();

        let current_config = executor.get_config().await;
        assert_eq!(current_config.batch_size, 42);
        assert_eq!(current_config.polling_interval_ms, 250);
    }

    #[tokio::test]
    async fn test_backpressure_application() {
        let pool = create_test_database_pool().await.unwrap();
        let executor = BaseExecutor::new(ExecutorType::StepResultProcessor, pool);

        // Apply backpressure
        assert!(executor.apply_backpressure(0.5).await.is_ok());

        let config = executor.get_config().await;
        assert_eq!(config.backpressure_factor, 0.5);

        // Test invalid backpressure factor
        assert!(executor.apply_backpressure(1.5).await.is_err());
    }

    #[tokio::test]
    async fn test_heartbeat_and_metrics() {
        let pool = create_test_database_pool().await.unwrap();
        let executor = BaseExecutor::new(ExecutorType::StepResultProcessor, pool);

        // Send heartbeat
        assert!(executor.heartbeat().await.is_ok());

        // Check metrics
        let metrics = executor.metrics().await;
        assert_eq!(metrics.executor_id, executor.id());
        assert_eq!(metrics.executor_type, "StepResultProcessor");
    }

    #[tokio::test]
    async fn test_double_start_error() {
        let pool = create_test_database_pool().await.unwrap();
        let executor = Arc::new(BaseExecutor::new(ExecutorType::TaskRequestProcessor, pool));

        // First start should succeed
        assert!(executor.clone().start().await.is_ok());

        // Second start should fail
        assert!(executor.clone().start().await.is_err());

        // Cleanup - don't require immediate shutdown, processing loop will eventually stop
        let _ = executor.stop(Duration::from_secs(1)).await;
    }
}
