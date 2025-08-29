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
    traits::PgmqClientTrait, ClientStatus, MessageClient, PgmqClient, PgmqStepMessage,
    PgmqStepMessageMetadata, ProtectedPgmqClient, QueueMetrics, UnifiedMessageClient,
};
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

use pgmq_notify::client::PgmqNotifyClient;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Unified PGMQ client that can be either standard or circuit-breaker protected
///
/// Enhanced with pgmq-notify capabilities for event-driven message processing.
/// This provides both standard PGMQ operations and notification-based specific message reading.
pub struct UnifiedPgmqClient {
    /// The underlying PGMQ client (standard or protected)
    inner: UnifiedPgmqClientInner,
    /// Enhanced notify client for specific message reading (optional)
    notify_client: Option<Arc<Mutex<PgmqNotifyClient>>>,
}

#[derive(Debug, Clone)]
enum UnifiedPgmqClientInner {
    /// Standard PGMQ client (no circuit breaker protection)
    Standard(PgmqClient),
    /// Circuit breaker protected PGMQ client
    Protected(ProtectedPgmqClient),
}

impl std::fmt::Debug for UnifiedPgmqClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnifiedPgmqClient")
            .field("inner", &self.inner)
            .field("notify_client_enabled", &self.notify_client.is_some())
            .finish()
    }
}

impl Clone for UnifiedPgmqClient {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            notify_client: None, // Don't clone the notify client to avoid complexity
        }
    }
}

#[async_trait::async_trait]
impl PgmqClientTrait for UnifiedPgmqClient {
    async fn create_queue(&self, queue_name: &str) -> MessagingResult<()> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => client.create_queue(queue_name).await,
            UnifiedPgmqClientInner::Protected(client) => {
                client.create_queue(queue_name).await.map_err(|e| e.into())
            }
        }
    }

    async fn send_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> MessagingResult<i64> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client.send_message(queue_name, message).await
            }
            UnifiedPgmqClientInner::Protected(client) => client
                .send_message(queue_name, message)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn send_json_message<T: serde::Serialize + Clone + Send + Sync>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> MessagingResult<i64> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client.send_json_message(queue_name, message).await
            }
            UnifiedPgmqClientInner::Protected(client) => client
                .send_json_message(queue_name, message)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn read_messages(
        &self,
        queue_name: &str,
        visibility_timeout: Option<i32>,
        qty: Option<i32>,
    ) -> MessagingResult<Vec<pgmq::types::Message<serde_json::Value>>> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client
                    .read_messages(queue_name, visibility_timeout, qty)
                    .await
            }
            UnifiedPgmqClientInner::Protected(client) => client
                .read_messages(queue_name, visibility_timeout, qty)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn delete_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client.delete_message(queue_name, message_id).await
            }
            UnifiedPgmqClientInner::Protected(client) => client
                .delete_message(queue_name, message_id)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn archive_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client.archive_message(queue_name, message_id).await
            }
            UnifiedPgmqClientInner::Protected(client) => client
                .archive_message(queue_name, message_id)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn purge_queue(&self, queue_name: &str) -> MessagingResult<u64> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => client.purge_queue(queue_name).await,
            UnifiedPgmqClientInner::Protected(client) => {
                client.purge_queue(queue_name).await.map_err(|e| e.into())
            }
        }
    }

    async fn drop_queue(&self, queue_name: &str) -> MessagingResult<()> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => client.drop_queue(queue_name).await,
            UnifiedPgmqClientInner::Protected(client) => {
                client.drop_queue(queue_name).await.map_err(|e| e.into())
            }
        }
    }

    async fn queue_metrics(&self, queue_name: &str) -> MessagingResult<QueueMetrics> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => client.queue_metrics(queue_name).await,
            UnifiedPgmqClientInner::Protected(client) => {
                client.queue_metrics(queue_name).await.map_err(|e| e.into())
            }
        }
    }

    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> MessagingResult<()> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client.initialize_namespace_queues(namespaces).await
            }
            UnifiedPgmqClientInner::Protected(client) => client
                .initialize_namespace_queues(namespaces)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn enqueue_step(
        &self,
        namespace: &str,
        step_message: PgmqStepMessage,
    ) -> MessagingResult<i64> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client.enqueue_step(namespace, step_message).await
            }
            UnifiedPgmqClientInner::Protected(client) => {
                // ProtectedPgmqClient now implements the trait method directly
                client.enqueue_step(namespace, step_message).await
            }
        }
    }

    async fn process_namespace_queue(
        &self,
        namespace: &str,
        visibility_timeout: Option<i32>,
        batch_size: i32,
    ) -> MessagingResult<Vec<pgmq::types::Message<serde_json::Value>>> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client
                    .process_namespace_queue(namespace, visibility_timeout, batch_size)
                    .await
            }
            UnifiedPgmqClientInner::Protected(client) => {
                // ProtectedPgmqClient now implements the trait method directly
                client
                    .process_namespace_queue(namespace, visibility_timeout, batch_size)
                    .await
            }
        }
    }

    async fn complete_message(&self, namespace: &str, message_id: i64) -> MessagingResult<()> {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => {
                client.complete_message(namespace, message_id).await
            }
            UnifiedPgmqClientInner::Protected(client) => {
                // ProtectedPgmqClient now implements the trait method directly
                client.complete_message(namespace, message_id).await
            }
        }
    }
}

