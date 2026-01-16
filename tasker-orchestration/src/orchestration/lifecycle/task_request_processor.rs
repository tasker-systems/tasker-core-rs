//! # Task Request Processor
//!
//! Processes task requests from the orchestration_task_requests queue.
//! Validates requests, creates tasks using existing models, and enqueues
//! validated tasks for orchestration processing.

use crate::orchestration::lifecycle::task_initialization::TaskInitializer;
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::messaging::client::MessageClient;
use tasker_shared::messaging::orchestration_messages::TaskRequestMessage;
use tasker_shared::messaging::service::QueuedMessage;
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

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

/// Processes task requests and validates them for orchestration (TAS-133e)
#[derive(Debug)]
pub struct TaskRequestProcessor {
    /// Message client for queue operations (TAS-133e: provider-agnostic)
    message_client: Arc<MessageClient>,
    /// Task handler registry for validation
    task_handler_registry: Arc<TaskHandlerRegistry>,
    /// Task initializer for creating tasks
    task_initializer: Arc<TaskInitializer>,
    /// Configuration
    config: TaskRequestProcessorConfig,
}

impl TaskRequestProcessor {
    /// Create a new task request processor with message client
    pub fn new(
        message_client: Arc<MessageClient>,
        task_handler_registry: Arc<TaskHandlerRegistry>,
        task_initializer: Arc<TaskInitializer>,
        config: TaskRequestProcessorConfig,
    ) -> Self {
        Self {
            message_client,
            task_handler_registry,
            task_initializer,
            config,
        }
    }

