//! # Task Request Processor
//!
//! Processes task requests from the orchestration_task_requests queue.
//! Validates requests, creates tasks using existing models, and enqueues
//! validated tasks for orchestration processing.

use crate::orchestration::lifecycle::task_initializer::TaskInitializer;
use std::sync::Arc;
use tasker_shared::messaging::{PgmqClientTrait, TaskRequestMessage, UnifiedPgmqClient};
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

    /// Process a batch of task request messages
    #[instrument(skip(self))]
    pub async fn process_batch(&self) -> TaskerResult<usize> {
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
    async fn process_single_request(
        &self,
        payload: &serde_json::Value,
        msg_id: i64,
    ) -> TaskerResult<()> {
        // Parse the task request message
        let request: TaskRequestMessage = serde_json::from_value(payload.clone()).map_err(|e| {
            TaskerError::ValidationError(format!("Invalid task request message format: {e}"))
        })?;

        info!(
            request_id = %request.request_id,
            namespace = %request.task_request.namespace,
            task_name = %request.task_request.name,
            task_version = %request.task_request.version,
            msg_id = msg_id,
            "Processing task request"
        );

        // Validate the task using the task handler registry
        match self.validate_task_request(&request).await {
            Ok(()) => self.handle_valid_task_request(&request).await,
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
    pub async fn get_statistics(&self) -> TaskerResult<TaskRequestProcessorStats> {
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
