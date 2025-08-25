//! # Unified Message Client Abstraction (TAS-40 Phase 5)
//!
//! Abstraction over multiple message queue backends to support:
//! - PostgreSQL Message Queue (pgmq) - current implementation
//! - RabbitMQ - future implementation
//! - In-Memory - testing implementation
//!
//! ## Key Features
//!
//! - **Backend Flexibility**: Easy to add new queue backends
//! - **Testing Support**: In-memory implementation for unit tests
//! - **Configuration-Driven**: Backend selection via configuration
//! - **Performance**: Zero-cost abstraction over concrete implementations
//! - **Future-Proof**: Ready for TAS-35 message service requirements
//!
//! ## Architecture Integration
//!
//! ```text
//! OrchestrationCore/WorkerProcessor
//!     ↓ (uses)
//! UnifiedMessageClient
//!     ↓ (delegates to)
//! PgmqClient | RabbitMqClient | InMemoryClient
//! ```
//!
//! ## Usage in SystemContext
//!
//! ```rust
//! // SystemContext uses UnifiedMessageClient instead of direct PgmqClient
//! pub struct SystemContext {
//!     pub message_client: Arc<UnifiedMessageClient>,
//!     // ...
//! }
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::messaging::{
    clients::types::{ClientStatus, QueueMetrics},
    execution_types::StepExecutionResult,
    message::{SimpleStepMessage, StepMessage},
    orchestration_messages::{StepResultMessage, TaskRequestMessage},
};
use crate::TaskerResult;

use super::{in_memory_client::InMemoryClient, pgmq_client::PgmqClient};

/// Abstraction over multiple message queue backends
///
/// This trait defines the core messaging operations needed for workflow orchestration.
/// All message queue backends (PGMQ, RabbitMQ, etc.) must implement this interface.
#[async_trait]
pub trait MessageClient: Send + Sync {
    /// Send a step execution message to the appropriate namespace queue
    async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()>;

    /// Send a simple step message (UUID-based) to namespace queue
    async fn send_simple_step_message(
        &self,
        namespace: &str,
        message: SimpleStepMessage,
    ) -> TaskerResult<()>;

