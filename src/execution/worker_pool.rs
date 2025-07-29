//! Worker Pool Management for Command-Based Architecture

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::execution::command::{StepExecutionRequest, TaskHandlerInfo, WorkerCapabilities};

/// Worker pool for managing connected workers and intelligent work distribution
///
/// Manages worker registration, capability tracking, load balancing, and
/// health monitoring. Supports namespace-aware routing and capacity-based
/// work distribution.
///
/// # Examples
///
/// ```rust
/// use tasker_core::execution::worker_pool::*;
/// use tasker_core::execution::command::WorkerCapabilities;
/// use std::collections::HashMap;
///
/// #[tokio::main]
/// async fn main() {
///     let pool = WorkerPool::new();
///
///     let capabilities = WorkerCapabilities {
///         worker_id: "worker_1".to_string(),
///         max_concurrent_steps: 10,
///         supported_namespaces: vec!["orders".to_string()],
///         step_timeout_ms: 30000,
///         supports_retries: true,
///         language_runtime: "ruby".to_string(),
///         version: "3.1.0".to_string(),
///         custom_capabilities: HashMap::new(),
///     };
///
///     pool.register_worker(capabilities).await.unwrap();
/// }
/// ```
#[derive(Clone)]
pub struct WorkerPool {
    /// Active workers by worker ID
    workers: Arc<RwLock<HashMap<String, WorkerState>>>,

    /// Pool configuration
    pub config: WorkerPoolConfig,

    /// Load balancing strategy
    load_balancer: LoadBalancer,
}

