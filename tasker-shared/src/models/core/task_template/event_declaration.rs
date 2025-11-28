//! # TAS-65: Event Declaration Module
//!
//! Types for declaring domain events in task templates.
//!
//! ## Overview
//!
//! This module provides types for configuring domain event publication in workflow step definitions.
//! Events are declared in YAML task templates and validated at template load time.
//!
//! ## Architecture Decision
//!
//! Events are published via **post-execution publisher callbacks** rather than in-handler calls.
//! This enforces configuration-as-contract, provides clean separation of concerns, and structurally
//! guarantees that event failures never affect workflow step execution.
//!
//! See: `docs/ticket-specs/TAS-65/domain-event-publication.md` for full architectural rationale.
//!
//! ## YAML Example
//!
//! ```yaml
//! steps:
//!   - name: process_payment
//!     handler:
//!       callable: PaymentHandler
//!     publishes_events:
//!       - name: payment.processed
//!         description: "Payment successfully processed"
//!         condition: success  # New field for conditional publishing
//!         schema:
//!           type: object
//!           required: [transaction_id, amount]
//!           properties:
//!             transaction_id: { type: string }
//!             amount: { type: number }
//!         delivery_mode: durable
//!         publisher: PaymentEventPublisher  # optional custom publisher
//!       - name: payment.failed
//!         description: "Payment processing failed"
//!         condition: failure
//!         schema:
//!           type: object
//!           required: [error_code, reason]
//!           properties:
//!             error_code: { type: string }
//!             reason: { type: string }
//!         delivery_mode: fast  # New variant for fire-and-forget
//! ```

use bon::Builder;
use jsonschema::JSONSchema;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Event delivery mode configuration
///
/// Controls how events are delivered to subscribers. Each mode has different
/// reliability, latency, and ordering guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventDeliveryMode {
    /// Durable delivery via PGMQ with at-least-once guarantees
    ///
    /// Events are persisted to PostgreSQL message queues (PGMQ) before
    /// acknowledgment. Provides durability, retry on failure, and
    /// at-least-once delivery semantics.
    ///
    /// **Trade-offs:**
    /// - Pro: Durable, survives crashes, reliable delivery
    /// - Pro: Automatic retry with backoff
    /// - Con: Higher latency (~5-10ms) due to database persistence
    /// - Con: Potential for duplicate delivery (at-least-once)
    ///
    /// **Use cases:** Critical business events, audit trails, cross-service
    /// coordination where reliability is paramount.
    Durable,

    /// Fast fire-and-forget delivery with no persistence
    ///
    /// Events are published in-memory without database persistence.
    /// Provides minimal latency but no durability guarantees.
    ///
    /// **Trade-offs:**
    /// - Pro: Very low latency (<1ms)
    /// - Pro: No database overhead
    /// - Con: Events lost on crash or failure
    /// - Con: No retry mechanism
    ///
    /// **Use cases:** Non-critical notifications, metrics, telemetry,
    /// logging where occasional loss is acceptable.
    Fast,
}

/// Publication condition for event triggering
///
/// Controls when an event should be published based on step execution outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublicationCondition {
    /// Publish only on successful step completion
    Success,

    /// Publish only on step failure
    Failure,

    /// Publish regardless of step outcome
    Always,
}

impl Default for PublicationCondition {
    fn default() -> Self {
        Self::Success
    }
}

