//! # Messaging Module
//!
//! PostgreSQL message queue (pgmq) based messaging for workflow orchestration.
//! Provides queue-based task and step processing, replacing the TCP command architecture.

pub mod clients;
pub mod errors;
pub mod execution_types;
pub mod message;
pub mod orchestration_messages;
pub mod step_handler_result;

pub use clients::{
    traits::PgmqClientTrait, MessageClient, PgmqClient, TaskerPgmqClientExt, UnifiedMessageClient,
};
// Import tasker-specific types (which re-export and extend pgmq-notify types)
pub use clients::types::{ClientStatus, PgmqStepMessage, PgmqStepMessageMetadata, QueueMetrics};
pub use errors::{MessagingError, MessagingResult};
pub use execution_types::{
    StepBatchRequest, StepBatchResponse, StepExecutionError, StepExecutionMetadata,
    StepExecutionRequest, StepExecutionResult, StepRequestMetadata,
};
pub use message::{StepMessage, StepMessageMetadata};
pub use orchestration_messages::*;
pub use step_handler_result::{
    StepHandlerCallResult, StepHandlerErrorResult, StepHandlerSuccessResult,
};

/// Unified PGMQ client with pgmq-notify capabilities
///
/// Enhanced with pgmq-notify capabilities for event-driven message processing.
/// This provides both standard PGMQ operations and notification-based specific message reading.
pub struct UnifiedPgmqClient {
    /// The underlying PGMQ client from pgmq-notify
    client: PgmqClient,
}

impl std::fmt::Debug for UnifiedPgmqClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnifiedPgmqClient")
            .field("has_notify", &self.client.has_notify_capabilities())
            .finish()
    }
}

impl Clone for UnifiedPgmqClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
        }
    }
}

#[async_trait::async_trait]
impl PgmqClientTrait for UnifiedPgmqClient {
    async fn create_queue(&self, queue_name: &str) -> MessagingResult<()> {
        self.client
            .create_queue(queue_name)
            .await
            .map_err(MessagingError::from)
    }

    async fn send_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> MessagingResult<i64> {
        // Use the extension trait method to send step messages directly
        TaskerPgmqClientExt::send_step_message(&self.client, queue_name, message).await
    }

    async fn send_json_message<T: serde::Serialize + Clone + Send + Sync>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> MessagingResult<i64> {
        self.client
            .send_json_message(queue_name, message)
            .await
            .map_err(MessagingError::from)
    }

    async fn read_messages(
        &self,
        queue_name: &str,
        visibility_timeout: Option<i32>,
        qty: Option<i32>,
    ) -> MessagingResult<Vec<pgmq::types::Message<serde_json::Value>>> {
        self.client
            .read_messages(queue_name, visibility_timeout, qty)
            .await
            .map_err(MessagingError::from)
    }

    async fn delete_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()> {
        self.client
            .delete_message(queue_name, message_id)
            .await
            .map_err(MessagingError::from)
    }

    async fn archive_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()> {
        self.client
            .archive_message(queue_name, message_id)
            .await
            .map_err(MessagingError::from)
    }

    async fn purge_queue(&self, queue_name: &str) -> MessagingResult<u64> {
        self.client
            .purge_queue(queue_name)
            .await
            .map_err(MessagingError::from)
    }

    async fn drop_queue(&self, queue_name: &str) -> MessagingResult<()> {
        self.client
            .drop_queue(queue_name)
            .await
            .map_err(MessagingError::from)
    }

    async fn queue_metrics(&self, queue_name: &str) -> MessagingResult<QueueMetrics> {
        let metrics = self
            .client
            .queue_metrics(queue_name)
            .await
            .map_err(MessagingError::from)?;
        // Since we're now re-exporting QueueMetrics from pgmq-notify, no conversion needed
        Ok(metrics)
    }

    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> MessagingResult<()> {
        self.client
            .initialize_namespace_queues(namespaces)
            .await
            .map_err(MessagingError::from)
    }

    async fn enqueue_step(
        &self,
        namespace: &str,
        step_message: PgmqStepMessage,
    ) -> MessagingResult<i64> {
        // Use the extension trait method to enqueue step messages directly
        TaskerPgmqClientExt::enqueue_step_message(&self.client, namespace, step_message).await
    }

    async fn process_namespace_queue(
        &self,
        namespace: &str,
        visibility_timeout: Option<i32>,
        batch_size: i32,
    ) -> MessagingResult<Vec<pgmq::types::Message<serde_json::Value>>> {
        self.client
            .process_namespace_queue(namespace, visibility_timeout, batch_size)
            .await
            .map_err(MessagingError::from)
    }

    async fn complete_message(&self, namespace: &str, message_id: i64) -> MessagingResult<()> {
        self.client
            .complete_message(namespace, message_id)
            .await
            .map_err(MessagingError::from)
    }
}

impl UnifiedPgmqClient {
    /// Create a new UnifiedPgmqClient from database URL
    pub async fn new(database_url: &str) -> Result<Self, MessagingError> {
        let client = PgmqClient::new(database_url)
            .await
            .map_err(MessagingError::from)?;
        Ok(Self { client })
    }

    /// Create a new UnifiedPgmqClient with configuration
    pub async fn new_with_config(
        database_url: &str,
        config: pgmq_notify::PgmqNotifyConfig,
    ) -> Result<Self, MessagingError> {
        let client = PgmqClient::new_with_config(database_url, config)
            .await
            .map_err(MessagingError::from)?;
        Ok(Self { client })
    }

    /// Create a new UnifiedPgmqClient with existing pool
    pub async fn new_with_pool(pool: sqlx::PgPool) -> Self {
        let client = PgmqClient::new_with_pool(pool).await;
        Self { client }
    }

    /// Create from an existing PgmqClient (for backward compatibility)
    pub fn new_standard(client: PgmqClient) -> Self {
        Self { client }
    }

    /// Read a specific message by ID from a queue using pgmq-notify capabilities
    ///
    /// This method provides event-driven message processing by reading a specific
    /// message that triggered a NOTIFY event, avoiding the need to scan multiple messages.
    pub async fn read_specific_message<T>(
        &self,
        queue_name: &str,
        message_id: i64,
        visibility_timeout: i32,
    ) -> Result<Option<pgmq::types::Message<T>>, MessagingError>
    where
        T: serde::de::DeserializeOwned + Send + Sync,
    {
        self.client
            .read_specific_message::<T>(queue_name, message_id, visibility_timeout)
            .await
            .map_err(MessagingError::from)
    }

    /// Check if this client has notify capabilities
    pub fn has_notify_capabilities(&self) -> bool {
        self.client.has_notify_capabilities()
    }

    /// Clone the inner client type (for backward compatibility)
    pub fn clone_inner(&self) -> UnifiedPgmqClient {
        Self {
            client: self.client.clone(),
        }
    }
}
