//! # Domain Event Infrastructure (TAS-65 Phase 2.1)
//!
//! Provides queue management and event publication for domain-level events.
//! Domain events are business-level events published from worker execution contexts
//! with full tracing correlation from Phase 1.5.
//!
//! ## Architecture
//!
//! - **Queue Management**: Creates namespace-specific domain event queues and DLQs
//! - **Event Publication**: Publishes events with metadata to PGMQ with notifications
//! - **Correlation**: Reuses correlation_id from Phase 1.5 worker span instrumentation
//! - **Observability**: OpenTelemetry metrics for event publication
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use tasker_shared::events::domain_events::{DomainEventPublisher, EventMetadata};
//! use tasker_shared::messaging::UnifiedPgmqClient;
//! use std::sync::Arc;
//! use serde_json::json;
//!
//! # async fn example(message_client: Arc<UnifiedPgmqClient>) -> Result<(), Box<dyn std::error::Error>> {
//! let publisher = DomainEventPublisher::new(message_client);
//!
//! let metadata = EventMetadata {
//!     task_uuid: uuid::Uuid::new_v4(),
//!     step_uuid: Some(uuid::Uuid::new_v4()),
//!     step_name: Some("process_order".to_string()),
//!     namespace: "payments".to_string(),
//!     correlation_id: uuid::Uuid::new_v4(),
//!     fired_at: chrono::Utc::now(),
//!     fired_by: "OrderProcessor".to_string(),
//! };
//!
//! let event_id = publisher.publish_event(
//!     "order.processed",
//!     json!({"order_id": 123, "amount": 99.99}),
//!     metadata
//! ).await?;
//! # Ok(())
//! # }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use crate::messaging::{errors::MessagingError, PgmqClientTrait, UnifiedPgmqClient};

/// Domain event with metadata for business-level events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEvent {
    /// Unique event identifier (UUID v7 for time-ordering)
    pub event_id: Uuid,
    /// Event name in dot notation (e.g., "order.processed")
    pub event_name: String,
    /// Event schema version for evolution
    pub event_version: String,
    /// Event payload as JSON
    pub payload: serde_json::Value,
    /// Event metadata for tracing and context
    pub metadata: EventMetadata,
}

/// Event metadata for correlation and tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Task UUID this event belongs to
    pub task_uuid: Uuid,
    /// Optional step UUID if fired from step execution
    pub step_uuid: Option<Uuid>,
    /// Optional step name if fired from step execution
    pub step_name: Option<String>,
    /// Namespace for queue routing
    pub namespace: String,
    /// Correlation ID from Phase 1.5 worker instrumentation
    pub correlation_id: Uuid,
    /// Timestamp when event was fired
    pub fired_at: DateTime<Utc>,
    /// Component that fired the event (e.g., handler class name)
    pub fired_by: String,
}

/// Publishes domain events to namespace queues
#[derive(Clone)]
pub struct DomainEventPublisher {
    message_client: Arc<UnifiedPgmqClient>,
}

impl DomainEventPublisher {
    /// Create a new domain event publisher with message client
    pub fn new(message_client: Arc<UnifiedPgmqClient>) -> Self {
        Self { message_client }
    }