/// Event declaration for workflow step event publishing (TAS-65)
///
/// Declares an event that a workflow step can publish during execution.
/// Includes schema validation via JSON Schema, conditional publishing, and delivery mode configuration.
///
/// ## Schema Validation
///
/// The `schema` field contains a JSON Schema (draft-07 compatible) that
/// validates event payloads at runtime. Invalid payloads are rejected
/// before publishing, ensuring downstream consumers receive well-formed data.
///
/// ## Conditional Publishing
///
/// The `condition` field controls when events are published:
/// - `success`: Only publish if step completes successfully
/// - `failure`: Only publish if step fails
/// - `always`: Publish regardless of outcome
///
/// ## Delivery Modes
///
/// - `durable`: Persistent delivery via PGMQ with at-least-once guarantees
/// - `fast`: Fire-and-forget in-memory delivery with no persistence
///
/// ## Custom Publishers
///
/// The optional `publisher` field allows specifying a custom publisher class
/// for complex event logic. If not specified, the generic publisher is used.
///
/// ## Example
///
/// ```yaml
/// publishes_events:
///   - name: order.created
///     description: "New order successfully created"
///     condition: success
///     schema:
///       type: object
///       properties:
///         order_id: { type: string, format: uuid }
///         customer_id: { type: string }
///         total_amount: { type: number, minimum: 0 }
///       required: [order_id, customer_id, total_amount]
///     delivery_mode: durable
///     publisher: OrderEventPublisher  # optional
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
pub struct EventDeclaration {
    /// Event name in dotted notation (e.g., "order.created", "payment.authorized")
    pub name: String,

    /// Human-readable description of when this event is published
    pub description: String,

    /// Condition for publishing: success, failure, or always
    #[serde(default)]
    #[builder(default)]
    pub condition: PublicationCondition,

    /// JSON Schema for validating event payloads
    ///
    /// Must be a valid JSON Schema (draft-07 compatible).
    /// Schema validation occurs before event publishing to ensure
    /// payload correctness.
    pub schema: serde_json::Value,

    /// Delivery mode for event distribution
    #[serde(default = "default_delivery_mode")]
    #[builder(default = EventDeliveryMode::Durable)]
    pub delivery_mode: EventDeliveryMode,

    /// Optional custom publisher class name for complex event logic
    ///
    /// If specified, must reference a registered publisher class.
    /// If None, the generic publisher is used.
    pub publisher: Option<String>,
}

fn default_delivery_mode() -> EventDeliveryMode {
    EventDeliveryMode::Durable
}

impl EventDeclaration {
    /// Get the event namespace (prefix before first dot)
    ///
    /// Example: "order.created" → "order"
    pub fn namespace(&self) -> Option<&str> {
        self.name.split('.').next()
    }

    /// Get the event action (suffix after last dot)
    ///
    /// Example: "order.items.added" → "added"
    pub fn action(&self) -> Option<&str> {
        self.name.split('.').last()
    }

    /// Validates the event name format
    ///
    /// Event names must:
    /// - Use lowercase letters, numbers, underscores, and dots
    /// - Have at least one dot (namespace.action format)
    /// - Not start or end with a dot
    /// - Not have consecutive dots
    ///
    /// # Errors
    ///
    /// Returns an error if the event name is invalid
    pub fn validate_name(&self) -> Result<(), String> {
        // Must have at least one dot
        if !self.name.contains('.') {
            return Err(format!(
                "Event name '{}' must contain at least one dot (namespace.action format)",
                self.name
            ));
        }

        // Must not start or end with dot
        if self.name.starts_with('.') || self.name.ends_with('.') {
            return Err(format!(
                "Event name '{}' must not start or end with a dot",
                self.name
            ));
        }

        // Must not have consecutive dots
        if self.name.contains("..") {
            return Err(format!(
                "Event name '{}' must not contain consecutive dots",
                self.name
            ));
        }

        // Must use valid characters (lowercase, numbers, underscores, dots)
        let valid_pattern = Regex::new(r"^[a-z0-9_.]+$").expect("Valid regex pattern");
        if !valid_pattern.is_match(&self.name) {
            return Err(format!(
                "Event name '{}' must contain only lowercase letters, numbers, underscores, and dots",
                self.name
            ));
        }

        Ok(())
    }

    /// Validates the JSON Schema
    ///
    /// Ensures the schema is a valid JSON Schema that can be compiled
    /// for payload validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the schema is invalid
    pub fn validate_schema(&self) -> Result<(), String> {
        JSONSchema::compile(&self.schema)
            .map_err(|e| format!("Invalid JSON Schema: {}", e))?;
        Ok(())
    }

