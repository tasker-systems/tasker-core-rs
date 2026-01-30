//! # Task Finalization Hydrator
//!
//! Hydrates task finalization requests from PGMQ messages.
//!
//! ## Purpose
//!
//! Workers and orchestration components send finalization notifications to the
//! orchestration_task_finalization queue. This hydrator extracts the task_uuid
//! from these messages for finalization processing.
//!
//! ## Process
//!
//! 1. Parse PGMQ message payload
//! 2. Extract task_uuid field
//! 3. Validate UUID format
//! 4. Return task_uuid for finalization

use pgmq::Message as PgmqMessage;
use tasker_shared::messaging::service::QueuedMessage;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{debug, error, info};
use uuid::Uuid;

/// Hydrates task_uuid from finalization messages
///
/// This service extracts and validates task_uuid from PGMQ finalization messages.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::orchestration::hydration::FinalizationHydrator;
///
/// # async fn example(message: pgmq::Message) -> tasker_shared::TaskerResult<()> {
/// let hydrator = FinalizationHydrator::new();
/// let task_uuid = hydrator.hydrate_from_message(&message).await?;
/// // task_uuid is now ready for finalization processing
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct FinalizationHydrator;

impl FinalizationHydrator {
    /// Create a new FinalizationHydrator
    pub fn new() -> Self {
        Self
    }

    /// Hydrate task_uuid from PGMQ finalization message
    ///
    /// Performs message parsing and validation:
    /// 1. Extract task_uuid field from message
    /// 2. Parse UUID string
    /// 3. Validate format
    ///
    /// # Arguments
    ///
    /// * `message` - PGMQ message containing task_uuid
    ///
    /// # Returns
    ///
    /// Validated `Uuid` ready for finalization processing
    ///
    /// # Errors
    ///
    /// - `ValidationError`: Invalid or missing task_uuid
    pub async fn hydrate_from_message(&self, message: &PgmqMessage) -> TaskerResult<Uuid> {
        debug!(
            msg_id = message.msg_id,
            message_size = message.message.to_string().len(),
            "HYDRATOR: Starting finalization hydration"
        );

        // Extract task_uuid from message
        let task_uuid = message
            .message
            .get("task_uuid")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| {
                error!(
                    msg_id = message.msg_id,
                    message_content = %message.message,
                    "HYDRATOR: Invalid or missing task_uuid in finalization message"
                );
                TaskerError::ValidationError(
                    "Invalid or missing task_uuid in finalization message".to_string(),
                )
            })?;

        info!(
            msg_id = message.msg_id,
            task_uuid = %task_uuid,
            "HYDRATOR: Successfully extracted task_uuid from finalization message"
        );

        Ok(task_uuid)
    }

    /// TAS-133: Hydrate task_uuid from provider-agnostic QueuedMessage
    ///
    /// This is the provider-agnostic version of hydrate_from_message, working with
    /// `QueuedMessage<serde_json::Value>` instead of PGMQ-specific `PgmqMessage`.
    pub async fn hydrate_from_queued_message(
        &self,
        message: &QueuedMessage<serde_json::Value>,
    ) -> TaskerResult<Uuid> {
        debug!(
            handle = ?message.handle,
            message_size = message.message.to_string().len(),
            "HYDRATOR: Starting finalization hydration from QueuedMessage"
        );

        // Extract task_uuid from message
        let task_uuid = message
            .message
            .get("task_uuid")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| {
                error!(
                    handle = ?message.handle,
                    message_content = %message.message,
                    "HYDRATOR: Invalid or missing task_uuid in finalization message"
                );
                TaskerError::ValidationError(
                    "Invalid or missing task_uuid in finalization message".to_string(),
                )
            })?;

        info!(
            handle = ?message.handle,
            task_uuid = %task_uuid,
            "HYDRATOR: Successfully extracted task_uuid from QueuedMessage"
        );

        Ok(task_uuid)
    }
}

