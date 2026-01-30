//! # Task Request Hydrator
//!
//! Hydrates task request messages from PGMQ messages.
//!
//! ## Purpose
//!
//! External clients submit TaskRequestMessage to the orchestration_task_requests queue.
//! This hydrator parses and validates these messages for task initialization.
//!
//! ## Process
//!
//! 1. Parse TaskRequestMessage from PGMQ message payload
//! 2. Validate message format
//! 3. Return hydrated TaskRequestMessage for processing
//!
//! Unlike StepResultHydrator, this hydrator doesn't need database lookup because
//! the message itself contains all required data for task initialization.

use pgmq::Message as PgmqMessage;
use tasker_shared::messaging::service::QueuedMessage;
use tasker_shared::messaging::TaskRequestMessage;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{debug, error, info};

/// Hydrates TaskRequestMessage from PGMQ messages
///
/// This service performs message parsing and validation, converting PGMQ messages
/// into TaskRequestMessage for orchestration processing.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::orchestration::hydration::TaskRequestHydrator;
///
/// # async fn example(message: pgmq::Message) -> tasker_shared::TaskerResult<()> {
/// let hydrator = TaskRequestHydrator::new();
/// let task_request = hydrator.hydrate_from_message(&message).await?;
/// // task_request is now ready for task initialization
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct TaskRequestHydrator;

impl TaskRequestHydrator {
    /// Create a new TaskRequestHydrator
    pub fn new() -> Self {
        Self
    }

    /// Hydrate TaskRequestMessage from PGMQ message
    ///
    /// Performs message parsing and validation:
    /// 1. Parse TaskRequestMessage from message payload
    /// 2. Validate message format
    /// 3. Return hydrated message
    ///
    /// # Arguments
    ///
    /// * `message` - PGMQ message containing TaskRequestMessage payload
    ///
    /// # Returns
    ///
    /// Fully hydrated `TaskRequestMessage` ready for task initialization
    ///
    /// # Errors
    ///
    /// - `ValidationError`: Invalid message format or missing data
    pub async fn hydrate_from_message(
        &self,
        message: &PgmqMessage,
    ) -> TaskerResult<TaskRequestMessage> {
        debug!(
            msg_id = message.msg_id,
            message_size = message.message.to_string().len(),
            "HYDRATOR: Starting task request hydration"
        );

        // Parse the task request message
        let task_request: TaskRequestMessage = serde_json::from_value(message.message.clone())
            .map_err(|e| {
                error!(
                    msg_id = message.msg_id,
                    error = %e,
                    message_content = %message.message,
                    "HYDRATOR: Failed to parse task request message"
                );
                TaskerError::ValidationError(format!("Invalid task request message format: {e}"))
            })?;

        info!(
            msg_id = message.msg_id,
            namespace = %task_request.task_request.namespace,
            handler_name = %task_request.task_request.name,
            "HYDRATOR: Successfully parsed TaskRequestMessage"
        );

        Ok(task_request)
    }

    /// TAS-133: Hydrate TaskRequestMessage from provider-agnostic QueuedMessage
    ///
    /// This is the provider-agnostic version of hydrate_from_message, working with
    /// `QueuedMessage<serde_json::Value>` instead of PGMQ-specific `PgmqMessage`.
    pub async fn hydrate_from_queued_message(
        &self,
        message: &QueuedMessage<serde_json::Value>,
    ) -> TaskerResult<TaskRequestMessage> {
        debug!(
            handle = ?message.handle,
            message_size = message.message.to_string().len(),
            "HYDRATOR: Starting task request hydration from QueuedMessage"
        );

        // Parse the task request message
        let task_request: TaskRequestMessage = serde_json::from_value(message.message.clone())
            .map_err(|e| {
                error!(
                    handle = ?message.handle,
                    error = %e,
                    message_content = %message.message,
                    "HYDRATOR: Failed to parse task request message"
                );
                TaskerError::ValidationError(format!("Invalid task request message format: {e}"))
            })?;

        info!(
            handle = ?message.handle,
            namespace = %task_request.task_request.namespace,
            handler_name = %task_request.task_request.name,
            "HYDRATOR: Successfully parsed TaskRequestMessage from QueuedMessage"
        );

        Ok(task_request)
    }
}

