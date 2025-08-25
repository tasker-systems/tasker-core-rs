use crate::messaging::clients::types::{PgmqStepMessage, QueueMetrics};
use crate::messaging::errors::MessagingResult;
/// Unified trait for PGMQ client operations
///
/// Provides a common interface for both standard PgmqClient and circuit breaker
/// protected ProtectedPgmqClient, allowing seamless switching based on configuration.
use async_trait;
use pgmq::types::Message as PgmqMessage;
use serde::Serialize;

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
    async fn send_json_message<T: Serialize + Clone + Send + Sync>(
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
    ) -> MessagingResult<Vec<PgmqMessage<serde_json::Value>>>;

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
    ) -> MessagingResult<Vec<PgmqMessage<serde_json::Value>>>;

    /// Complete message processing (delete from queue)
    async fn complete_message(&self, namespace: &str, message_id: i64) -> MessagingResult<()>;
}
