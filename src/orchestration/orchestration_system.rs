//! # Orchestration System (pgmq-based)
//!
//! Main orchestration system using the OrchestrationLoop for individual step processing.
//! Implements priority fairness task claiming and autonomous queue-based step execution.

use crate::error::{Result, TaskerError};
use crate::messaging::PgmqClient;
use crate::orchestration::{
    orchestration_loop::{
        ContinuousOrchestrationSummary, OrchestrationCycleResult, OrchestrationLoop,
        OrchestrationLoopConfig,
    },
    task_request_processor::TaskRequestProcessor,
    TaskInitializer,
};
use crate::registry::TaskHandlerRegistry;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, warn};

/// Configuration for the orchestration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSystemConfig {
    /// Queue name for task requests
    pub task_requests_queue_name: String,
    /// Orchestrator instance identifier
    pub orchestrator_id: String,
    /// Orchestration loop configuration
    pub orchestration_loop_config: OrchestrationLoopConfig,
    /// Task request processor polling interval in milliseconds
    pub task_request_polling_interval_ms: u64,
    /// Visibility timeout for task request messages (seconds)
    pub task_request_visibility_timeout_seconds: i32,
    /// Number of task requests to process per batch
    pub task_request_batch_size: i32,
    /// Namespaces to create queues for
    pub active_namespaces: Vec<String>,
    /// Maximum concurrent orchestration loops
    pub max_concurrent_orchestrators: usize,
    /// Enable comprehensive performance logging
    pub enable_performance_logging: bool,
}

impl Default for OrchestrationSystemConfig {
    fn default() -> Self {
        use std::time::SystemTime;

        // Generate a simple orchestrator ID using timestamp
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        Self {
            task_requests_queue_name: "task_requests_queue".to_string(),
            orchestrator_id: format!("orchestrator-{timestamp}"),
            orchestration_loop_config: OrchestrationLoopConfig::default(),
            task_request_polling_interval_ms: 250, // 250ms = 4x/sec default
            task_request_visibility_timeout_seconds: 300, // 5 minutes
            task_request_batch_size: 10,
            active_namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
                "analytics".to_string(),
            ],
            max_concurrent_orchestrators: 3,
            enable_performance_logging: false,
        }
    }
}

impl OrchestrationSystemConfig {
    /// Create OrchestrationSystemConfig from ConfigManager
    pub fn from_config_manager(config_manager: &crate::config::ConfigManager) -> Self {
        use std::time::SystemTime;

        let config = config_manager.config();

        // Generate a simple orchestrator ID using timestamp
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        Self {
            task_requests_queue_name: config.orchestration.task_requests_queue_name.clone(),
            orchestrator_id: format!("orchestrator-{timestamp}"),
            orchestration_loop_config: OrchestrationLoopConfig::from_config_manager(config_manager),
            task_request_polling_interval_ms: config.orchestration.task_request_polling_interval_ms,
            task_request_visibility_timeout_seconds: config
                .orchestration
                .task_request_visibility_timeout_seconds
                as i32,
            task_request_batch_size: config.orchestration.task_request_batch_size as i32,
            active_namespaces: config.orchestration.active_namespaces.clone(),
            max_concurrent_orchestrators: config.orchestration.max_concurrent_orchestrators
                as usize,
            enable_performance_logging: config.orchestration.enable_performance_logging,
        }
    }
}

/// Main orchestration system using OrchestrationLoop for individual step processing
pub struct OrchestrationSystem {
    /// PostgreSQL message queue client
    pgmq_client: Arc<PgmqClient>,
    /// Task request processor
    task_request_processor: Arc<TaskRequestProcessor>,
    /// Main orchestration loop
    orchestration_loop: Arc<OrchestrationLoop>,
    /// Task handler registry
    task_handler_registry: Arc<TaskHandlerRegistry>,
    /// Database connection pool
    pool: PgPool,
    /// Configuration
    config: OrchestrationSystemConfig,
}

