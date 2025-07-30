//! Worker Management Command Handler

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

use crate::database::optimized_queries::OptimizedWorkerQueries;
use crate::execution::command::{Command, CommandPayload, CommandSource, CommandType};
use crate::execution::command_router::CommandHandler;
use crate::execution::worker_pool::{WorkerPool, WorkerPoolError};
use crate::models::core::{
    named_task::NamedTask,
    task_namespace::TaskNamespace,
    worker::{NewWorker, Worker, WorkerMetadata},
    worker_named_task::{NewWorkerNamedTask, WorkerNamedTask},
    worker_registration::{NewWorkerRegistration, WorkerRegistration, WorkerStatus},
    worker_transport_availability::{
        NewWorkerTransportAvailability, TransportType, WorkerTransportAvailability,
    },
};
use serde::{Deserialize, Serialize};
use serde_json;

/// Uptime information for the worker management handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UptimeInfo {
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Human-readable uptime duration
    pub uptime_duration: String,
    /// When the handler was started (UTC timestamp)
    pub started_at_utc: String,
}

/// Comprehensive system status including uptime and worker statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    /// Overall system status
    pub status: String,
    /// Uptime information
    pub uptime_info: UptimeInfo,
    /// Core instance identifier
    pub core_instance_id: String,
    /// Worker pool statistics
    pub worker_statistics: WorkerStatistics,
}

/// Worker pool statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatistics {
    pub total_workers: usize,
    pub healthy_workers: usize,
    pub unhealthy_workers: usize,
    pub total_capacity: usize,
    pub current_load: usize,
    pub available_capacity: usize,
}

/// Handler for worker lifecycle management commands
///
/// Processes worker registration, unregistration, and heartbeat commands.
/// Coordinates with the WorkerPool to maintain worker state and health.
///
/// # Supported Commands
///
/// - `RegisterWorker`: Register new worker with capabilities
/// - `UnregisterWorker`: Remove worker from pool
/// - `WorkerHeartbeat`: Update worker health and load status
///
/// # Examples
///
/// ```rust
/// use tasker_core::execution::command_handlers::WorkerManagementHandler;
/// use tasker_core::execution::worker_pool::WorkerPool;
/// use std::sync::Arc;
///
/// let worker_pool = Arc::new(WorkerPool::new());
/// let handler = WorkerManagementHandler::new(worker_pool);
/// ```
pub struct WorkerManagementHandler {
    /// Reference to the worker pool for state management
    worker_pool: Arc<WorkerPool>,
    /// Database connection pool for persistent storage
    db_pool: PgPool,
    /// Optimized database queries with caching
    optimized_queries: OptimizedWorkerQueries,
    /// Core instance identifier for distributed worker availability tracking
    core_instance_id: String,
    /// Start time for uptime calculation
    start_time: Instant,
    /// Start time as UTC timestamp for serialization
    start_time_utc: String,
}

impl WorkerManagementHandler {
    /// Create new worker management handler with database persistence and optimized queries
    pub fn new(worker_pool: Arc<WorkerPool>, db_pool: PgPool, core_instance_id: String) -> Self {
        let start_time_utc = chrono::Utc::now().to_rfc3339();
        let optimized_queries = OptimizedWorkerQueries::new(db_pool.clone());
        
        Self {
            worker_pool,
            db_pool,
            optimized_queries,
            core_instance_id,
            start_time: Instant::now(),
            start_time_utc,
        }
    }

    /// Get the uptime of this worker management handler in seconds
    pub fn get_uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get detailed uptime information
    pub fn get_uptime_info(&self) -> UptimeInfo {
        let elapsed = self.start_time.elapsed();
        UptimeInfo {
            uptime_seconds: elapsed.as_secs(),
            uptime_duration: format!(
                "{}d {}h {}m {}s",
                elapsed.as_secs() / 86400,
                (elapsed.as_secs() % 86400) / 3600,
                (elapsed.as_secs() % 3600) / 60,
                elapsed.as_secs() % 60
            ),
            started_at_utc: self.start_time_utc.clone(),
        }
    }

    /// Handle worker registration command with database persistence
    async fn handle_register_worker(
        &self,
        command: &Command,
        worker_capabilities: crate::execution::command::WorkerCapabilities,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Registering worker: {} with database persistence",
            worker_capabilities.worker_id
        );

