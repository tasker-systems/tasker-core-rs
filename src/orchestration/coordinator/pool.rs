//! # Executor Pool Management
//!
//! This module manages pools of executors for each executor type, handling lifecycle
//! management, scaling operations, and health monitoring.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, instrument, warn};

use crate::config::ConfigManager;
use crate::error::Result;
use crate::orchestration::executor::base::BaseExecutor;
use crate::orchestration::executor::traits::{ExecutorType, OrchestrationExecutor};
use crate::orchestration::OrchestrationCore;

/// A pool of executors for a specific executor type
pub struct ExecutorPool {
    /// Type of executors in this pool
    executor_type: ExecutorType,
    /// Active executors in the pool
    executors: Vec<Arc<BaseExecutor>>,
    /// Configuration for this pool
    config: Arc<ConfigManager>,
    /// Orchestration core for creating executors with access to orchestration components
    orchestration_core: Arc<OrchestrationCore>,
    /// Minimum number of executors
    min_executors: usize,
    /// Maximum number of executors
    max_executors: usize,
    /// Pool creation time
    created_at: Instant,
}

impl ExecutorPool {
    /// Create a new executor pool
    pub async fn new(
        executor_type: ExecutorType,
        config: Arc<ConfigManager>,
        orchestration_core: Arc<OrchestrationCore>,
    ) -> Result<Self> {
        let pool_config = config.config().get_executor_instance_config(executor_type);

        info!(
            "🏊 POOL: Creating executor pool for {} (min: {}, max: {})",
            executor_type.name(),
            pool_config.min_executors,
            pool_config.max_executors
        );

        let mut pool = Self {
            executor_type,
            executors: Vec::new(),
            config: config.clone(),
            orchestration_core,
            min_executors: pool_config.min_executors,
            max_executors: pool_config.max_executors,
            created_at: Instant::now(),
        };

        // Initialize with minimum number of executors
        pool.scale_to_count(pool_config.min_executors).await?;

        info!(
            "✅ POOL: Executor pool for {} created with {} executors",
            executor_type.name(),
            pool.executors.len()
        );

        Ok(pool)
    }

