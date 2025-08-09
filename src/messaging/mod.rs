//! # Messaging Module
//!
//! PostgreSQL message queue (pgmq) based messaging for workflow orchestration.
//! Provides queue-based task and step processing, replacing the TCP command architecture.

pub mod execution_types;
pub mod message;
pub mod orchestration_messages;
pub mod pgmq_client;
pub mod protected_pgmq_client;

pub use execution_types::{
    StepBatchRequest, StepBatchResponse, StepExecutionError, StepExecutionRequest,
    StepExecutionResult, StepRequestMetadata, StepResultMetadata,
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
    async fn create_queue(&self, queue_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    /// Send step message to queue
    async fn send_message(&self, queue_name: &str, message: &PgmqStepMessage) -> Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Send generic JSON message to queue
    async fn send_json_message<T: serde::Serialize + Clone + Send + Sync>(&self, queue_name: &str, message: &T) -> Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Read messages from queue
    async fn read_messages(&self, queue_name: &str, visibility_timeout: Option<i32>, qty: Option<i32>) -> Result<Vec<pgmq::types::Message<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Delete message from queue
    async fn delete_message(&self, queue_name: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    /// Archive message (move to archive)
    async fn archive_message(&self, queue_name: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    /// Purge queue (delete all messages)
    async fn purge_queue(&self, queue_name: &str) -> Result<u64, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Drop queue completely
    async fn drop_queue(&self, queue_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    /// Get queue metrics/statistics
    async fn queue_metrics(&self, queue_name: &str) -> Result<QueueMetrics, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Initialize standard namespace queues
    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    /// Send step execution message to namespace queue
    async fn enqueue_step(&self, namespace: &str, step_message: PgmqStepMessage) -> Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Process messages from namespace queue
    async fn process_namespace_queue(&self, namespace: &str, visibility_timeout: Option<i32>, batch_size: i32) -> Result<Vec<pgmq::types::Message<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Complete message processing (delete from queue)
    async fn complete_message(&self, namespace: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
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
    async fn create_queue(&self, queue_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.create_queue(queue_name).await,
            Self::Protected(client) => client.create_queue(queue_name).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn send_message(&self, queue_name: &str, message: &PgmqStepMessage) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.send_message(queue_name, message).await,
            Self::Protected(client) => client.send_message(queue_name, message).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn send_json_message<T: serde::Serialize + Clone + Send + Sync>(&self, queue_name: &str, message: &T) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.send_json_message(queue_name, message).await,
            Self::Protected(client) => client.send_json_message(queue_name, message).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn read_messages(&self, queue_name: &str, visibility_timeout: Option<i32>, qty: Option<i32>) -> Result<Vec<pgmq::types::Message<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.read_messages(queue_name, visibility_timeout, qty).await,
            Self::Protected(client) => client.read_messages(queue_name, visibility_timeout, qty).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn delete_message(&self, queue_name: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.delete_message(queue_name, message_id).await,
            Self::Protected(client) => client.delete_message(queue_name, message_id).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn archive_message(&self, queue_name: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.archive_message(queue_name, message_id).await,
            Self::Protected(client) => client.archive_message(queue_name, message_id).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn purge_queue(&self, queue_name: &str) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.purge_queue(queue_name).await,
            Self::Protected(client) => client.purge_queue(queue_name).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn drop_queue(&self, queue_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.drop_queue(queue_name).await,
            Self::Protected(client) => client.drop_queue(queue_name).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn queue_metrics(&self, queue_name: &str) -> Result<QueueMetrics, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.queue_metrics(queue_name).await,
            Self::Protected(client) => client.queue_metrics(queue_name).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.initialize_namespace_queues(namespaces).await,
            Self::Protected(client) => client.initialize_namespace_queues(namespaces).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
    
    async fn enqueue_step(&self, namespace: &str, step_message: PgmqStepMessage) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.enqueue_step(namespace, step_message).await,
            Self::Protected(client) => {
                // ProtectedPgmqClient delegates to inner client for this method
                client.inner().enqueue_step(namespace, step_message).await
            }
        }
    }
    
    async fn process_namespace_queue(&self, namespace: &str, visibility_timeout: Option<i32>, batch_size: i32) -> Result<Vec<pgmq::types::Message<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.process_namespace_queue(namespace, visibility_timeout, batch_size).await,
            Self::Protected(client) => {
                // ProtectedPgmqClient delegates to inner client for this method
                client.inner().process_namespace_queue(namespace, visibility_timeout, batch_size).await
            }
        }
    }
    
    async fn complete_message(&self, namespace: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Standard(client) => client.complete_message(namespace, message_id).await,
            Self::Protected(client) => {
                // ProtectedPgmqClient delegates to inner client for this method  
                client.inner().complete_message(namespace, message_id).await
            }
        }
    }
}