impl WorkerPool {
    /// Create new worker pool with default configuration
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            config: WorkerPoolConfig::default(),
            load_balancer: LoadBalancer::RoundRobin { current_index: 0 },
        }
    }

    /// Create worker pool with custom configuration
    pub fn with_config(config: WorkerPoolConfig) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            config,
            load_balancer: LoadBalancer::RoundRobin { current_index: 0 },
        }
    }

    /// Register a new worker with capabilities
    pub async fn register_worker(
        &self,
        capabilities: WorkerCapabilities,
    ) -> Result<(), WorkerPoolError> {
        let worker_id = capabilities.worker_id.clone();

        // Validate capabilities
        self.validate_capabilities(&capabilities)?;

        let max_capacity = capabilities.max_concurrent_steps;
        let worker_state = WorkerState {
            capabilities,
            current_load: 0,
            max_capacity,
            last_heartbeat: Instant::now(),
            connection_health: ConnectionHealth::Healthy,
            total_steps_processed: 0,
            successful_steps: 0,
            failed_steps: 0,
            average_step_time_ms: 0.0,
            registered_at: chrono::Utc::now(),
            supported_task_handlers: HashMap::new(),
        };

        let mut workers = self.workers.write().await;

        // Check if worker already registered
        if workers.contains_key(&worker_id) {
            warn!(
                "Worker {} already registered, updating capabilities",
                worker_id
            );
        }

        workers.insert(worker_id.clone(), worker_state);
        info!(
            "Worker registered: {} with {} max concurrent steps",
            worker_id, max_capacity
        );

        Ok(())
    }

    /// Unregister a worker
    pub async fn unregister_worker(&self, worker_id: &str) -> Result<(), WorkerPoolError> {
        let mut workers = self.workers.write().await;

        match workers.remove(worker_id) {
            Some(worker_state) => {
                info!(
                    "Worker unregistered: {} (was processing {} steps)",
                    worker_id, worker_state.current_load
                );
                Ok(())
            }
            None => {
                warn!("Attempted to unregister non-existent worker: {}", worker_id);
                Err(WorkerPoolError::WorkerNotFound {
                    worker_id: worker_id.to_string(),
                })
            }
        }
    }

    /// Update worker heartbeat and load
    pub async fn update_worker_heartbeat(
        &self,
        worker_id: &str,
        current_load: usize,
    ) -> Result<(), WorkerPoolError> {
        let mut workers = self.workers.write().await;

        match workers.get_mut(worker_id) {
            Some(worker_state) => {
                worker_state.last_heartbeat = Instant::now();
                worker_state.current_load = current_load;
                worker_state.connection_health = ConnectionHealth::Healthy;
                debug!(
                    "Heartbeat updated for worker {}: load={}",
                    worker_id, current_load
                );
                Ok(())
            }
            None => {
                warn!("Heartbeat for non-existent worker: {}", worker_id);
                Err(WorkerPoolError::WorkerNotFound {
                    worker_id: worker_id.to_string(),
                })
            }
        }
    }

    /// Select best worker for batch execution based on capabilities and load
    pub async fn select_worker_for_batch(&self, batch_request: &BatchRequest) -> Option<String> {
        let workers = self.workers.read().await;

        // Filter workers by capability and availability
        let eligible_workers: Vec<(&String, &WorkerState)> = workers
            .iter()
            .filter(|(_, worker)| {
                // Check namespace compatibility
                let namespace_compatible = batch_request
                    .namespace
                    .as_ref()
                    .map(|ns| worker.capabilities.supported_namespaces.contains(ns))
                    .unwrap_or(true);

                // Check capacity availability
                let has_capacity = worker.current_load < worker.max_capacity;

                // Check connection health
                let is_healthy = worker.is_healthy(&self.config);

                namespace_compatible && has_capacity && is_healthy
            })
            .collect();

        if eligible_workers.is_empty() {
            debug!("No eligible workers found for batch request");
            return None;
        }

        // Apply load balancing strategy
        let selected_worker = match &self.load_balancer {
            LoadBalancer::RoundRobin { .. } => {
                // Simple round-robin among eligible workers
                eligible_workers.first().map(|(id, _)| (*id).clone())
            }
            LoadBalancer::LeastLoad => {
                // Select worker with lowest current load
                eligible_workers
                    .iter()
                    .min_by_key(|(_, worker)| worker.current_load)
                    .map(|(id, _)| (*id).clone())
            }
            LoadBalancer::CapacityBased => {
                // Select worker with highest available capacity percentage
                eligible_workers
                    .iter()
                    .max_by_key(|(_, worker)| {
                        worker.max_capacity.saturating_sub(worker.current_load)
                    })
                    .map(|(id, _)| (*id).clone())
            }
        };

        if let Some(ref worker_id) = selected_worker {
            debug!("Selected worker {} for batch execution", worker_id);
        }

        selected_worker
    }

    /// Get worker by ID
    pub async fn get_worker(&self, worker_id: &str) -> Option<WorkerState> {
        let workers = self.workers.read().await;
        workers.get(worker_id).cloned()
    }

    /// Get all registered workers
    pub async fn get_all_workers(&self) -> HashMap<String, WorkerState> {
        self.workers.read().await.clone()
    }

    /// Get workers by namespace
    pub async fn get_workers_by_namespace(&self, namespace: &str) -> Vec<(String, WorkerState)> {
        let workers = self.workers.read().await;
        workers
            .iter()
            .filter(|(_, worker)| {
                worker
                    .capabilities
                    .supported_namespaces
                    .contains(&namespace.to_string())
            })
            .map(|(id, worker)| (id.clone(), worker.clone()))
            .collect()
    }

    /// Increment worker load when assigning new steps
    pub async fn increment_worker_load(
        &self,
        worker_id: &str,
        step_count: usize,
    ) -> Result<(), WorkerPoolError> {
        let mut workers = self.workers.write().await;

        match workers.get_mut(worker_id) {
            Some(worker) => {
                worker.current_load += step_count;
                debug!(
                    "Incremented worker {} load by {} to {}/{}",
                    worker_id, step_count, worker.current_load, worker.max_capacity
                );
                Ok(())
            }
            None => {
                error!(
                    "Attempted to increment load for non-existent worker: {}",
                    worker_id
                );
                Err(WorkerPoolError::WorkerNotFound {
                    worker_id: worker_id.to_string(),
                })
            }
        }
    }

    /// Decrement worker load when steps complete
    pub async fn decrement_worker_load(
        &self,
        worker_id: &str,
        step_count: usize,
    ) -> Result<(), WorkerPoolError> {
        let mut workers = self.workers.write().await;

        match workers.get_mut(worker_id) {
            Some(worker) => {
                worker.current_load = worker.current_load.saturating_sub(step_count);
                debug!(
                    "Decremented worker {} load by {} to {}/{}",
                    worker_id, step_count, worker.current_load, worker.max_capacity
                );
                Ok(())
            }
            None => {
                error!(
                    "Attempted to decrement load for non-existent worker: {}",
                    worker_id
                );
                Err(WorkerPoolError::WorkerNotFound {
                    worker_id: worker_id.to_string(),
                })
            }
        }
    }

    /// Remove unhealthy workers
    pub async fn cleanup_unhealthy_workers(&self) -> usize {
        let mut workers = self.workers.write().await;
        let unhealthy_workers: Vec<String> = workers
            .iter()
            .filter(|(_, worker)| !worker.is_healthy(&self.config))
            .map(|(id, _)| id.clone())
            .collect();

        let removed_count = unhealthy_workers.len();
        for worker_id in unhealthy_workers {
            workers.remove(&worker_id);
            warn!("Removed unhealthy worker: {}", worker_id);
        }

        removed_count
    }

    /// Register a TaskHandler for a specific worker
    pub async fn register_task_handler(
        &self,
        worker_id: &str,
        task_handler_info: TaskHandlerInfo,
    ) -> Result<(), WorkerPoolError> {
        let mut workers = self.workers.write().await;

        match workers.get_mut(worker_id) {
            Some(worker) => {
                let namespace = task_handler_info.namespace.clone();
                let handler_name = task_handler_info.handler_name.clone();

                // Get or create namespace map
                let namespace_map = worker
                    .supported_task_handlers
                    .entry(namespace.clone())
                    .or_insert_with(HashMap::new);

                // Add or update the handler
                namespace_map.insert(handler_name.clone(), task_handler_info);

                info!(
                    "TaskHandler registered: worker={}, namespace={}, handler={}",
                    worker_id, namespace, handler_name
                );

                Ok(())
            }
            None => {
                warn!(
                    "Attempted to register TaskHandler for non-existent worker: {}",
                    worker_id
                );
                Err(WorkerPoolError::WorkerNotFound {
                    worker_id: worker_id.to_string(),
                })
            }
        }
    }

    /// Unregister a TaskHandler from a specific worker
    pub async fn unregister_task_handler(
        &self,
        worker_id: &str,
        namespace: &str,
        handler_name: &str,
    ) -> Result<(), WorkerPoolError> {
        let mut workers = self.workers.write().await;

        match workers.get_mut(worker_id) {
            Some(worker) => {
                let removed = worker
                    .supported_task_handlers
                    .get_mut(namespace)
                    .and_then(|namespace_map| namespace_map.remove(handler_name))
                    .is_some();

                if removed {
                    info!(
                        "TaskHandler unregistered: worker={}, namespace={}, handler={}",
                        worker_id, namespace, handler_name
                    );

                    // Clean up empty namespace maps
                    if let Some(namespace_map) = worker.supported_task_handlers.get(namespace) {
                        if namespace_map.is_empty() {
                            worker.supported_task_handlers.remove(namespace);
                        }
                    }
                } else {
                    warn!(
                        "Attempted to unregister non-existent TaskHandler: worker={}, namespace={}, handler={}",
                        worker_id, namespace, handler_name
                    );
                }

                Ok(())
            }
            None => {
                warn!(
                    "Attempted to unregister TaskHandler for non-existent worker: {}",
                    worker_id
                );
                Err(WorkerPoolError::WorkerNotFound {
                    worker_id: worker_id.to_string(),
                })
            }
        }
    }

    /// Find workers that support a specific TaskHandler
    pub async fn get_workers_supporting_task_handler(
        &self,
        namespace: &str,
        handler_name: &str,
    ) -> Vec<(String, WorkerState)> {
        let workers = self.workers.read().await;
        workers
            .iter()
            .filter(|(_, worker)| {
                worker
                    .supported_task_handlers
                    .get(namespace)
                    .map(|namespace_map| namespace_map.contains_key(handler_name))
                    .unwrap_or(false)
            })
            .map(|(id, worker)| (id.clone(), worker.clone()))
            .collect()
    }

    /// Get all TaskHandlers supported by a specific worker
    pub async fn get_worker_task_handlers(
        &self,
        worker_id: &str,
    ) -> Option<HashMap<String, HashMap<String, TaskHandlerInfo>>> {
        let workers = self.workers.read().await;
        workers
            .get(worker_id)
            .map(|worker| worker.supported_task_handlers.clone())
    }

    /// Select best worker for a specific TaskHandler execution
    pub async fn select_worker_for_task_handler(
        &self,
        namespace: &str,
        handler_name: &str,
    ) -> Option<String> {
        let workers = self.workers.read().await;

        // Filter workers that support this TaskHandler and are healthy
        let eligible_workers: Vec<(&String, &WorkerState)> = workers
            .iter()
            .filter(|(_, worker)| {
                // Check if worker supports this TaskHandler
                let supports_handler = worker
                    .supported_task_handlers
                    .get(namespace)
                    .map(|namespace_map| namespace_map.contains_key(handler_name))
                    .unwrap_or(false);

                // Check if worker has capacity and is healthy
                let has_capacity = worker.current_load < worker.max_capacity;
                let is_healthy = worker.is_healthy(&self.config);

                supports_handler && has_capacity && is_healthy
            })
            .collect();

        if eligible_workers.is_empty() {
            debug!(
                "No eligible workers found for TaskHandler: namespace={}, handler={}",
                namespace, handler_name
            );
            return None;
        }

        // Apply load balancing strategy (same as existing worker selection)
        let selected_worker = match &self.load_balancer {
            LoadBalancer::RoundRobin { .. } => {
                eligible_workers.first().map(|(id, _)| (*id).clone())
            }
            LoadBalancer::LeastLoad => eligible_workers
                .iter()
                .min_by_key(|(_, worker)| worker.current_load)
                .map(|(id, _)| (*id).clone()),
            LoadBalancer::CapacityBased => eligible_workers
                .iter()
                .max_by_key(|(_, worker)| worker.max_capacity.saturating_sub(worker.current_load))
                .map(|(id, _)| (*id).clone()),
        };

        if let Some(ref worker_id) = selected_worker {
            debug!(
                "Selected worker {} for TaskHandler: namespace={}, handler={}",
                worker_id, namespace, handler_name
            );
        }

        selected_worker
    }

    /// Get worker pool statistics
    pub async fn get_stats(&self) -> WorkerPoolStats {
        let workers = self.workers.read().await;

        let total_workers = workers.len();
        let healthy_workers = workers
            .iter()
            .filter(|(_, w)| w.is_healthy(&self.config))
            .count();
        let total_capacity = workers.iter().map(|(_, w)| w.max_capacity).sum();
        let current_load = workers.iter().map(|(_, w)| w.current_load).sum();
        let total_steps_processed = workers.iter().map(|(_, w)| w.total_steps_processed).sum();
        let total_successful_steps = workers.iter().map(|(_, w)| w.successful_steps).sum();
        let total_failed_steps = workers.iter().map(|(_, w)| w.failed_steps).sum();

        // Get namespace distribution
        let mut namespace_distribution = HashMap::new();
        for (_, worker) in workers.iter() {
            for namespace in &worker.capabilities.supported_namespaces {
                *namespace_distribution.entry(namespace.clone()).or_insert(0) += 1;
            }
        }

        WorkerPoolStats {
            total_workers,
            healthy_workers,
            unhealthy_workers: total_workers - healthy_workers,
            total_capacity,
            current_load,
            available_capacity: total_capacity.saturating_sub(current_load),
            total_steps_processed,
            successful_steps: total_successful_steps,
            failed_steps: total_failed_steps,
            namespace_distribution,
        }
    }

    /// Validate worker capabilities
    fn validate_capabilities(
        &self,
        capabilities: &WorkerCapabilities,
    ) -> Result<(), WorkerPoolError> {
        if capabilities.worker_id.is_empty() {
            return Err(WorkerPoolError::InvalidCapabilities {
                reason: "Worker ID cannot be empty".to_string(),
            });
        }

        if capabilities.max_concurrent_steps == 0 {
            return Err(WorkerPoolError::InvalidCapabilities {
                reason: "Max concurrent steps must be greater than 0".to_string(),
            });
        }

        if capabilities.step_timeout_ms == 0 {
            return Err(WorkerPoolError::InvalidCapabilities {
                reason: "Step timeout must be greater than 0".to_string(),
            });
        }

        if capabilities.language_runtime.is_empty() {
            return Err(WorkerPoolError::InvalidCapabilities {
                reason: "Language runtime cannot be empty".to_string(),
            });
        }

        Ok(())
    }
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Worker state information
#[derive(Debug, Clone)]
pub struct WorkerState {
    /// Worker capabilities
    pub capabilities: WorkerCapabilities,

