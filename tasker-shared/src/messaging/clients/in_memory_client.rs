//! # In-Memory Message Client Implementation (TAS-40 Phase 5)
//!
//! In-memory message queue client implementation for testing purposes.
//! Provides a complete MessageClient implementation that stores messages
//! in memory with proper visibility timeout semantics.
//!
//! ## Key Features
//!
//! - **Full MessageClient Implementation**: Complete async trait implementation
//! - **Visibility Timeout Support**: Proper message claiming with timeout semantics
//! - **Testing-Optimized**: Designed for unit tests and integration testing
//! - **Queue Management**: Dynamic queue creation and message lifecycle management
//! - **Observability**: Queue metrics and client status for monitoring
//!
//! ## Usage
//!
//! ```rust
//! use crate::messaging::clients::{UnifiedMessageClient, MessageClient};
//!
//! let client = UnifiedMessageClient::new_in_memory();
//! client.initialize_namespace_queues(&["test_namespace"]).await?;
//! client.send_step_message("test_namespace", step_message).await?;
//! let messages = client.receive_step_messages("test_namespace", 10, 30).await?;
//! ```

use async_trait::async_trait;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

use crate::messaging::{
    execution_types::StepExecutionResult,
    message::{SimpleStepMessage, StepExecutionContext, StepMessage, StepMessageMetadata},
    orchestration_messages::{StepResultMessage, TaskRequestMessage},
};
use crate::TaskerResult;

use super::{
    traits::MessageClient,
    types::{ClientStatus, QueueMetrics},
};

/// Message wrapper for in-memory storage with metadata
#[derive(Debug, Clone)]
pub struct InMemoryMessage {
    pub id: i64,
    pub content: Value,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
    pub visibility_timeout: Option<chrono::DateTime<chrono::Utc>>,
}

/// In-memory queue storage
#[derive(Debug, Default)]
pub struct InMemoryQueue {
    pub messages: VecDeque<InMemoryMessage>,
    pub next_id: i64,
}

/// In-memory client implementation for testing
#[derive(Debug)]
pub struct InMemoryClient {
    queues: tokio::sync::Mutex<HashMap<String, InMemoryQueue>>,
    created_at: chrono::DateTime<chrono::Utc>,
    last_activity: tokio::sync::Mutex<chrono::DateTime<chrono::Utc>>,
}

impl InMemoryClient {
    pub fn new() -> Self {
        let now = chrono::Utc::now();
        Self {
            queues: tokio::sync::Mutex::new(HashMap::new()),
            created_at: now,
            last_activity: tokio::sync::Mutex::new(now),
        }
    }

    /// Update last activity timestamp
    async fn update_activity(&self) {
        *self.last_activity.lock().await = chrono::Utc::now();
    }

    /// Get queue name for namespace (matches tasker_pgmq_client naming)
    fn get_queue_name(&self, namespace: &str) -> String {
        format!("worker_{}_queue", namespace)
    }

    /// Send a message to a specific queue
    async fn send_message_to_queue(&self, queue_name: &str, content: Value) -> TaskerResult<i64> {
        self.update_activity().await;
        let mut queues = self.queues.lock().await;
        let queue = queues.entry(queue_name.to_string()).or_default();

        let message_id = queue.next_id;
        queue.next_id += 1;

        let message = InMemoryMessage {
            id: message_id,
            content,
            enqueued_at: chrono::Utc::now(),
            visibility_timeout: None,
        };

        queue.messages.push_back(message);
        Ok(message_id)
    }

    /// Receive messages from a specific queue
    async fn receive_messages_from_queue(
        &self,
        queue_name: &str,
        limit: i32,
        visibility_timeout_seconds: i32,
    ) -> TaskerResult<Vec<(i64, Value)>> {
        self.update_activity().await;
        let mut queues = self.queues.lock().await;
        let queue = queues.entry(queue_name.to_string()).or_default();

        let now = chrono::Utc::now();
        let visibility_timeout = now + chrono::Duration::seconds(visibility_timeout_seconds as i64);

        let mut result = Vec::new();
        let mut messages_to_update = Vec::new();

        // Find available messages (not currently claimed or expired claims)
        for (index, message) in queue.messages.iter().enumerate() {
            if result.len() >= limit as usize {
                break;
            }

            // Check if message is available (no timeout or timeout expired)
            let is_available = message
                .visibility_timeout
                .map(|timeout| timeout <= now)
                .unwrap_or(true);

            if is_available {
                result.push((message.id, message.content.clone()));
                messages_to_update.push(index);
            }
        }

        // Update visibility timeout for claimed messages
        for index in messages_to_update {
            if let Some(message) = queue.messages.get_mut(index) {
                message.visibility_timeout = Some(visibility_timeout);
            }
        }

        Ok(result)
    }
}

