//! # Messaging Module
//!
//! PostgreSQL message queue (pgmq) based messaging for workflow orchestration.
//! Provides queue-based task and step processing, replacing the TCP command architecture.

pub mod errors;
pub mod execution_types;
pub mod message;
pub mod orchestration_messages;
pub mod pgmq_client;
pub mod protected_pgmq_client;
pub mod step_handler_result;

pub use errors::{MessagingError, MessagingResult};
pub use execution_types::{
    StepBatchRequest, StepBatchResponse, StepExecutionError, StepExecutionRequest,
    StepExecutionResult, StepRequestMetadata, StepExecutionMetadata,
};
pub use step_handler_result::{
    StepHandlerCallResult, StepHandlerErrorResult, StepHandlerSuccessResult,
};
pub use message::{StepMessage, StepMessageMetadata};
pub use orchestration_messages::*;
pub use pgmq_client::*;
pub use protected_pgmq_client::{ProtectedPgmqClient, ProtectedPgmqError};

/// Unified trait for PGMQ client operations
///
/// Provides a common interface for both standard PgmqClient and circuit breaker
/// protected ProtectedPgmqClient, allowing seamless switching based on configuration.
#[async_trait::async_trait]
pub trait PgmqClientTrait: Send + Sync {
    /// Create queue if it doesn't exist
    async fn create_queue(&self, queue_name: &str) -> MessagingResult<()>;

    /// Send step message to queue
    async fn send_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> MessagingResult<i64>;

    /// Send generic JSON message to queue
    async fn send_json_message<T: serde::Serialize + Clone + Send + Sync>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> MessagingResult<i64>;

    /// Read messages from queue
    async fn read_messages(
        &self,
        queue_name: &str,
        visibility_timeout: Option<i32>,
        qty: Option<i32>,
    ) -> MessagingResult<Vec<pgmq::types::Message<serde_json::Value>>>;

    /// Delete message from queue
    async fn delete_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()>;

    /// Archive message (move to archive)
    async fn archive_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()>;

    /// Purge queue (delete all messages)
    async fn purge_queue(&self, queue_name: &str) -> MessagingResult<u64>;

    /// Drop queue completely
    async fn drop_queue(&self, queue_name: &str) -> MessagingResult<()>;

    /// Get queue metrics/statistics
    async fn queue_metrics(&self, queue_name: &str) -> MessagingResult<QueueMetrics>;

    /// Initialize standard namespace queues
    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> MessagingResult<()>;

    /// Send step execution message to namespace queue
    async fn enqueue_step(
        &self,
        namespace: &str,
        step_message: PgmqStepMessage,
    ) -> MessagingResult<i64>;

    /// Process messages from namespace queue
    async fn process_namespace_queue(
        &self,
        namespace: &str,
        visibility_timeout: Option<i32>,
        batch_size: i32,
    ) -> MessagingResult<Vec<pgmq::types::Message<serde_json::Value>>>;

    /// Complete message processing (delete from queue)
    async fn complete_message(&self, namespace: &str, message_id: i64) -> MessagingResult<()>;
}

/// Unified PGMQ client that can be either standard or circuit-breaker protected
///
/// This enum wrapper solves the trait object compatibility issue while providing
/// a unified interface for both client types based on configuration.
#[derive(Debug, Clone)]
pub enum UnifiedPgmqClient {
    /// Standard PGMQ client (no circuit breaker protection)
    Standard(PgmqClient),
    /// Circuit breaker protected PGMQ client
    Protected(ProtectedPgmqClient),
}

#[async_trait::async_trait]
impl PgmqClientTrait for UnifiedPgmqClient {
    async fn create_queue(&self, queue_name: &str) -> MessagingResult<()> {
        match self {
            Self::Standard(client) => client.create_queue(queue_name).await,
            Self::Protected(client) => client.create_queue(queue_name).await.map_err(|e| e.into()),
        }
    }

    async fn send_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> MessagingResult<i64> {
        match self {
            Self::Standard(client) => client.send_message(queue_name, message).await,
            Self::Protected(client) => client
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
        match self {
            Self::Standard(client) => client.send_json_message(queue_name, message).await,
            Self::Protected(client) => client
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
        match self {
            Self::Standard(client) => {
                client
                    .read_messages(queue_name, visibility_timeout, qty)
                    .await
            }
            Self::Protected(client) => client
                .read_messages(queue_name, visibility_timeout, qty)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn delete_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()> {
        match self {
            Self::Standard(client) => client.delete_message(queue_name, message_id).await,
            Self::Protected(client) => client
                .delete_message(queue_name, message_id)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn archive_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()> {
        match self {
            Self::Standard(client) => client.archive_message(queue_name, message_id).await,
            Self::Protected(client) => client
                .archive_message(queue_name, message_id)
                .await
                .map_err(|e| e.into()),
        }
    }

    async fn purge_queue(&self, queue_name: &str) -> MessagingResult<u64> {
        match self {
            Self::Standard(client) => client.purge_queue(queue_name).await,
            Self::Protected(client) => client.purge_queue(queue_name).await.map_err(|e| e.into()),
        }
    }

    async fn drop_queue(&self, queue_name: &str) -> MessagingResult<()> {
        match self {
            Self::Standard(client) => client.drop_queue(queue_name).await,
            Self::Protected(client) => client.drop_queue(queue_name).await.map_err(|e| e.into()),
        }
    }

    async fn queue_metrics(&self, queue_name: &str) -> MessagingResult<QueueMetrics> {
        match self {
            Self::Standard(client) => client.queue_metrics(queue_name).await,
            Self::Protected(client) => client.queue_metrics(queue_name).await.map_err(|e| e.into()),
        }
    }

    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> MessagingResult<()> {
        match self {
            Self::Standard(client) => client.initialize_namespace_queues(namespaces).await,
            Self::Protected(client) => client
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
        match self {
            Self::Standard(client) => client.enqueue_step(namespace, step_message).await,
            Self::Protected(client) => {
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
        match self {
            Self::Standard(client) => {
                client
                    .process_namespace_queue(namespace, visibility_timeout, batch_size)
                    .await
            }
            Self::Protected(client) => {
                // ProtectedPgmqClient now implements the trait method directly
                client
                    .process_namespace_queue(namespace, visibility_timeout, batch_size)
                    .await
            }
        }
    }

    async fn complete_message(&self, namespace: &str, message_id: i64) -> MessagingResult<()> {
        match self {
            Self::Standard(client) => client.complete_message(namespace, message_id).await,
            Self::Protected(client) => {
                // ProtectedPgmqClient now implements the trait method directly
                client.complete_message(namespace, message_id).await
            }
        }
    }
}
