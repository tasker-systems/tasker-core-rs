use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

use crate::executor::Executor;
use crate::resilience::CircuitState;
use crate::{TaskerError, TaskerResult};

use super::base::{BaseExecutor, ProcessingLoop, ProcessingState};

/// This struct separates the processing loop concerns from the BaseExecutor,
/// providing cleaner architecture and better testability.

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
            executor_processor: executor.executor_processor.name().to_string(),
        });

        Self {
            weak_executor: Arc::downgrade(&executor),
            state,
        }
    }

    /// Start the processing loop
    pub async fn start(&self) -> TaskerResult<()> {
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
    pub async fn stop(&self, timeout: Duration) -> TaskerResult<()> {
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
    async fn process_batch_with_executor(&self, executor: &Arc<BaseExecutor>) -> TaskerResult<()> {
        let loop_start = Instant::now();

        // Send heartbeat to mark executor as healthy
        if let Err(e) = executor.heartbeat().await {
            debug!("Failed to send heartbeat: {}", e);
        }

        // Check circuit breaker state
        let cb_state = executor.circuit_breaker.state();

        // Calculate retry time for circuit breaker (estimated based on typical pattern)
        let retry_time_instant = match cb_state {
            CircuitState::Open => Some(Instant::now() + Duration::from_secs(30)), // Typical open circuit timeout
            CircuitState::HalfOpen => Some(Instant::now() + Duration::from_secs(5)), // Shorter retry for half-open
            CircuitState::Closed => None, // Normal operation
        };

        let state_str = match cb_state {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        };

        executor
            .metrics_collector
            .set_circuit_breaker_state(state_str, retry_time_instant)
            .await?;

        if matches!(cb_state, CircuitState::Open) {
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
    pub async fn run(&self) -> TaskerResult<()> {
        info!(
            executor_id = %self.state.executor_id,
            executor_type = %self.state.executor_processor,
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
            executor_type = %self.state.executor_processor,
            "Processing loop ended"
        );
        Ok(())
    }
}