    /// Publish a domain event with metadata
    ///
    /// Creates a `DomainEvent` with UUID v7 for time-ordering, publishes to the
    /// namespace-specific queue using `UnifiedPgmqClient`, and emits OpenTelemetry metrics.
    ///
    /// Returns the generated event_id on success.
    #[instrument(skip(self, payload, metadata), fields(
        event_name = %event_name,
        namespace = %metadata.namespace,
        correlation_id = %metadata.correlation_id
    ))]
    pub async fn publish_event(
        &self,
        event_name: &str,
        payload: serde_json::Value,
        metadata: EventMetadata,
    ) -> Result<Uuid, DomainEventError> {
        let event_id = Uuid::now_v7();
        let queue_name = format!("{}_domain_events", metadata.namespace);

        debug!(
            event_id = %event_id,
            event_name = %event_name,
            queue_name = %queue_name,
            task_uuid = %metadata.task_uuid,
            correlation_id = %metadata.correlation_id,
            "Publishing domain event"
        );

        // Create domain event
        let event = DomainEvent {
            event_id,
            event_name: event_name.to_string(),
            event_version: "1.0".to_string(), // Default version
            payload,
            metadata: metadata.clone(),
        };

        // Serialize event
        let event_json =
            serde_json::to_value(&event).map_err(|e| DomainEventError::SerializationFailed {
                event_name: event_name.to_string(),
                reason: e.to_string(),
            })?;

        // Publish to PGMQ using UnifiedPgmqClient (uses pgmq_send_with_notify internally)
        let message_id = self
            .message_client
            .send_json_message(&queue_name, &event_json)
            .await
            .map_err(|e| DomainEventError::PublishFailed {
                event_name: event_name.to_string(),
                queue_name: queue_name.clone(),
                reason: e.to_string(),
            })?;

        // Emit OpenTelemetry metric
        let counter = opentelemetry::global::meter("tasker")
            .u64_counter("tasker.domain_events.published.total")
            .with_description("Total number of domain events published")
            .build();

        counter.add(
            1,
            &[
                opentelemetry::KeyValue::new("event_name", event_name.to_string()),
                opentelemetry::KeyValue::new("namespace", metadata.namespace.clone()),
            ],
        );

        info!(
            event_id = %event_id,
            message_id = message_id,
            event_name = %event_name,
            queue_name = %queue_name,
            correlation_id = %metadata.correlation_id,
            "Domain event published successfully"
        );

        Ok(event_id)
    }
}

// Manual Debug implementation for DomainEventPublisher
impl std::fmt::Debug for DomainEventPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainEventPublisher")
            .field("message_client", &"<UnifiedPgmqClient>")
            .finish()
    }
}

/// Errors for domain event operations
#[derive(Debug, thiserror::Error)]
pub enum DomainEventError {
    #[error("Failed to serialize event {event_name}: {reason}")]
    SerializationFailed { event_name: String, reason: String },

    #[error("Failed to publish event {event_name} to {queue_name}: {reason}")]
    PublishFailed {
        event_name: String,
        queue_name: String,
        reason: String,
    },

    #[error("Messaging error: {0}")]
    MessagingError(#[from] MessagingError),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create test metadata
    fn create_test_metadata(namespace: &str) -> EventMetadata {
        EventMetadata {
            task_uuid: Uuid::new_v4(),
            step_uuid: Some(Uuid::new_v4()),
            step_name: Some("test_step".to_string()),
            namespace: namespace.to_string(),
            correlation_id: Uuid::new_v4(),
            fired_at: Utc::now(),
            fired_by: "TestHandler".to_string(),
        }
    }

    #[test]
    fn test_event_metadata_serialization() {
        let metadata = EventMetadata {
            task_uuid: Uuid::new_v4(),
            step_uuid: Some(Uuid::new_v4()),
            step_name: Some("test_step".to_string()),
            namespace: "test_namespace".to_string(),
            correlation_id: Uuid::new_v4(),
            fired_at: Utc::now(),
            fired_by: "TestHandler".to_string(),
        };

        let json = serde_json::to_value(&metadata).expect("Failed to serialize metadata");
        let deserialized: EventMetadata =
            serde_json::from_value(json).expect("Failed to deserialize metadata");

        assert_eq!(metadata.task_uuid, deserialized.task_uuid);
        assert_eq!(metadata.step_uuid, deserialized.step_uuid);
        assert_eq!(metadata.namespace, deserialized.namespace);
        assert_eq!(metadata.correlation_id, deserialized.correlation_id);
    }

    #[test]
    fn test_domain_event_serialization() {
        let metadata = create_test_metadata("test_namespace");
        let event = DomainEvent {
            event_id: Uuid::now_v7(),
            event_name: "test.event".to_string(),
            event_version: "1.0".to_string(),
            payload: serde_json::json!({"key": "value"}),
            metadata,
        };

        let json = serde_json::to_value(&event).expect("Failed to serialize event");
        let deserialized: DomainEvent =
            serde_json::from_value(json).expect("Failed to deserialize event");

        assert_eq!(event.event_id, deserialized.event_id);
        assert_eq!(event.event_name, deserialized.event_name);
        assert_eq!(event.event_version, deserialized.event_version);
        assert_eq!(event.payload, deserialized.payload);
    }
}
