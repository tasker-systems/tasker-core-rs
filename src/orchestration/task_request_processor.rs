//! # Task Request Processor
//!
//! Processes task requests from the orchestration_task_requests queue.
//! Validates requests, creates tasks using existing models, and enqueues
//! validated tasks for orchestration processing.

use crate::error::{Result, TaskerError};
use crate::messaging::{PgmqClient, TaskProcessingMessage, TaskRequestMessage};
use crate::models::core::{
    named_task::NamedTask,
    task::{NewTask, Task},
    task_namespace::TaskNamespace,
};
use crate::orchestration::task_initializer::TaskInitializer;
use crate::registry::TaskHandlerRegistry;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

/// Configuration for task request processing
#[derive(Debug, Clone)]
pub struct TaskRequestProcessorConfig {
    /// Queue name to poll for task requests
    pub request_queue_name: String,
    /// Queue name to send validated tasks for processing
    pub processing_queue_name: String,
    /// Number of messages to read per batch
    pub batch_size: i32,
    /// Visibility timeout for messages (seconds)
    pub visibility_timeout_seconds: i32,
    /// Polling interval when no messages (seconds)
    pub polling_interval_seconds: u64,
    /// Maximum processing attempts before giving up
    pub max_processing_attempts: i32,
}

impl Default for TaskRequestProcessorConfig {
    fn default() -> Self {
        Self {
            request_queue_name: "orchestration_task_requests".to_string(),
            processing_queue_name: "orchestration_tasks_to_be_processed".to_string(),
            batch_size: 10,
            visibility_timeout_seconds: 300, // 5 minutes
            polling_interval_seconds: 1,
            max_processing_attempts: 3,
        }
    }
}

/// Processes task requests and validates them for orchestration
pub struct TaskRequestProcessor {
    /// PostgreSQL message queue client
    pgmq_client: Arc<PgmqClient>,
    /// Task handler registry for validation
    task_handler_registry: Arc<TaskHandlerRegistry>,
    /// Task initializer for creating tasks
    task_initializer: Arc<TaskInitializer>,
    /// Database connection pool
    pool: PgPool,
    /// Configuration
    config: TaskRequestProcessorConfig,
}

impl TaskRequestProcessor {
    /// Create a new task request processor
    pub fn new(
        pgmq_client: Arc<PgmqClient>,
        task_handler_registry: Arc<TaskHandlerRegistry>,
        task_initializer: Arc<TaskInitializer>,
        pool: PgPool,
        config: TaskRequestProcessorConfig,
    ) -> Self {
        Self {
            pgmq_client,
            task_handler_registry,
            task_initializer,
            pool,
            config,
        }
    }

    /// Start the task request processing loop
    #[instrument(skip(self))]
    pub async fn start_processing_loop(&self) -> Result<()> {
        info!(
            request_queue = %self.config.request_queue_name,
            processing_queue = %self.config.processing_queue_name,
            "Starting task request processing loop"
        );

        // Ensure queues exist
        self.ensure_queues_exist().await?;

        loop {
            match self.process_batch().await {
                Ok(processed_count) => {
                    if processed_count == 0 {
                        // No messages processed, wait before polling again
                        tokio::time::sleep(Duration::from_secs(
                            self.config.polling_interval_seconds,
                        ))
                        .await;
                    }
                    // If we processed messages, continue immediately for better throughput
                }
                Err(e) => {
                    error!(error = %e, "Error in task request processing batch");
                    // Wait before retrying on error
                    tokio::time::sleep(Duration::from_secs(self.config.polling_interval_seconds))
                        .await;
                }
            }
        }
    }

