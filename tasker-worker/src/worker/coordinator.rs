//! Worker coordinator mirroring OrchestrationLoopCoordinator patterns
//! Manages WorkerExecutor pools that listen on namespaced queues

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use tasker_shared::{
    config::ConfigManager,
    constants::SystemOperationalState,
    database::Connection,
    messaging::UnifiedPgmqClient,
    registry::{HandlerRegistry, TaskTemplateRegistry},
};

use crate::{
    config::WorkerConfig,
    error::{Result, WorkerError},
    event_publisher::EventPublisher,
    worker::{
        executor_pool::WorkerExecutorPool, health_monitor::WorkerHealthMonitor,
        resource_validator::WorkerResourceValidator,
    },
};

/// Namespace metrics for auto-scaling decisions
#[derive(Debug, Clone)]
pub struct NamespaceMetrics {
    pub namespace: String,
    pub queue_depth: usize,
    pub processing_rate: f64,
    pub error_rate: f64,
    pub active_executors: usize,
}

/// Worker coordinator mirroring OrchestrationLoopCoordinator patterns
/// Manages WorkerExecutor pools that listen on namespaced queues
pub struct WorkerLoopCoordinator {
    config_manager: Arc<ConfigManager>,
    database_connection: Arc<Connection>,
    messaging_client: Arc<UnifiedPgmqClient>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    handler_registry: Arc<HandlerRegistry>,
    event_publisher: Arc<EventPublisher>,
    worker_config: WorkerConfig,

    // WorkerExecutor pools per namespace (mirroring OrchestratorExecutor pattern)
    namespace_pools: Arc<RwLock<HashMap<String, Arc<WorkerExecutorPool>>>>,

    // Health and scaling management
    health_monitor: Arc<WorkerHealthMonitor>,
    resource_validator: Arc<WorkerResourceValidator>,
    operational_state: Arc<RwLock<SystemOperationalState>>,

