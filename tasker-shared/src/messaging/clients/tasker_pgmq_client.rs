//! # Tasker-Specific PGMQ Client Extensions
//!
//! This module extends the pgmq-notify PgmqClient with tasker-specific functionality
//! using Rust's trait system. This approach keeps pgmq-notify generic while adding
//! domain-specific methods and types to tasker-shared.

use chrono::{DateTime, Utc};
use pgmq_notify::PgmqClient;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};
use uuid::Uuid;

use crate::messaging::{clients::types::ClientStatus, errors::MessagingResult};

/// Tasker-specific step message for PGMQ queues
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PgmqStepMessage {
    /// Step UUID for tracking
    pub step_uuid: Uuid,
    /// Task UUID this step belongs to
    pub task_uuid: Uuid,
    /// Namespace for routing
    pub namespace: String,
    /// Name of the step
    pub step_name: String,
    /// Step payload data
    pub step_payload: serde_json::Value,
    /// Message metadata
    pub metadata: PgmqStepMessageMetadata,
}

/// Metadata for tasker PGMQ step messages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PgmqStepMessageMetadata {
    /// When message was enqueued
    pub enqueued_at: DateTime<Utc>,
    /// Current retry count
    pub retry_count: i32,
    /// Maximum retry attempts
    pub max_retries: i32,
    /// Timeout in seconds (optional)
    pub timeout_seconds: Option<i64>,
}

impl Default for PgmqStepMessageMetadata {
    fn default() -> Self {
        Self {
            enqueued_at: Utc::now(),
            retry_count: 0,
            max_retries: 3,
            timeout_seconds: Some(30),
        }
    }
}

/// Trait to extend PgmqClient with tasker-specific functionality
pub trait TaskerPgmqClientExt {
    /// Send tasker-specific step message to queue
    fn send_step_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> impl std::future::Future<Output = MessagingResult<i64>> + Send;

    /// Read tasker-specific step messages from queue
    fn read_step_messages(
        &self,
        queue_name: &str,
        visibility_timeout: Option<i32>,
        qty: Option<i32>,
    ) -> impl std::future::Future<Output = MessagingResult<Vec<PgmqStepMessage>>> + Send;

    /// Enqueue step message to namespace queue (tasker-specific workflow pattern)
    fn enqueue_step_message(
        &self,
        namespace: &str,
        step_message: PgmqStepMessage,
    ) -> impl std::future::Future<Output = MessagingResult<i64>> + Send;

    /// Process namespace queue (batch read with workflow context)
    fn process_namespace_step_messages(
        &self,
        namespace: &str,
        visibility_timeout: Option<i32>,
        batch_size: i32,
    ) -> impl std::future::Future<Output = MessagingResult<Vec<PgmqStepMessage>>> + Send;

    /// Complete step message processing (delete with workflow context)
    fn complete_step_message(
        &self,
        namespace: &str,
        message_id: i64,
    ) -> impl std::future::Future<Output = MessagingResult<()>> + Send;

    /// Health check with comprehensive status for tasker integration
    fn tasker_health_check(
        &self,
    ) -> impl std::future::Future<Output = MessagingResult<ClientStatus>> + Send;
}