    /// Process a batch of task request messages
    #[instrument(skip(self))]
    async fn process_batch(&self) -> Result<usize> {
        // Read messages from the request queue
        let messages = self
            .pgmq_client
            .read_messages(
                &self.config.request_queue_name,
                Some(self.config.visibility_timeout_seconds),
                Some(self.config.batch_size),
            )
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to read task request messages: {}", e))
            })?;

        if messages.is_empty() {
            return Ok(0);
        }

        let message_count = messages.len();

        debug!(
            message_count = message_count,
            queue = %self.config.request_queue_name,
            "Processing batch of task request messages"
        );

        let mut processed_count = 0;

        for message in messages {
            match self
                .process_single_request(&message.message, message.msg_id)
                .await
            {
                Ok(()) => {
                    // Delete the successfully processed message
                    if let Err(e) = self
                        .pgmq_client
                        .delete_message(&self.config.request_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %e,
                            "Failed to delete processed message"
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

                    // Archive malformed or repeatedly failing messages
                    if let Err(archive_err) = self
                        .pgmq_client
                        .archive_message(&self.config.request_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %archive_err,
                            "Failed to archive failed message"
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

    /// Process a single task request message
    #[instrument(skip(self, payload))]
    async fn process_single_request(&self, payload: &serde_json::Value, msg_id: i64) -> Result<()> {
        // Parse the task request message
        let request: TaskRequestMessage = serde_json::from_value(payload.clone()).map_err(|e| {
            TaskerError::ValidationError(format!("Invalid task request message format: {}", e))
        })?;

        info!(
            request_id = %request.request_id,
            namespace = %request.namespace,
            task_name = %request.task_name,
            task_version = %request.task_version,
            msg_id = msg_id,
            "Processing task request"
        );

        // Validate the task using the task handler registry
        match self.validate_task_request(&request).await {
            Ok(()) => {
                // Create the task using existing models
                let task = self.create_task_from_request(&request).await?;

                // Create task processing message
                let processing_message = TaskProcessingMessage::new(
                    task.task_id,
                    request.namespace.clone(),
                    request.task_name.clone(),
                    request.task_version.clone(),
                    request.request_id.clone(),
                    request.metadata.priority.clone(),
                );

                // Enqueue for orchestration processing
                self.pgmq_client
                    .send_json_message(&self.config.processing_queue_name, &processing_message)
                    .await
                    .map_err(|e| {
                        TaskerError::MessagingError(format!(
                            "Failed to enqueue task for processing: {}",
                            e
                        ))
                    })?;

                info!(
                    request_id = %request.request_id,
                    task_id = task.task_id,
                    processing_queue = %self.config.processing_queue_name,
                    "Task validated and enqueued for processing"
                );

                Ok(())
            }
            Err(validation_error) => {
                warn!(
                    request_id = %request.request_id,
                    namespace = %request.namespace,
                    task_name = %request.task_name,
                    error = %validation_error,
                    "Task request validation failed"
                );
                Err(validation_error)
            }
        }
    }

    /// Validate a task request using the task handler registry
    async fn validate_task_request(&self, request: &TaskRequestMessage) -> Result<()> {
        debug!(
            namespace = %request.namespace,
            task_name = %request.task_name,
            task_version = %request.task_version,
            "Validating task request"
        );

        // Use the task handler registry to validate the task exists and is configured
        match self
            .task_handler_registry
            .get_task_template(
                &request.namespace,
                &request.task_name,
                &request.task_version,
            )
            .await
        {
            Ok(_template) => {
                debug!(
                    namespace = %request.namespace,
                    task_name = %request.task_name,
                    "Task request validation successful"
                );
                Ok(())
            }
            Err(e) => Err(TaskerError::ValidationError(format!(
                "Task validation failed for {}/{}/{}: {}",
                request.namespace, request.task_name, request.task_version, e
            ))),
        }
    }

    /// Create a task from a validated request
    async fn create_task_from_request(&self, request: &TaskRequestMessage) -> Result<Task> {
        debug!(
            request_id = %request.request_id,
            namespace = %request.namespace,
            task_name = %request.task_name,
            "Creating task from request"
        );

        // Find the task namespace
        let task_namespace = TaskNamespace::find_by_name(&self.pool, &request.namespace)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to query namespace: {}", e)))?
            .ok_or_else(|| {
                TaskerError::ValidationError(format!("Namespace not found: {}", request.namespace))
            })?;

        // Find the named task
        let named_task = NamedTask::find_latest_by_name_namespace(
            &self.pool,
            &request.task_name,
            task_namespace.task_namespace_id as i64,
        )
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to query named task: {}", e)))?
        .ok_or_else(|| {
            TaskerError::ValidationError(format!(
                "Task not found: {}/{}",
                request.namespace, request.task_name
            ))
        })?;

        // Create identity hash for the task (using simple hash for now)
        let identity_hash = format!(
            "{}-{}-{}-{}",
            request.namespace,
            request.task_name,
            request.request_id,
            Utc::now().timestamp_millis()
        );

        // Create the task
        let new_task = NewTask {
            named_task_id: named_task.named_task_id,
            requested_at: Some(request.metadata.requested_at.naive_utc()),
            initiator: Some(request.metadata.requester.clone()),
            source_system: Some("task_request_processor".to_string()),
            reason: Some(format!("Task request {}", request.request_id)),
            bypass_steps: None,
            tags: Some(serde_json::json!({
                "priority": request.metadata.priority,
                "created_by_processor": true
            })),
            context: Some(request.input_data.clone()),
            identity_hash,
        };

        let task = Task::create(&self.pool, new_task)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to create task: {}", e)))?;

        // The task is created and will be initialized by the orchestration system
        // when it processes the task. We don't need to initialize it here since
        // initialization happens during workflow setup.

        info!(
            request_id = %request.request_id,
            task_id = task.task_id,
            namespace = %request.namespace,
            task_name = %request.task_name,
            "Task created successfully"
        );

        Ok(task)
    }

    /// Ensure required queues exist
    async fn ensure_queues_exist(&self) -> Result<()> {
        info!("Ensuring orchestration queues exist");

        // Create request queue if it doesn't exist
        if let Err(e) = self
            .pgmq_client
            .create_queue(&self.config.request_queue_name)
            .await
        {
            debug!(
                queue = %self.config.request_queue_name,
                error = %e,
                "Queue may already exist"
            );
        }

        // Create processing queue if it doesn't exist
        if let Err(e) = self
            .pgmq_client
            .create_queue(&self.config.processing_queue_name)
            .await
        {
            debug!(
                queue = %self.config.processing_queue_name,
                error = %e,
                "Queue may already exist"
            );
        }

        Ok(())
    }

    /// Get processing statistics
    pub async fn get_statistics(&self) -> Result<TaskRequestProcessorStats> {
        // For now, return basic stats without queue sizes since the method doesn't exist yet
        // TODO: Implement queue_size method in PgmqClient
        Ok(TaskRequestProcessorStats {
            request_queue_size: -1,    // Not available yet
            processing_queue_size: -1, // Not available yet
            request_queue_name: self.config.request_queue_name.clone(),
            processing_queue_name: self.config.processing_queue_name.clone(),
        })
    }
}

/// Statistics for task request processing
#[derive(Debug, Clone)]
pub struct TaskRequestProcessorStats {
    pub request_queue_size: i64,
    pub processing_queue_size: i64,
    pub request_queue_name: String,
    pub processing_queue_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_config_defaults() {
        let config = TaskRequestProcessorConfig::default();
        assert_eq!(config.request_queue_name, "orchestration_task_requests");
        assert_eq!(
            config.processing_queue_name,
            "orchestration_tasks_to_be_processed"
        );
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.visibility_timeout_seconds, 300);
    }

    #[test]
    fn test_task_request_message_parsing() {
        let request = TaskRequestMessage::new(
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            json!({"order_id": 12345}),
            "api_gateway".to_string(),
        );

        let serialized = serde_json::to_value(&request).unwrap();
        let parsed: TaskRequestMessage = serde_json::from_value(serialized).unwrap();

        assert_eq!(parsed.namespace, "fulfillment");
        assert_eq!(parsed.task_name, "process_order");
        assert_eq!(parsed.task_version, "1.0.0");
        assert_eq!(parsed.metadata.requester, "api_gateway");
    }
}