impl Default for InMemoryClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageClient for InMemoryClient {
    async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()> {
        let queue_name = self.get_queue_name(namespace);
        let message_json = serde_json::to_value(&message).map_err(|e| {
            crate::TaskerError::ValidationError(format!("JSON serialization error: {}", e))
        })?;
        self.send_message_to_queue(&queue_name, message_json)
            .await?;
        Ok(())
    }

    async fn send_simple_step_message(
        &self,
        namespace: &str,
        message: SimpleStepMessage,
    ) -> TaskerResult<()> {
        let queue_name = self.get_queue_name(namespace);
        let message_json = serde_json::to_value(&message).map_err(|e| {
            crate::TaskerError::ValidationError(format!("JSON serialization error: {}", e))
        })?;
        self.send_message_to_queue(&queue_name, message_json)
            .await?;
        Ok(())
    }

    async fn receive_step_messages(
        &self,
        namespace: &str,
        limit: i32,
        visibility_timeout: i32,
    ) -> TaskerResult<Vec<StepMessage>> {
        let queue_name = self.get_queue_name(namespace);
        let raw_messages = self
            .receive_messages_from_queue(&queue_name, limit, visibility_timeout)
            .await?;

        let mut step_messages = Vec::new();
        for (_, content) in raw_messages {
            match serde_json::from_value::<StepMessage>(content.clone()) {
                Ok(step_message) => step_messages.push(step_message),
                Err(e) => {
                    // Try to parse as SimpleStepMessage and convert
                    if let Ok(simple_message) = serde_json::from_value::<SimpleStepMessage>(content)
                    {
                        // Convert SimpleStepMessage to StepMessage (simplified conversion for testing)
                        let step_message = StepMessage {
                            step_uuid: simple_message.step_uuid,
                            task_uuid: simple_message.task_uuid,
                            namespace: namespace.to_string(),
                            task_name: "test_task".to_string(), // Default for testing
                            task_version: "1.0.0".to_string(),  // Default for testing
                            step_name: "test_step".to_string(), // Default for testing
                            step_payload: serde_json::json!({}), // Empty payload for testing
                            execution_context: StepExecutionContext {
                                task: serde_json::json!({}),
                                sequence: vec![],
                                step: serde_json::json!({}),
                                additional_context: HashMap::new(),
                            }, // Empty context for testing
                            metadata: StepMessageMetadata {
                                created_at: chrono::Utc::now(),
                                retry_count: 0,
                                max_retries: 3,
                                timeout_ms: 30000,
                                correlation_id: Some(Uuid::new_v4().to_string()),
                                priority: 5,
                                context: HashMap::new(),
                            },
                        };
                        step_messages.push(step_message);
                    } else {
                        // Log error but don't fail the whole operation
                        eprintln!("Failed to deserialize step message: {}", e);
                    }
                }
            }
        }

        Ok(step_messages)
    }

    async fn send_step_result(&self, result: StepExecutionResult) -> TaskerResult<()> {
        let queue_name = "orchestration_step_results_queue";
        let result_json = serde_json::to_value(&result).map_err(|e| {
            crate::TaskerError::ValidationError(format!("JSON serialization error: {}", e))
        })?;
        self.send_message_to_queue(queue_name, result_json).await?;
        Ok(())
    }

    async fn send_task_request(&self, request: TaskRequestMessage) -> TaskerResult<()> {
        let queue_name = "orchestration_task_requests_queue";
        let request_json = serde_json::to_value(&request).map_err(|e| {
            crate::TaskerError::ValidationError(format!("JSON serialization error: {}", e))
        })?;
        self.send_message_to_queue(queue_name, request_json).await?;
        Ok(())
    }

    async fn receive_task_requests(&self, limit: i32) -> TaskerResult<Vec<TaskRequestMessage>> {
        let queue_name = "orchestration_task_requests_queue";
        let raw_messages = self
            .receive_messages_from_queue(queue_name, limit, 30)
            .await?;

        let mut task_requests = Vec::new();
        for (_, content) in raw_messages {
            match serde_json::from_value::<TaskRequestMessage>(content) {
                Ok(task_request) => task_requests.push(task_request),
                Err(e) => eprintln!("Failed to deserialize task request: {}", e),
            }
        }

        Ok(task_requests)
    }