    /// Claim/receive step messages from a namespace queue (worker operation)
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

/// Unified message client supporting multiple backends
///
/// This enum provides a zero-cost abstraction over different message queue backends.
/// The actual backend is selected at runtime based on configuration.
#[derive(Debug, Clone)]
pub enum UnifiedMessageClient {
    /// PostgreSQL Message Queue (pgmq) - current production backend
    Pgmq(Arc<PgmqClient>),
    /// RabbitMQ - future implementation for enterprise deployments
    RabbitMq(Arc<RabbitMqClient>),
    /// In-memory - testing implementation for unit tests
    InMemory(Arc<InMemoryClient>),
}

#[async_trait]
impl MessageClient for UnifiedMessageClient {
    async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client.send_step_message(namespace, message).await,
            Self::RabbitMq(client) => client.send_step_message(namespace, message).await,
            Self::InMemory(client) => client.send_step_message(namespace, message).await,
        }
    }

    async fn send_simple_step_message(
        &self,
        namespace: &str,
        message: SimpleStepMessage,
    ) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client.send_simple_step_message(namespace, message).await,
            Self::RabbitMq(client) => client.send_simple_step_message(namespace, message).await,
            Self::InMemory(client) => client.send_simple_step_message(namespace, message).await,
        }
    }

    async fn receive_step_messages(
        &self,
        namespace: &str,
        limit: i32,
        visibility_timeout: i32,
    ) -> TaskerResult<Vec<StepMessage>> {
        match self {
            Self::Pgmq(client) => {
                client
                    .receive_step_messages(namespace, limit, visibility_timeout)
                    .await
            }
            Self::RabbitMq(client) => {
                client
                    .receive_step_messages(namespace, limit, visibility_timeout)
                    .await
            }
            Self::InMemory(client) => {
                client
                    .receive_step_messages(namespace, limit, visibility_timeout)
                    .await
            }
        }
    }

    async fn send_step_result(&self, result: StepExecutionResult) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client.send_step_result(result).await,
            Self::RabbitMq(client) => client.send_step_result(result).await,
            Self::InMemory(client) => client.send_step_result(result).await,
        }
    }

    async fn send_task_request(&self, request: TaskRequestMessage) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client.send_task_request(request).await,
            Self::RabbitMq(client) => client.send_task_request(request).await,
            Self::InMemory(client) => client.send_task_request(request).await,
        }
    }

    async fn receive_task_requests(&self, limit: i32) -> TaskerResult<Vec<TaskRequestMessage>> {
        match self {
            Self::Pgmq(client) => client.receive_task_requests(limit).await,
            Self::RabbitMq(client) => client.receive_task_requests(limit).await,
            Self::InMemory(client) => client.receive_task_requests(limit).await,
        }
    }

    async fn send_step_result_message(&self, result: StepResultMessage) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client.send_step_result_message(result).await,
            Self::RabbitMq(client) => client.send_step_result_message(result).await,
            Self::InMemory(client) => client.send_step_result_message(result).await,
        }
    }

    async fn receive_step_result_messages(
        &self,
        limit: i32,
    ) -> TaskerResult<Vec<StepResultMessage>> {
        match self {
            Self::Pgmq(client) => client.receive_step_result_messages(limit).await,
            Self::RabbitMq(client) => client.receive_step_result_messages(limit).await,
            Self::InMemory(client) => client.receive_step_result_messages(limit).await,
        }
    }

    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client
                .initialize_namespace_queues(namespaces)
                .await
                .map_err(|e| crate::TaskerError::MessagingError(e.to_string())),
            Self::RabbitMq(client) => client.initialize_namespace_queues(namespaces).await,
            Self::InMemory(client) => client.initialize_namespace_queues(namespaces).await,
        }
    }

    async fn create_queue(&self, queue_name: &str) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client
                .create_queue(queue_name)
                .await
                .map_err(|e| crate::TaskerError::MessagingError(e.to_string())),
            Self::RabbitMq(client) => client.create_queue(queue_name).await,
            Self::InMemory(client) => client.create_queue(queue_name).await,
        }
    }

    async fn delete_message(&self, queue_name: &str, message_id: i64) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client
                .delete_message(queue_name, message_id)
                .await
                .map_err(|e| crate::TaskerError::MessagingError(e.to_string())),
            Self::RabbitMq(client) => client.delete_message(queue_name, message_id).await,
            Self::InMemory(client) => client.delete_message(queue_name, message_id).await,
        }
    }

    async fn get_queue_metrics(&self, queue_name: &str) -> TaskerResult<QueueMetrics> {
        match self {
            Self::Pgmq(client) => client.get_queue_metrics(queue_name).await,
            Self::RabbitMq(client) => client.get_queue_metrics(queue_name).await,
            Self::InMemory(client) => client.get_queue_metrics(queue_name).await,
        }
    }

    fn client_type(&self) -> &'static str {
        match self {
            Self::Pgmq(_) => "pgmq",
            Self::RabbitMq(_) => "rabbitmq",
            Self::InMemory(_) => "in_memory",
        }
    }

    async fn get_client_status(&self) -> TaskerResult<ClientStatus> {
        match self {
            Self::Pgmq(client) => client.get_client_status().await,
            Self::RabbitMq(client) => client.get_client_status().await,
            Self::InMemory(client) => client.get_client_status().await,
        }
    }
}

impl UnifiedMessageClient {
    /// Create a new PGMQ-based unified client
    pub async fn new_pgmq(database_url: &str) -> TaskerResult<Self> {
        let pgmq_client = PgmqClient::new(database_url)
            .await
            .map_err(|e| crate::TaskerError::MessagingError(e.to_string()))?;
        Ok(Self::Pgmq(Arc::new(pgmq_client)))
    }

    /// Create a new PGMQ-based client with shared pool
    pub async fn new_pgmq_with_pool(pool: sqlx::PgPool) -> Self {
        let pgmq_client = PgmqClient::new_with_pool(pool).await;
        Self::Pgmq(Arc::new(pgmq_client))
    }

