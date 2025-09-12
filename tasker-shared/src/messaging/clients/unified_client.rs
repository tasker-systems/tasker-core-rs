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
//! use std::sync::Arc;
//! use tasker_shared::messaging::clients::UnifiedMessageClient;
//! 
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

use super::in_memory_client::InMemoryClient;
use super::traits::MessageClient;
use super::TaskerPgmqClientExt;
use pgmq_notify::PgmqClient;

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
            Self::Pgmq(client) => {
                MessageClient::send_step_message(client.as_ref(), namespace, message).await
            }
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
            Self::Pgmq(client) => client
                .get_client_status()
                .await
                .map_err(|e| crate::TaskerError::MessagingError(e.to_string())),
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

// MessageClient implementation for PgmqClient (pgmq-notify unified client)
#[async_trait]
impl MessageClient for PgmqClient {
    async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()> {
        // Convert StepMessage to PgmqStepMessage
        let pgmq_message = super::types::PgmqStepMessage {
            step_uuid: message.step_uuid,
            task_uuid: message.task_uuid,
            namespace: namespace.to_string(),
            step_name: message.step_name.clone(),
            step_payload: message.step_payload.clone(),
            metadata: super::types::PgmqStepMessageMetadata {
                enqueued_at: chrono::Utc::now(),
                retry_count: 0,
                max_retries: 3,
                timeout_seconds: Some((message.metadata.timeout_ms / 1000) as i64),
            },
        };

        let queue_name = format!("worker_{}_queue", namespace);
        TaskerPgmqClientExt::send_step_message(self, &queue_name, &pgmq_message)
            .await
            .map_err(|e| {
                crate::TaskerError::MessagingError(format!("Failed to send step message: {}", e))
            })?;
        Ok(())
    }