    /// Validates an event payload against the declared schema
    ///
    /// # Arguments
    ///
    /// * `payload` - The event payload to validate
    ///
    /// # Errors
    ///
    /// Returns an error if the payload does not match the schema
    pub fn validate_payload(&self, payload: &serde_json::Value) -> Result<(), String> {
        let compiled_schema = JSONSchema::compile(&self.schema)
            .map_err(|e| format!("Failed to compile schema: {}", e))?;

        let result = compiled_schema.validate(payload);

        if let Err(errors) = result {
            let error_messages: Vec<String> = errors
                .into_iter()
                .map(|e| format!("{} at {}", e, e.instance_path))
                .collect();
            return Err(format!(
                "Payload validation failed: {}",
                error_messages.join("; ")
            ));
        }

        Ok(())
    }

    /// Validates the custom publisher name format
    ///
    /// Publisher names must:
    /// - Be valid Ruby class names (PascalCase)
    /// - May include module namespaces (::)
    /// - Start with an uppercase letter
    ///
    /// # Errors
    ///
    /// Returns an error if the publisher name is invalid
    pub fn validate_publisher(&self) -> Result<(), String> {
        if let Some(publisher) = &self.publisher {
            // Must be a valid Ruby class name (PascalCase with :: for modules)
            let valid_pattern =
                Regex::new(r"^[A-Z][a-zA-Z0-9]*(::[A-Z][a-zA-Z0-9]*)*$")
                    .expect("Valid regex pattern");
            if !valid_pattern.is_match(publisher) {
                return Err(format!(
                    "Publisher name '{}' must be a valid Ruby class name (PascalCase, may include ::)",
                    publisher
                ));
            }
        }
        Ok(())
    }

    /// Validates the entire event declaration
    ///
    /// Performs all validation checks:
    /// - Event name format
    /// - JSON Schema validity
    /// - Publisher name format (if specified)
    ///
    /// # Errors
    ///
    /// Returns the first validation error encountered
    pub fn validate(&self) -> Result<(), String> {
        self.validate_name()?;
        self.validate_schema()?;
        self.validate_publisher()?;
        Ok(())
    }