    /// Process a batch of task request messages (TAS-133e)
    #[instrument(skip(self))]
    pub async fn process_batch(&self) -> TaskerResult<usize> {
        // Read messages from the request queue
        let visibility_timeout = Duration::from_secs(self.config.visibility_timeout_seconds as u64);
        let messages: Vec<QueuedMessage<TaskRequestMessage>> = self
            .message_client
            .receive_messages(
                &self.config.request_queue_name,
                self.config.batch_size as usize,
                visibility_timeout,
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
            let msg_id = message.receipt_handle.as_str();
            match self.process_single_request(&message.message).await {
                Ok(()) => {
                    // Ack the successfully processed message (TAS-133e)
                    if let Err(e) = self
                        .message_client
                        .ack_message(&self.config.request_queue_name, &message.receipt_handle)
                        .await
                    {
                        warn!(
                            msg_id = %msg_id,
                            error = %e,
                            "Failed to ack processed message"
                        );
                    } else {
                        processed_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        msg_id = %msg_id,
                        error = %e,
                        "Failed to process task request message"
                    );

                    // Nack malformed or repeatedly failing messages without requeue (TAS-133e)
                    if let Err(nack_err) = self
                        .message_client
                        .nack_message(
                            &self.config.request_queue_name,
                            &message.receipt_handle,
                            false,
                        )
                        .await
                    {
                        warn!(
                            msg_id = %msg_id,
                            error = %nack_err,
                            "Failed to nack failed message"
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

    /// Process a single task request message (TAS-133e: now receives typed message)
    #[instrument(skip(self, request))]
    async fn process_single_request(&self, request: &TaskRequestMessage) -> TaskerResult<()> {
        info!(
            request_id = %request.request_id,
            namespace = %request.task_request.namespace,
            task_name = %request.task_request.name,
            task_version = %request.task_request.version,
            "Processing task request"
        );

        // Validate the task using the task handler registry
        match self.validate_task_request(request).await {
            Ok(()) => self.handle_valid_task_request(request).await,
            Err(validation_error) => {
                warn!(
                    request_id = %request.request_id,
                    namespace = %request.task_request.namespace,
                    task_name = %request.task_request.name,
                    error = %validation_error,
                    "Task request validation failed"
                );
                Err(validation_error)
            }
        }
    }

    /// Handle a validated task request by creating task with immediate step enqueuing
    async fn handle_valid_task_request(&self, request: &TaskRequestMessage) -> TaskerResult<()> {
        // Use the embedded TaskRequest directly - no conversion needed
        // Now using create_and_enqueue_task_from_request for immediate step enqueuing
        let initialization_result = self
            .task_initializer
            .create_and_enqueue_task_from_request(request.task_request.clone())
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Task initialization failed: {e}"))
            })?;

        info!(
            request_id = %request.request_id,
            task_uuid = %initialization_result.task_uuid,
            namespace = %request.task_request.namespace,
            task_name = %request.task_request.name,
            step_count = initialization_result.step_count,
            "Task validated, created, and steps immediately enqueued"
        );

        Ok(())
    }

    /// Validate a task request using the task handler registry
    async fn validate_task_request(&self, request: &TaskRequestMessage) -> TaskerResult<()> {
        debug!(
            namespace = %request.task_request.namespace,
            task_name = %request.task_request.name,
            task_version = %request.task_request.version,
            "Validating task request"
        );

        // Use the task handler registry to validate the task exists and is configured
        match self
            .task_handler_registry
            .get_task_template(
                &request.task_request.namespace,
                &request.task_request.name,
                &request.task_request.version,
            )
            .await
        {
            Ok(_template) => {
                debug!(
                    namespace = %request.task_request.namespace,
                    task_name = %request.task_request.name,
                    "Task request validation successful"
                );
                Ok(())
            }
            Err(e) => Err(TaskerError::ValidationError(format!(
                "Task validation failed for {}/{}/{}: {}",
                request.task_request.namespace,
                request.task_request.name,
                request.task_request.version,
                e
            ))),
        }
    }

    /// Process a task request directly using TaskInitializer (bypassing message queues)
    /// This is the preferred method for direct task creation with proper initialization
    #[instrument(skip(self))]
    pub async fn process_task_request(&self, payload: &serde_json::Value) -> TaskerResult<Uuid> {
        // Parse the task request message
        let request: TaskRequestMessage = serde_json::from_value(payload.clone()).map_err(|e| {
            TaskerError::ValidationError(format!("Invalid task request message format: {e}"))
        })?;

        info!(
            request_id = %request.request_id,
            namespace = %request.task_request.namespace,
            task_name = %request.task_request.name,
            "Processing task request directly with proper initialization"
        );

        // Validate the task using the task handler registry
        self.validate_task_request(&request).await?;

        // Use the embedded TaskRequest directly - no conversion needed
        // Now using create_and_enqueue_task_from_request for immediate step enqueuing
        let initialization_result = self
            .task_initializer
            .create_and_enqueue_task_from_request(request.task_request.clone())
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Task initialization failed: {e}"))
            })?;

        info!(
            request_id = %request.request_id,
            task_uuid = %initialization_result.task_uuid,
            step_count = initialization_result.step_count,
            handler_config = ?initialization_result.handler_config_name,
            "Task initialized successfully with proper workflow setup"
        );

        Ok(initialization_result.task_uuid)
    }

    /// Get processing statistics
    ///
    /// TAS-142: Implement real queue size metrics
    /// TAS-133e: Updated to use MessageClient.get_queue_stats
    pub async fn get_statistics(&self) -> TaskerResult<TaskRequestProcessorStats> {
        // Query actual queue depth using MessageClient.get_queue_stats
        let request_queue_size = self
            .message_client
            .get_queue_stats(&self.config.request_queue_name)
            .await
            .map(|stats| stats.message_count as i64)
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    queue_name = %self.config.request_queue_name,
                    "Failed to get queue stats, returning -1"
                );
                -1
            });

        Ok(TaskRequestProcessorStats {
            request_queue_size,
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
        use tasker_shared::models::core::task_request::TaskRequest;

        let task_request = TaskRequest::new("process_order".to_string(), "fulfillment".to_string())
            .with_version("1.0.0".to_string())
            .with_context(json!({"order_id": 12345}))
            .with_initiator("api_gateway".to_string())
            .with_source_system("test".to_string())
            .with_reason("Test parsing".to_string());

        let request = TaskRequestMessage::new(task_request, "api_gateway".to_string());

        let serialized = serde_json::to_value(&request).unwrap();
        let parsed: TaskRequestMessage = serde_json::from_value(serialized).unwrap();

        assert_eq!(parsed.task_request.namespace, "fulfillment");
        assert_eq!(parsed.task_request.name, "process_order");
        assert_eq!(parsed.task_request.version, "1.0.0");
        assert_eq!(parsed.metadata.requester, "api_gateway");
    }
}