    /// Current number of steps being processed
    pub current_load: usize,

    /// Maximum concurrent steps this worker can handle
    pub max_capacity: usize,

    /// Last heartbeat timestamp
    pub last_heartbeat: Instant,

    /// Connection health status
    pub connection_health: ConnectionHealth,

    /// Total steps processed by this worker
    pub total_steps_processed: u64,

    /// Number of successful steps
    pub successful_steps: u64,

    /// Number of failed steps
    pub failed_steps: u64,

    /// Average step execution time in milliseconds
    pub average_step_time_ms: f64,

    /// When this worker was registered
    pub registered_at: chrono::DateTime<chrono::Utc>,

    /// TaskHandlers this worker supports (namespace -> handler_name -> TaskHandlerInfo)
    pub supported_task_handlers: HashMap<String, HashMap<String, TaskHandlerInfo>>,
}

impl WorkerState {
    /// Check if worker is healthy based on configuration
    pub fn is_healthy(&self, config: &WorkerPoolConfig) -> bool {
        let heartbeat_age = self.last_heartbeat.elapsed();
        let is_heartbeat_recent =
            heartbeat_age <= Duration::from_millis(config.heartbeat_timeout_ms);
        let is_connection_healthy = matches!(self.connection_health, ConnectionHealth::Healthy);

        is_heartbeat_recent && is_connection_healthy
    }