        // 1. Register with in-memory worker pool first (for backward compatibility)
        if let Err(e) = self
            .worker_pool
            .register_worker(worker_capabilities.clone())
            .await
        {
            error!("Worker pool registration failed: {}", e);
            return Ok(command.create_response(
                CommandType::Error,
                CommandPayload::Error {
                    error_type: "WorkerRegistrationError".to_string(),
                    message: format!(
                        "Failed to register worker {}: {}",
                        worker_capabilities.worker_id, e
                    ),
                    details: None,
                    retryable: match e {
                        WorkerPoolError::InvalidCapabilities { .. } => false,
                        _ => true,
                    },
                },
                CommandSource::RustServer {
                    id: "worker_management_handler".to_string(),
                },
            ));
        }

        // 2. Create or update worker in database
        let worker_metadata = WorkerMetadata {
            version: Some(worker_capabilities.version.clone()),
            ruby_version: worker_capabilities
                .runtime_info
                .as_ref()
                .and_then(|info| info.language_version.clone()),
            hostname: worker_capabilities
                .runtime_info
                .as_ref()
                .and_then(|info| info.hostname.clone()),
            pid: worker_capabilities
                .runtime_info
                .as_ref()
                .and_then(|info| info.pid.map(|p| p as i32)),
            started_at: worker_capabilities
                .runtime_info
                .as_ref()
                .and_then(|info| info.started_at.clone())
                .or_else(|| Some(chrono::Utc::now().to_rfc3339())),
            custom: worker_capabilities
                .custom_capabilities
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.to_string())))
                .collect(),
        };

        let worker = match Worker::create_or_update_by_worker_name(
            &self.db_pool,
            &worker_capabilities.worker_id,
            serde_json::to_value(worker_metadata)?,
        )
        .await
        {
            Ok(worker) => {
                info!(
                    "Worker persisted to database: {} (id: {})",
                    worker.worker_name, worker.worker_id
                );
                worker
            }
            Err(e) => {
                error!("Failed to persist worker to database: {}", e);
                return Ok(command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "DatabaseError".to_string(),
                        message: format!("Failed to persist worker to database: {}", e),
                        details: None,
                        retryable: true,
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                ));
            }
        };

        // 3. Create worker registration entry
        let connection_details = if let Some(conn_info) = &worker_capabilities.connection_info {
            serde_json::json!({
                "host": conn_info.host,
                "port": conn_info.port,
                "listener_port": conn_info.listener_port,
                "transport_type": conn_info.transport_type,
                "protocol_version": conn_info.protocol_version,
            })
        } else {
            // Fallback to defaults if no connection info provided
            warn!(
                "No connection info provided for worker {}, defaulting to localhost:8080 - worker may be unreachable",
                worker_capabilities.worker_id
            );
            serde_json::json!({
                "host": "localhost",
                "port": 8080,
                "listener_port": None::<u16>,
                "transport_type": "tcp",
            })
        };

        let connection_type = worker_capabilities
            .connection_info
            .as_ref()
            .map(|info| info.transport_type.clone())
            .unwrap_or_else(|| "tcp".to_string());

        let new_registration = NewWorkerRegistration {
            worker_id: worker.worker_id,
            status: WorkerStatus::Registered,
            connection_type,
            connection_details,
        };

        if let Err(e) = WorkerRegistration::register(&self.db_pool, new_registration).await {
            error!("Failed to create worker registration: {}", e);
            return Ok(command.create_response(
                CommandType::Error,
                CommandPayload::Error {
                    error_type: "DatabaseError".to_string(),
                    message: format!("Failed to create worker registration: {}", e),
                    details: None,
                    retryable: true,
                },
                CommandSource::RustServer {
                    id: "worker_management_handler".to_string(),
                },
            ));
        }

        // 4. Register supported tasks with database-first approach
        if let Some(ref supported_tasks) = worker_capabilities.supported_tasks {
            // NEW: Process explicit task registrations from supported_tasks
            if let Err(e) = self
                .register_worker_with_explicit_tasks(&worker, supported_tasks)
                .await
            {
                error!(
                    "Failed to register worker with explicit tasks: {}",
                    e
                );
                return Ok(command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "TaskRegistrationError".to_string(),
                        message: format!("Failed to register worker tasks: {}", e),
                        details: None,
                        retryable: true,
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                ));
            }
        } else {
            // FALLBACK: Legacy namespace-based registration for backward compatibility
            for namespace in &worker_capabilities.supported_namespaces {
                if let Err(e) = self
                    .register_worker_for_namespace_tasks(&worker, namespace)
                    .await
                {
                    warn!(
                        "Worker {} cannot register for namespace {} - tasks may go unprocessed: {}",
                        worker_capabilities.worker_id, namespace, e
                    );
                    // Continue with other namespaces - partial failure is acceptable
                }
            }
        }

        // 5. Create transport availability entry
        let (transport_type, transport_connection_details) = if let Some(conn_info) =
            &worker_capabilities.connection_info
        {
            let details = match conn_info.transport_type.as_str() {
                "tcp" => serde_json::json!({
                    "host": conn_info.host,
                    "port": conn_info.port,
                }),
                "unix" => serde_json::json!({
                    "socket_path": format!("/tmp/tasker_worker_{}.sock", worker_capabilities.worker_id),
                }),
                _ => {
                    debug!(
                        "Unknown transport type '{}', using TCP default configuration",
                        conn_info.transport_type
                    );
                    serde_json::json!({
                        "host": conn_info.host,
                        "port": conn_info.port,
                    })
                }
            };
            (conn_info.transport_type.clone(), details)
        } else {
            // Fallback to TCP defaults
            info!("No connection info provided, using default TCP transport configuration");
            (
                "tcp".to_string(),
                serde_json::json!({
                    "host": "localhost",
                    "port": 8080,
                }),
            )
        };

        let transport_availability = NewWorkerTransportAvailability {
            worker_id: worker.worker_id,
            core_instance_id: self.core_instance_id.clone(),
            transport_type,
            connection_details: transport_connection_details,
            is_reachable: true, // Assume reachable on registration
        };

        if let Err(e) =
            WorkerTransportAvailability::upsert(&self.db_pool, transport_availability).await
        {
            debug!("Transport availability update failed (non-critical): {}", e);
            // Transport availability is supplementary - continue with registration
        }

        info!(
            "Worker registration completed successfully: {}",
            worker_capabilities.worker_id
        );

        // Create success response
        let response = command.create_response(
            CommandType::WorkerRegistered,
            CommandPayload::WorkerRegistered {
                worker_id: worker_capabilities.worker_id.clone(),
                assigned_pool: "default".to_string(),
                queue_position: self
                    .get_queue_position(&worker_capabilities.worker_id)
                    .await,
            },
            CommandSource::RustServer {
                id: "worker_management_handler".to_string(),
            },
        );

        Ok(response)
    }

    /// Register worker for all named tasks in a namespace (temporary mapping)
    async fn register_worker_for_namespace_tasks(
        &self,
        worker: &Worker,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // First get the namespace by name
        let task_namespace = match TaskNamespace::find_by_name(&self.db_pool, namespace).await? {
            Some(ns) => ns,
            None => {
                // Create namespace if it doesn't exist
                TaskNamespace::find_or_create(&self.db_pool, namespace).await?
            }
        };

        // Find all named tasks in this namespace
        let named_tasks =
            NamedTask::list_by_namespace(&self.db_pool, task_namespace.task_namespace_id as i64)
                .await?;

        for named_task in named_tasks {
            // Create or update the association using the correct method signature
            if let Err(e) = WorkerNamedTask::create_or_update(
                &self.db_pool,
                worker.worker_id,
                named_task.named_task_id,
                serde_json::json!({}), // Default empty configuration
                Some(100),             // Default priority
            )
            .await
            {
                warn!(
                    "Worker {} cannot be associated with task {} - may not receive work: {}",
                    worker.worker_name, named_task.name, e
                );
            } else {
                debug!(
                    "Associated worker {} with task {} in namespace {}",
                    worker.worker_name, named_task.name, namespace
                );
            }
        }

        Ok(())
    }

    /// Register worker with explicit task definitions (database-first approach)
    async fn register_worker_with_explicit_tasks(
        &self,
        worker: &Worker,
        supported_tasks: &[crate::execution::command::TaskHandlerInfo],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Registering worker {} with {} explicit task definitions",
            worker.worker_name,
            supported_tasks.len()
        );

        for task_info in supported_tasks {
            // 1. Create or find namespace
            let task_namespace = match TaskNamespace::find_by_name(&self.db_pool, &task_info.namespace).await? {
                Some(ns) => {
                    debug!("Found existing namespace: {}", task_info.namespace);
                    ns
                }
                None => {
                    info!("Creating new namespace: {}", task_info.namespace);
                    TaskNamespace::find_or_create(&self.db_pool, &task_info.namespace).await?
                }
            };

            // 2. Create or update named task
            let named_task = match NamedTask::find_by_name_version_namespace(
                &self.db_pool,
                &task_info.handler_name,
                &task_info.version,
                task_namespace.task_namespace_id as i64,
            )
            .await?
            {
                Some(existing_task) => {
                    debug!(
                        "Found existing task: {}/{} v{}",
                        task_info.namespace, task_info.handler_name, task_info.version
                    );
                    existing_task
                }
                None => {
                    info!(
                        "Creating new task: {}/{} v{}",
                        task_info.namespace, task_info.handler_name, task_info.version
                    );
                    let new_named_task = crate::models::core::named_task::NewNamedTask {
                        name: task_info.handler_name.clone(),
                        version: Some(task_info.version.clone()),
                        description: task_info.description.clone()
                            .or_else(|| Some(format!("Auto-created task: {} v{}", task_info.handler_name, task_info.version))),
                        task_namespace_id: task_namespace.task_namespace_id as i64,
                        configuration: Some(serde_json::to_value(task_info)?),
                    };
                    NamedTask::create(&self.db_pool, new_named_task).await?
                }
            };

            // 3. Associate worker with the named task
            let worker_config = serde_json::to_value(&task_info.handler_config)?;
            let priority = task_info.priority.unwrap_or(100);

            if let Err(e) = WorkerNamedTask::create_or_update(
                &self.db_pool,
                worker.worker_id,
                named_task.named_task_id,
                worker_config,
                Some(priority),
            )
            .await
            {
                error!(
                    "Failed to associate worker {} with task {}/{} v{}: {}",
                    worker.worker_name, task_info.namespace, task_info.handler_name, task_info.version, e
                );
                return Err(Box::new(e));
            } else {
                info!(
                    "✅ Associated worker {} with task {}/{} v{} (priority: {})",
                    worker.worker_name, task_info.namespace, task_info.handler_name, task_info.version, priority
                );
            }
        }

        Ok(())
    }

    /// Handle worker unregistration command
    async fn handle_unregister_worker(
        &self,
        command: &Command,
        worker_id: String,
        reason: String,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!("Unregistering worker: {} (reason: {})", worker_id, reason);

        match self.worker_pool.unregister_worker(&worker_id).await {
            Ok(()) => {
                info!("Worker unregistered successfully: {}", worker_id);

                let response = command.create_response(
                    CommandType::WorkerUnregistered,
                    CommandPayload::WorkerUnregistered {
                        worker_id: worker_id.clone(),
                        unregistered_at: chrono::Utc::now().to_rfc3339(),
                        reason: reason.clone(),
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );

                Ok(response)
            }
            Err(e) => {
                error!("Worker unregistration failed: {}", e);

                let response = command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "WorkerUnregistrationError".to_string(),
                        message: format!("Failed to unregister worker {}: {}", worker_id, e),
                        details: None,
                        retryable: false, // Unregistration failures are typically not retryable
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );

                Ok(response)
            }
        }
    }

    /// Handle health check command
    async fn handle_health_check(
        &self,
        command: &Command,
        diagnostic_level: crate::execution::command::HealthCheckLevel,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Processing health check request with level: {:?}",
            diagnostic_level
        );

        // Get worker pool statistics and uptime info
        let pool_stats = self.worker_pool.get_stats().await;
        let uptime_info = self.get_uptime_info();

        let response_data = match diagnostic_level {
            crate::execution::command::HealthCheckLevel::Basic => {
                serde_json::json!({
                    "status": "healthy",
                    "total_workers": pool_stats.total_workers,
                    "healthy_workers": pool_stats.healthy_workers,
                    "current_load": pool_stats.current_load,
                    "uptime_seconds": uptime_info.uptime_seconds,
                    "uptime_duration": uptime_info.uptime_duration,
                })
            }
            crate::execution::command::HealthCheckLevel::Detailed => {
                serde_json::json!({
                    "status": "healthy",
                    "total_workers": pool_stats.total_workers,
                    "healthy_workers": pool_stats.healthy_workers,
                    "unhealthy_workers": pool_stats.unhealthy_workers,
                    "total_capacity": pool_stats.total_capacity,
                    "current_load": pool_stats.current_load,
                    "available_capacity": pool_stats.available_capacity,
                    "uptime_seconds": uptime_info.uptime_seconds,
                    "uptime_duration": uptime_info.uptime_duration,
                    "core_instance_id": self.core_instance_id,
                })
            }
            crate::execution::command::HealthCheckLevel::Full => {
                serde_json::json!({
                    "status": "healthy",
                    "total_workers": pool_stats.total_workers,
                    "healthy_workers": pool_stats.healthy_workers,
                    "unhealthy_workers": pool_stats.unhealthy_workers,
                    "total_capacity": pool_stats.total_capacity,
                    "current_load": pool_stats.current_load,
                    "available_capacity": pool_stats.available_capacity,
                    "total_steps_processed": pool_stats.total_steps_processed,
                    "successful_steps": pool_stats.successful_steps,
                    "failed_steps": pool_stats.failed_steps,
                    "namespace_distribution": pool_stats.namespace_distribution,
                    "uptime_info": uptime_info,
                    "core_instance_id": self.core_instance_id,
                })
            }
        };

        let response = command.create_response(
            CommandType::HealthCheckResult,
            CommandPayload::HealthCheckResult {
                status: "healthy".to_string(),
                uptime_seconds: self.get_uptime_seconds(),
                total_workers: pool_stats.total_workers,
                active_commands: pool_stats.current_load,
                diagnostics: Some(response_data),
            },
            CommandSource::RustServer {
                id: "worker_management_handler".to_string(),
            },
        );

        Ok(response)
    }

    /// Handle worker heartbeat command
    async fn handle_worker_heartbeat(
        &self,
        command: &Command,
        worker_id: String,
        current_load: usize,
        system_stats: Option<crate::execution::command::SystemStats>,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "Processing heartbeat for worker: {} (load: {})",
            worker_id, current_load
        );

        match self
            .worker_pool
            .update_worker_heartbeat(&worker_id, current_load)
            .await
        {
            Ok(()) => {
                debug!("Heartbeat processed successfully for worker: {}", worker_id);

                // Get current worker state for response
                let worker_state = self.worker_pool.get_worker(&worker_id).await;

                let response = command.create_response(
                    CommandType::HeartbeatAcknowledged,
                    CommandPayload::HeartbeatAcknowledged {
                        worker_id: worker_id.clone(),
                        acknowledged_at: chrono::Utc::now().to_rfc3339(),
                        status: "healthy".to_string(),
                        next_heartbeat_in: Some(30), // Suggest next heartbeat in 30 seconds
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );

                Ok(response)
            }
            Err(e) => {
                error!(
                    "Heartbeat processing failed for worker {}: {}",
                    worker_id, e
                );

                let response = command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "HeartbeatError".to_string(),
                        message: format!(
                            "Failed to process heartbeat for worker {}: {}",
                            worker_id, e
                        ),
                        details: None,
                        retryable: true, // Heartbeat failures are typically retryable
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );

                Ok(response)
            }
        }
    }

    /// Get queue position for a worker (simplified implementation)
    async fn get_queue_position(&self, _worker_id: &str) -> usize {
        let stats = self.worker_pool.get_stats().await;
        // Simple queue position based on registration order
        // In a more sophisticated implementation, this could be based on
        // priority, capabilities, or other factors
        stats.total_workers
    }

    /// Get comprehensive system status including uptime and worker statistics
    pub async fn get_system_status(&self) -> SystemStatus {
        let pool_stats = self.worker_pool.get_stats().await;
        let uptime_info = self.get_uptime_info();

        SystemStatus {
            status: "healthy".to_string(),
            uptime_info,
            core_instance_id: self.core_instance_id.clone(),
            worker_statistics: WorkerStatistics {
                total_workers: pool_stats.total_workers,
                healthy_workers: pool_stats.healthy_workers,
                unhealthy_workers: pool_stats.unhealthy_workers,
                total_capacity: pool_stats.total_capacity,
                current_load: pool_stats.current_load,
                available_capacity: pool_stats.available_capacity,
            },
        }
    }

    /// Check if the system has been running for more than a specified duration
    pub fn has_been_running_for(&self, duration_seconds: u64) -> bool {
        self.get_uptime_seconds() >= duration_seconds
    }

    /// **NEW**: Get optimal worker for task using optimized queries with caching
    /// 
    /// This method demonstrates the performance improvements of our optimized
    /// database queries compared to the original implementation.
    pub async fn get_optimal_worker_for_task(
        &self,
        named_task_id: i32,
        required_capacity: Option<i32>,
    ) -> Result<Option<crate::database::OptimalWorkerResult>, Box<dyn std::error::Error + Send + Sync>> {
        let capacity = required_capacity.unwrap_or(1);
        
        info!(
            "🔍 Selecting optimal worker for task {} with capacity requirement {}",
            named_task_id, capacity
        );

        match self
            .optimized_queries
            .select_optimal_worker_for_task(named_task_id, capacity)
            .await
        {
            Ok(Some(optimal_worker)) => {
                info!(
                    "✅ Found optimal worker: {} (health: {:.1}, load: {}/{}, score: calculated)",
                    optimal_worker.worker_name,
                    optimal_worker.health_score,
                    optimal_worker.current_load,
                    optimal_worker.max_concurrent_steps
                );
                Ok(Some(optimal_worker))
            }
            Ok(None) => {
                warn!(
                    "⚠️ No optimal worker found for task {} with capacity {}",
                    named_task_id, capacity
                );
                Ok(None)
            }
            Err(e) => {
                error!(
                    "❌ Failed to select optimal worker for task {}: {}",
                    named_task_id, e
                );
                Err(Box::new(e))
            }
        }
    }

    /// **NEW**: Invalidate worker caches when worker state changes
    /// 
    /// Call this method when workers register, unregister, or change health status
    /// to ensure cache consistency.
    pub async fn invalidate_worker_caches(&self, task_id: Option<i32>) -> Result<bool, sqlx::Error> {
        info!("🧹 Invalidating worker caches for task: {:?}", task_id);
        self.optimized_queries.invalidate_worker_caches(task_id).await
    }

    /// **NEW**: Get database performance metrics
    /// 
    /// Returns connection pool statistics for monitoring and alerting
    pub async fn get_database_performance_metrics(&self) -> Result<crate::database::PoolStatistics, sqlx::Error> {
        self.optimized_queries.get_pool_statistics().await
    }
}