    /// Start all executors in the pool
    #[instrument(skip(self), fields(executor_type = %self.executor_type.name()))]
    pub async fn start_all(&self) -> Result<()> {
        info!(
            "🚀 POOL: Starting all {} executors",
            self.executor_type.name()
        );

        for (index, executor) in self.executors.iter().enumerate() {
            match executor.clone().start().await {
                Ok(()) => {
                    info!(
                        "✅ POOL: Started {} executor #{}",
                        self.executor_type.name(),
                        index + 1
                    );
                }
                Err(e) => {
                    error!(
                        "❌ POOL: Failed to start {} executor #{}: {}",
                        self.executor_type.name(),
                        index + 1,
                        e
                    );
                    return Err(e);
                }
            }

            // Small delay to prevent connection race conditions
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!(
            "🎉 POOL: All {} executors started successfully",
            self.executor_type.name()
        );
        Ok(())
    }

    /// Stop all executors in the pool
    #[instrument(skip(self), fields(executor_type = %self.executor_type.name()))]
    pub async fn stop_all(&self, timeout: Duration) -> Result<()> {
        info!(
            "🛑 POOL: Stopping all {} executors",
            self.executor_type.name()
        );

        let individual_timeout = timeout / (self.executors.len() as u32).max(1);

        for (index, executor) in self.executors.iter().enumerate() {
            match executor.stop(individual_timeout).await {
                Ok(()) => {
                    info!(
                        "✅ POOL: Stopped {} executor #{}",
                        self.executor_type.name(),
                        index + 1
                    );
                }
                Err(e) => {
                    warn!(
                        "⚠️ POOL: Failed to stop {} executor #{}: {}",
                        self.executor_type.name(),
                        index + 1,
                        e
                    );
                    // Continue stopping other executors even if one fails
                }
            }
        }

        info!(
            "🛑 POOL: All {} executors stopped",
            self.executor_type.name()
        );
        Ok(())
    }

    /// Scale the pool to a specific number of executors
    pub async fn scale_to_count(&mut self, target_count: usize) -> Result<()> {
        let current_count = self.executors.len();
        let clamped_target = target_count.clamp(self.min_executors, self.max_executors);

        if clamped_target != target_count {
            warn!(
                "POOL: Target count {} for {} clamped to {} (min: {}, max: {})",
                target_count,
                self.executor_type.name(),
                clamped_target,
                self.min_executors,
                self.max_executors
            );
        }

        if clamped_target == current_count {
            return Ok(());
        }

        if clamped_target > current_count {
            // Scale up
            let add_count = clamped_target - current_count;
            self.add_executors(add_count).await?;
        } else {
            // Scale down
            let remove_count = current_count - clamped_target;
            self.remove_executors(remove_count).await?;
        }

        Ok(())
    }

    /// Add executors to the pool
    async fn add_executors(&mut self, count: usize) -> Result<()> {
        info!(
            "🔼 POOL: Adding {} executors to {} pool",
            count,
            self.executor_type.name()
        );

        for _ in 0..count {
            // Create executor directly since we've simplified the bootstrap system
            let executor_config = self.config.config().get_executor_config(self.executor_type);
            let executor =
                crate::orchestration::executor::base::BaseExecutor::with_orchestration_core(
                    self.executor_type,
                    self.orchestration_core.clone(),
                    executor_config,
                )?;

            let executor: Arc<BaseExecutor> = Arc::new(executor);

            // Start the executor if we're in an active pool
            // (Check if any existing executors are running to determine pool state)
            let should_start = if let Some(first_executor) = self.executors.first() {
                first_executor.should_continue()
            } else {
                false // New pool, will be started explicitly
            };

            if should_start {
                executor.clone().start().await?;
                info!(
                    "✅ POOL: Started new {} executor #{}",
                    self.executor_type.name(),
                    self.executors.len() + 1
                );
            }

            self.executors.push(executor);
        }

        info!(
            "🔼 POOL: Added {} executors to {} pool (total: {})",
            count,
            self.executor_type.name(),
            self.executors.len()
        );

        Ok(())
    }

    /// Remove executors from the pool using two-phase removal process
    async fn remove_executors(&mut self, count: usize) -> Result<()> {
        info!(
            "🔽 POOL: Removing {} executors from {} pool using two-phase removal",
            count,
            self.executor_type.name()
        );

        let remove_count = count.min(self.executors.len());
        let timeout = Duration::from_secs(30);

        // Phase 1: Remove executors from active pool first
        let mut pending_removal = Vec::new();
        for _ in 0..remove_count {
            if let Some(executor) = self.executors.pop() {
                pending_removal.push(executor);
            }
        }

        // Phase 2: Attempt graceful shutdown with timeout tracking
        let mut failed_stops = Vec::new();
        let mut successfully_stopped = 0;

        for executor in pending_removal {
            // Use tokio::timeout to enforce per-executor timeout
            match tokio::time::timeout(timeout, executor.stop(timeout)).await {
                Ok(Ok(())) => {
                    // Successfully stopped
                    successfully_stopped += 1;
                    info!("✅ POOL: Successfully stopped executor during pool scaling");
                    // Executor is safely dropped here
                }
                Ok(Err(e)) => {
                    warn!("⚠️ POOL: Executor stop failed: {}", e);
                    failed_stops.push(executor);
                }
                Err(_) => {
                    warn!("⚠️ POOL: Executor stop timed out after {:?}", timeout);
                    failed_stops.push(executor);
                }
            }
        }

        // Phase 3: Handle failed stops with recovery strategy
        let failed_count = failed_stops.len();
        if !failed_stops.is_empty() {
            warn!(
                "⚠️ POOL: {} out of {} executors failed to stop gracefully during removal",
                failed_count, remove_count
            );

            // Recovery strategy: Try to return failed executors to pool for retry
            // This prevents dropping executors with running background tasks
            if self.handle_failed_stops(failed_stops).await.is_err() {
                warn!("⚠️ POOL: Failed stop recovery strategy unsuccessful - some executors may have leaked resources");
            }
        }

        info!(
            "🔽 POOL: Removed {} executors from {} pool (successfully stopped: {}, total remaining: {})",
            remove_count - failed_count,
            self.executor_type.name(),
            successfully_stopped,
            self.executors.len()
        );

        Ok(())
    }

    /// Handle executors that failed to stop gracefully
    ///
    /// Implements recovery strategies for executors that couldn't be stopped cleanly.
    /// This prevents resource leaks from dropping executors with active background tasks.
    async fn handle_failed_stops(
        &mut self,
        failed_executors: Vec<Arc<BaseExecutor>>,
    ) -> Result<()> {
        warn!(
            "🔧 POOL: Attempting recovery for {} executors that failed to stop",
            failed_executors.len()
        );

        for executor in failed_executors {
            // Strategy 1: Check if executor is actually still running
            if !executor.should_continue() {
                // Executor claims it's not running, safe to drop
                info!("✅ POOL: Executor reports as stopped, safe to drop");
                continue;
            }

            // Strategy 2: Try one more quick stop attempt
            match tokio::time::timeout(
                Duration::from_secs(5),
                executor.stop(Duration::from_secs(5)),
            )
            .await
            {
                Ok(Ok(())) => {
                    info!("✅ POOL: Executor stopped on retry attempt");
                    continue;
                }
                _ => {
                    warn!("⚠️ POOL: Executor still failed to stop on retry");
                }
            }

            // Strategy 3: Return to pool with warning for manual intervention
            // This prevents dropping an executor with potentially running background tasks
            warn!(
                "🚨 POOL: Returning problematic executor to pool - requires manual intervention. \
                Executor ID: {}, Type: {}. Monitor for resource leaks.",
                executor.id(),
                self.executor_type.name()
            );

            self.executors.push(executor);
        }

        Ok(())
    }

    /// Get pool status
    pub async fn get_status(&self) -> PoolStatus {
        let mut healthy_count = 0;
        let mut unhealthy_count = 0;
        let mut total_items_processed = 0;
        let mut total_items_failed = 0;
        let mut average_utilization = 0.0;

        for executor in &self.executors {
            let health = executor.health().await;
            let metrics = executor.metrics().await;

            // Use is_operational() to properly classify health states
            // Both Healthy and Degraded states are considered operational
            if health.is_operational() {
                healthy_count += 1
            } else {
                unhealthy_count += 1
            }

            total_items_processed += metrics.total_items_processed;
            total_items_failed += metrics.total_items_failed;
            average_utilization += metrics.utilization;
        }

        if !self.executors.is_empty() {
            average_utilization /= self.executors.len() as f64;
        }

        PoolStatus {
            executor_type: self.executor_type,
            active_executors: self.executors.len(),
            healthy_executors: healthy_count,
            unhealthy_executors: unhealthy_count,
            min_executors: self.min_executors,
            max_executors: self.max_executors,
            total_items_processed,
            total_items_failed,
            average_utilization,
            uptime_seconds: self.created_at.elapsed().as_secs(),
        }
    }

    /// Get pool metrics for scaling decisions
    pub async fn get_metrics(&self) -> PoolMetrics {
        let mut total_throughput = 0.0;
        let mut total_utilization = 0.0;
        let mut max_queue_depth = 0;
        let mut total_error_rate = 0.0;

        for executor in &self.executors {
            let metrics = executor.metrics().await;
            total_throughput += metrics.items_per_second;
            total_utilization += metrics.utilization;
            max_queue_depth = max_queue_depth.max(metrics.queue_depth);
            total_error_rate += metrics.error_rate;
        }

        let executor_count = self.executors.len();
        let average_utilization = if executor_count > 0 {
            total_utilization / executor_count as f64
        } else {
            0.0
        };

        let average_error_rate = if executor_count > 0 {
            total_error_rate / executor_count as f64
        } else {
            0.0
        };

        PoolMetrics {
            executor_type: self.executor_type,
            current_executors: executor_count,
            min_executors: self.min_executors,
            max_executors: self.max_executors,
            total_throughput,
            average_utilization,
            max_queue_depth: max_queue_depth as usize,
            average_error_rate,
        }
    }
}

/// Manager for all executor pools
pub struct PoolManager {
    /// Map of executor type to pool
    pools: HashMap<ExecutorType, ExecutorPool>,
    /// Configuration manager
    config: Arc<ConfigManager>,
    /// Orchestration core for creating executors
    orchestration_core: Arc<OrchestrationCore>,
}

impl std::fmt::Debug for PoolManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolManager")
            .field("pools", &format!("{} pools", self.pools.len()))
            .field("config", &"ConfigManager")
            .field("orchestration_core", &"OrchestrationCore")
            .finish()
    }
}