    async fn send_simple_step_message(
        &self,
        namespace: &str,
        message: SimpleStepMessage,
    ) -> TaskerResult<()> {
        // Convert SimpleStepMessage to a basic JSON message and use wrapper function
        let queue_name = format!("worker_{}_queue", namespace);
        let serialized = serde_json::to_value(&message).map_err(|e| {
            crate::TaskerError::MessagingError(format!("Serialization error: {}", e))
        })?;

        sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2, $3)",
            &queue_name,
            &serialized,
            0i32
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            crate::TaskerError::MessagingError(format!("Failed to send simple message: {}", e))
        })?
        .ok_or_else(|| {
            crate::TaskerError::MessagingError(
                "Wrapper function returned NULL message ID".to_string(),
            )
        })?;
        Ok(())
    }

    async fn receive_step_messages(
        &self,
        namespace: &str,
        limit: i32,
        visibility_timeout: i32,
    ) -> TaskerResult<Vec<StepMessage>> {
        let queue_name = format!("worker_{}_queue", namespace);
        let messages = self
            .read_messages(&queue_name, Some(visibility_timeout), Some(limit))
            .await
            .map_err(|e| {
                crate::TaskerError::MessagingError(format!("Failed to receive messages: {}", e))
            })?;

        // Convert pgmq messages to StepMessage
        let mut step_messages = Vec::new();
        for msg in messages {
            // Parse the message as StepMessage directly
            if let Ok(step_msg) = serde_json::from_value::<StepMessage>(msg.message) {
                step_messages.push(step_msg);
            }
        }

        Ok(step_messages)
    }

    async fn send_step_result(&self, result: StepExecutionResult) -> TaskerResult<()> {
        let serialized = serde_json::to_value(&result).map_err(|e| {
            crate::TaskerError::MessagingError(format!("Serialization error: {}", e))
        })?;

        sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2, $3)",
            "orchestration_step_results",
            &serialized,
            0i32
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            crate::TaskerError::MessagingError(format!("Failed to send step result: {}", e))
        })?
        .ok_or_else(|| {
            crate::TaskerError::MessagingError(
                "Wrapper function returned NULL message ID".to_string(),
            )
        })?;
        Ok(())
    }

    async fn send_task_request(&self, request: TaskRequestMessage) -> TaskerResult<()> {
        let serialized = serde_json::to_value(&request).map_err(|e| {
            crate::TaskerError::MessagingError(format!("Serialization error: {}", e))
        })?;

        sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2, $3)",
            "orchestration_task_requests_queue",
            &serialized,
            0i32
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            crate::TaskerError::MessagingError(format!("Failed to send task request: {}", e))
        })?
        .ok_or_else(|| {
            crate::TaskerError::MessagingError(
                "Wrapper function returned NULL message ID".to_string(),
            )
        })?;
        Ok(())
    }

    async fn receive_task_requests(&self, limit: i32) -> TaskerResult<Vec<TaskRequestMessage>> {
        let messages = self
            .read_messages("orchestration_task_requests_queue", Some(30), Some(limit))
            .await
            .map_err(|e| {
                crate::TaskerError::MessagingError(format!(
                    "Failed to receive task requests: {}",
                    e
                ))
            })?;

        let mut requests = Vec::new();
        for msg in messages {
            if let Ok(request) = serde_json::from_value::<TaskRequestMessage>(msg.message) {
                requests.push(request);
            }
        }

        Ok(requests)
    }

    async fn send_step_result_message(&self, result: StepResultMessage) -> TaskerResult<()> {
        let serialized = serde_json::to_value(&result).map_err(|e| {
            crate::TaskerError::MessagingError(format!("Serialization error: {}", e))
        })?;

        sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2, $3)",
            "orchestration_step_results_queue",
            &serialized,
            0i32
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            crate::TaskerError::MessagingError(format!("Failed to send step result: {}", e))
        })?
        .ok_or_else(|| {
            crate::TaskerError::MessagingError(
                "Wrapper function returned NULL message ID".to_string(),
            )
        })?;
        Ok(())
    }

    async fn receive_step_result_messages(
        &self,
        limit: i32,
    ) -> TaskerResult<Vec<StepResultMessage>> {
        let messages = self
            .read_messages("orchestration_step_results_queue", Some(30), Some(limit))
            .await
            .map_err(|e| {
                crate::TaskerError::MessagingError(format!("Failed to receive step results: {}", e))
            })?;

        let mut results = Vec::new();
        for msg in messages {
            if let Ok(result) = serde_json::from_value::<StepResultMessage>(msg.message) {
                results.push(result);
            }
        }

        Ok(results)
    }

    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        self.initialize_namespace_queues(namespaces)
            .await
            .map_err(|e| {
                crate::TaskerError::MessagingError(format!(
                    "Failed to initialize namespace queues: {}",
                    e
                ))
            })
    }

    async fn create_queue(&self, queue_name: &str) -> TaskerResult<()> {
        self.create_queue(queue_name).await.map_err(|e| {
            crate::TaskerError::MessagingError(format!("Failed to create queue: {}", e))
        })
    }

    async fn delete_message(&self, queue_name: &str, message_id: i64) -> TaskerResult<()> {
        self.delete_message(queue_name, message_id)
            .await
            .map_err(|e| {
                crate::TaskerError::MessagingError(format!("Failed to delete message: {}", e))
            })
    }

    async fn get_queue_metrics(&self, queue_name: &str) -> TaskerResult<QueueMetrics> {
        let metrics = self.queue_metrics(queue_name).await.map_err(|e| {
            crate::TaskerError::MessagingError(format!("Failed to get queue metrics: {}", e))
        })?;
        Ok(metrics)
    }

    fn client_type(&self) -> &'static str {
        "pgmq"
    }

    async fn get_client_status(&self) -> TaskerResult<ClientStatus> {
        // Create a basic ClientStatus since PgmqClient doesn't have a get_client_status method
        Ok(ClientStatus {
            client_type: "pgmq".to_string(),
            connected: true, // Assume connected if we can reach this method
            connection_info: HashMap::new(),
            last_activity: Some(chrono::Utc::now()),
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
