//! # PGMQ Message Resolver
//!
//! Encapsulates PGMQ-specific infrastructure for resolving signal-only notifications
//! into full provider-agnostic `QueuedMessage` objects.
//!
//! ## Context
//!
//! In the three-flow lifecycle model:
//! - **Flow 3 (FromEvent)** is PGMQ-only: a `MessageEvent` signal arrives via LISTEN/NOTIFY,
//!   and the resolver fetches the full message payload from the PGMQ queue.
//! - The resolved `QueuedMessage` is then handed off to **Flow 2 (FromMessage)** for
//!   hydration, processing, and acknowledgment.
//!
//! This struct owns the PGMQ-specific logic that was previously inlined in the service.

use std::sync::Arc;

use pgmq::Message as PgmqMessage;
use tracing::{debug, error};

use tasker_shared::messaging::client::MessageClient;
use tasker_shared::messaging::service::{MessageEvent, MessageHandle, MessageMetadata, QueuedMessage};
use tasker_shared::{TaskerError, TaskerResult};

/// Resolves PGMQ signal-only notifications into full `QueuedMessage` objects.
///
/// PGMQ's large-message flow (>7KB) sends a lightweight signal via LISTEN/NOTIFY
/// containing only the queue name and message ID. This resolver fetches the actual
/// message payload from the PGMQ queue using `read_specific_message`.
#[derive(Debug)]
pub(crate) struct PgmqMessageResolver {
    message_client: Arc<MessageClient>,
}

impl PgmqMessageResolver {
    pub fn new(message_client: Arc<MessageClient>) -> Self {
        Self { message_client }
    }

    /// Resolve a `MessageEvent` signal into a full `QueuedMessage`.
    ///
    /// Validates the provider supports fetch-by-message-ID, parses the message ID,
    /// reads the specific message from PGMQ, and wraps it as a provider-agnostic
    /// `QueuedMessage`.
    ///
    /// The `entity_label` is used in error messages (e.g., "step result", "task request").
    pub async fn resolve_message_event(
        &self,
        event: &MessageEvent,
        entity_label: &str,
    ) -> TaskerResult<QueuedMessage<serde_json::Value>> {
        // TAS-133: Guard clause - this code path is only valid for PGMQ
        let provider = self.message_client.provider();
        if !provider.supports_fetch_by_message_id() {
            error!(
                provider = provider.provider_name(),
                msg_id = %event.message_id,
                queue = %event.queue_name,
                "CRITICAL: Signal-only notification received but provider does not support fetch-by-message-ID"
            );
            return Err(TaskerError::MessagingError(format!(
                "Provider '{}' does not support fetch-by-message-ID flow.",
                provider.provider_name()
            )));
        }

        let msg_id: i64 = event.message_id.as_str().parse().map_err(|e| {
            TaskerError::ValidationError(format!(
                "Invalid message ID '{}' for PGMQ: {e}",
                event.message_id
            ))
        })?;

        debug!(
            msg_id = msg_id,
            queue = %event.queue_name,
            entity = entity_label,
            "Fetching message from PGMQ for signal-only notification"
        );

        let message = self
            .pgmq_client()?
            .read_specific_message::<serde_json::Value>(&event.queue_name, msg_id, 30)
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!(
                    "Failed to read specific {} message {} from queue {}: {e}",
                    entity_label, msg_id, event.queue_name
                ))
            })?
            .ok_or_else(|| {
                TaskerError::MessagingError(format!(
                    "{} message {} not found in queue {}",
                    entity_label, msg_id, event.queue_name
                ))
            })?;

        Ok(Self::wrap_pgmq_message(&event.queue_name, message))
    }

    /// Get the underlying PGMQ client for event-driven operations (TAS-133).
    ///
    /// Returns an error if the provider is not PGMQ.
    fn pgmq_client(&self) -> TaskerResult<&tasker_pgmq::PgmqClient> {
        self.message_client
            .provider()
            .as_pgmq()
            .map(|s| s.client())
            .ok_or_else(|| {
                TaskerError::ConfigurationError(
                    "Event-driven operations require PGMQ provider. Use polling mode with RabbitMQ."
                        .to_string(),
                )
            })
    }

    /// Convert a PGMQ message to a provider-agnostic `QueuedMessage`.
    fn wrap_pgmq_message(
        queue_name: &str,
        message: PgmqMessage,
    ) -> QueuedMessage<serde_json::Value> {
        QueuedMessage::with_handle(
            message.message.clone(),
            MessageHandle::Pgmq {
                msg_id: message.msg_id,
                queue_name: queue_name.to_string(),
            },
            MessageMetadata::new(message.read_ct as u32, message.enqueued_at),
        )
    }
}
