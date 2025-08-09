//! # Task Request Processor
//!
//! Processes task requests from the orchestration_task_requests queue.
//! Validates requests, creates tasks using existing models, and enqueues
//! validated tasks for orchestration processing.

use crate::error::{Result, TaskerError};
use crate::messaging::{PgmqClient, PgmqClientTrait, UnifiedPgmqClient, TaskRequestMessage};
use crate::models::core::task_request::TaskRequest;
use crate::orchestration::task_initializer::TaskInitializer;
use crate::registry::TaskHandlerRegistry;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

/// Configuration for task request processing
#[derive(Debug, Clone)]
pub struct TaskRequestProcessorConfig {
    /// Queue name to poll for task requests
    pub request_queue_name: String,
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
            batch_size: 10,
            visibility_timeout_seconds: 300, // 5 minutes
            polling_interval_seconds: 1,
            max_processing_attempts: 3,
        }
    }
}

/// Processes task requests and validates them for orchestration
pub struct TaskRequestProcessor {
    /// PostgreSQL message queue client (unified for circuit breaker flexibility)
    pgmq_client: Arc<UnifiedPgmqClient>,
    /// Task handler registry for validation
    task_handler_registry: Arc<TaskHandlerRegistry>,
    /// Task initializer for creating tasks
    task_initializer: Arc<TaskInitializer>,
    /// Configuration
    config: TaskRequestProcessorConfig,
}

impl TaskRequestProcessor {
    /// Create a new task request processor with unified client
    pub fn new(
        pgmq_client: Arc<UnifiedPgmqClient>,
        task_handler_registry: Arc<TaskHandlerRegistry>,
        task_initializer: Arc<TaskInitializer>,
        config: TaskRequestProcessorConfig,
    ) -> Self {
        Self {
            pgmq_client,
            task_handler_registry,
            task_initializer,
            config,
        }
    }

    /// Start the task request processing loop
    #[instrument(skip(self))]
    pub async fn start_processing_loop(&self) -> Result<()> {
        info!(
            request_queue = %self.config.request_queue_name,
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
                TaskerError::MessagingError(format!("Failed to read task request messages: {e}"))
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
            TaskerError::ValidationError(format!("Invalid task request message format: {e}"))
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
            Ok(()) => self.handle_valid_task_request(&request).await,
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

    /// Handle a validated task request by creating task (ready for SQL-based discovery)
    async fn handle_valid_task_request(&self, request: &TaskRequestMessage) -> Result<()> {
        // Convert TaskRequestMessage to TaskRequest for proper initialization
        let task_request = self.convert_message_to_task_request(request)?;

        // Create the task using the proper TaskInitializer with full workflow setup
        let initialization_result = self
            .task_initializer
            .create_task_from_request(task_request)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Task initialization failed: {e}"))
            })?;

        info!(
            request_id = %request.request_id,
            task_id = initialization_result.task_id,
            namespace = %request.namespace,
            task_name = %request.task_name,
            step_count = initialization_result.step_count,
            "Task validated and created - ready for SQL-based discovery"
        );

        Ok(())
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

    /// Convert TaskRequestMessage to TaskRequest for proper initialization
    fn convert_message_to_task_request(&self, request: &TaskRequestMessage) -> Result<TaskRequest> {
        debug!(
            request_id = %request.request_id,
            namespace = %request.namespace,
            task_name = %request.task_name,
            "Converting message to TaskRequest for proper initialization"
        );

        // Convert the messaging format to the core model format
        let task_request = TaskRequest::new(request.task_name.clone(), request.namespace.clone())
            .with_version(request.task_version.clone())
            .with_context(request.input_data.clone())
            .with_initiator(request.metadata.requester.clone())
            .with_source_system("task_request_processor".to_string())
            .with_reason(format!("Task request {}", request.request_id))
            .with_priority(request.metadata.priority.into())
            .with_claim_timeout_seconds(300); // Default 5 minutes for task claims

        info!(
            request_id = %request.request_id,
            namespace = %request.namespace,
            task_name = %request.task_name,
            "TaskRequest converted successfully for proper initialization"
        );

        Ok(task_request)
    }

    /// Process a task request directly using TaskInitializer (bypassing message queues)
    /// This is the preferred method for direct task creation with proper initialization
    #[instrument(skip(self))]
    pub async fn process_task_request(&self, payload: &serde_json::Value) -> Result<i64> {
        // Parse the task request message
        let request: TaskRequestMessage = serde_json::from_value(payload.clone()).map_err(|e| {
            TaskerError::ValidationError(format!("Invalid task request message format: {e}"))
        })?;

        info!(
            request_id = %request.request_id,
            namespace = %request.namespace,
            task_name = %request.task_name,
            "Processing task request directly with proper initialization"
        );

        // Validate the task using the task handler registry
        self.validate_task_request(&request).await?;

        // Convert TaskRequestMessage to TaskRequest for proper initialization
        let task_request = self.convert_message_to_task_request(&request)?;

        // Create the task using the proper TaskInitializer with full workflow setup
        let initialization_result = self
            .task_initializer
            .create_task_from_request(task_request)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Task initialization failed: {e}"))
            })?;

        info!(
            request_id = %request.request_id,
            task_id = initialization_result.task_id,
            step_count = initialization_result.step_count,
            handler_config = ?initialization_result.handler_config_name,
            "Task initialized successfully with proper workflow setup"
        );

        Ok(initialization_result.task_id)
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

        Ok(())
    }

    /// Get processing statistics
    pub async fn get_statistics(&self) -> Result<TaskRequestProcessorStats> {
        // For now, return basic stats without queue sizes since the method doesn't exist yet
        // TODO: Implement queue_size method in PgmqClient
        Ok(TaskRequestProcessorStats {
            request_queue_size: -1, // Not available yet
            request_queue_name: self.config.request_queue_name.clone(),
        })
    }
}

/// Statistics for task request processing
#[derive(Debug, Clone)]
pub struct TaskRequestProcessorStats {
    pub request_queue_size: i64,
    pub request_queue_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_config_defaults() {
        let config = TaskRequestProcessorConfig::default();
        assert_eq!(config.request_queue_name, "orchestration_task_requests");
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
