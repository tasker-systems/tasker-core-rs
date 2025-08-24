//! Pool of WorkerExecutors for a specific namespace
//! Mirrors OrchestratorExecutorPool patterns

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use tasker_shared::{
    database::Connection,
    messaging::UnifiedPgmqClient,
    registry::{HandlerRegistry, TaskTemplateRegistry},
    types::QueueMessageData,
};

use crate::{
    config::{ExecutorPoolConfig, QueueConfig},
    error::{Result, WorkerError},
    event_publisher::EventPublisher,
    worker::{executor::WorkerExecutor, coordinator::NamespaceMetrics},
};

/// Pool of WorkerExecutors for a specific namespace
/// Mirrors OrchestratorExecutorPool patterns
pub struct WorkerExecutorPool {
    namespace: String,
    messaging_client: Arc<UnifiedPgmqClient>,
    database_connection: Arc<Connection>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    handler_registry: Arc<HandlerRegistry>,
    event_publisher: Arc<EventPublisher>,

    // Pool of WorkerExecutors
    executors: Arc<RwLock<Vec<Arc<WorkerExecutor>>>>,
    executor_handles: Arc<RwLock<Vec<JoinHandle<()>>>>,

    // Pool configuration
    initial_pool_size: usize,
    max_pool_size: usize,
    current_pool_size: Arc<RwLock<usize>>,

    // Queue configuration
    queue_name: String,
    visibility_timeout: u32,
    batch_size: u32,
    polling_interval_ms: u64,

    // Scaling configuration
    scale_up_threshold: f64,
    scale_down_threshold: f64,
    scale_up_increment: usize,
    scale_down_increment: usize,
    cooldown_seconds: u64,
    last_scale_time: Arc<RwLock<std::time::Instant>>,
}

impl WorkerExecutorPool {
    pub async fn new(
        namespace: String,
        messaging_client: Arc<UnifiedPgmqClient>,
        database_connection: Arc<Connection>,
        task_template_registry: Arc<TaskTemplateRegistry>,
        handler_registry: Arc<HandlerRegistry>,
        event_publisher: Arc<EventPublisher>,
        pool_config: &ExecutorPoolConfig,
        queue_config: &QueueConfig,
    ) -> Result<Self> {
        let queue_name = format!("{}_queue", namespace);

        // Create initial WorkerExecutors
        let mut executors = Vec::new();
        for i in 0..pool_config.initial_pool_size {
            let executor = Arc::new(
                WorkerExecutor::new(
                    format!("{}_executor_{}", namespace, i),
                    namespace.clone(),
                    messaging_client.clone(),
                    database_connection.clone(),
                    task_template_registry.clone(),
                    handler_registry.clone(),
                    event_publisher.clone(),
                )
                .await?,
            );
            executors.push(executor);
        }

        let current_pool_size = Arc::new(RwLock::new(pool_config.initial_pool_size));

        Ok(Self {
            namespace,
            messaging_client,
            database_connection,
            task_template_registry,
            handler_registry,
            event_publisher,
            executors: Arc::new(RwLock::new(executors)),
            executor_handles: Arc::new(RwLock::new(Vec::new())),
            initial_pool_size: pool_config.initial_pool_size,
            max_pool_size: pool_config.max_pool_size,
            current_pool_size,
            queue_name,
            visibility_timeout: queue_config.visibility_timeout_seconds,
            batch_size: queue_config.batch_size,
            polling_interval_ms: queue_config.polling_interval_ms,
            scale_up_threshold: pool_config.scale_up_threshold,
            scale_down_threshold: pool_config.scale_down_threshold,
            scale_up_increment: pool_config.scale_up_increment,
            scale_down_increment: pool_config.scale_down_increment,
            cooldown_seconds: pool_config.cooldown_seconds,
            last_scale_time: Arc::new(RwLock::new(std::time::Instant::now())),
        })
    }

    pub async fn start(&self) -> Result<()> {
        info!(
            "🔄 Starting WorkerExecutorPool for namespace: {}",
            self.namespace
        );

        // Start each WorkerExecutor in the pool
        let executors = self.executors.read().await;
        let mut handles = self.executor_handles.write().await;

        for executor in executors.iter() {
            let handle = self.spawn_executor_loop(executor.clone()).await;
            handles.push(handle);
        }

        info!(
            "✅ WorkerExecutorPool started for namespace: {} with {} executors",
            self.namespace,
            executors.len()
        );
        Ok(())
    }

