//! # MessageClient Domain Facade (TAS-133c)
//!
//! Domain-level messaging client that provides convenient, Tasker-specific
//! messaging methods. Wraps `MessagingProvider` (enum) and `MessageRouterKind`
//! (enum) - no trait objects, all enum dispatch.
//!
//! ## Design
//!
//! This is a **struct**, not a trait. The struct pattern is simpler and the
//! trait was only used polymorphically in one place (which has been refactored).
//!
//! ```text
//! MessageClient
//!   ├── provider: Arc<MessagingProvider>  <- Actual messaging backend
//!   └── router: MessageRouterKind         <- Queue name resolution
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use tasker_shared::messaging::client::MessageClient;
//! use tasker_shared::messaging::service::{MessagingProvider, MessageRouterKind};
//!
//! // Create client with provider and router
//! let provider = Arc::new(MessagingProvider::new_in_memory());
//! let router = MessageRouterKind::default();
//! let client = MessageClient::new(provider, router);
//!
//! // Send step message to worker queue
//! let msg = StepMessage::new(task_uuid, step_uuid, correlation_id);
//! client.send_step_message("payments", msg).await?;
//!
//! // Initialize namespace queues
//! client.initialize_namespace_queues(&["payments", "fulfillment"]).await?;
//! ```

use std::sync::Arc;
use std::time::Duration;

use super::message::StepMessage;
use super::orchestration_messages::{StepResultMessage, TaskRequestMessage};
use super::service::{
    MessageRouterKind, MessagingError, MessagingProvider, QueueMessage, QueueStats, QueuedMessage,
    ReceiptHandle,
};
use crate::TaskerResult;

/// Domain-level messaging client for Tasker
///
/// Provides convenient, Tasker-specific methods for messaging operations.
/// Wraps a `MessagingProvider` (enum) and `MessageRouterKind` (enum) for
/// zero-cost dispatch without trait objects.
///
/// ## Thread Safety
///
/// The client is `Send + Sync` and can be safely shared across threads.
/// The inner `MessagingProvider` is wrapped in `Arc` for efficient cloning.
#[derive(Debug, Clone)]
pub struct MessageClient {
    /// The underlying messaging provider
    provider: Arc<MessagingProvider>,
    /// Queue name router
    router: MessageRouterKind,
}

impl MessageClient {
    /// Create a new MessageClient
    ///
    /// # Arguments
    ///
    /// * `provider` - The messaging provider (PGMQ, RabbitMQ, InMemory)
    /// * `router` - The queue name router
    ///
    /// # Example
    ///
    /// ```ignore
    /// let provider = Arc::new(MessagingProvider::new_in_memory());
    /// let router = MessageRouterKind::default();
    /// let client = MessageClient::new(provider, router);
    /// ```
    pub fn new(provider: Arc<MessagingProvider>, router: MessageRouterKind) -> Self {
        Self { provider, router }
    }

    /// Get the underlying messaging provider
    ///
    /// Use this for advanced operations not covered by the domain methods.
    pub fn provider(&self) -> &Arc<MessagingProvider> {
        &self.provider
    }

    /// Get the router for queue name lookups
    pub fn router(&self) -> &MessageRouterKind {
        &self.router
    }