    /// Get available capacity
    pub fn available_capacity(&self) -> usize {
        self.max_capacity.saturating_sub(self.current_load)
    }

    /// Get load percentage
    pub fn load_percentage(&self) -> f64 {
        if self.max_capacity == 0 {
            0.0
        } else {
            (self.current_load as f64 / self.max_capacity as f64) * 100.0
        }
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_steps + self.failed_steps;
        if total == 0 {
            0.0
        } else {
            (self.successful_steps as f64 / total as f64) * 100.0
        }
    }

    /// Check if this worker supports a specific TaskHandler
    pub fn supports_task_handler(&self, namespace: &str, handler_name: &str) -> bool {
        self.supported_task_handlers
            .get(namespace)
            .map(|namespace_map| namespace_map.contains_key(handler_name))
            .unwrap_or(false)
    }

    /// Get all TaskHandlers in a specific namespace
    pub fn get_task_handlers_in_namespace(&self, namespace: &str) -> Vec<&TaskHandlerInfo> {
        self.supported_task_handlers
            .get(namespace)
            .map(|namespace_map| namespace_map.values().collect())
            .unwrap_or_default()
    }

    /// Get total number of supported TaskHandlers
    pub fn total_supported_handlers(&self) -> usize {
        self.supported_task_handlers
            .values()
            .map(|namespace_map| namespace_map.len())
            .sum()
    }

