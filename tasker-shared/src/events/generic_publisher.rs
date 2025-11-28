//! # TAS-65 Phase 2: Generic Event Publisher
//!
//! High-level event publisher that validates payloads against EventDeclaration schemas
//! and delegates to DomainEventPublisher for actual publishing.
//!
//! ## Overview
//!
//! The GenericEventPublisher bridges the gap between YAML-declared events
//! (EventDeclaration) and the low-level DomainEventPublisher. It:
//!
//! - Validates event payloads against JSON schemas
//! - Checks publication conditions (success/failure/always)
//! - Ensures only declared events are published
//! - Provides a clean API for step handlers
//!
//! ## Architecture
//!
//! ```text
//! Step Handler
//!      ↓
//! GenericEventPublisher
//!      ├─> Validate schema
//!      ├─> Check condition
//!      └─> DomainEventPublisher → PGMQ
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use tasker_shared::events::generic_publisher::GenericEventPublisher;
//! use tasker_shared::events::domain_events::{DomainEventPublisher, EventMetadata};
//! use tasker_shared::models::EventDeclaration;
//! use serde_json::json;
//!
//! # async fn example(
//! #     domain_publisher: DomainEventPublisher,
//! #     event_decl: EventDeclaration,
//! #     metadata: EventMetadata,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let publisher = GenericEventPublisher::new(domain_publisher);
//!
//! // Publish an event declared in task template
//! let event_id = publisher.publish(
//!     &event_decl,
//!     json!({"order_id": "123", "amount": 99.99}),
//!     metadata,
//!     true  // step_succeeded
//! ).await?;
//! # Ok(())
//! # }
//! ```

use super::domain_events::{DomainEventPayload, DomainEventPublisher, EventMetadata};
use crate::messaging::execution_types::StepExecutionResult;
use crate::models::core::task_template::EventDeclaration;
use crate::types::base::TaskSequenceStep;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;

/// High-level event publisher with schema validation and condition checking
#[derive(Clone, Debug)]
pub struct GenericEventPublisher {
    /// Low-level domain event publisher (PGMQ-based)
    domain_publisher: Arc<DomainEventPublisher>,
}

impl GenericEventPublisher {
    /// Create a new generic event publisher
    ///
    /// # Arguments
    ///
    /// * `domain_publisher` - The underlying domain event publisher for PGMQ
    pub fn new(domain_publisher: DomainEventPublisher) -> Self {
        Self {
            domain_publisher: Arc::new(domain_publisher),
        }
    }