    // Background tasks
    coordinator_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    health_monitor_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl WorkerLoopCoordinator {
    pub async fn new(
        config_manager: Arc<ConfigManager>,
        database_connection: Arc<Connection>,
        messaging_client: Arc<UnifiedPgmqClient>,
        task_template_registry: Arc<TaskTemplateRegistry>,
        handler_registry: Arc<HandlerRegistry>,
        event_publisher: Arc<EventPublisher>,
        worker_config: WorkerConfig,
    ) -> Result<Self> {
        // Initialize WorkerExecutor pools for each namespace
        let mut namespace_pools = HashMap::new();
        for namespace in &worker_config.namespaces {
            let pool = Arc::new(
                WorkerExecutorPool::new(
                    namespace.clone(),
                    messaging_client.clone(),
                    database_connection.clone(),
                    task_template_registry.clone(),
                    handler_registry.clone(),
                    event_publisher.clone(),
                    &worker_config.step_executor_pool,
                    &worker_config.queue_config,
                )
                .await?,
            );
            namespace_pools.insert(namespace.clone(), pool);
        }

        // Initialize health monitoring
        let health_monitor = Arc::new(
            WorkerHealthMonitor::new(&worker_config)
                .await
                .map_err(|e| WorkerError::Other(e))?,
        );

        // Initialize resource validation
        let resource_validator = Arc::new(
            WorkerResourceValidator::new(&worker_config)
                .await
                .map_err(|e| WorkerError::Other(e))?,
        );

        let operational_state = Arc::new(RwLock::new(SystemOperationalState::Startup));

        Ok(Self {
            config_manager,
            database_connection,
            messaging_client,
            task_template_registry,
            handler_registry,
            event_publisher,
            worker_config,
            namespace_pools: Arc::new(RwLock::new(namespace_pools)),
            health_monitor,
            resource_validator,
            operational_state,
            coordinator_handle: Arc::new(RwLock::new(None)),
            health_monitor_handle: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("🔄 Starting WorkerLoopCoordinator...");

        // Transition to normal operational state
        {
            let mut state = self.operational_state.write().await;
            *state = SystemOperationalState::Normal;
        }

        // Start WorkerExecutor pools (each listens on namespaced queues)
        {
            let pools = self.namespace_pools.read().await;
            for (namespace, pool) in pools.iter() {
                info!("🔄 Starting WorkerExecutor pool for namespace: {}", namespace);
                pool.start().await?;
            }
        }

        // Start coordinator loop
        {
            let mut handle = self.coordinator_handle.write().await;
            *handle = Some(self.spawn_coordinator_loop().await);
        }

        // Start health monitoring
        {
            let mut handle = self.health_monitor_handle.write().await;
            *handle = Some(self.spawn_health_monitor().await);
        }

        info!("✅ WorkerLoopCoordinator started successfully");
        Ok(())
    }

    pub async fn transition_to_graceful_shutdown(&mut self) -> Result<()> {
        info!("🛑 Transitioning WorkerLoopCoordinator to graceful shutdown...");

        // Update operational state
        {
            let mut state = self.operational_state.write().await;
            *state = SystemOperationalState::GracefulShutdown;
        }

        // Signal shutdown to WorkerExecutor pools
        {
            let pools = self.namespace_pools.read().await;
            for pool in pools.values() {
                pool.initiate_graceful_shutdown().await?;
            }
        }

        Ok(())
    }

    pub async fn wait_for_shutdown(&mut self) -> Result<()> {
        info!("⏳ Waiting for WorkerLoopCoordinator shutdown...");

        // Wait for coordinator loop to finish
        {
            let mut handle = self.coordinator_handle.write().await;
            if let Some(h) = handle.take() {
                let _ = h.await;
            }
        }

        // Wait for health monitor to finish
        {
            let mut handle = self.health_monitor_handle.write().await;
            if let Some(h) = handle.take() {
                let _ = h.await;
            }
        }

        // Wait for all pools to shut down
        {
            let pools = self.namespace_pools.read().await;
            for pool in pools.values() {
                pool.wait_for_shutdown().await?;
            }
        }

        // Update operational state
        {
            let mut state = self.operational_state.write().await;
            *state = SystemOperationalState::Stopped;
        }

        info!("✅ WorkerLoopCoordinator shutdown complete");
        Ok(())
    }

    async fn spawn_coordinator_loop(&self) -> JoinHandle<()> {
        let operational_state = self.operational_state.clone();
        let namespace_pools = self.namespace_pools.clone();
        let health_monitor = self.health_monitor.clone();
        let resource_validator = self.resource_validator.clone();
        let config = self.worker_config.clone();

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(config.health_monitoring.health_check_interval_seconds));

            loop {
                interval.tick().await;

                let state = *operational_state.read().await;

                match state {
                    SystemOperationalState::Normal => {
                        // Perform auto-scaling decisions
                        if let Err(e) =
                            Self::manage_auto_scaling(&namespace_pools, &health_monitor).await
                        {
                            error!("Auto-scaling error: {}", e);
                        }

                        // Resource validation
                        if let Err(e) =
                            Self::validate_system_resources(&namespace_pools, &resource_validator)
                                .await
                        {
                            error!("Resource validation error: {}", e);
                        }
                    }
                    SystemOperationalState::GracefulShutdown => {
                        // Wait for pools to finish current work
                        if Self::all_pools_idle(&namespace_pools).await {
                            info!("All worker pools are idle, ready for shutdown");
                            break;
                        }
                    }
                    SystemOperationalState::Stopped => break,
                    _ => {
                        // Handle other states
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }
        })
    }

    async fn spawn_health_monitor(&self) -> JoinHandle<()> {
        let health_monitor = self.health_monitor.clone();
        let namespace_pools = self.namespace_pools.clone();
        let operational_state = self.operational_state.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));

            loop {
                interval.tick().await;

                let state = *operational_state.read().await;
                if !matches!(state, SystemOperationalState::Normal) {
                    if matches!(state, SystemOperationalState::Stopped) {
                        break;
                    }
                    continue;
                }

                // Collect health metrics
                let pools = namespace_pools.read().await;
                for (namespace, pool) in pools.iter() {
                    if let Ok(metrics) = pool.get_metrics().await {
                        health_monitor.record_namespace_metrics(namespace, metrics).await;
                    }
                }

                // Check overall health
                if let Err(e) = health_monitor.check_system_health().await {
                    error!("Health check failed: {}", e);
                }
            }
        })
    }

    async fn manage_auto_scaling(
        namespace_pools: &Arc<RwLock<HashMap<String, Arc<WorkerExecutorPool>>>>,
        health_monitor: &WorkerHealthMonitor,
    ) -> Result<()> {
        let pools = namespace_pools.read().await;

        for (namespace, pool) in pools.iter() {
            let metrics = health_monitor.get_namespace_metrics(namespace).await?;

            // Auto-scaling logic based on queue depth and processing rate
            if metrics.queue_depth > 100 && metrics.processing_rate < 10.0 {
                info!(
                    "📈 Scaling up namespace {} (queue_depth: {}, processing_rate: {:.2})",
                    namespace, metrics.queue_depth, metrics.processing_rate
                );
                pool.scale_up().await?;
            } else if metrics.queue_depth < 10 && metrics.processing_rate > 50.0 {
                info!(
                    "📉 Scaling down namespace {} (queue_depth: {}, processing_rate: {:.2})",
                    namespace, metrics.queue_depth, metrics.processing_rate
                );
                pool.scale_down().await?;
            }
        }

        Ok(())
    }

    async fn validate_system_resources(
        namespace_pools: &Arc<RwLock<HashMap<String, Arc<WorkerExecutorPool>>>>,
        resource_validator: &WorkerResourceValidator,
    ) -> Result<()> {
        // Check database connections
        resource_validator.validate_database_connections().await?;

        // Check memory usage
        resource_validator.validate_memory_usage().await?;

        // Check CPU usage
        resource_validator.validate_cpu_usage().await?;

        // Validate per-pool resources
        let pools = namespace_pools.read().await;
        for (namespace, pool) in pools.iter() {
            let pool_size = pool.current_size().await;
            if pool_size > 0 {
                resource_validator
                    .validate_pool_resources(namespace, pool_size)
                    .await?;
            }
        }

        Ok(())
    }

    async fn all_pools_idle(
        namespace_pools: &Arc<RwLock<HashMap<String, Arc<WorkerExecutorPool>>>>,
    ) -> bool {
        let pools = namespace_pools.read().await;
        for pool in pools.values() {
            if !pool.is_idle().await {
                return false;
            }
        }
        true
    }

    /// Get metrics for a specific namespace
    pub async fn get_namespace_metrics(&self, namespace: &str) -> Result<NamespaceMetrics> {
        self.health_monitor.get_namespace_metrics(namespace).await
    }

    /// Get current operational state
    pub async fn operational_state(&self) -> SystemOperationalState {
        *self.operational_state.read().await
    }
}