impl Default for TaskRequestHydrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use tasker_shared::messaging::service::{MessageHandle, MessageMetadata};
    use tasker_shared::models::core::task_request::TaskRequest;

    fn create_pgmq_message(payload: serde_json::Value) -> PgmqMessage {
        PgmqMessage {
            msg_id: 1,
            message: payload,
            vt: Utc::now(),
            read_ct: 1,
            enqueued_at: Utc::now(),
        }
    }

    fn create_queued_message(payload: serde_json::Value) -> QueuedMessage<serde_json::Value> {
        QueuedMessage::with_handle(
            payload,
            MessageHandle::Pgmq {
                msg_id: 1,
                queue_name: "test_task_requests".to_string(),
            },
            MessageMetadata::new(1, Utc::now()),
        )
    }

    fn create_valid_task_request_json() -> serde_json::Value {
        let task_request = TaskRequest::new("process_order".to_string(), "fulfillment".to_string())
            .with_version("1.0.0".to_string())
            .with_context(json!({"order_id": 12345}))
            .with_initiator("api_gateway".to_string())
            .with_source_system("test".to_string())
            .with_reason("Test hydration".to_string());

        let request = TaskRequestMessage::new(task_request, "test_requester".to_string());
        serde_json::to_value(&request).expect("Failed to serialize TaskRequestMessage")
    }

    // --- Construction and trait tests ---

    #[test]
    fn test_task_request_hydrator_construction() {
        let _hydrator = TaskRequestHydrator::new();
        let _hydrator = TaskRequestHydrator;
    }

    #[test]
    fn test_default_impl() {
        let hydrator = TaskRequestHydrator::default();
        let debug_str = format!("{:?}", hydrator);
        assert_eq!(debug_str, "TaskRequestHydrator");
    }

    // --- hydrate_from_message tests (PgmqMessage) ---

    #[tokio::test]
    async fn test_hydrate_from_message_valid_task_request() {
        let hydrator = TaskRequestHydrator::new();
        let payload = create_valid_task_request_json();
        let message = create_pgmq_message(payload);

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_ok());

        let task_request_msg = result.unwrap();
        assert_eq!(task_request_msg.task_request.name, "process_order");
        assert_eq!(task_request_msg.task_request.namespace, "fulfillment");
        assert_eq!(task_request_msg.task_request.version, "1.0.0");
        assert_eq!(task_request_msg.metadata.requester, "test_requester");
    }

    #[tokio::test]
    async fn test_hydrate_from_message_invalid_json_format() {
        let hydrator = TaskRequestHydrator::new();
        let message = create_pgmq_message(json!({"not": "a task request"}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Invalid task request message"),
            "Error should indicate invalid format: {err}"
        );
    }

    #[tokio::test]
    async fn test_hydrate_from_message_empty_object() {
        let hydrator = TaskRequestHydrator::new();
        let message = create_pgmq_message(json!({}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_message_string_payload() {
        let hydrator = TaskRequestHydrator::new();
        let message = create_pgmq_message(json!("just a string"));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_message_null_payload() {
        let hydrator = TaskRequestHydrator::new();
        let message = create_pgmq_message(json!(null));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_message_preserves_context() {
        let hydrator = TaskRequestHydrator::new();
        let payload = create_valid_task_request_json();
        let message = create_pgmq_message(payload);

        let result = hydrator.hydrate_from_message(&message).await.unwrap();
        assert_eq!(result.task_request.context["order_id"], 12345);
    }

    // --- hydrate_from_queued_message tests (QueuedMessage) ---

    #[tokio::test]
    async fn test_hydrate_from_queued_message_valid() {
        let hydrator = TaskRequestHydrator::new();
        let payload = create_valid_task_request_json();
        let message = create_queued_message(payload);

        let result = hydrator.hydrate_from_queued_message(&message).await;
        assert!(result.is_ok());

        let task_request_msg = result.unwrap();
        assert_eq!(task_request_msg.task_request.name, "process_order");
        assert_eq!(task_request_msg.task_request.namespace, "fulfillment");
    }

    #[tokio::test]
    async fn test_hydrate_from_queued_message_invalid_format() {
        let hydrator = TaskRequestHydrator::new();
        let message = create_queued_message(json!({"invalid": true}));

        let result = hydrator.hydrate_from_queued_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_queued_message_empty_object() {
        let hydrator = TaskRequestHydrator::new();
        let message = create_queued_message(json!({}));

        let result = hydrator.hydrate_from_queued_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_preserves_metadata() {
        let hydrator = TaskRequestHydrator::new();
        let payload = create_valid_task_request_json();
        let message = create_queued_message(payload);

        let result = hydrator
            .hydrate_from_queued_message(&message)
            .await
            .unwrap();
        assert_eq!(result.metadata.requester, "test_requester");
        assert!(!result.request_id.is_empty());
    }
}
