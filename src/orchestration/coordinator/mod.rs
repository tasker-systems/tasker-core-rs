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
pub mod operational_state;
pub mod pool;
pub mod resource_limits;
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
use self::operational_state::{OperationalStateManager, SystemOperationalState};
use self::pool::PoolManager;
use self::resource_limits::ResourceValidator;
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
    /// Operational state manager for shutdown-aware monitoring
    operational_state: OperationalStateManager,
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
        Self::with_operational_state(
            config_manager,
            orchestration_core.clone(),
            orchestration_core.operational_state_manager.clone(),
        )
        .await
    }

    /// Create a new OrchestrationLoopCoordinator with a specific operational state manager
    pub async fn with_operational_state(
        config_manager: Arc<ConfigManager>,
        orchestration_core: Arc<OrchestrationCore>,
        operational_state: OperationalStateManager,
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
            operational_state,
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

        // TAS-37 Supplemental: Operational state is already managed by OrchestrationCore
        // No need to transition to startup here - OrchestrationCore handles it

        info!("🚀 COORDINATOR: Starting OrchestrationLoopCoordinator");

        // TAS-37 Supplemental: Enhanced startup coordination with error handling
        let startup_result = self.coordinate_startup().await;

        if let Err(e) = startup_result {
            // If startup fails, transition to stopped state and return error
            if let Err(transition_err) = self
                .operational_state
                .transition_to(SystemOperationalState::Stopped)
                .await
            {
                warn!(
                    "Failed to transition to stopped state after startup failure: {}",
                    transition_err
                );
            }

            error!("❌ COORDINATOR: Startup failed: {}", e);
            return Err(e);
        }

        // Mark as running
        *running = true;
        drop(running);

        // TAS-37 Supplemental: Operational state is already Normal from OrchestrationCore initialization
        // No need to transition again - maintaining shared state with OrchestrationCore

        info!("🎉 COORDINATOR: OrchestrationLoopCoordinator started successfully");
        Ok(())
    }

    /// Coordinate the startup process with operational state awareness (TAS-37 Supplemental)
    async fn coordinate_startup(&self) -> Result<()> {
        let start_time = std::time::Instant::now();

        info!("🔄 COORDINATOR: Beginning startup coordination");

        // PHASE 1: Resource Constraint Validation (TAS-34)
        info!("🔍 COORDINATOR: Performing resource constraint validation before startup");
        let resource_validator = ResourceValidator::new(
            self.orchestration_core.database_pool(),
            self.config_manager.clone(),
        )
        .await?;

        let validation_result = resource_validator.validate_and_log_info().await?;

        info!(
            "✅ COORDINATOR: Resource validation passed - proceeding with startup (Max executors: {}, Available DB connections: {})",
            validation_result.executor_requirements.total_max_executors,
            validation_result.resource_limits.available_database_connections
        );

        // Log recommended database pool size if current configuration is at risk
        let recommended_size = validation_result.recommended_database_pool_size();
        if recommended_size > validation_result.resource_limits.max_database_connections {
            warn!(
                "⚠️ COORDINATOR: For optimal performance under full load, consider increasing database pool size to {}",
                recommended_size
            );
        }

        // PHASE 2: Start all executor pools
        let pool_startup_start = std::time::Instant::now();
        let pool_startup_result = {
            let pool_manager = self.pool_manager.read().await;
            pool_manager.start_all_pools().await
        };

        if let Err(e) = pool_startup_result {
            error!("❌ COORDINATOR: Pool startup failed: {}", e);
            return Err(e);
        }

        let pool_startup_duration = pool_startup_start.elapsed();
        info!(
            "✅ COORDINATOR: All executor pools started in {:?}",
            pool_startup_duration
        );

        // PHASE 3: Start background monitoring loops
        let monitoring_startup_result = self.start_monitoring_loops().await;

        if let Err(e) = monitoring_startup_result {
            error!("❌ COORDINATOR: Monitoring loop startup failed: {}", e);
            return Err(e);
        }

        // PHASE 4: Startup completion summary
        let total_startup_time = start_time.elapsed();
        info!(
            "🎯 COORDINATOR: Startup coordination completed successfully in {:?}",
            total_startup_time
        );

        Ok(())
    }

    /// Stop the coordinator gracefully
    #[instrument(skip(self), fields(coordinator_id = %self.id))]
    pub async fn stop(&self, timeout: Duration) -> Result<()> {
        self.stop_with_state(timeout, SystemOperationalState::GracefulShutdown)
            .await
    }

    /// Stop the coordinator with a specific operational state (TAS-37 Supplemental)
    ///
    /// This method provides fine-grained control over shutdown coordination by allowing
    /// the caller to specify whether this is a graceful shutdown, emergency shutdown, etc.
    #[instrument(skip(self), fields(coordinator_id = %self.id))]
    pub async fn stop_with_state(
        &self,
        timeout: Duration,
        shutdown_state: SystemOperationalState,
    ) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            info!("COORDINATOR: Already stopped");
            return Ok(());
        }

        // TAS-37 Supplemental: Validate that the requested state is a shutdown state
        if !shutdown_state.is_shutdown() {
            return Err(TaskerError::InvalidParameter(format!(
                "stop_with_state requires a shutdown state, got: {shutdown_state}"
            )));
        }

        // TAS-37 Supplemental: Set operational state before stopping
        if let Err(e) = self
            .operational_state
            .transition_to(shutdown_state.clone())
            .await
        {
            warn!("Failed to transition to {} state: {}", shutdown_state, e);
        }

        match shutdown_state {
            SystemOperationalState::GracefulShutdown => {
                info!("🛑 COORDINATOR: Stopping OrchestrationLoopCoordinator gracefully");
            }
            SystemOperationalState::Emergency => {
                warn!("🚨 COORDINATOR: Emergency stop of OrchestrationLoopCoordinator");
            }
            _ => {
                info!(
                    "🛑 COORDINATOR: Stopping OrchestrationLoopCoordinator ({})",
                    shutdown_state
                );
            }
        }

        // Signal shutdown to monitoring loops
        self.shutdown_notify.notify_waiters();

        // TAS-37 Supplemental: Enhanced shutdown coordination with error handling
        let shutdown_result = self.coordinate_shutdown(timeout, &shutdown_state).await;

        // Mark as stopped
        *running = false;

        // TAS-37 Supplemental: Transition to stopped state after shutdown attempt
        if let Err(e) = self
            .operational_state
            .transition_to(SystemOperationalState::Stopped)
            .await
        {
            warn!("Failed to transition to stopped state: {}", e);
        }

        match shutdown_result {
            Ok(_) => {
                info!("✅ COORDINATOR: OrchestrationLoopCoordinator stopped successfully");
                Ok(())
            }
            Err(e) => {
                error!("❌ COORDINATOR: Shutdown completed with errors: {}", e);
                // Return success since we're stopped, but log the issues
                Ok(())
            }
        }
    }

    /// Emergency stop the coordinator immediately (TAS-37 Supplemental)
    ///
    /// This method performs an immediate shutdown without waiting for graceful completion.
    /// Should only be used in emergency situations.
    #[instrument(skip(self), fields(coordinator_id = %self.id))]
    pub async fn emergency_stop(&self) -> Result<()> {
        warn!("🚨 COORDINATOR: Emergency stop requested");

        // Use configured emergency timeout from operational state configuration
        let config = self.config_manager.config();
        let emergency_timeout = config
            .orchestration
            .operational_state
            .emergency_shutdown_timeout();
        self.stop_with_state(emergency_timeout, SystemOperationalState::Emergency)
            .await
    }

    /// Check if coordinator is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get the current operational state (TAS-37 Supplemental)
    pub async fn operational_state(&self) -> SystemOperationalState {
        self.operational_state.current_state().await
    }

    /// Check if the system is currently in shutdown mode (TAS-37 Supplemental)
    pub async fn is_shutdown(&self) -> bool {
        self.operational_state.is_shutdown().await
    }

    /// Check if health monitoring should be active (TAS-37 Supplemental)
    pub async fn should_monitor_health(&self) -> bool {
        self.operational_state.should_monitor_health().await
    }

    /// Check if health alerts should be suppressed (TAS-37 Supplemental)
    pub async fn should_suppress_health_alerts(&self) -> bool {
        self.operational_state.should_suppress_alerts().await
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
        let operational_state = self.operational_state.clone();
        let last_health_check = self.last_health_check.clone();
        let shutdown_notify = self.shutdown_notify.clone();
        let config_manager = self.config_manager.clone(); // TAS-37 Supplemental: Pass config for configuration-aware monitoring
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
                            &operational_state,
                            &last_health_check,
                            &config_manager, // TAS-37 Supplemental: Pass config for configuration-aware health monitoring
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

    /// Coordinate the shutdown process with operational state awareness (TAS-37 Supplemental)
    async fn coordinate_shutdown(
        &self,
        timeout: Duration,
        shutdown_state: &SystemOperationalState,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();

        info!(
            "🔄 COORDINATOR: Beginning shutdown coordination (state: {}, timeout: {:?})",
            shutdown_state, timeout
        );

        // Phase 1: Stop background monitoring loops (already signaled via shutdown_notify)
        info!("📡 COORDINATOR: Monitoring loops will shut down on next cycle");

        // Phase 2: Stop executor pools with context-aware logging
        let pool_shutdown_start = std::time::Instant::now();
        let pool_shutdown_result = {
            let pool_manager = self.pool_manager.read().await;

            match shutdown_state {
                SystemOperationalState::Emergency => {
                    // For emergency shutdowns, use a shorter timeout and log appropriately
                    let emergency_pool_timeout = timeout.min(Duration::from_secs(10));
                    warn!(
                        "🚨 COORDINATOR: Emergency pool shutdown initiated (timeout: {:?})",
                        emergency_pool_timeout
                    );
                    pool_manager.stop_all_pools(emergency_pool_timeout).await
                }
                SystemOperationalState::GracefulShutdown => {
                    info!(
                        "🔄 COORDINATOR: Graceful pool shutdown initiated (timeout: {:?})",
                        timeout
                    );
                    pool_manager.stop_all_pools(timeout).await
                }
                _ => {
                    info!(
                        "🔄 COORDINATOR: Pool shutdown initiated for {} (timeout: {:?})",
                        shutdown_state, timeout
                    );
                    pool_manager.stop_all_pools(timeout).await
                }
            }
        };

        let pool_shutdown_duration = pool_shutdown_start.elapsed();

        // Phase 3: Log shutdown results with appropriate level based on state
        match (&pool_shutdown_result, shutdown_state) {
            (Ok(_), SystemOperationalState::GracefulShutdown) => {
                info!(
                    "✅ COORDINATOR: Pool shutdown completed successfully in {:?}",
                    pool_shutdown_duration
                );
            }
            (Ok(_), SystemOperationalState::Emergency) => {
                warn!(
                    "✅ COORDINATOR: Emergency pool shutdown completed in {:?}",
                    pool_shutdown_duration
                );
            }
            (Err(e), SystemOperationalState::GracefulShutdown) => {
                warn!(
                    "⚠️ COORDINATOR: Pool shutdown completed with errors in {:?}: {}",
                    pool_shutdown_duration, e
                );
            }
            (Err(e), _) => {
                error!(
                    "❌ COORDINATOR: Pool shutdown failed in {:?}: {}",
                    pool_shutdown_duration, e
                );
            }
            (Ok(_), _) => {
                info!(
                    "✅ COORDINATOR: Pool shutdown completed in {:?}",
                    pool_shutdown_duration
                );
            }
        }

        // Phase 4: Final coordination summary
        let total_shutdown_time = start_time.elapsed();

        match shutdown_state {
            SystemOperationalState::GracefulShutdown => {
                info!(
                    "🎯 COORDINATOR: Graceful shutdown coordination completed in {:?}",
                    total_shutdown_time
                );
            }
            SystemOperationalState::Emergency => {
                warn!(
                    "🚨 COORDINATOR: Emergency shutdown coordination completed in {:?}",
                    total_shutdown_time
                );
            }
            _ => {
                info!(
                    "🎯 COORDINATOR: Shutdown coordination completed in {:?}",
                    total_shutdown_time
                );
            }
        }

        pool_shutdown_result
    }

    /// Perform a health check cycle (TAS-37 Supplemental: Enhanced with configuration-aware monitoring)
    async fn health_check_cycle(
        health_monitor: &HealthMonitor,
        pool_manager: &Arc<RwLock<PoolManager>>,
        operational_state: &OperationalStateManager,
        last_health_check: &Arc<RwLock<Instant>>,
        config_manager: &Arc<ConfigManager>, // TAS-37 Supplemental: Added for configuration-aware health monitoring
    ) -> Result<()> {
        // TAS-37 Supplemental: Check if health monitoring should be active
        if !operational_state.should_monitor_health().await {
            debug!(
                "Skipping health check - health monitoring disabled in current operational state"
            );
            return Ok(());
        }

        let pool_manager = pool_manager.read().await;
        let health_report = pool_manager.get_health_report().await?;

        // TAS-37 Supplemental: Use configuration-aware health monitoring with operational state and config
        let config = config_manager.config();
        let operational_config = &config.orchestration.operational_state;
        health_monitor
            .record_health_report_with_config(
                health_report,
                Some(operational_state),
                Some(operational_config),
            )
            .await?;

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

    #[test]
    fn test_coordinator_status_creation() {
        use std::collections::HashMap;
        use std::time::Instant;
        use uuid::Uuid;

        let status = CoordinatorStatus {
            coordinator_id: Uuid::new_v4(),
            running: false,
            total_pools: 5,
            total_executors: 25,
            last_health_check: Instant::now(),
            last_scaling_action: Instant::now(),
            pool_statuses: HashMap::new(),
        };

        assert!(!status.running);
        assert_eq!(status.total_pools, 5);
        assert_eq!(status.total_executors, 25);
    }

    #[test]
    fn test_processing_trigger_result() {
        use std::time::Duration;

        let result = ProcessingTriggerResult {
            processing_duration: Duration::from_millis(150),
            tasks_processed: 10,
            orchestration_cycles: 1,
            steps_enqueued: 45,
            step_results_processed: 22,
        };

        assert_eq!(result.tasks_processed, 10);
        assert_eq!(result.orchestration_cycles, 1);
        assert_eq!(result.steps_enqueued, 45);
        assert_eq!(result.step_results_processed, 22);
    }

    // TAS-37 Supplemental: Comprehensive integration tests for shutdown-aware health monitoring

    /// Test that operational state transitions follow expected lifecycle
    #[tokio::test]
    async fn test_operational_state_lifecycle() {
        use crate::orchestration::coordinator::operational_state::{
            OperationalStateManager, SystemOperationalState,
        };

        let state_manager = OperationalStateManager::new();

        // Should start in Startup state
        assert_eq!(
            state_manager.current_state().await,
            SystemOperationalState::Startup
        );

        // Transition to Normal operation
        state_manager
            .transition_to(SystemOperationalState::Normal)
            .await
            .unwrap();
        assert_eq!(
            state_manager.current_state().await,
            SystemOperationalState::Normal
        );

        // Transition to GracefulShutdown
        state_manager
            .transition_to(SystemOperationalState::GracefulShutdown)
            .await
            .unwrap();
        assert_eq!(
            state_manager.current_state().await,
            SystemOperationalState::GracefulShutdown
        );

        // Transition to Stopped
        state_manager
            .transition_to(SystemOperationalState::Stopped)
            .await
            .unwrap();
        assert_eq!(
            state_manager.current_state().await,
            SystemOperationalState::Stopped
        );

        // Can restart from Stopped to Startup
        state_manager
            .transition_to(SystemOperationalState::Startup)
            .await
            .unwrap();
        assert_eq!(
            state_manager.current_state().await,
            SystemOperationalState::Startup
        );
    }

    /// Test configuration-aware health threshold multipliers
    #[tokio::test]
    async fn test_configuration_aware_health_thresholds() {
        use crate::config::OperationalStateConfig;
        use crate::orchestration::coordinator::operational_state::{
            OperationalStateManager, SystemOperationalState,
        };

        let state_manager = OperationalStateManager::new();

        // Create custom configuration with specific multipliers
        let config = OperationalStateConfig {
            enable_shutdown_aware_monitoring: true,
            suppress_alerts_during_shutdown: true,
            startup_health_threshold_multiplier: 0.3, // Very relaxed during startup
            shutdown_health_threshold_multiplier: 0.1, // Almost no requirements during shutdown
            graceful_shutdown_timeout_seconds: 45,
            emergency_shutdown_timeout_seconds: 10,
            enable_transition_logging: true,
            transition_log_level: "INFO".to_string(),
        };

        // Test startup state with custom config
        assert_eq!(
            state_manager.current_state().await,
            SystemOperationalState::Startup
        );
        let multiplier = state_manager
            .health_threshold_multiplier_with_config(&config)
            .await;
        assert_eq!(multiplier, 0.3);

        // Test normal state
        state_manager
            .transition_to(SystemOperationalState::Normal)
            .await
            .unwrap();
        let multiplier = state_manager
            .health_threshold_multiplier_with_config(&config)
            .await;
        assert_eq!(multiplier, 1.0);

        // Test graceful shutdown state with custom config
        state_manager
            .transition_to(SystemOperationalState::GracefulShutdown)
            .await
            .unwrap();
        let multiplier = state_manager
            .health_threshold_multiplier_with_config(&config)
            .await;
        assert_eq!(multiplier, 0.1);

        // Test emergency state (should always be 0.0)
        state_manager
            .transition_to(SystemOperationalState::Emergency)
            .await
            .unwrap();
        let multiplier = state_manager
            .health_threshold_multiplier_with_config(&config)
            .await;
        assert_eq!(multiplier, 0.0);
    }

    /// Test health monitoring behavior during different operational states
    #[tokio::test]
    async fn test_health_monitoring_operational_state_awareness() {
        use crate::orchestration::coordinator::{
            monitor::HealthMonitor,
            operational_state::{OperationalStateManager, SystemOperationalState},
        };

        let state_manager = OperationalStateManager::new();
        let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 10, 0.8);

        // Create a test health report
        let health_report = create_test_health_report(5, 10); // 50% healthy

        // Test during startup - should monitor but not alert aggressively
        assert_eq!(
            state_manager.current_state().await,
            SystemOperationalState::Startup
        );
        assert!(state_manager.should_monitor_health().await);
        assert!(!state_manager.should_suppress_alerts().await);

        // Record health report during startup
        monitor
            .record_health_report_with_operational_state(
                health_report.clone(),
                Some(&state_manager),
            )
            .await
            .unwrap();

        // Transition to normal operation
        state_manager
            .transition_to(SystemOperationalState::Normal)
            .await
            .unwrap();
        assert!(state_manager.should_monitor_health().await);
        assert!(!state_manager.should_suppress_alerts().await);

        // Record health report during normal operation
        monitor
            .record_health_report_with_operational_state(
                health_report.clone(),
                Some(&state_manager),
            )
            .await
            .unwrap();

        // Transition to graceful shutdown
        state_manager
            .transition_to(SystemOperationalState::GracefulShutdown)
            .await
            .unwrap();
        assert!(!state_manager.should_monitor_health().await);
        assert!(state_manager.should_suppress_alerts().await);

        // Record health report during graceful shutdown
        monitor
            .record_health_report_with_operational_state(
                health_report.clone(),
                Some(&state_manager),
            )
            .await
            .unwrap();

        // Transition to stopped
        state_manager
            .transition_to(SystemOperationalState::Stopped)
            .await
            .unwrap();
        assert!(!state_manager.should_monitor_health().await);
        assert!(state_manager.should_suppress_alerts().await);
    }

    /// Test default configuration values
    #[tokio::test]
    async fn test_default_configuration_values() {
        use crate::config::OperationalStateConfig;

        let config = OperationalStateConfig::default();

        // Verify defaults
        assert!(config.enable_shutdown_aware_monitoring);
        assert!(config.suppress_alerts_during_shutdown);
        assert_eq!(config.startup_health_threshold_multiplier, 0.5);
        assert_eq!(config.shutdown_health_threshold_multiplier, 0.0);
        assert_eq!(config.graceful_shutdown_timeout_seconds, 30);
        assert_eq!(config.emergency_shutdown_timeout_seconds, 5);
        assert!(config.enable_transition_logging);
        assert_eq!(config.transition_log_level, "INFO");

        // Should validate successfully
        config.validate().unwrap();
    }

    /// Helper function to create test health reports
    fn create_test_health_report(
        healthy_executors: usize,
        total_executors: usize,
    ) -> crate::orchestration::coordinator::pool::HealthReport {
        use crate::orchestration::coordinator::pool::{HealthReport, PoolStatus};
        use crate::orchestration::executor::traits::ExecutorType;
        use std::collections::HashMap;

        let mut pool_statuses = HashMap::new();

        pool_statuses.insert(
            ExecutorType::TaskRequestProcessor,
            PoolStatus {
                executor_type: ExecutorType::TaskRequestProcessor,
                active_executors: total_executors,
                healthy_executors,
                unhealthy_executors: total_executors - healthy_executors,
                min_executors: 1,
                max_executors: 15,
                total_items_processed: 1000,
                total_items_failed: 50,
                average_utilization: 0.7,
                uptime_seconds: 3600,
            },
        );

        HealthReport {
            total_pools: 1,
            total_executors,
            healthy_executors,
            unhealthy_executors: total_executors - healthy_executors,
            pool_statuses,
        }
    }
}