#[async_trait]
impl CommandHandler for WorkerManagementHandler {
    async fn handle_command(
        &self,
        command: Command,
    ) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "WorkerManagementHandler processing command: {:?}",
            command.command_type
        );

        let response = match &command.payload {
            CommandPayload::RegisterWorker {
                worker_capabilities,
            } => {
                self.handle_register_worker(&command, worker_capabilities.clone())
                    .await?
            }

            CommandPayload::UnregisterWorker { worker_id, reason } => {
                self.handle_unregister_worker(&command, worker_id.clone(), reason.clone())
                    .await?
            }

            CommandPayload::WorkerHeartbeat {
                worker_id,
                current_load,
                system_stats,
            } => {
                self.handle_worker_heartbeat(
                    &command,
                    worker_id.clone(),
                    *current_load,
                    system_stats.clone(),
                )
                .await?
            }

            CommandPayload::HealthCheck { diagnostic_level } => {
                self.handle_health_check(&command, diagnostic_level.clone())
                    .await?
            }

            _ => {
                error!(
                    "WorkerManagementHandler received unsupported command: {:?}",
                    command.command_type
                );

                command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "UnsupportedCommand".to_string(),
                        message: format!(
                            "WorkerManagementHandler does not support command type: {:?}",
                            command.command_type
                        ),
                        details: None,
                        retryable: false,
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                )
            }
        };

        Ok(Some(response))
    }

    fn handler_name(&self) -> &str {
        "WorkerManagementHandler"
    }

    fn supported_commands(&self) -> Vec<CommandType> {
        vec![
            CommandType::RegisterWorker,
            CommandType::UnregisterWorker,
            CommandType::WorkerHeartbeat,
        ]
    }
}

// Note: Integration tests for WorkerManagementHandler are located in tests/
// directory using #[sqlx::test] for proper database testing.