    /// Publish an event with full execution context
    ///
    /// Validates the business payload against the event's JSON schema, checks
    /// publication conditions based on execution result, and publishes with
    /// complete task and step context.
    ///
    /// # Arguments
    ///
    /// * `event_decl` - The event declaration from task template
    /// * `task_sequence_step` - The complete task sequence step that was executed
    /// * `execution_result` - The step execution result
    /// * `business_payload` - The business-specific event data
    /// * `metadata` - Event metadata for correlation and tracing
    ///
    /// # Returns
    ///
    /// - `Ok(Some(event_id))` - Event published successfully
    /// - `Ok(None)` - Event not published (condition not met)
    /// - `Err(error)` - Publishing failed
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Business payload validation fails against schema
    /// - Publishing to PGMQ fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::events::generic_publisher::GenericEventPublisher;
    /// # use tasker_shared::events::domain_events::{DomainEventPublisher, EventMetadata};
    /// # use tasker_shared::models::EventDeclaration;
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # use tasker_shared::messaging::execution_types::StepExecutionResult;
    /// # use serde_json::json;
    ///
    /// # async fn example(
    /// #     publisher: GenericEventPublisher,
    /// #     event_decl: EventDeclaration,
    /// #     tss: TaskSequenceStep,
    /// #     result: StepExecutionResult,
    /// #     metadata: EventMetadata,
    /// # ) -> Result<(), Box<dyn std::error::Error>> {
    /// let event_id = publisher.publish(
    ///     &event_decl,
    ///     tss,
    ///     result,
    ///     json!({"order_id": "123"}),
    ///     metadata,
    /// ).await?;
    ///
    /// match event_id {
    ///     Some(id) => println!("Published: {}", id),
    ///     None => println!("Not published (condition not met)"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn publish(
        &self,
        event_decl: &EventDeclaration,
        task_sequence_step: TaskSequenceStep,
        execution_result: StepExecutionResult,
        business_payload: Value,
        metadata: EventMetadata,
    ) -> Result<Option<Uuid>, GenericPublisherError> {
        // Check if event should be published based on execution result
        let step_succeeded = execution_result.success;
        if !event_decl.should_publish(step_succeeded) {
            debug!(
                event_name = %event_decl.name,
                condition = ?event_decl.condition,
                step_succeeded = step_succeeded,
                "Skipping event publication - condition not met"
            );
            return Ok(None);
        }

        // Validate business payload against schema
        if let Err(validation_error) = event_decl.validate_payload(&business_payload) {
            return Err(GenericPublisherError::SchemaValidationFailed {
                event_name: event_decl.name.clone(),
                validation_error,
            });
        }

        // Create full domain event payload with execution context
        let domain_payload = DomainEventPayload {
            task_sequence_step,
            execution_result,
            payload: business_payload,
        };

        // Publish event using domain publisher
        let event_id = self
            .domain_publisher
            .publish_event(&event_decl.name, domain_payload, metadata)
            .await
            .map_err(|e| GenericPublisherError::PublishFailed {
                event_name: event_decl.name.clone(),
                reason: e.to_string(),
            })?;

        Ok(Some(event_id))
    }

    /// Publish all declared events for a step based on execution outcome
    ///
    /// Convenience method to publish multiple events declared in a step.
    /// Validates each event and only publishes those matching the condition.
    ///
    /// # Arguments
    ///
    /// * `event_declarations` - All events declared in the step
    /// * `task_sequence_step` - The complete task sequence step that was executed
    /// * `execution_result` - The step execution result
    /// * `payloads` - Map of event names to business payloads
    /// * `metadata` - Event metadata for correlation
    ///
    /// # Returns
    ///
    /// A result containing published event IDs and any errors encountered
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::events::generic_publisher::GenericEventPublisher;
    /// # use tasker_shared::events::domain_events::EventMetadata;
    /// # use tasker_shared::models::EventDeclaration;
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # use tasker_shared::messaging::execution_types::StepExecutionResult;
    /// # use std::collections::HashMap;
    /// # use serde_json::json;
    ///
    /// # async fn example(
    /// #     publisher: GenericEventPublisher,
    /// #     declarations: Vec<EventDeclaration>,
    /// #     tss: TaskSequenceStep,
    /// #     result: StepExecutionResult,
    /// #     metadata: EventMetadata,
    /// # ) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut payloads = HashMap::new();
    /// payloads.insert("order.created".to_string(), json!({"order_id": "123"}));
    /// payloads.insert("payment.processed".to_string(), json!({"amount": 99.99}));
    ///
    /// let result = publisher.publish_all(
    ///     &declarations,
    ///     tss,
    ///     result,
    ///     payloads,
    ///     metadata,
    /// ).await;
    ///
    /// println!("Published {} events", result.published_count());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn publish_all(
        &self,
        event_declarations: &[EventDeclaration],
        task_sequence_step: TaskSequenceStep,
        execution_result: StepExecutionResult,
        payloads: std::collections::HashMap<String, Value>,
        metadata: EventMetadata,
    ) -> PublishAllResult {
        let mut published = Vec::new();
        let mut skipped = Vec::new();
        let mut errors = Vec::new();

        for event_decl in event_declarations {
            // Get payload for this event
            let payload = match payloads.get(&event_decl.name) {
                Some(p) => p.clone(),
                None => {
                    warn!(
                        event_name = %event_decl.name,
                        "No payload provided for declared event"
                    );
                    skipped.push(event_decl.name.clone());
                    continue;
                }
            };

            // Publish event
            match self
                .publish(
                    event_decl,
                    task_sequence_step.clone(),
                    execution_result.clone(),
                    payload,
                    metadata.clone(),
                )
                .await
            {
                Ok(Some(event_id)) => {
                    published.push(PublishedEvent {
                        event_name: event_decl.name.clone(),
                        event_id,
                    });
                }
                Ok(None) => {
                    skipped.push(event_decl.name.clone());
                }
                Err(error) => {
                    errors.push(PublishError {
                        event_name: event_decl.name.clone(),
                        error: error.to_string(),
                    });
                }
            }
        }

        PublishAllResult {
            published,
            skipped,
            errors,
        }
    }
}

/// Result of publishing multiple events
#[derive(Debug, Clone)]
pub struct PublishAllResult {
    /// Successfully published events
    pub published: Vec<PublishedEvent>,
    /// Skipped events (condition not met or no payload)
    pub skipped: Vec<String>,
    /// Failed events
    pub errors: Vec<PublishError>,
}