impl UnifiedPgmqClient {
    /// Create a new UnifiedPgmqClient with standard PGMQ client
    pub fn new_standard(client: PgmqClient) -> Self {
        Self {
            inner: UnifiedPgmqClientInner::Standard(client),
            notify_client: None,
        }
    }

    /// Create a new UnifiedPgmqClient with circuit-breaker protected client
    pub fn new_protected(client: ProtectedPgmqClient) -> Self {
        Self {
            inner: UnifiedPgmqClientInner::Protected(client),
            notify_client: None,
        }
    }

    /// Create a new UnifiedPgmqClient with standard client and notify capabilities
    pub async fn new_standard_with_notify(
        client: PgmqClient,
        database_url: &str,
    ) -> Result<Self, pgmq_notify::PgmqNotifyError> {
        let notify_config = pgmq_notify::PgmqNotifyConfig::new();
        let notify_client = Arc::new(Mutex::new(
            PgmqNotifyClient::new(database_url, notify_config).await?,
        ));

        Ok(Self {
            inner: UnifiedPgmqClientInner::Standard(client),
            notify_client: Some(notify_client),
        })
    }

    /// Create a new UnifiedPgmqClient with protected client and notify capabilities
    pub async fn new_protected_with_notify(
        client: ProtectedPgmqClient,
        database_url: &str,
    ) -> Result<Self, pgmq_notify::PgmqNotifyError> {
        let notify_config = pgmq_notify::PgmqNotifyConfig::new();
        let notify_client = Arc::new(Mutex::new(
            PgmqNotifyClient::new(database_url, notify_config).await?,
        ));

        Ok(Self {
            inner: UnifiedPgmqClientInner::Protected(client),
            notify_client: Some(notify_client),
        })
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
        if let Some(ref notify_client) = self.notify_client {
            let mut client = notify_client.lock().await;
            client
                .read_specific_message::<T>(queue_name, message_id, visibility_timeout)
                .await
                .map_err(|e| {
                    MessagingError::database_connection(format!(
                        "Failed to read specific message: {e}"
                    ))
                })
        } else {
            // Fallback to standard batch reading and filtering
            let messages = self
                .read_messages(queue_name, Some(visibility_timeout), Some(1))
                .await?;

            // Find the specific message by ID
            for msg in messages {
                if msg.msg_id == message_id {
                    // Deserialize the message
                    let typed_message: T =
                        serde_json::from_value(msg.message.clone()).map_err(|e| {
                            MessagingError::message_deserialization(format!(
                                "Failed to deserialize message: {e}"
                            ))
                        })?;

                    return Ok(Some(pgmq::types::Message {
                        msg_id: msg.msg_id,
                        vt: msg.vt,
                        read_ct: msg.read_ct,
                        enqueued_at: msg.enqueued_at,
                        message: typed_message,
                    }));
                }
            }

            Ok(None)
        }
    }

    /// Check if this client has notify capabilities
    pub fn has_notify_capabilities(&self) -> bool {
        self.notify_client.is_some()
    }

    /// Clone the inner client type (for backward compatibility)
    pub fn clone_inner(&self) -> UnifiedPgmqClient {
        match &self.inner {
            UnifiedPgmqClientInner::Standard(client) => Self::new_standard(client.clone()),
            UnifiedPgmqClientInner::Protected(client) => Self::new_protected(client.clone()),
        }
    }
}