impl PoolManager {
    /// Create a new pool manager
    pub async fn new(
        config: Arc<ConfigManager>,
        orchestration_core: Arc<OrchestrationCore>,
    ) -> Result<Self> {
        info!("🏗️ POOL_MANAGER: Creating pools for all executor types");

        let mut pools = HashMap::new();

        // Create a pool for each executor type
        for executor_type in ExecutorType::all() {
            let pool = ExecutorPool::new(executor_type, config.clone(), orchestration_core.clone())
                .await?;

            pools.insert(executor_type, pool);
        }

        info!(
            "✅ POOL_MANAGER: Created {} executor pools successfully",
            pools.len()
        );

        Ok(Self {
            pools,
            config,
            orchestration_core,
        })
    }

    /// Start all pools
    pub async fn start_all_pools(&self) -> Result<()> {
        info!("🚀 POOL_MANAGER: Starting all executor pools");

        for (executor_type, pool) in &self.pools {
            info!("🔄 POOL_MANAGER: Starting {} pool", executor_type.name());
            pool.start_all().await?;
        }

        info!("🎉 POOL_MANAGER: All executor pools started successfully");
        Ok(())
    }

    /// Stop all pools
    pub async fn stop_all_pools(&self, timeout: Duration) -> Result<()> {
        info!("🛑 POOL_MANAGER: Stopping all executor pools");

        let per_pool_timeout = timeout / (self.pools.len() as u32).max(1);

        for (executor_type, pool) in &self.pools {
            info!("🔄 POOL_MANAGER: Stopping {} pool", executor_type.name());
            if let Err(e) = pool.stop_all(per_pool_timeout).await {
                warn!(
                    "⚠️ POOL_MANAGER: Failed to stop {} pool: {}",
                    executor_type.name(),
                    e
                );
                // Continue stopping other pools
            }
        }

        info!("🛑 POOL_MANAGER: All executor pools stopped");
        Ok(())
    }

