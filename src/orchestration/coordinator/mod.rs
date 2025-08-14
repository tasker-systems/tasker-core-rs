//! # Orchestration Coordinator Module
//!
//! This module implements the OrchestrationLoopCoordinator that manages executor pools
//! dynamically, providing auto-scaling, health monitoring, and configuration management.
//!
//! ## Key Components
//!
//! - [`OrchestrationLoopCoordinator`] - Main coordinator managing executor pools
//! - [`scaling`] - Auto-scaling algorithms and policies
//! - [`monitor`] - Health monitoring and metrics aggregation
//! - [`pool`] - Executor pool management and lifecycle

pub mod monitor;
pub mod pool;
pub mod scaling;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, RwLock};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::config::ConfigManager;
use crate::error::{Result, TaskerError};
use crate::orchestration::{executor::traits::ExecutorType, OrchestrationCore};

use self::monitor::HealthMonitor;
use self::pool::PoolManager;
use self::scaling::{ScalingAction, ScalingEngine};

/// Main coordinator for orchestration executor pools
///
/// The OrchestrationLoopCoordinator manages multiple executor pools, each handling
/// a specific type of orchestration work. It provides auto-scaling, health monitoring,
/// and dynamic load balancing capabilities.
#[derive(Debug)]
pub struct OrchestrationLoopCoordinator {
    /// Unique identifier for this coordinator instance
    id: Uuid,
    /// Configuration manager for accessing YAML settings
    config_manager: Arc<ConfigManager>,
    /// Orchestration core for executor access to orchestration components
    orchestration_core: Arc<OrchestrationCore>,
    /// Pool manager for executor lifecycle management
    pool_manager: Arc<RwLock<PoolManager>>,
    /// Health monitor for system health tracking
    health_monitor: Arc<HealthMonitor>,
    /// Scaling engine for auto-scaling decisions
    scaling_engine: Arc<ScalingEngine>,
    /// Shutdown notification
    shutdown_notify: Arc<Notify>,
    /// Whether the coordinator is running
    running: Arc<RwLock<bool>>,
    /// Last health check time
    last_health_check: Arc<RwLock<Instant>>,
    /// Last scaling action time
    last_scaling_action: Arc<RwLock<Instant>>,
}

impl OrchestrationLoopCoordinator {
    /// Create a new OrchestrationLoopCoordinator
    pub async fn new(
        config_manager: Arc<ConfigManager>,
        orchestration_core: Arc<OrchestrationCore>,
    ) -> Result<Self> {
        let id = Uuid::new_v4();
        let config = config_manager.config();
        let coordinator_config = &config.executor_pools().coordinator;

        info!(
            "🏗️ COORDINATOR: Creating OrchestrationLoopCoordinator with auto-scaling {}",
            if coordinator_config.auto_scaling_enabled {
                "ENABLED"
            } else {
                "DISABLED"
            }
        );

        // Create pool manager with all executor types
        let pool_manager = Arc::new(RwLock::new(
            PoolManager::new(config_manager.clone(), orchestration_core.clone()).await?,
        ));

        // Create health monitor
        let health_monitor = Arc::new(HealthMonitor::new(
            id,
            coordinator_config.health_check_interval_seconds,
            coordinator_config.max_db_pool_usage,
        ));

        // Create scaling engine
        let scaling_engine = Arc::new(ScalingEngine::new(
            coordinator_config.auto_scaling_enabled,
            coordinator_config.target_utilization,
            coordinator_config.scaling_interval_seconds,
            coordinator_config.scaling_cooldown_seconds,
        ));

        Ok(Self {
            id,
            config_manager,
            orchestration_core,
            pool_manager,
            health_monitor,
            scaling_engine,
            shutdown_notify: Arc::new(Notify::new()),
            running: Arc::new(RwLock::new(false)),
            last_health_check: Arc::new(RwLock::new(Instant::now())),
            last_scaling_action: Arc::new(RwLock::new(Instant::now())),
        })
    }