    /// Create a new RabbitMQ-based client (future implementation)
    pub async fn new_rabbitmq(connection_url: &str) -> TaskerResult<Self> {
        let rabbitmq_client = RabbitMqClient::new(connection_url).await?;
        Ok(Self::RabbitMq(Arc::new(rabbitmq_client)))
    }

    /// Create a new in-memory client for testing
    pub fn new_in_memory() -> Self {
        let in_memory_client = InMemoryClient::new();
        Self::InMemory(Arc::new(in_memory_client))
    }

    /// Get the underlying PGMQ client if this is a PGMQ variant
    pub fn as_pgmq(&self) -> Option<&Arc<PgmqClient>> {
        match self {
            Self::Pgmq(client) => Some(client),
            _ => None,
        }
    }

    /// Get the underlying RabbitMQ client if this is a RabbitMQ variant
    pub fn as_rabbitmq(&self) -> Option<&Arc<RabbitMqClient>> {
        match self {
            Self::RabbitMq(client) => Some(client),
            _ => None,
        }
    }

    /// Get the underlying in-memory client if this is an in-memory variant
    pub fn as_in_memory(&self) -> Option<&Arc<InMemoryClient>> {
        match self {
            Self::InMemory(client) => Some(client),
            _ => None,
        }
    }
}

/// RabbitMQ client implementation (placeholder for future implementation)
#[derive(Debug)]
pub struct RabbitMqClient {
    _connection_url: String,
}

impl RabbitMqClient {
    pub async fn new(_connection_url: &str) -> TaskerResult<Self> {
        // TODO: Implement RabbitMQ client for TAS-35
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }
}

#[async_trait]
impl MessageClient for RabbitMqClient {
    async fn send_step_message(&self, _namespace: &str, _message: StepMessage) -> TaskerResult<()> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn send_simple_step_message(
        &self,
        _namespace: &str,
        _message: SimpleStepMessage,
    ) -> TaskerResult<()> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn receive_step_messages(
        &self,
        _namespace: &str,
        _limit: i32,
        _visibility_timeout: i32,
    ) -> TaskerResult<Vec<StepMessage>> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn send_step_result(&self, _result: StepExecutionResult) -> TaskerResult<()> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn send_task_request(&self, _request: TaskRequestMessage) -> TaskerResult<()> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn receive_task_requests(&self, _limit: i32) -> TaskerResult<Vec<TaskRequestMessage>> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn send_step_result_message(&self, _result: StepResultMessage) -> TaskerResult<()> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn receive_step_result_messages(
        &self,
        _limit: i32,
    ) -> TaskerResult<Vec<StepResultMessage>> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn initialize_namespace_queues(&self, _namespaces: &[&str]) -> TaskerResult<()> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn create_queue(&self, _queue_name: &str) -> TaskerResult<()> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn delete_message(&self, _queue_name: &str, _message_id: i64) -> TaskerResult<()> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    async fn get_queue_metrics(&self, _queue_name: &str) -> TaskerResult<QueueMetrics> {
        Err(crate::TaskerError::MessagingError(
            "RabbitMQ client not yet implemented".to_string(),
        ))
    }

    fn client_type(&self) -> &'static str {
        "rabbitmq"
    }

    async fn get_client_status(&self) -> TaskerResult<ClientStatus> {
        Ok(ClientStatus {
            client_type: "rabbitmq".to_string(),
            connected: false,
            connection_info: HashMap::new(),
            last_activity: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_client_creation() {
        let in_memory_client = UnifiedMessageClient::new_in_memory();
        assert_eq!(in_memory_client.client_type(), "in_memory");
    }

    #[test]
    fn test_client_type_variants() {
        let in_memory = UnifiedMessageClient::new_in_memory();
        assert_eq!(in_memory.client_type(), "in_memory");

        // Test variant accessors
        assert!(in_memory.as_in_memory().is_some());
        assert!(in_memory.as_pgmq().is_none());
        assert!(in_memory.as_rabbitmq().is_none());
    }
}