    /// Get the provider name for logging/metrics
    pub fn provider_name(&self) -> &'static str {
        self.provider.provider_name()
    }

    // =========================================================================
    // Domain Methods - Step Messages
    // =========================================================================

    /// Send a step message to the appropriate worker queue
    ///
    /// Routes the message to `worker_{namespace}_queue` using the configured router.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The task namespace (e.g., "payments", "fulfillment")
    /// * `message` - The step message containing task/step UUIDs
    ///
    /// # Example
    ///
    /// ```ignore
    /// let msg = StepMessage::new(task_uuid, step_uuid, correlation_id);
    /// client.send_step_message("payments", msg).await?;
    /// ```
    pub async fn send_step_message(
        &self,
        namespace: &str,
        message: StepMessage,
    ) -> TaskerResult<()> {
        let queue = self.router.step_queue(namespace);
        self.provider
            .send_message(&queue, &message)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    /// Receive step messages from a namespace queue
    ///
    /// # Arguments
    ///
    /// * `namespace` - The task namespace
    /// * `max_messages` - Maximum number of messages to receive
    /// * `visibility_timeout` - How long messages are hidden from other consumers
    pub async fn receive_step_messages(
        &self,
        namespace: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<StepMessage>>, MessagingError> {
        let queue = self.router.step_queue(namespace);
        self.provider
            .receive_messages(&queue, max_messages, visibility_timeout)
            .await
    }

    // =========================================================================
    // Domain Methods - Step Results
    // =========================================================================

    /// Send a step result to the orchestration results queue
    ///
    /// Routes the message to the configured step results queue (e.g., `orchestration_step_results`).
    pub async fn send_step_result(&self, result: StepResultMessage) -> TaskerResult<()> {
        let queue = self.router.result_queue();
        self.provider
            .send_message(&queue, &result)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    /// Receive step results from the orchestration results queue
    pub async fn receive_step_results(
        &self,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<StepResultMessage>>, MessagingError> {
        let queue = self.router.result_queue();
        self.provider
            .receive_messages(&queue, max_messages, visibility_timeout)
            .await
    }

    // =========================================================================
    // Domain Methods - Task Requests
    // =========================================================================

    /// Send a task request to the orchestration task requests queue
    ///
    /// Routes the message to the configured task requests queue (e.g., `orchestration_task_requests`).
    pub async fn send_task_request(&self, request: TaskRequestMessage) -> TaskerResult<()> {
        let queue = self.router.task_request_queue();
        self.provider
            .send_message(&queue, &request)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    /// Receive task requests from the orchestration task requests queue
    pub async fn receive_task_requests(
        &self,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<TaskRequestMessage>>, MessagingError> {
        let queue = self.router.task_request_queue();
        self.provider
            .receive_messages(&queue, max_messages, visibility_timeout)
            .await
    }

    // =========================================================================
    // Domain Methods - Task Finalization
    // =========================================================================

    /// Send a task finalization message
    ///
    /// Routes the message to the configured task finalization queue.
    pub async fn send_task_finalization<T: QueueMessage>(
        &self,
        message: &T,
    ) -> TaskerResult<()> {
        let queue = self.router.task_finalization_queue();
        self.provider
            .send_message(&queue, message)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    /// Receive task finalization messages
    pub async fn receive_task_finalizations<T: QueueMessage>(
        &self,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        let queue = self.router.task_finalization_queue();
        self.provider
            .receive_messages(&queue, max_messages, visibility_timeout)
            .await
    }

    // =========================================================================
    // Queue Management
    // =========================================================================

    /// Initialize queues for the given namespaces
    ///
    /// Creates worker queues for each namespace (e.g., `worker_payments_queue`).
    /// Also ensures orchestration queues exist.
    ///
    /// # Arguments
    ///
    /// * `namespaces` - List of namespace names to create queues for
    ///
    /// # Example
    ///
    /// ```ignore
    /// client.initialize_namespace_queues(&["payments", "fulfillment", "notifications"]).await?;
    /// ```
    pub async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        // Create worker queues for each namespace
        let worker_queues: Vec<String> = namespaces
            .iter()
            .map(|ns| self.router.step_queue(ns))
            .collect();

        // Create orchestration queues
        let orchestration_queues = vec![
            self.router.result_queue(),
            self.router.task_request_queue(),
            self.router.task_finalization_queue(),
        ];

        // Combine all queues
        let mut all_queues = worker_queues;
        all_queues.extend(orchestration_queues);

        self.provider
            .ensure_queues(&all_queues)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;

        Ok(())
    }

    /// Ensure a single queue exists
    pub async fn ensure_queue(&self, queue_name: &str) -> TaskerResult<()> {
        self.provider
            .ensure_queue(queue_name)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    // =========================================================================
    // Message Lifecycle
    // =========================================================================

    /// Acknowledge (delete) a processed message
    ///
    /// Call this after successfully processing a message to remove it from the queue.
    ///
    /// # Arguments
    ///
    /// * `queue_name` - The queue the message was received from
    /// * `receipt_handle` - The receipt handle from the received message
    pub async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> TaskerResult<()> {
        self.provider
            .ack_message(queue_name, receipt_handle)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    /// Negative acknowledge a message (release it back to queue or discard)
    ///
    /// # Arguments
    ///
    /// * `queue_name` - The queue the message was received from
    /// * `receipt_handle` - The receipt handle from the received message
    /// * `requeue` - If true, message becomes visible again; if false, it's discarded
    pub async fn nack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        requeue: bool,
    ) -> TaskerResult<()> {
        self.provider
            .nack_message(queue_name, receipt_handle, requeue)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    /// Extend the visibility timeout for a message
    ///
    /// Use this when processing takes longer than expected to prevent
    /// the message from becoming visible to other consumers.
    pub async fn extend_visibility(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        extension: Duration,
    ) -> TaskerResult<()> {
        self.provider
            .extend_visibility(queue_name, receipt_handle, extension)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    // =========================================================================
    // Queue Metrics
    // =========================================================================

    /// Get statistics for a queue
    ///
    /// Returns message counts, oldest message age, etc.
    pub async fn get_queue_stats(&self, queue_name: &str) -> Result<QueueStats, MessagingError> {
        self.provider.queue_stats(queue_name).await
    }

    /// Get statistics for a namespace's worker queue
    pub async fn get_namespace_queue_stats(
        &self,
        namespace: &str,
    ) -> Result<QueueStats, MessagingError> {
        let queue = self.router.step_queue(namespace);
        self.provider.queue_stats(&queue).await
    }

    /// Health check for the messaging provider
    pub async fn health_check(&self) -> Result<bool, MessagingError> {
        self.provider.health_check().await
    }

    // =========================================================================
    // Generic Messaging (for advanced use cases)
    // =========================================================================

    /// Send a generic message to any queue
    ///
    /// Use this for message types not covered by the domain methods.
    pub async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> TaskerResult<()> {
        self.provider
            .send_message(queue_name, message)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(())
    }

    /// Receive generic messages from any queue
    pub async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        self.provider
            .receive_messages(queue_name, max_messages, visibility_timeout)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_client() -> MessageClient {
        let provider = Arc::new(MessagingProvider::new_in_memory());
        let router = MessageRouterKind::default();
        MessageClient::new(provider, router)
    }

    #[test]
    fn test_message_client_creation() {
        let client = create_test_client();
        assert_eq!(client.provider_name(), "in_memory");
    }

    #[test]
    fn test_router_queue_names() {
        let client = create_test_client();

        // Verify router is accessible and returns expected queue names
        assert_eq!(
            client.router().step_queue("payments"),
            "worker_payments_queue"
        );
        assert_eq!(
            client.router().result_queue(),
            "orchestration_step_results"
        );
        assert_eq!(
            client.router().task_request_queue(),
            "orchestration_task_requests"
        );
    }

    #[tokio::test]
    async fn test_send_step_message() {
        let client = create_test_client();

        // Initialize the queue first
        client.ensure_queue("worker_payments_queue").await.unwrap();

        // Create and send a step message
        let msg = StepMessage::new(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());

        let result = client.send_step_message("payments", msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_initialize_namespace_queues() {
        let client = create_test_client();

        let result = client
            .initialize_namespace_queues(&["payments", "fulfillment"])
            .await;
        assert!(result.is_ok());

        // Verify queues were created by checking health
        assert!(client.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_send_and_receive_step_messages() {
        let client = create_test_client();

        // Initialize queue
        client.ensure_queue("worker_test_queue").await.unwrap();

        // Create router that uses "test" namespace
        let queue_name = client.router().step_queue("test");
        client.ensure_queue(&queue_name).await.unwrap();

        // Send a message
        let original_msg = StepMessage::new(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        client
            .send_step_message("test", original_msg.clone())
            .await
            .unwrap();

        // Receive the message
        let messages = client
            .receive_step_messages("test", 10, Duration::from_secs(30))
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message.step_uuid, original_msg.step_uuid);
        assert_eq!(messages[0].message.task_uuid, original_msg.task_uuid);
        assert_eq!(
            messages[0].message.correlation_id,
            original_msg.correlation_id
        );
    }

    #[tokio::test]
    async fn test_ack_message() {
        let client = create_test_client();

        // Initialize queue
        let queue_name = client.router().step_queue("test");
        client.ensure_queue(&queue_name).await.unwrap();

        // Send a message
        let msg = StepMessage::new(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        client.send_step_message("test", msg).await.unwrap();

        // Receive it
        let messages = client
            .receive_step_messages("test", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);

        // Ack it
        let result = client
            .ack_message(&queue_name, &messages[0].receipt_handle)
            .await;
        assert!(result.is_ok());

        // Verify queue is now empty
        let messages_after = client
            .receive_step_messages("test", 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(messages_after.len(), 0);
    }
}