    /// Get status for all pools
    pub async fn get_all_pool_statuses(&self) -> HashMap<ExecutorType, PoolStatus> {
        let mut statuses = HashMap::new();

        for (executor_type, pool) in &self.pools {
            let status = pool.get_status().await;
            statuses.insert(*executor_type, status);
        }

        statuses
    }

    /// Get metrics for all pools
    pub async fn get_pool_metrics(&self) -> Result<HashMap<ExecutorType, PoolMetrics>> {
        let mut metrics = HashMap::new();

        for (executor_type, pool) in &self.pools {
            let pool_metrics = pool.get_metrics().await;
            metrics.insert(*executor_type, pool_metrics);
        }

        Ok(metrics)
    }

    /// Get health report for all pools
    pub async fn get_health_report(&self) -> Result<HealthReport> {
        let pool_statuses = self.get_all_pool_statuses().await;

        let total_executors: usize = pool_statuses.values().map(|s| s.active_executors).sum();
        let healthy_executors: usize = pool_statuses.values().map(|s| s.healthy_executors).sum();
        let unhealthy_executors: usize =
            pool_statuses.values().map(|s| s.unhealthy_executors).sum();

        Ok(HealthReport {
            total_pools: pool_statuses.len(),
            total_executors,
            healthy_executors,
            unhealthy_executors,
            pool_statuses,
        })
    }

