use crate::messaging::{
    clients::types::{ClientStatus, PgmqStepMessage, QueueMetrics},
    execution_types::StepExecutionResult,
    message::StepMessage,
    orchestration_messages::{StepResultMessage, TaskRequestMessage},
};
use crate::TaskerResult;

use crate::messaging::errors::MessagingResult;
/// Unified trait for PGMQ client operations
///
/// Provides a common interface for both standard PgmqClient and circuit breaker
/// protected ProtectedPgmqClient, allowing seamless switching based on configuration.
use async_trait::async_trait;
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

/// Abstraction over multiple message queue backends
///
/// This trait defines the core messaging operations needed for workflow orchestration.
/// All message queue backends (PGMQ, RabbitMQ, etc.) must implement this interface.
///
/// # TAS-133 Note
///
/// As of TAS-133, `StepMessage` is now the UUID-based type (renamed from `SimpleStepMessage`).
/// The old `StepMessage` with embedded execution context has been removed.
/// The `send_simple_step_message` method has been removed - use `send_step_message` instead.
#[async_trait]
pub trait MessageClient: Send + Sync {
    /// Send a step execution message to the appropriate namespace queue
    ///
    /// The message contains only UUIDs - workers query the database for full context.
    async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()>;

    /// Claim/receive step messages from a namespace queue (worker operation)
    ///
    /// Returns UUID-based messages - workers must hydrate full context from database.
    async fn receive_step_messages(
        &self,
        namespace: &str,
        limit: i32,
        visibility_timeout: i32,
    ) -> TaskerResult<Vec<StepMessage>>;

    /// Send step execution result back to orchestration
    async fn send_step_result(&self, result: StepExecutionResult) -> TaskerResult<()>;

    /// Send task request for initialization
    async fn send_task_request(&self, request: TaskRequestMessage) -> TaskerResult<()>;

    /// Receive task requests (orchestration operation)
    async fn receive_task_requests(&self, limit: i32) -> TaskerResult<Vec<TaskRequestMessage>>;

    /// Send step result message to orchestration
    async fn send_step_result_message(&self, result: StepResultMessage) -> TaskerResult<()>;

    /// Receive step result messages (orchestration operation)
    async fn receive_step_result_messages(
        &self,
        limit: i32,
    ) -> TaskerResult<Vec<StepResultMessage>>;

    /// Initialize queues for given namespaces
    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> TaskerResult<()>;

    /// Create a specific queue
    async fn create_queue(&self, queue_name: &str) -> TaskerResult<()>;

    /// Delete/acknowledge a processed message
    async fn delete_message(&self, queue_name: &str, message_id: i64) -> TaskerResult<()>;

    /// Get queue metrics and statistics
    async fn get_queue_metrics(&self, queue_name: &str) -> TaskerResult<QueueMetrics>;

    /// Get client type for debugging/observability
    fn client_type(&self) -> &'static str;

    /// Get client-specific configuration/status
    async fn get_client_status(&self) -> TaskerResult<ClientStatus>;
}