    /// Start the coordinator and all its monitoring loops
    #[instrument(skip(self), fields(coordinator_id = %self.id))]
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(TaskerError::InvalidState(
                "Coordinator is already running".to_string(),
            ));
        }

        info!("🚀 COORDINATOR: Starting OrchestrationLoopCoordinator");

        // Start all executor pools
        {
            let pool_manager = self.pool_manager.read().await;
            pool_manager.start_all_pools().await?;
        }

        info!("✅ COORDINATOR: All executor pools started");

        // Mark as running
        *running = true;
        drop(running);

        // Start background monitoring loops
        self.start_monitoring_loops().await?;

        info!("🎉 COORDINATOR: OrchestrationLoopCoordinator started successfully");
        Ok(())
    }

    /// Stop the coordinator gracefully
    #[instrument(skip(self), fields(coordinator_id = %self.id))]
    pub async fn stop(&self, timeout: Duration) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            info!("COORDINATOR: Already stopped");
            return Ok(());
        }

        info!("🛑 COORDINATOR: Stopping OrchestrationLoopCoordinator");

        // Signal shutdown to monitoring loops
        self.shutdown_notify.notify_waiters();

        // Stop all executor pools
        {
            let pool_manager = self.pool_manager.read().await;
            pool_manager.stop_all_pools(timeout).await?;
        }

        // Mark as stopped
        *running = false;

        info!("✅ COORDINATOR: OrchestrationLoopCoordinator stopped successfully");
        Ok(())
    }

    /// Check if coordinator is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get coordinator status
    pub async fn status(&self) -> CoordinatorStatus {
        let running = self.is_running().await;
        let pool_manager = self.pool_manager.read().await;
        let pool_statuses = pool_manager.get_all_pool_statuses().await;

        CoordinatorStatus {
            coordinator_id: self.id,
            running,
            total_pools: pool_statuses.len(),
            total_executors: pool_statuses.values().map(|s| s.active_executors).sum(),
            last_health_check: *self.last_health_check.read().await,
            last_scaling_action: *self.last_scaling_action.read().await,
            pool_statuses,
        }
    }

    /// Start background monitoring loops
    async fn start_monitoring_loops(&self) -> Result<()> {
        info!("🔄 COORDINATOR: Starting background monitoring loops");

        // Start health monitoring loop
        let health_monitor = self.health_monitor.clone();
        let pool_manager = self.pool_manager.clone();
        let last_health_check = self.last_health_check.clone();
        let shutdown_notify = self.shutdown_notify.clone();
        let health_interval = Duration::from_secs(
            self.config_manager
                .config()
                .executor_pools()
                .coordinator
                .health_check_interval_seconds,
        );

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(health_interval) => {
                        if let Err(e) = Self::health_check_cycle(
                            &health_monitor,
                            &pool_manager,
                            &last_health_check,
                        ).await {
                            error!("Health check cycle failed: {}", e);
                        }
                    }
                    _ = shutdown_notify.notified() => {
                        info!("Health monitoring loop shutting down");
                        break;
                    }
                }
            }
        });

        // Start scaling monitoring loop
        let scaling_engine = self.scaling_engine.clone();
        let pool_manager_clone = self.pool_manager.clone();
        let last_scaling_action = self.last_scaling_action.clone();
        let shutdown_notify_clone = self.shutdown_notify.clone();
        let scaling_interval = Duration::from_secs(
            self.config_manager
                .config()
                .executor_pools()
                .coordinator
                .scaling_interval_seconds,
        );

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(scaling_interval) => {
                        if let Err(e) = Self::scaling_cycle(
                            &scaling_engine,
                            &pool_manager_clone,
                            &last_scaling_action,
                        ).await {
                            error!("Scaling cycle failed: {}", e);
                        }
                    }
                    _ = shutdown_notify_clone.notified() => {
                        info!("Scaling monitoring loop shutting down");
                        break;
                    }
                }
            }
        });

        info!("✅ COORDINATOR: Background monitoring loops started");
        Ok(())
    }

    /// Perform a health check cycle
    async fn health_check_cycle(
        health_monitor: &HealthMonitor,
        pool_manager: &Arc<RwLock<PoolManager>>,
        last_health_check: &Arc<RwLock<Instant>>,
    ) -> Result<()> {
        let pool_manager = pool_manager.read().await;
        let health_report = pool_manager.get_health_report().await?;

        health_monitor.record_health_report(health_report).await?;

        // Update last health check time
        *last_health_check.write().await = Instant::now();

        Ok(())
    }

    /// Perform a scaling cycle
    async fn scaling_cycle(
        scaling_engine: &ScalingEngine,
        pool_manager: &Arc<RwLock<PoolManager>>,
        last_scaling_action: &Arc<RwLock<Instant>>,
    ) -> Result<()> {
        let pool_metrics = {
            let pool_manager_read = pool_manager.read().await;
            pool_manager_read.get_pool_metrics().await?
        };

        for (executor_type, metrics) in pool_metrics {
            let action = scaling_engine.evaluate_scaling_action(&metrics).await?;

            match action {
                ScalingAction::ScaleUp { count } => {
                    info!(
                        "🔼 COORDINATOR: Scaling up {} by {} executors",
                        executor_type.name(),
                        count
                    );
                    let mut pool_manager_write = pool_manager.write().await;
                    if let Err(e) = pool_manager_write.scale_up_pool(executor_type, count).await {
                        warn!(
                            "❌ COORDINATOR: Failed to scale up {} pool: {}",
                            executor_type.name(),
                            e
                        );
                    } else {
                        info!(
                            "✅ COORDINATOR: Successfully scaled up {} pool",
                            executor_type.name()
                        );
                        // Update last scaling action time
                        *last_scaling_action.write().await = Instant::now();
                    }
                }
                ScalingAction::ScaleDown { count } => {
                    info!(
                        "🔽 COORDINATOR: Scaling down {} by {} executors",
                        executor_type.name(),
                        count
                    );
                    let mut pool_manager_write = pool_manager.write().await;
                    if let Err(e) = pool_manager_write
                        .scale_down_pool(executor_type, count)
                        .await
                    {
                        warn!(
                            "❌ COORDINATOR: Failed to scale down {} pool: {}",
                            executor_type.name(),
                            e
                        );
                    } else {
                        info!(
                            "✅ COORDINATOR: Successfully scaled down {} pool",
                            executor_type.name()
                        );
                        // Update last scaling action time
                        *last_scaling_action.write().await = Instant::now();
                    }
                }
                ScalingAction::NoChange => {
                    // No action needed
                }
            }
        }

        // Update last scaling action time
        *last_scaling_action.write().await = Instant::now();

        Ok(())
    }

    /// Get access to the orchestration core for direct component access
    pub fn orchestration_core(&self) -> &Arc<OrchestrationCore> {
        &self.orchestration_core
    }

    /// Get orchestration system health by querying core components
    pub async fn get_orchestration_health(&self) -> Result<OrchestrationSystemHealth> {
        info!("🏥 COORDINATOR: Checking orchestration system health via core components");

        // Use the orchestration_core to get detailed component health
        let database_healthy = self
            .orchestration_core
            .database_pool()
            .acquire()
            .await
            .is_ok();

        let task_request_processor_healthy = match self
            .orchestration_core
            .task_request_processor
            .process_batch()
            .await
        {
            Ok(_) => true,
            Err(e) => {
                debug!("TaskRequestProcessor health check failed: {}", e);
                false
            }
        };

        let pool_health = {
            let pool_manager = self.pool_manager.read().await;
            pool_manager.get_health_report().await?
        };

        Ok(OrchestrationSystemHealth {
            coordinator_id: self.id,
            database_healthy,
            task_request_processor_healthy,
            total_pools: pool_health.total_pools,
            total_executors: pool_health.total_executors,
            healthy_executors: pool_health.healthy_executors,
            unhealthy_executors: pool_health.unhealthy_executors,
        })
    }

    /// Trigger immediate task processing via orchestration core
    pub async fn trigger_immediate_processing(&self) -> Result<ProcessingTriggerResult> {
        info!("⚡ COORDINATOR: Triggering immediate processing via orchestration core");

        let start_time = Instant::now();

        // Use orchestration_core to trigger immediate processing
        let task_processing_result = self
            .orchestration_core
            .task_request_processor
            .process_batch()
            .await?;

        let orchestration_result = self
            .orchestration_core
            .orchestration_loop
            .run_cycle()
            .await?;

        let step_result_processing = self
            .orchestration_core
            .step_result_processor
            .process_step_result_batch()
            .await?;

        let processing_time = start_time.elapsed();

        info!(
            "✅ COORDINATOR: Immediate processing completed in {:?} - Tasks: {}, Steps: {}, Results: {}",
            processing_time,
            task_processing_result,
            orchestration_result.tasks_processed,
            step_result_processing
        );

        Ok(ProcessingTriggerResult {
            processing_duration: processing_time,
            tasks_processed: task_processing_result,
            orchestration_cycles: 1,
            steps_enqueued: orchestration_result.total_steps_enqueued,
            step_results_processed: step_result_processing,
        })
    }
}