/// Tasker-specific extensions to PgmqClient
impl TaskerPgmqClientExt for PgmqClient {
    /// Send tasker-specific step message to queue
    #[instrument(skip(self, message), fields(queue = %queue_name, step_uuid = %message.step_uuid))]
    async fn send_step_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> MessagingResult<i64> {
        debug!(
            "📤 Sending step message to queue: {} for step: {}",
            queue_name, message.step_uuid
        );

        // Use wrapper function to ensure notifications are sent atomically
        let serialized = serde_json::to_value(message)
            .map_err(|e| crate::messaging::MessagingError::message_serialization(e.to_string()))?;

        let message_id = sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2, $3)",
            queue_name,
            &serialized,
            0i32
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            crate::messaging::MessagingError::database_query("pgmq_send_with_notify", e.to_string())
        })?
        .ok_or_else(|| {
            crate::messaging::MessagingError::internal("Wrapper function returned NULL message ID")
        })?;

        info!(
            "✅ Step message sent to queue: {} with ID: {}",
            queue_name, message_id
        );
        Ok(message_id)
    }

    /// Read tasker-specific step messages from queue
    #[instrument(skip(self), fields(queue = %queue_name))]
    async fn read_step_messages(
        &self,
        queue_name: &str,
        visibility_timeout: Option<i32>,
        qty: Option<i32>,
    ) -> MessagingResult<Vec<PgmqStepMessage>> {
        let messages = self
            .read_messages(queue_name, visibility_timeout, qty)
            .await
            .map_err(|e| crate::messaging::MessagingError::from(e))?;

        let mut step_messages = Vec::new();
        for msg in messages {
            if let Ok(step_msg) = serde_json::from_value::<PgmqStepMessage>(msg.message) {
                step_messages.push(step_msg);
            }
        }

        debug!(
            "📥 Read {} step messages from queue: {}",
            step_messages.len(),
            queue_name
        );

        Ok(step_messages)
    }

    /// Enqueue step message to namespace queue (tasker-specific workflow pattern)
    async fn enqueue_step_message(
        &self,
        namespace: &str,
        step_message: PgmqStepMessage,
    ) -> MessagingResult<i64> {
        let queue_name = format!("worker_{}_queue", namespace);
        self.send_step_message(&queue_name, &step_message).await
    }

    /// Process namespace queue (batch read with workflow context)
    async fn process_namespace_step_messages(
        &self,
        namespace: &str,
        visibility_timeout: Option<i32>,
        batch_size: i32,
    ) -> MessagingResult<Vec<PgmqStepMessage>> {
        let queue_name = format!("worker_{}_queue", namespace);
        self.read_step_messages(&queue_name, visibility_timeout, Some(batch_size))
            .await
    }

    /// Complete step message processing (delete with workflow context)
    async fn complete_step_message(&self, namespace: &str, message_id: i64) -> MessagingResult<()> {
        let queue_name = format!("worker_{}_queue", namespace);
        self.delete_message(&queue_name, message_id)
            .await
            .map_err(|e| crate::messaging::MessagingError::from(e))?;

        debug!(
            "✅ Step message processing completed: namespace={}, message_id={}",
            namespace, message_id
        );

        Ok(())
    }

    /// Health check with comprehensive status for tasker integration
    async fn tasker_health_check(&self) -> MessagingResult<ClientStatus> {
        // Test basic connectivity by trying to get client status
        let status = self
            .get_client_status()
            .await
            .map_err(|e| crate::messaging::MessagingError::from(e))?;

        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_step_message() -> PgmqStepMessage {
        PgmqStepMessage {
            step_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            namespace: "test".to_string(),
            step_name: "test_step".to_string(),
            step_payload: serde_json::json!({"test": true}),
            metadata: PgmqStepMessageMetadata::default(),
        }
    }

    #[test]
    fn test_step_message_creation() {
        let msg = create_test_step_message();
        assert!(!msg.step_uuid.is_nil());
        assert!(!msg.task_uuid.is_nil());
        assert_eq!(msg.namespace, "test");
        assert_eq!(msg.step_name, "test_step");
    }

    #[test]
    fn test_step_message_metadata_default() {
        let metadata = PgmqStepMessageMetadata::default();
        assert_eq!(metadata.retry_count, 0);
        assert_eq!(metadata.max_retries, 3);
        assert_eq!(metadata.timeout_seconds, Some(30));
    }

    #[test]
    fn test_step_message_serialization() {
        let msg = create_test_step_message();
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: PgmqStepMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(msg.step_uuid, deserialized.step_uuid);
        assert_eq!(msg.task_uuid, deserialized.task_uuid);
        assert_eq!(msg.namespace, deserialized.namespace);
        assert_eq!(msg.step_name, deserialized.step_name);
    }
}