    /// Get all supported namespaces for TaskHandlers
    pub fn supported_handler_namespaces(&self) -> Vec<&String> {
        self.supported_task_handlers.keys().collect()
    }
}

/// Connection health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionHealth {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Batch request for worker selection
#[derive(Debug, Clone)]
pub struct BatchRequest {
    pub batch_id: String,
    pub steps: Vec<StepExecutionRequest>,
    pub namespace: Option<String>,
    pub priority: Option<u32>,
    pub estimated_duration_ms: Option<u64>,
}

/// Load balancing strategies
#[derive(Debug, Clone)]
pub enum LoadBalancer {
    /// Simple round-robin distribution
    RoundRobin { current_index: usize },

    /// Select worker with lowest current load
    LeastLoad,

    /// Select worker with highest available capacity
    CapacityBased,
}

/// Worker pool configuration
#[derive(Debug, Clone)]
pub struct WorkerPoolConfig {
    /// Maximum number of workers
    pub max_workers: usize,

    /// Heartbeat timeout in milliseconds
    pub heartbeat_timeout_ms: u64,

    /// Cleanup interval for unhealthy workers in milliseconds
    pub cleanup_interval_ms: u64,

    /// Default worker timeout in milliseconds
    pub default_worker_timeout_ms: u64,
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            max_workers: 1000,
            heartbeat_timeout_ms: 30000,      // 30 seconds
            cleanup_interval_ms: 60000,       // 1 minute
            default_worker_timeout_ms: 30000, // 30 seconds
        }
    }
}