/// Status information for the coordinator
#[derive(Debug, Clone)]
pub struct CoordinatorStatus {
    pub coordinator_id: Uuid,
    pub running: bool,
    pub total_pools: usize,
    pub total_executors: usize,
    pub last_health_check: Instant,
    pub last_scaling_action: Instant,
    pub pool_statuses: HashMap<ExecutorType, pool::PoolStatus>,
}

/// Orchestration system health information from core components
#[derive(Debug, Clone)]
pub struct OrchestrationSystemHealth {
    pub coordinator_id: Uuid,
    pub database_healthy: bool,
    pub task_request_processor_healthy: bool,
    pub total_pools: usize,
    pub total_executors: usize,
    pub healthy_executors: usize,
    pub unhealthy_executors: usize,
}

/// Result of triggering immediate processing
#[derive(Debug, Clone)]
pub struct ProcessingTriggerResult {
    pub processing_duration: Duration,
    pub tasks_processed: usize,
    pub orchestration_cycles: usize,
    pub steps_enqueued: usize,
    pub step_results_processed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::OrchestrationCore;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_coordinator_creation() {
        // Setup test environment (respects existing DATABASE_URL in CI)
        crate::test_utils::setup_test_environment();

        let config_manager = ConfigManager::load_from_directory_with_env(None, "test")
            .expect("Failed to load test configuration");
        let orchestration_core = Arc::new(OrchestrationCore::new().await.unwrap());

        let coordinator =
            OrchestrationLoopCoordinator::new(config_manager, orchestration_core).await;
        assert!(coordinator.is_ok());

        let coordinator = coordinator.unwrap();
        assert!(!coordinator.is_running().await);
    }

    #[tokio::test]
    async fn test_coordinator_lifecycle() {
        // Setup test environment (respects existing DATABASE_URL in CI)
        crate::test_utils::setup_test_environment();

        let config_manager = ConfigManager::load_from_directory_with_env(None, "test")
            .expect("Failed to load test configuration");
        let orchestration_core = Arc::new(OrchestrationCore::new().await.unwrap());

        let coordinator = OrchestrationLoopCoordinator::new(config_manager, orchestration_core)
            .await
            .unwrap();

        // Start coordinator
        assert!(coordinator.start().await.is_ok());
        assert!(coordinator.is_running().await);

        // Stop coordinator
        assert!(coordinator.stop(Duration::from_secs(5)).await.is_ok());
        assert!(!coordinator.is_running().await);
    }
}