    /// Scale a specific executor pool up by count
    pub async fn scale_up_pool(&mut self, executor_type: ExecutorType, count: usize) -> Result<()> {
        let pool = self.pools.get_mut(&executor_type).ok_or_else(|| {
            crate::error::TaskerError::OrchestrationError(format!(
                "No pool found for executor type: {}",
                executor_type.name()
            ))
        })?;

        let current_count = pool.executors.len();
        let target_count = (current_count + count).min(pool.max_executors);

        if target_count > current_count {
            info!(
                "🔼 POOL_MANAGER: Scaling up {} from {} to {} executors",
                executor_type.name(),
                current_count,
                target_count
            );
            pool.scale_to_count(target_count).await?;
        } else {
            warn!(
                "⚠️ POOL_MANAGER: Cannot scale up {} - already at max capacity ({}/{})",
                executor_type.name(),
                current_count,
                pool.max_executors
            );
        }

        Ok(())
    }

    /// Scale a specific executor pool down by count
    pub async fn scale_down_pool(
        &mut self,
        executor_type: ExecutorType,
        count: usize,
    ) -> Result<()> {
        let pool = self.pools.get_mut(&executor_type).ok_or_else(|| {
            crate::error::TaskerError::OrchestrationError(format!(
                "No pool found for executor type: {}",
                executor_type.name()
            ))
        })?;

        let current_count = pool.executors.len();
        let target_count = (current_count.saturating_sub(count)).max(pool.min_executors);

        if target_count < current_count {
            info!(
                "🔽 POOL_MANAGER: Scaling down {} from {} to {} executors",
                executor_type.name(),
                current_count,
                target_count
            );
            pool.scale_to_count(target_count).await?;
        } else {
            warn!(
                "⚠️ POOL_MANAGER: Cannot scale down {} - already at min capacity ({}/{})",
                executor_type.name(),
                current_count,
                pool.min_executors
            );
        }

        Ok(())
    }

    /// Get the current configuration manager (for dynamic reconfiguration)
    pub fn config_manager(&self) -> &Arc<ConfigManager> {
        &self.config
    }

    /// Get the orchestration core (for accessing core orchestration components)
    pub fn orchestration_core(&self) -> &Arc<OrchestrationCore> {
        &self.orchestration_core
    }

    /// Reconfigure all pools based on updated configuration
    pub async fn reconfigure_pools(&mut self) -> Result<()> {
        info!("🔧 POOL_MANAGER: Reconfiguring pools based on updated configuration");

        for (executor_type, pool) in &mut self.pools {
            let new_config = self
                .config
                .config()
                .get_executor_instance_config(*executor_type);

            info!(
                "🔧 POOL_MANAGER: Updating {} pool configuration (min: {}, max: {})",
                executor_type.name(),
                new_config.min_executors,
                new_config.max_executors
            );

            // Update pool configuration
            pool.min_executors = new_config.min_executors;
            pool.max_executors = new_config.max_executors;

            // Adjust pool size if needed
            let current_count = pool.executors.len();
            if current_count < new_config.min_executors {
                // Scale up to meet minimum
                pool.scale_to_count(new_config.min_executors).await?;
            } else if current_count > new_config.max_executors {
                // Scale down to maximum
                pool.scale_to_count(new_config.max_executors).await?;
            }
        }

        info!("✅ POOL_MANAGER: Pool reconfiguration completed");
        Ok(())
    }

    /// Add a new executor to a specific pool using orchestration core
    pub async fn add_executor_to_pool(&mut self, executor_type: ExecutorType) -> Result<()> {
        let pool = self.pools.get_mut(&executor_type).ok_or_else(|| {
            crate::error::TaskerError::OrchestrationError(format!(
                "No pool found for executor type: {}",
                executor_type.name()
            ))
        })?;

        if pool.executors.len() >= pool.max_executors {
            return Err(crate::error::TaskerError::InvalidState(format!(
                "Cannot add executor to {} pool - already at maximum capacity ({}/{})",
                executor_type.name(),
                pool.executors.len(),
                pool.max_executors
            )));
        }

        info!(
            "➕ POOL_MANAGER: Adding new executor to {} pool using orchestration core",
            executor_type.name()
        );

        // Use the orchestration_core to get proper configuration
        let executor_config = self.config.config().get_executor_config(executor_type);
        let executor = crate::orchestration::executor::base::BaseExecutor::with_orchestration_core(
            executor_type,
            self.orchestration_core.clone(),
            executor_config,
        )?;

        let executor: Arc<BaseExecutor> = Arc::new(executor);

        // Start the executor if the pool is currently running
        if let Some(first_executor) = pool.executors.first() {
            if first_executor.should_continue() {
                executor.clone().start().await?;
            }
        }

        pool.executors.push(executor);

        info!(
            "✅ POOL_MANAGER: Successfully added executor to {} pool ({}/{})",
            executor_type.name(),
            pool.executors.len(),
            pool.max_executors
        );

        Ok(())
    }
}