/// Worker pool statistics
#[derive(Debug, Clone)]
pub struct WorkerPoolStats {
    pub total_workers: usize,
    pub healthy_workers: usize,
    pub unhealthy_workers: usize,
    pub total_capacity: usize,
    pub current_load: usize,
    pub available_capacity: usize,
    pub total_steps_processed: u64,
    pub successful_steps: u64,
    pub failed_steps: u64,
    pub namespace_distribution: HashMap<String, usize>,
}

/// Worker pool errors
#[derive(Debug, thiserror::Error)]
pub enum WorkerPoolError {
    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: String },

    #[error("Invalid worker capabilities: {reason}")]
    InvalidCapabilities { reason: String },

    #[error("Worker pool full: maximum {max_workers} workers allowed")]
    PoolFull { max_workers: usize },

    #[error("Worker operation failed: {message}")]
    OperationFailed { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_capabilities(worker_id: &str, namespaces: Vec<&str>) -> WorkerCapabilities {
        WorkerCapabilities {
            worker_id: worker_id.to_string(),
            max_concurrent_steps: 10,
            supported_namespaces: namespaces.iter().map(|s| s.to_string()).collect(),
            step_timeout_ms: 30000,
            supports_retries: true,
            language_runtime: "ruby".to_string(),
            version: "3.1.0".to_string(),
            custom_capabilities: HashMap::new(),
            connection_info: None,
            runtime_info: None,
        }
    }

    #[tokio::test]
    async fn test_worker_registration() {
        let pool = WorkerPool::new();
        let capabilities = create_test_capabilities("worker_1", vec!["orders", "inventory"]);

        // Register worker
        assert!(pool.register_worker(capabilities.clone()).await.is_ok());

        // Check worker exists
        let worker = pool.get_worker("worker_1").await;
        assert!(worker.is_some());
        assert_eq!(worker.unwrap().capabilities.worker_id, "worker_1");

        // Get stats
        let stats = pool.get_stats().await;
        assert_eq!(stats.total_workers, 1);
        assert_eq!(stats.healthy_workers, 1);
    }

    #[tokio::test]
    async fn test_worker_unregistration() {
        let pool = WorkerPool::new();
        let capabilities = create_test_capabilities("worker_1", vec!["orders"]);

        // Register then unregister
        pool.register_worker(capabilities).await.unwrap();
        assert!(pool.unregister_worker("worker_1").await.is_ok());

        // Check worker no longer exists
        assert!(pool.get_worker("worker_1").await.is_none());

        // Unregister non-existent worker should fail
        assert!(pool.unregister_worker("worker_1").await.is_err());
    }

    #[tokio::test]
    async fn test_worker_selection() {
        let pool = WorkerPool::new();

        // Register workers with different namespaces
        let worker1 = create_test_capabilities("worker_1", vec!["orders"]);
        let worker2 = create_test_capabilities("worker_2", vec!["inventory"]);
        let worker3 = create_test_capabilities("worker_3", vec!["orders", "inventory"]);

        pool.register_worker(worker1).await.unwrap();
        pool.register_worker(worker2).await.unwrap();
        pool.register_worker(worker3).await.unwrap();

        // Create batch request for orders namespace
        let batch_request = BatchRequest {
            batch_id: "batch_1".to_string(),
            steps: vec![],
            namespace: Some("orders".to_string()),
            priority: None,
            estimated_duration_ms: None,
        };

        // Should select a worker that supports orders
        let selected = pool.select_worker_for_batch(&batch_request).await;
        assert!(selected.is_some());

        let worker_id = selected.unwrap();
        assert!(worker_id == "worker_1" || worker_id == "worker_3");

        // Request for unsupported namespace
        let batch_request = BatchRequest {
            batch_id: "batch_2".to_string(),
            steps: vec![],
            namespace: Some("unsupported".to_string()),
            priority: None,
            estimated_duration_ms: None,
        };

        let selected = pool.select_worker_for_batch(&batch_request).await;
        assert!(selected.is_none());
    }

    #[tokio::test]
    async fn test_namespace_filtering() {
        let pool = WorkerPool::new();

        let worker1 = create_test_capabilities("worker_1", vec!["orders"]);
        let worker2 = create_test_capabilities("worker_2", vec!["inventory"]);
        let worker3 = create_test_capabilities("worker_3", vec!["orders", "inventory"]);

        pool.register_worker(worker1).await.unwrap();
        pool.register_worker(worker2).await.unwrap();
        pool.register_worker(worker3).await.unwrap();

        // Get workers by namespace
        let orders_workers = pool.get_workers_by_namespace("orders").await;
        assert_eq!(orders_workers.len(), 2);

        let inventory_workers = pool.get_workers_by_namespace("inventory").await;
        assert_eq!(inventory_workers.len(), 2);

        let unknown_workers = pool.get_workers_by_namespace("unknown").await;
        assert_eq!(unknown_workers.len(), 0);
    }

    #[tokio::test]
    async fn test_invalid_capabilities() {
        let pool = WorkerPool::new();

        // Empty worker ID
        let mut invalid_capabilities = create_test_capabilities("", vec!["orders"]);
        assert!(pool
            .register_worker(invalid_capabilities.clone())
            .await
            .is_err());

        // Zero max concurrent steps
        invalid_capabilities.worker_id = "worker_1".to_string();
        invalid_capabilities.max_concurrent_steps = 0;
        assert!(pool
            .register_worker(invalid_capabilities.clone())
            .await
            .is_err());

        // Zero timeout
        invalid_capabilities.max_concurrent_steps = 10;
        invalid_capabilities.step_timeout_ms = 0;
        assert!(pool.register_worker(invalid_capabilities).await.is_err());
    }

    #[tokio::test]
    async fn test_worker_health() {
        let config = WorkerPoolConfig {
            heartbeat_timeout_ms: 1000, // 1 second for testing
            ..WorkerPoolConfig::default()
        };

        let pool = WorkerPool::with_config(config.clone());
        let capabilities = create_test_capabilities("worker_1", vec!["orders"]);

        pool.register_worker(capabilities).await.unwrap();

        // Worker should be healthy initially
        let worker = pool.get_worker("worker_1").await.unwrap();
        assert!(worker.is_healthy(&config));

        // Simulate heartbeat timeout
        tokio::time::sleep(Duration::from_millis(1100)).await;

        let worker = pool.get_worker("worker_1").await.unwrap();
        assert!(!worker.is_healthy(&config));
    }

    #[tokio::test]
    async fn test_task_handler_registration() {
        let pool = WorkerPool::new();
        let capabilities = create_test_capabilities("worker_1", vec!["orders"]);

        // Register worker
        pool.register_worker(capabilities).await.unwrap();

        // Create TaskHandlerInfo
        let task_handler_info = TaskHandlerInfo {
            namespace: "orders".to_string(),
            handler_name: "OrderProcessorHandler".to_string(),
            version: "1.0.0".to_string(),
            handler_class: "Orders::OrderProcessorHandler".to_string(),
            supported_step_types: vec!["validate_order".to_string(), "process_payment".to_string()],
            handler_config: HashMap::new(),
            timeout_ms: 30000,
            supports_retries: true,
        };

        // Register TaskHandler for worker
        assert!(pool
            .register_task_handler("worker_1", task_handler_info)
            .await
            .is_ok());

        // Check if worker supports the TaskHandler
        let worker = pool.get_worker("worker_1").await.unwrap();
        assert!(worker.supports_task_handler("orders", "OrderProcessorHandler"));
        assert!(!worker.supports_task_handler("orders", "NonexistentHandler"));
        assert!(!worker.supports_task_handler("inventory", "OrderProcessorHandler"));

        // Get workers supporting the TaskHandler
        let supporting_workers = pool
            .get_workers_supporting_task_handler("orders", "OrderProcessorHandler")
            .await;
        assert_eq!(supporting_workers.len(), 1);
        assert_eq!(supporting_workers[0].0, "worker_1");

        // Test worker selection for TaskHandler
        let selected_worker = pool
            .select_worker_for_task_handler("orders", "OrderProcessorHandler")
            .await;
        assert_eq!(selected_worker, Some("worker_1".to_string()));

        // Test unregistration
        assert!(pool
            .unregister_task_handler("worker_1", "orders", "OrderProcessorHandler")
            .await
            .is_ok());

        let worker = pool.get_worker("worker_1").await.unwrap();
        assert!(!worker.supports_task_handler("orders", "OrderProcessorHandler"));

        let supporting_workers = pool
            .get_workers_supporting_task_handler("orders", "OrderProcessorHandler")
            .await;
        assert_eq!(supporting_workers.len(), 0);
    }

    #[tokio::test]
    async fn test_task_handler_worker_selection() {
        let pool = WorkerPool::new();

        // Register multiple workers
        let worker1_caps = create_test_capabilities("worker_1", vec!["orders"]);
        let worker2_caps = create_test_capabilities("worker_2", vec!["orders"]);
        let worker3_caps = create_test_capabilities("worker_3", vec!["inventory"]);

        pool.register_worker(worker1_caps).await.unwrap();
        pool.register_worker(worker2_caps).await.unwrap();
        pool.register_worker(worker3_caps).await.unwrap();

        // Register same TaskHandler on worker_1 and worker_2
        let order_handler = TaskHandlerInfo {
            namespace: "orders".to_string(),
            handler_name: "OrderProcessorHandler".to_string(),
            version: "1.0.0".to_string(),
            handler_class: "Orders::OrderProcessorHandler".to_string(),
            supported_step_types: vec!["validate_order".to_string()],
            handler_config: HashMap::new(),
            timeout_ms: 30000,
            supports_retries: true,
        };

        pool.register_task_handler("worker_1", order_handler.clone())
            .await
            .unwrap();
        pool.register_task_handler("worker_2", order_handler.clone())
            .await
            .unwrap();

        // Register different TaskHandler on worker_3
        let inventory_handler = TaskHandlerInfo {
            namespace: "inventory".to_string(),
            handler_name: "InventoryHandler".to_string(),
            version: "1.0.0".to_string(),
            handler_class: "Inventory::InventoryHandler".to_string(),
            supported_step_types: vec!["check_stock".to_string()],
            handler_config: HashMap::new(),
            timeout_ms: 30000,
            supports_retries: true,
        };

        pool.register_task_handler("worker_3", inventory_handler)
            .await
            .unwrap();

        // Test selection for orders namespace
        let supporting_workers = pool
            .get_workers_supporting_task_handler("orders", "OrderProcessorHandler")
            .await;
        assert_eq!(supporting_workers.len(), 2);

        let selected_worker = pool
            .select_worker_for_task_handler("orders", "OrderProcessorHandler")
            .await;
        assert!(
            selected_worker == Some("worker_1".to_string())
                || selected_worker == Some("worker_2".to_string())
        );

        // Test selection for inventory namespace
        let inventory_workers = pool
            .get_workers_supporting_task_handler("inventory", "InventoryHandler")
            .await;
        assert_eq!(inventory_workers.len(), 1);
        assert_eq!(inventory_workers[0].0, "worker_3");

        let inventory_selected = pool
            .select_worker_for_task_handler("inventory", "InventoryHandler")
            .await;
        assert_eq!(inventory_selected, Some("worker_3".to_string()));

        // Test non-existent TaskHandler
        let nonexistent = pool
            .select_worker_for_task_handler("nonexistent", "NonexistentHandler")
            .await;
        assert_eq!(nonexistent, None);
    }
}