    async fn send_step_result_message(&self, result: StepResultMessage) -> TaskerResult<()> {
        let queue_name = "orchestration_step_results_queue";
        let result_json = serde_json::to_value(&result).map_err(|e| {
            crate::TaskerError::ValidationError(format!("JSON serialization error: {}", e))
        })?;
        self.send_message_to_queue(queue_name, result_json).await?;
        Ok(())
    }

    async fn receive_step_result_messages(
        &self,
        limit: i32,
    ) -> TaskerResult<Vec<StepResultMessage>> {
        let queue_name = "orchestration_step_results_queue";
        let raw_messages = self
            .receive_messages_from_queue(queue_name, limit, 30)
            .await?;

        let mut step_results = Vec::new();
        for (_, content) in raw_messages {
            match serde_json::from_value::<StepResultMessage>(content) {
                Ok(step_result) => step_results.push(step_result),
                Err(e) => eprintln!("Failed to deserialize step result: {}", e),
            }
        }

        Ok(step_results)
    }

    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        for namespace in namespaces {
            let queue_name = self.get_queue_name(namespace);
            self.create_queue(&queue_name).await?;
        }

        // Also create orchestration queues
        self.create_queue("orchestration_task_requests").await?;
        self.create_queue("orchestration_step_results").await?;