/// Status of a single executor pool
#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub executor_type: ExecutorType,
    pub active_executors: usize,
    pub healthy_executors: usize,
    pub unhealthy_executors: usize,
    pub min_executors: usize,
    pub max_executors: usize,
    pub total_items_processed: u64,
    pub total_items_failed: u64,
    pub average_utilization: f64,
    pub uptime_seconds: u64,
}

/// Metrics for scaling decisions
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    pub executor_type: ExecutorType,
    pub current_executors: usize,
    pub min_executors: usize,
    pub max_executors: usize,
    pub total_throughput: f64,
    pub average_utilization: f64,
    pub max_queue_depth: usize,
    pub average_error_rate: f64,
}

/// Health report for all pools
#[derive(Debug, Clone)]
pub struct HealthReport {
    pub total_pools: usize,
    pub total_executors: usize,
    pub healthy_executors: usize,
    pub unhealthy_executors: usize,
    pub pool_statuses: HashMap<ExecutorType, PoolStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Pool integration tests removed as they required full database connectivity
    // and orchestration core setup, testing I/O rather than business logic.
    //
    // These tests were creating database connections and full orchestration systems just to test
    // basic pool creation, which duplicates testing of lower-level components.
    //
    // The pool manager's business logic (executor lifecycle, metrics, scaling) is better tested
    // through focused unit tests of individual methods and integration tests that run against
    // a real environment when needed.

    #[test]
    fn test_pool_status_creation() {
        let status = PoolStatus {
            executor_type: ExecutorType::TaskRequestProcessor,
            active_executors: 5,
            healthy_executors: 4,
            unhealthy_executors: 1,
            min_executors: 2,
            max_executors: 10,
            total_items_processed: 1000,
            total_items_failed: 25,
            average_utilization: 0.75,
            uptime_seconds: 3600,
        };

        assert_eq!(status.active_executors, 5);
        assert_eq!(status.healthy_executors, 4);
        assert_eq!(status.unhealthy_executors, 1);
        assert_eq!(status.total_items_processed, 1000);
        assert_eq!(status.total_items_failed, 25);
    }

    #[test]
    fn test_pool_metrics_creation() {
        let metrics = PoolMetrics {
            executor_type: ExecutorType::StepResultProcessor,
            current_executors: 8,
            min_executors: 2,
            max_executors: 15,
            total_throughput: 125.5,
            average_utilization: 0.82,
            max_queue_depth: 250,
            average_error_rate: 0.03,
        };

        assert_eq!(metrics.current_executors, 8);
        assert_eq!(metrics.min_executors, 2);
        assert_eq!(metrics.max_executors, 15);
        assert!((metrics.total_throughput - 125.5).abs() < f64::EPSILON);
        assert!((metrics.average_utilization - 0.82).abs() < f64::EPSILON);
    }

    #[test]
    fn test_health_report_creation() {
        use std::collections::HashMap;

        let health_report = HealthReport {
            total_pools: 6,
            total_executors: 30,
            healthy_executors: 28,
            unhealthy_executors: 2,
            pool_statuses: HashMap::new(),
        };

        assert_eq!(health_report.total_pools, 6);
        assert_eq!(health_report.total_executors, 30);
        assert_eq!(health_report.healthy_executors, 28);
        assert_eq!(health_report.unhealthy_executors, 2);
    }
}