impl Default for FinalizationHydrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use tasker_shared::messaging::service::{MessageHandle, MessageMetadata};

    fn create_pgmq_message(payload: serde_json::Value) -> PgmqMessage {
        PgmqMessage {
            msg_id: 1,
            message: payload,
            vt: Utc::now(),
            read_ct: 1,
            enqueued_at: Utc::now(),
        }
    }

    fn create_queued_message(payload: serde_json::Value) -> QueuedMessage<serde_json::Value> {
        QueuedMessage::with_handle(
            payload,
            MessageHandle::Pgmq {
                msg_id: 1,
                queue_name: "test_finalization_queue".to_string(),
            },
            MessageMetadata::new(1, Utc::now()),
        )
    }

    // --- Construction and trait tests ---

    #[test]
    fn test_finalization_hydrator_construction() {
        let _hydrator = FinalizationHydrator::new();
        let _hydrator = FinalizationHydrator;
    }

    #[test]
    fn test_default_impl() {
        let hydrator = FinalizationHydrator::default();
        // Verify Debug impl produces expected output
        let debug_str = format!("{:?}", hydrator);
        assert_eq!(debug_str, "FinalizationHydrator");
    }

    // --- hydrate_from_message tests (PgmqMessage) ---

    #[tokio::test]
    async fn test_hydrate_from_message_valid_uuid() {
        let hydrator = FinalizationHydrator::new();
        let uuid = Uuid::now_v7();
        let message = create_pgmq_message(json!({"task_uuid": uuid.to_string()}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), uuid);
    }

    #[tokio::test]
    async fn test_hydrate_from_message_missing_task_uuid() {
        let hydrator = FinalizationHydrator::new();
        let message = create_pgmq_message(json!({"other_field": "value"}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("task_uuid"),
            "Error should mention task_uuid: {err}"
        );
    }

    #[tokio::test]
    async fn test_hydrate_from_message_invalid_uuid_string() {
        let hydrator = FinalizationHydrator::new();
        let message = create_pgmq_message(json!({"task_uuid": "not-a-valid-uuid"}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_message_null_task_uuid() {
        let hydrator = FinalizationHydrator::new();
        let message = create_pgmq_message(json!({"task_uuid": null}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_message_numeric_task_uuid() {
        let hydrator = FinalizationHydrator::new();
        let message = create_pgmq_message(json!({"task_uuid": 12345}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_message_empty_object() {
        let hydrator = FinalizationHydrator::new();
        let message = create_pgmq_message(json!({}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_message_empty_string_uuid() {
        let hydrator = FinalizationHydrator::new();
        let message = create_pgmq_message(json!({"task_uuid": ""}));

        let result = hydrator.hydrate_from_message(&message).await;
        assert!(result.is_err());
    }

    // --- hydrate_from_queued_message tests (QueuedMessage) ---

    #[tokio::test]
    async fn test_hydrate_from_queued_message_valid_uuid() {
        let hydrator = FinalizationHydrator::new();
        let uuid = Uuid::now_v7();
        let message = create_queued_message(json!({"task_uuid": uuid.to_string()}));

        let result = hydrator.hydrate_from_queued_message(&message).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), uuid);
    }

    #[tokio::test]
    async fn test_hydrate_from_queued_message_missing_task_uuid() {
        let hydrator = FinalizationHydrator::new();
        let message = create_queued_message(json!({"other": "value"}));

        let result = hydrator.hydrate_from_queued_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_queued_message_invalid_uuid() {
        let hydrator = FinalizationHydrator::new();
        let message = create_queued_message(json!({"task_uuid": "invalid-uuid-format"}));

        let result = hydrator.hydrate_from_queued_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_queued_message_null_task_uuid() {
        let hydrator = FinalizationHydrator::new();
        let message = create_queued_message(json!({"task_uuid": null}));

        let result = hydrator.hydrate_from_queued_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hydrate_from_queued_message_extra_fields_ignored() {
        let hydrator = FinalizationHydrator::new();
        let uuid = Uuid::now_v7();
        let message = create_queued_message(json!({
            "task_uuid": uuid.to_string(),
            "extra_field": "ignored",
            "another": 42
        }));

        let result = hydrator.hydrate_from_queued_message(&message).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), uuid);
    }
}