    /// Checks if this event should be published for the given step outcome
    ///
    /// # Arguments
    ///
    /// * `step_succeeded` - Whether the step completed successfully
    ///
    /// # Returns
    ///
    /// `true` if the event should be published based on the condition
    pub fn should_publish(&self, step_succeeded: bool) -> bool {
        match self.condition {
            PublicationCondition::Success => step_succeeded,
            PublicationCondition::Failure => !step_succeeded,
            PublicationCondition::Always => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_name_validation_success() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .build();

        assert!(event.validate_name().is_ok());
    }

    #[test]
    fn test_event_name_validation_missing_dot() {
        let event = EventDeclaration::builder()
            .name("ordercreated".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .build();

        let result = event.validate_name();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must contain at least one dot"));
    }

    #[test]
    fn test_event_name_validation_starts_with_dot() {
        let event = EventDeclaration::builder()
            .name(".order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .build();

        let result = event.validate_name();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not start or end with a dot"));
    }

    #[test]
    fn test_event_name_validation_consecutive_dots() {
        let event = EventDeclaration::builder()
            .name("order..created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .build();

        let result = event.validate_name();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("consecutive dots"));
    }

    #[test]
    fn test_event_name_validation_invalid_characters() {
        let event = EventDeclaration::builder()
            .name("Order.Created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .build();

        let result = event.validate_name();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("lowercase letters, numbers, underscores"));
    }

    #[test]
    fn test_schema_validation_success() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"}
                },
                "required": ["id"]
            }))
            .build();

        assert!(event.validate_schema().is_ok());
    }

    #[test]
    fn test_schema_validation_invalid() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({
                "type": "invalid_type"
            }))
            .build();

        let result = event.validate_schema();
        assert!(result.is_err());
    }

    #[test]
    fn test_payload_validation_success() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "amount": {"type": "number"}
                },
                "required": ["id", "amount"]
            }))
            .build();

        let payload = json!({"id": "123", "amount": 99.99});
        assert!(event.validate_payload(&payload).is_ok());
    }

    #[test]
    fn test_payload_validation_failure_missing_required() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "amount": {"type": "number"}
                },
                "required": ["id", "amount"]
            }))
            .build();

        let payload = json!({"id": "123"});
        let result = event.validate_payload(&payload);
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("amount") || error_msg.contains("required"));
    }

    #[test]
    fn test_payload_validation_failure_wrong_type() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "amount": {"type": "number"}
                },
                "required": ["id", "amount"]
            }))
            .build();

        let payload = json!({"id": "123", "amount": "not_a_number"});
        let result = event.validate_payload(&payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_publisher_validation_success() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("OrderEventPublisher".to_string())
            .build();

        assert!(event.validate_publisher().is_ok());
    }

    #[test]
    fn test_publisher_validation_with_namespace() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("MyModule::OrderEventPublisher".to_string())
            .build();

        assert!(event.validate_publisher().is_ok());
    }

    #[test]
    fn test_publisher_validation_invalid_format() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("order_event_publisher".to_string())
            .build();

        let result = event.validate_publisher();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("must be a valid Ruby class name"));
    }

    #[test]
    fn test_complete_validation_success() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }))
            .publisher("OrderEventPublisher".to_string())
            .build();

        assert!(event.validate().is_ok());
    }

    #[test]
    fn test_namespace_extraction() {
        let event = EventDeclaration::builder()
            .name("order.items.added".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .build();

        assert_eq!(event.namespace(), Some("order"));
    }

    #[test]
    fn test_action_extraction() {
        let event = EventDeclaration::builder()
            .name("order.items.added".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .build();

        assert_eq!(event.action(), Some("added"));
    }

    #[test]
    fn test_should_publish_success_condition() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .condition(PublicationCondition::Success)
            .schema(json!({"type": "object"}))
            .build();

        assert!(event.should_publish(true));
        assert!(!event.should_publish(false));
    }

    #[test]
    fn test_should_publish_failure_condition() {
        let event = EventDeclaration::builder()
            .name("order.failed".to_string())
            .description("Test event".to_string())
            .condition(PublicationCondition::Failure)
            .schema(json!({"type": "object"}))
            .build();

        assert!(!event.should_publish(true));
        assert!(event.should_publish(false));
    }

    #[test]
    fn test_should_publish_always_condition() {
        let event = EventDeclaration::builder()
            .name("order.attempted".to_string())
            .description("Test event".to_string())
            .condition(PublicationCondition::Always)
            .schema(json!({"type": "object"}))
            .build();

        assert!(event.should_publish(true));
        assert!(event.should_publish(false));
    }

    #[test]
    fn test_delivery_mode_serialization() {
        let durable = EventDeliveryMode::Durable;
        let json = serde_json::to_string(&durable).unwrap();
        assert_eq!(json, "\"durable\"");

        let fast = EventDeliveryMode::Fast;
        let json = serde_json::to_string(&fast).unwrap();
        assert_eq!(json, "\"fast\"");
    }

    #[test]
    fn test_delivery_mode_deserialization() {
        let durable: EventDeliveryMode =
            serde_json::from_str("\"durable\"").unwrap();
        assert_eq!(durable, EventDeliveryMode::Durable);

        let fast: EventDeliveryMode = serde_json::from_str("\"fast\"").unwrap();
        assert_eq!(fast, EventDeliveryMode::Fast);
    }

    #[test]
    fn test_builder_defaults() {
        let event = EventDeclaration::builder()
            .name("test.event".to_string())
            .description("Test".to_string())
            .schema(json!({"type": "object"}))
            .build();

        assert_eq!(event.condition, PublicationCondition::Success);
        assert_eq!(event.delivery_mode, EventDeliveryMode::Durable);
        assert_eq!(event.publisher, None);
    }

    #[test]
    fn test_publication_condition_serialization() {
        let success = PublicationCondition::Success;
        assert_eq!(
            serde_json::to_string(&success).unwrap(),
            "\"success\""
        );

        let failure = PublicationCondition::Failure;
        assert_eq!(
            serde_json::to_string(&failure).unwrap(),
            "\"failure\""
        );

        let always = PublicationCondition::Always;
        assert_eq!(serde_json::to_string(&always).unwrap(), "\"always\"");
    }
}