    async fn spawn_executor_loop(&self, executor: Arc<WorkerExecutor>) -> JoinHandle<()> {
        let queue_name = self.queue_name.clone();
        let messaging_client = self.messaging_client.clone();
        let visibility_timeout = self.visibility_timeout;
        let batch_size = self.batch_size;
        let polling_interval = self.polling_interval_ms;
        let namespace = self.namespace.clone();

        tokio::spawn(async move {
            info!("🎯 Starting executor loop for queue: {}", queue_name);

            loop {
                // Check if executor is shutting down
                if executor.is_shutting_down().await {
                    info!("Executor shutting down for queue: {}", queue_name);
                    break;
                }

                // Listen on namespaced queue for step messages
                match messaging_client
                    .read_step_messages(&namespace, visibility_timeout, batch_size)
                    .await
                {
                    Ok(messages) => {
                        if !messages.is_empty() {
                            info!(
                                "📨 Received {} messages for queue: {}",
                                messages.len(),
                                queue_name
                            );
                        }

                        for message_data in messages {
                            if let Err(e) = executor.process_step_message(message_data).await {
                                error!(
                                    "WorkerExecutor failed to process message in {}: {}",
                                    queue_name, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to read messages from queue {}: {}", queue_name, e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }

                // Brief pause to prevent tight polling
                tokio::time::sleep(tokio::time::Duration::from_millis(polling_interval)).await;
            }

            info!("✅ Executor loop finished for queue: {}", queue_name);
        })
    }

    pub async fn scale_up(&self) -> Result<()> {
        // Check cooldown period
        {
            let last_scale = *self.last_scale_time.read().await;
            if last_scale.elapsed().as_secs() < self.cooldown_seconds {
                return Ok(()); // Still in cooldown
            }
        }

        let current_size = *self.current_pool_size.read().await;
        if current_size >= self.max_pool_size {
            warn!(
                "Cannot scale up namespace {}: already at max size {}",
                self.namespace, self.max_pool_size
            );
            return Ok(());
        }

        let new_size = std::cmp::min(
            current_size + self.scale_up_increment,
            self.max_pool_size,
        );
        let executors_to_add = new_size - current_size;

        info!(
            "📈 Scaling up WorkerExecutorPool for namespace: {} ({} -> {}, adding {})",
            self.namespace, current_size, new_size, executors_to_add
        );

        // Add new executors
        let mut executors = self.executors.write().await;
        let mut handles = self.executor_handles.write().await;

        for i in 0..executors_to_add {
            let executor_id = format!("{}_executor_{}", self.namespace, current_size + i);
            let executor = Arc::new(
                WorkerExecutor::new(
                    executor_id,
                    self.namespace.clone(),
                    self.messaging_client.clone(),
                    self.database_connection.clone(),
                    self.task_template_registry.clone(),
                    self.handler_registry.clone(),
                    self.event_publisher.clone(),
                )
                .await?,
            );

            // Start the new executor
            let handle = self.spawn_executor_loop(executor.clone()).await;
            handles.push(handle);
            executors.push(executor);
        }

        // Update pool size and scale time
        {
            let mut pool_size = self.current_pool_size.write().await;
            *pool_size = new_size;
        }
        {
            let mut last_scale = self.last_scale_time.write().await;
            *last_scale = std::time::Instant::now();
        }

        info!(
            "✅ Scaled up WorkerExecutorPool for namespace: {} to {} executors",
            self.namespace, new_size
        );

        Ok(())
    }

    pub async fn scale_down(&self) -> Result<()> {
        // Check cooldown period
        {
            let last_scale = *self.last_scale_time.read().await;
            if last_scale.elapsed().as_secs() < self.cooldown_seconds {
                return Ok(()); // Still in cooldown
            }
        }

        let current_size = *self.current_pool_size.read().await;
        if current_size <= self.initial_pool_size {
            return Ok(()); // Don't scale below initial size
        }

        let new_size = std::cmp::max(
            current_size.saturating_sub(self.scale_down_increment),
            self.initial_pool_size,
        );
        let executors_to_remove = current_size - new_size;

        if executors_to_remove == 0 {
            return Ok(());
        }

        info!(
            "📉 Scaling down WorkerExecutorPool for namespace: {} ({} -> {}, removing {})",
            self.namespace, current_size, new_size, executors_to_remove
        );

        // Signal shutdown to excess executors
        {
            let executors = self.executors.read().await;
            for i in new_size..current_size {
                if let Some(executor) = executors.get(i) {
                    executor.initiate_graceful_shutdown().await?;
                }
            }
        }

        // Remove executors after they finish current work
        // Note: In a full implementation, we'd wait for graceful shutdown
        {
            let mut executors = self.executors.write().await;
            executors.truncate(new_size);
        }

        // Update pool size and scale time
        {
            let mut pool_size = self.current_pool_size.write().await;
            *pool_size = new_size;
        }
        {
            let mut last_scale = self.last_scale_time.write().await;
            *last_scale = std::time::Instant::now();
        }

        info!(
            "✅ Scaled down WorkerExecutorPool for namespace: {} to {} executors",
            self.namespace, new_size
        );

        Ok(())
    }

    pub async fn is_idle(&self) -> bool {
        let executors = self.executors.read().await;
        for executor in executors.iter() {
            if !executor.is_idle().await {
                return false;
            }
        }
        true
    }

    pub async fn current_size(&self) -> usize {
        *self.current_pool_size.read().await
    }

    pub async fn initiate_graceful_shutdown(&self) -> Result<()> {
        info!(
            "🛑 Initiating graceful shutdown for WorkerExecutorPool: {}",
            self.namespace
        );

        // Signal shutdown to all WorkerExecutors
        let executors = self.executors.read().await;
        for executor in executors.iter() {
            executor.initiate_graceful_shutdown().await?;
        }

        Ok(())
    }

    pub async fn wait_for_shutdown(&self) -> Result<()> {
        info!(
            "⏳ Waiting for WorkerExecutorPool shutdown: {}",
            self.namespace
        );

        // Wait for all executor handles to complete
        let mut handles = self.executor_handles.write().await;
        while let Some(handle) = handles.pop() {
            let _ = handle.await;
        }

        info!(
            "✅ WorkerExecutorPool shutdown complete: {}",
            self.namespace
        );
        Ok(())
    }

    pub async fn get_metrics(&self) -> Result<NamespaceMetrics> {
        // Get queue depth
        let queue_depth = self
            .messaging_client
            .get_queue_depth(&self.queue_name)
            .await
            .unwrap_or(0);

        // Calculate processing rate (mock for now)
        let processing_rate = 25.0; // TODO: Calculate actual processing rate

        // Get error rate (mock for now)
        let error_rate = 0.01; // TODO: Calculate actual error rate

        // Get active executors
        let active_executors = self.current_size().await;

        Ok(NamespaceMetrics {
            namespace: self.namespace.clone(),
            queue_depth,
            processing_rate,
            error_rate,
            active_executors,
        })
    }
}