impl PublishAllResult {
    /// Get the count of successfully published events
    pub fn published_count(&self) -> usize {
        self.published.len()
    }

    /// Get the count of skipped events
    pub fn skipped_count(&self) -> usize {
        self.skipped.len()
    }

    /// Get the count of failed events
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Check if all events were published successfully
    pub fn all_succeeded(&self) -> bool {
        self.errors.is_empty()
    }

    /// Check if any events failed to publish
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Successfully published event
#[derive(Debug, Clone)]
pub struct PublishedEvent {
    /// Event name
    pub event_name: String,
    /// Generated event ID
    pub event_id: Uuid,
}

/// Failed event publication
#[derive(Debug, Clone)]
pub struct PublishError {
    /// Event name
    pub event_name: String,
    /// Error message
    pub error: String,
}

/// Errors for generic event publishing
#[derive(Debug, thiserror::Error)]
pub enum GenericPublisherError {
    #[error("Schema validation failed for event {event_name}: {validation_error}")]
    SchemaValidationFailed {
        event_name: String,
        validation_error: String,
    },

    #[error("Failed to publish event {event_name}: {reason}")]
    PublishFailed { event_name: String, reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::core::task_template::{EventDeliveryMode, PublicationCondition};
    use serde_json::json;

    fn create_test_event_decl(
        name: &str,
        condition: PublicationCondition,
    ) -> EventDeclaration {
        EventDeclaration::builder()
            .name(name.to_string())
            .description("Test event".to_string())
            .condition(condition)
            .schema(json!({
                "type": "object",
                "properties": {
                    "order_id": {"type": "string"},
                    "amount": {"type": "number"}
                },
                "required": ["order_id", "amount"]
            }))
            .delivery_mode(EventDeliveryMode::Durable)
            .build()
    }

    #[test]
    fn test_publish_all_result_counts() {
        let result = PublishAllResult {
            published: vec![
                PublishedEvent {
                    event_name: "event1".to_string(),
                    event_id: Uuid::new_v4(),
                },
                PublishedEvent {
                    event_name: "event2".to_string(),
                    event_id: Uuid::new_v4(),
                },
            ],
            skipped: vec!["event3".to_string()],
            errors: vec![PublishError {
                event_name: "event4".to_string(),
                error: "Test error".to_string(),
            }],
        };

        assert_eq!(result.published_count(), 2);
        assert_eq!(result.skipped_count(), 1);
        assert_eq!(result.error_count(), 1);
        assert!(!result.all_succeeded());
        assert!(result.has_errors());
    }

    #[test]
    fn test_publish_all_result_all_succeeded() {
        let result = PublishAllResult {
            published: vec![PublishedEvent {
                event_name: "event1".to_string(),
                event_id: Uuid::new_v4(),
            }],
            skipped: vec![],
            errors: vec![],
        };

        assert!(result.all_succeeded());
        assert!(!result.has_errors());
    }

    #[test]
    fn test_event_declaration_should_publish_success() {
        let event = create_test_event_decl("test.event", PublicationCondition::Success);
        assert!(event.should_publish(true));
        assert!(!event.should_publish(false));
    }

    #[test]
    fn test_event_declaration_should_publish_failure() {
        let event = create_test_event_decl("test.event", PublicationCondition::Failure);
        assert!(!event.should_publish(true));
        assert!(event.should_publish(false));
    }

    #[test]
    fn test_event_declaration_should_publish_always() {
        let event = create_test_event_decl("test.event", PublicationCondition::Always);
        assert!(event.should_publish(true));
        assert!(event.should_publish(false));
    }

    #[test]
    fn test_schema_validation() {
        let event = create_test_event_decl("test.event", PublicationCondition::Always);

        // Valid payload
        let valid_payload = json!({"order_id": "123", "amount": 99.99});
        assert!(event.validate_payload(&valid_payload).is_ok());

        // Invalid payload - missing required field
        let invalid_payload = json!({"order_id": "123"});
        assert!(event.validate_payload(&invalid_payload).is_err());

        // Invalid payload - wrong type
        let invalid_type = json!({"order_id": "123", "amount": "not a number"});
        assert!(event.validate_payload(&invalid_type).is_err());
    }

    // Note: Integration tests with actual PGMQ publishing would require
    // database setup and are better suited for integration test suite
}