        Ok(())
    }

    async fn create_queue(&self, queue_name: &str) -> TaskerResult<()> {
        self.update_activity().await;
        let mut queues = self.queues.lock().await;
        queues.entry(queue_name.to_string()).or_default();
        Ok(())
    }

    async fn delete_message(&self, queue_name: &str, message_id: i64) -> TaskerResult<()> {
        self.update_activity().await;
        let mut queues = self.queues.lock().await;
        if let Some(queue) = queues.get_mut(queue_name) {
            // Find and remove the message with the given ID
            if let Some(pos) = queue.messages.iter().position(|msg| msg.id == message_id) {
                queue.messages.remove(pos);
            }
        }
        Ok(())
    }

    async fn get_queue_metrics(&self, queue_name: &str) -> TaskerResult<QueueMetrics> {
        self.update_activity().await;
        let queues = self.queues.lock().await;

        let message_count = queues
            .get(queue_name)
            .map(|queue| queue.messages.len() as i64)
            .unwrap_or(0);

        let oldest_message_age_seconds = if let Some(queue) = queues.get(queue_name) {
            queue.messages.front().map(|msg| {
                let duration = chrono::Utc::now() - msg.enqueued_at;
                duration.num_seconds()
            })
        } else {
            None
        };

        Ok(QueueMetrics {
            queue_name: queue_name.to_string(),
            message_count,
            consumer_count: None, // In-memory doesn't track consumers
            oldest_message_age_seconds,
        })
    }

    fn client_type(&self) -> &'static str {
        "in_memory"
    }

    async fn get_client_status(&self) -> TaskerResult<ClientStatus> {
        let last_activity = *self.last_activity.lock().await;
        let queues = self.queues.lock().await;
        let queue_count = queues.len();
        let total_messages: i64 = queues.values().map(|q| q.messages.len() as i64).sum();

        let mut connection_info = HashMap::new();
        connection_info.insert("type".to_string(), Value::String("testing".to_string()));
        connection_info.insert(
            "created_at".to_string(),
            Value::String(self.created_at.to_rfc3339()),
        );
        connection_info.insert("queue_count".to_string(), Value::Number(queue_count.into()));
        connection_info.insert(
            "total_messages".to_string(),
            Value::Number(total_messages.into()),
        );

        Ok(ClientStatus {
            client_type: "in_memory".to_string(),
            connected: true,
            connection_info,
            last_activity: Some(last_activity),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_client_basic_operations() {
        let client = InMemoryClient::new();

        // Test queue creation
        assert!(client.create_queue("test_queue").await.is_ok());

        // Test metrics
        let metrics = client.get_queue_metrics("test_queue").await.unwrap();
        assert_eq!(metrics.queue_name, "test_queue");
        assert_eq!(metrics.message_count, 0);

        // Test status
        let status = client.get_client_status().await.unwrap();
        assert_eq!(status.client_type, "in_memory");
        assert!(status.connected);
    }

    #[tokio::test]
    async fn test_in_memory_message_flow() {
        let client = InMemoryClient::new();
        let namespace = "test_namespace";

        // Initialize namespace queues
        client
            .initialize_namespace_queues(&[namespace])
            .await
            .unwrap();

        // Create test step message
        let step_message = StepMessage {
            step_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            namespace: namespace.to_string(),
            task_name: "test_task".to_string(),
            task_version: "1.0.0".to_string(),
            step_name: "test_step".to_string(),
            step_payload: serde_json::json!({"test": "data"}),
            execution_context: StepExecutionContext {
                task: serde_json::json!({}),
                sequence: vec![],
                step: serde_json::json!({}),
                additional_context: HashMap::new(),
            },
            metadata: StepMessageMetadata {
                created_at: chrono::Utc::now(),
                retry_count: 0,
                max_retries: 3,
                timeout_ms: 30000,
                correlation_id: Some(Uuid::new_v4().to_string()),
                priority: 5,
                context: HashMap::new(),
            },
        };

        // Send step message
        client
            .send_step_message(namespace, step_message.clone())
            .await
            .unwrap();

        // Verify queue metrics
        let queue_name = format!("worker_{}_queue", namespace);
        let metrics = client.get_queue_metrics(&queue_name).await.unwrap();
        assert_eq!(metrics.message_count, 1);

        // Receive step messages
        let received_messages = client
            .receive_step_messages(namespace, 10, 30)
            .await
            .unwrap();
        assert_eq!(received_messages.len(), 1);
        assert_eq!(received_messages[0].step_uuid, step_message.step_uuid);
        assert_eq!(received_messages[0].task_uuid, step_message.task_uuid);
    }

    #[tokio::test]
    async fn test_in_memory_simple_step_message_flow() {
        let client = InMemoryClient::new();
        let namespace = "simple_test";

        // Initialize namespace queues
        client
            .initialize_namespace_queues(&[namespace])
            .await
            .unwrap();

        // Create test simple step message
        let simple_message = SimpleStepMessage {
            step_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
        };

        // Send simple step message
        client
            .send_simple_step_message(namespace, simple_message.clone())
            .await
            .unwrap();

        // Verify queue metrics
        let queue_name = format!("worker_{}_queue", namespace);
        let metrics = client.get_queue_metrics(&queue_name).await.unwrap();
        assert_eq!(metrics.message_count, 1);

        // Receive step messages (should be converted to full StepMessage)
        let received_messages = client
            .receive_step_messages(namespace, 10, 30)
            .await
            .unwrap();
        assert_eq!(received_messages.len(), 1);
        assert_eq!(received_messages[0].step_uuid, simple_message.step_uuid);
        assert_eq!(received_messages[0].task_uuid, simple_message.task_uuid);
        assert_eq!(received_messages[0].namespace, namespace);
    }

    #[tokio::test]
    async fn test_in_memory_visibility_timeout() {
        let client = InMemoryClient::new();
        let namespace = "timeout_test";

        client
            .initialize_namespace_queues(&[namespace])
            .await
            .unwrap();

        // Send test message
        let step_message = StepMessage {
            step_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            namespace: namespace.to_string(),
            task_name: "test_task".to_string(),
            task_version: "1.0.0".to_string(),
            step_name: "test_step".to_string(),
            step_payload: serde_json::json!({}),
            execution_context: StepExecutionContext {
                task: serde_json::json!({}),
                sequence: vec![],
                step: serde_json::json!({}),
                additional_context: HashMap::new(),
            },
            metadata: StepMessageMetadata {
                created_at: chrono::Utc::now(),
                retry_count: 0,
                max_retries: 3,
                timeout_ms: 30000,
                correlation_id: Some(Uuid::new_v4().to_string()),
                priority: 5,
                context: HashMap::new(),
            },
        };

        client
            .send_step_message(namespace, step_message.clone())
            .await
            .unwrap();

        // First receive should get the message
        let messages1 = client
            .receive_step_messages(namespace, 10, 5)
            .await
            .unwrap();
        assert_eq!(messages1.len(), 1);

        // Second immediate receive should not get the message (visibility timeout)
        let messages2 = client
            .receive_step_messages(namespace, 10, 5)
            .await
            .unwrap();
        assert_eq!(messages2.len(), 0);

        // Sleep for visibility timeout to expire, then receive again
        tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
        let messages3 = client
            .receive_step_messages(namespace, 10, 5)
            .await
            .unwrap();
        assert_eq!(messages3.len(), 1);
    }
}