impl OrchestrationSystem {
    /// Create a new orchestration system
    pub async fn new(
        pgmq_client: Arc<PgmqClient>,
        task_request_processor: Arc<TaskRequestProcessor>,
        task_handler_registry: Arc<TaskHandlerRegistry>,
        pool: PgPool,
        config: OrchestrationSystemConfig,
    ) -> Result<Self> {
        // Create orchestration loop with the configured pgmq client
        let orchestration_loop = Arc::new(
            OrchestrationLoop::with_config(
                pool.clone(),
                (*pgmq_client).clone(),
                config.orchestrator_id.clone(),
                config.orchestration_loop_config.clone(),
            )
            .await?,
        );

        Ok(Self {
            pgmq_client,
            task_request_processor,
            orchestration_loop,
            task_handler_registry,
            pool,
            config,
        })
    }

    /// Bootstrap orchestration system from YAML configuration
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_core::orchestration::{OrchestrationSystem, config::ConfigurationManager};
    /// use sqlx::PgPool;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Load configuration from YAML
    ///     let config_manager = ConfigurationManager::load_from_file("config/tasker-config.yaml").await?;
    ///     let pool = PgPool::connect("postgresql://localhost/tasker").await?;
    ///     
    ///     // Bootstrap orchestration system
    ///     let orchestration_system = OrchestrationSystem::from_config(
    ///         config_manager,
    ///         pool,
    ///     ).await?;
    ///     
    ///     // Start orchestration
    ///     orchestration_system.start().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_config(
        config_manager: crate::orchestration::config::ConfigurationManager,
        pool: PgPool,
    ) -> Result<Self> {
        let tasker_config = config_manager.system_config();
        let orchestration_system_config =
            tasker_config.orchestration.to_orchestration_system_config();

        // Create pgmq client
        let pgmq_client = Arc::new(PgmqClient::new_with_pool(pool.clone()).await);

        // Create task handler registry
        let task_handler_registry = Arc::new(TaskHandlerRegistry::new(pool.clone()));

        // Create task initializer
        let task_initializer = Arc::new(TaskInitializer::new(pool.clone()));

        // Create task request processor
        let task_request_processor = Arc::new(TaskRequestProcessor::new(
            pgmq_client.clone(),
            task_handler_registry.clone(),
            task_initializer,
            crate::orchestration::task_request_processor::TaskRequestProcessorConfig::default(),
        ));

        Self::new(
            pgmq_client,
            task_request_processor,
            task_handler_registry,
            pool,
            orchestration_system_config,
        )
        .await
    }

    /// Bootstrap orchestration system from YAML configuration file path
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_core::orchestration::OrchestrationSystem;
    /// use sqlx::PgPool;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let pool = PgPool::connect("postgresql://localhost/tasker").await?;
    ///     
    ///     // Bootstrap directly from config file
    ///     let orchestration_system = OrchestrationSystem::from_config_file(
    ///         "config/tasker-config.yaml",
    ///         pool,
    ///     ).await?;
    ///     
    ///     // Start orchestration
    ///     orchestration_system.start().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_config_file<P: AsRef<std::path::Path> + std::fmt::Debug>(
        config_path: P,
        pool: PgPool,
    ) -> Result<Self> {
        let config_manager =
            crate::orchestration::config::ConfigurationManager::load_from_file(config_path)
                .await
                .map_err(|e| {
                    TaskerError::ConfigurationError(format!("Failed to load config: {e}"))
                })?;

        Self::from_config(config_manager, pool).await
    }

    /// Start the complete orchestration system with OrchestrationLoop
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<ContinuousOrchestrationSummary> {
        info!(
            orchestrator_id = %self.config.orchestrator_id,
            "🚀 Starting pgmq-based orchestration system with OrchestrationLoop"
        );

        // Initialize all required queues
        self.initialize_queues().await?;

        // Start task request processing, orchestration, and step results processing loops concurrently
        let task_request_processor = self.clone_for_task_request_processing();
        let orchestration_loop = self.orchestration_loop.clone();
        let step_result_processor = self.orchestration_loop.step_result_processor().clone();

        let task_request_handle = tokio::spawn(async move {
            task_request_processor
                .start_task_request_processing_loop()
                .await
        });

        let orchestration_handle =
            tokio::spawn(async move { orchestration_loop.run_continuous().await });

        let step_results_handle =
            tokio::spawn(async move { step_result_processor.start_processing_loop().await });

        // Wait for all three loops - in practice, orchestration should run indefinitely
        let (task_request_result, orchestration_result, step_results_result) = tokio::join!(
            task_request_handle,
            orchestration_handle,
            step_results_handle
        );

        // Log any errors that caused the loops to exit
        if let Err(e) = task_request_result {
            error!(error = %e, "Task request processing loop panicked");
        }

        if let Err(e) = step_results_result {
            error!(error = %e, "Step results processing loop panicked");
        }

        match orchestration_result {
            Ok(Ok(summary)) => {
                info!(
                    total_cycles = summary.total_cycles,
                    total_tasks_processed = summary.total_tasks_processed,
                    total_steps_enqueued = summary.total_steps_enqueued,
                    success_rate = summary.success_rate_percentage(),
                    "Orchestration system completed"
                );
                Ok(summary)
            }
            Ok(Err(e)) => {
                error!(error = %e, "Orchestration loop failed");
                Err(e)
            }
            Err(e) => {
                error!(error = %e, "Orchestration loop panicked");
                Err(TaskerError::OrchestrationError(format!(
                    "Orchestration loop panicked: {e}"
                )))
            }
        }
    }

    /// Start the task request processing loop
    #[instrument(skip(self))]
    async fn start_task_request_processing_loop(&self) -> Result<()> {
        info!(
            queue = %self.config.task_requests_queue_name,
            polling_interval_ms = %self.config.task_request_polling_interval_ms,
            "Starting task request processing loop"
        );

        loop {
            match self.process_task_request_batch().await {
                Ok(processed_count) => {
                    if processed_count == 0 {
                        // No task requests processed, wait before polling again
                        sleep(Duration::from_millis(
                            self.config.task_request_polling_interval_ms,
                        ))
                        .await;
                    }
                    // If we processed requests, continue immediately for better throughput
                }
                Err(e) => {
                    error!(error = %e, "Error in task request processing batch");
                    // Wait before retrying on error
                    sleep(Duration::from_millis(
                        self.config.task_request_polling_interval_ms,
                    ))
                    .await;
                }
            }
        }
    }

    /// Run a single orchestration cycle (for testing or controlled execution)
    #[instrument(skip(self))]
    pub async fn run_single_cycle(&self) -> Result<OrchestrationCycleResult> {
        info!(
            orchestrator_id = %self.config.orchestrator_id,
            "Running single orchestration cycle"
        );

        self.orchestration_loop.run_cycle().await
    }

    /// Process a batch of task request messages
    #[instrument(skip(self))]
    async fn process_task_request_batch(&self) -> Result<usize> {
        // Read messages from the task requests queue
        let messages = self
            .pgmq_client
            .read_messages(
                &self.config.task_requests_queue_name,
                Some(self.config.task_request_visibility_timeout_seconds),
                Some(self.config.task_request_batch_size),
            )
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to read task request messages: {e}"))
            })?;

        if messages.is_empty() {
            return Ok(0);
        }

        let message_count = messages.len();
        debug!(
            message_count = message_count,
            queue = %self.config.task_requests_queue_name,
            "Processing batch of task request messages"
        );

        let mut processed_count = 0;

        for message in messages {
            match self
                .process_single_task_request(&message.message, message.msg_id)
                .await
            {
                Ok(()) => {
                    // Delete the successfully processed message
                    if let Err(e) = self
                        .pgmq_client
                        .delete_message(&self.config.task_requests_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %e,
                            "Failed to delete processed task request message"
                        );
                    } else {
                        processed_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        msg_id = message.msg_id,
                        error = %e,
                        "Failed to process task request message"
                    );

                    // Archive failed messages
                    if let Err(archive_err) = self
                        .pgmq_client
                        .archive_message(&self.config.task_requests_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %archive_err,
                            "Failed to archive failed task request message"
                        );
                    }
                }
            }
        }

        if processed_count > 0 {
            info!(
                processed_count = processed_count,
                total_messages = message_count,
                "Completed task request processing batch"
            );
        }

        Ok(processed_count)
    }

    /// Process a single task request message (simplified for OrchestrationLoop approach)
    #[instrument(skip(self, payload))]
    async fn process_single_task_request(
        &self,
        payload: &serde_json::Value,
        msg_id: i64,
    ) -> Result<()> {
        info!(msg_id = msg_id, "Processing task request message");

        // Use task request processor to handle the request
        // This will create a task in the database, making it available for the OrchestrationLoop to claim
        match self
            .task_request_processor
            .process_task_request(payload)
            .await
        {
            Ok(task_id) => {
                info!(
                    task_id = task_id,
                    msg_id = msg_id,
                    "Task request processed successfully - task created and available for claiming"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    msg_id = msg_id,
                    error = %e,
                    "Failed to process task request"
                );
                Err(TaskerError::OrchestrationError(format!(
                    "Task request processing failed: {e}"
                )))
            }
        }
    }

    /// Initialize all required queues for OrchestrationLoop approach
    async fn initialize_queues(&self) -> Result<()> {
        info!("Initializing orchestration queues for OrchestrationLoop");

        // Create task requests queue
        if let Err(e) = self
            .pgmq_client
            .create_queue(&self.config.task_requests_queue_name)
            .await
        {
            debug!(
                queue = %self.config.task_requests_queue_name,
                error = %e,
                "Task requests queue may already exist"
            );
        }

        // Create step results queue
        let step_results_queue = &self
            .config
            .orchestration_loop_config
            .step_result_processor_config
            .step_results_queue_name;
        if let Err(e) = self.pgmq_client.create_queue(step_results_queue).await {
            debug!(
                queue = %step_results_queue,
                error = %e,
                "Step results queue may already exist"
            );
        }

        // Create namespace-specific step queues (used by OrchestrationLoop)
        for namespace in &self.config.active_namespaces {
            let step_queue_name = format!("{namespace}_queue");
            if let Err(e) = self.pgmq_client.create_queue(&step_queue_name).await {
                debug!(
                    queue = %step_queue_name,
                    error = %e,
                    "Step queue may already exist"
                );
            }
        }

        info!(
            task_requests_queue = %self.config.task_requests_queue_name,
            step_results_queue = %step_results_queue,
            namespace_step_queues = self.config.active_namespaces.len(),
            "All orchestration queues initialized"
        );

        Ok(())
    }

    /// Get orchestration statistics for OrchestrationLoop approach
    pub async fn get_statistics(&self) -> Result<OrchestrationStats> {
        // Get task requests queue statistics
        let task_requests_queue_size = match self
            .pgmq_client
            .queue_metrics(&self.config.task_requests_queue_name)
            .await
        {
            Ok(metrics) => metrics.message_count,
            Err(e) => {
                warn!(
                    queue = %self.config.task_requests_queue_name,
                    error = %e,
                    "Failed to get task requests queue metrics"
                );
                -1 // Indicate unavailable
            }
        };

        // Get namespace-specific step queue statistics
        let mut namespace_queue_sizes = Vec::new();
        for namespace in &self.config.active_namespaces {
            let queue_name = format!("{namespace}_queue");
            let queue_size = match self.pgmq_client.queue_metrics(&queue_name).await {
                Ok(metrics) => metrics.message_count,
                Err(e) => {
                    warn!(
                        queue = %queue_name,
                        error = %e,
                        "Failed to get namespace queue metrics"
                    );
                    -1 // Indicate unavailable
                }
            };
            namespace_queue_sizes.push((queue_name, queue_size));
        }

        Ok(OrchestrationStats {
            task_requests_queue_size,
            namespace_queue_sizes,
            active_namespaces: self.config.active_namespaces.clone(),
            orchestrator_id: self.config.orchestrator_id.clone(),
            orchestration_loop_config: self.config.orchestration_loop_config.clone(),
        })
    }

    /// Clone for task request processing (to avoid Arc<Arc<>> issues)
    fn clone_for_task_request_processing(&self) -> OrchestrationSystem {
        OrchestrationSystem {
            pgmq_client: self.pgmq_client.clone(),
            task_request_processor: self.task_request_processor.clone(),
            orchestration_loop: self.orchestration_loop.clone(),
            task_handler_registry: self.task_handler_registry.clone(),
            pool: self.pool.clone(),
            config: self.config.clone(),
        }
    }
}

/// Statistics for the orchestration system using OrchestrationLoop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationStats {
    /// Task requests queue size (pending task creation requests)
    pub task_requests_queue_size: i64,
    /// Namespace-specific step queue sizes: (queue_name, queue_length)
    pub namespace_queue_sizes: Vec<(String, i64)>,
    /// Active namespaces in the system
    pub active_namespaces: Vec<String>,
    /// Orchestrator instance ID
    pub orchestrator_id: String,
    /// Current orchestration loop configuration
    pub orchestration_loop_config: OrchestrationLoopConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = OrchestrationSystemConfig::default();

        assert_eq!(config.task_requests_queue_name, "task_requests_queue");
        assert!(config.orchestrator_id.starts_with("orchestrator-"));
        assert_eq!(config.task_request_polling_interval_ms, 250);
        assert_eq!(config.task_request_visibility_timeout_seconds, 300);
        assert_eq!(config.task_request_batch_size, 10);
        assert_eq!(config.max_concurrent_orchestrators, 3);
        assert!(!config.enable_performance_logging);

        // Verify default namespaces
        let expected_namespaces = vec![
            "fulfillment",
            "inventory",
            "notifications",
            "payments",
            "analytics",
        ];
        assert_eq!(config.active_namespaces.len(), expected_namespaces.len());
        for namespace in expected_namespaces {
            assert!(config.active_namespaces.contains(&namespace.to_string()));
        }

        // Verify orchestration loop config defaults
        assert_eq!(config.orchestration_loop_config.tasks_per_cycle, 5);
        assert_eq!(config.orchestration_loop_config.cycle_interval.as_secs(), 1);
    }

    #[test]
    fn test_config_customization() {
        let config = OrchestrationSystemConfig {
            task_requests_queue_name: "custom_task_requests".to_string(),
            orchestrator_id: "custom-orchestrator-123".to_string(),
            orchestration_loop_config: OrchestrationLoopConfig {
                tasks_per_cycle: 20,
                cycle_interval: Duration::from_secs(5),
                ..OrchestrationLoopConfig::default()
            },
            task_request_polling_interval_ms: 500, // 500ms for test
            task_request_visibility_timeout_seconds: 600,
            task_request_batch_size: 25,
            active_namespaces: vec!["custom".to_string(), "test".to_string()],
            max_concurrent_orchestrators: 5,
            enable_performance_logging: true,
        };

        assert_eq!(config.task_requests_queue_name, "custom_task_requests");
        assert_eq!(config.orchestrator_id, "custom-orchestrator-123");
        assert_eq!(config.task_request_polling_interval_ms, 500);
        assert_eq!(config.task_request_visibility_timeout_seconds, 600);
        assert_eq!(config.task_request_batch_size, 25);
        assert_eq!(config.max_concurrent_orchestrators, 5);
        assert!(config.enable_performance_logging);
        assert_eq!(config.active_namespaces, vec!["custom", "test"]);
        assert_eq!(config.orchestration_loop_config.tasks_per_cycle, 20);
        assert_eq!(config.orchestration_loop_config.cycle_interval.as_secs(), 5);
    }
}
