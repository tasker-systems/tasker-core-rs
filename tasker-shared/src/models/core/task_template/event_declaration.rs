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
use derive_more::Display;
use jsonschema::JSONSchema;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Event delivery mode configuration
///
/// Controls how events are delivered to subscribers. Each mode has different
/// reliability, latency, and ordering guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
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
    #[display("durable")]
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
    #[display("fast")]
    Fast,

    /// Broadcast delivery to both durable and fast paths
    ///
    /// Events are published to both PGMQ (durable) AND in-process bus (fast).
    /// Fast delivery occurs first for real-time responsiveness, then durable
    /// for reliable persistence.
    ///
    /// **Trade-offs:**
    /// - Pro: Both external consumers AND internal subscribers receive event
    /// - Pro: Fast path provides real-time internal notifications
    /// - Con: Data goes to public boundary - use carefully with sensitive data
    /// - Con: Slightly higher overhead than single-path delivery
    ///
    /// **Security note:** Any data published goes to BOTH the public PGMQ
    /// boundary AND internal subscribers. Do not use for sensitive data
    /// that should stay internal (use `fast` for internal-only events).
    ///
    /// **Use cases:** Events needing both audit trails (durable) and real-time
    /// internal processing (fast) like order completion with metrics tracking.
    #[display("broadcast")]
    Broadcast,
}

/// Publication condition for event triggering
///
/// Controls when an event should be published based on step execution outcome.
///
/// ## Condition Types
///
/// - `success`: Publish only when step completes successfully
/// - `failure`: Publish on any failure (backward compatible)
/// - `retryable_failure`: Publish only on retryable failures (DB determines retryability)
/// - `permanent_failure`: Publish only on permanent failures (exhausted retries or non-retryable)
/// - `always`: Publish regardless of outcome
///
/// ## Retryability Source
///
/// Retryability is determined from the database via `get_step_readiness_status()`,
/// not from the execution result. The DB knows the authoritative state including
/// retry attempts remaining.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, Default)]
#[serde(rename_all = "snake_case")]
pub enum PublicationCondition {
    /// Publish only on successful step completion
    #[display("success")]
    #[default]
    Success,

    /// Publish only on step failure (any type - backward compatible)
    #[display("failure")]
    Failure,

    /// Publish only on retryable failures
    ///
    /// Step failed but can be retried (retryable=true AND attempts < max_attempts).
    /// Useful for internal alerting systems that want early notification of transient issues.
    #[display("retryable_failure")]
    RetryableFailure,

    /// Publish only on permanent failures
    ///
    /// Step failed and cannot be retried (retryable=false OR attempts >= max_attempts).
    /// Useful for external DLQ consumers and escalation workflows.
    #[display("permanent_failure")]
    PermanentFailure,

    /// Publish regardless of step outcome
    #[display("always")]
    Always,
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
        self.name.split('.').next_back()
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
        JSONSchema::compile(&self.schema).map_err(|e| format!("Invalid JSON Schema: {}", e))?;
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
    /// Publisher names are language-agnostic identifiers that must:
    /// - Start with a letter (upper or lower case) or underscore
    /// - Contain only letters, numbers, and underscores within segments
    /// - May use `::` (Rust/Ruby) or `.` (Python/Java) as namespace separators
    /// - Each segment after a separator must also start with a letter or underscore
    ///
    /// # Valid Examples
    ///
    /// - `PaymentEventPublisher` (PascalCase - Rust/Ruby/TypeScript)
    /// - `payment_event_publisher` (snake_case - Rust/Python)
    /// - `MyModule::PaymentPublisher` (Rust/Ruby namespacing)
    /// - `my_module::PaymentPublisher` (Rust module path)
    /// - `my_module.PaymentPublisher` (Python/Java namespacing)
    ///
    /// # Errors
    ///
    /// Returns an error if the publisher name is invalid
    pub fn validate_publisher(&self) -> Result<(), String> {
        if let Some(publisher) = &self.publisher {
            // Check for empty string
            if publisher.is_empty() {
                return Err("Publisher name cannot be empty".to_string());
            }

            // Language-agnostic identifier pattern:
            // - Each segment starts with letter or underscore
            // - Contains letters, numbers, underscores
            // - Segments separated by :: or .
            let valid_pattern =
                Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*((::|\.)[a-zA-Z_][a-zA-Z0-9_]*)*$")
                    .expect("Valid regex pattern");

            if !valid_pattern.is_match(publisher) {
                return Err(format!(
                    "Publisher name '{}' must be a valid identifier. \
                     Use letters, numbers, underscores, with '::' or '.' for namespacing. \
                     Examples: PaymentEventPublisher, my_module::Publisher, package.Publisher",
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
    /// This is the legacy method for backward compatibility. For new code that
    /// needs to distinguish between retryable and permanent failures, use
    /// `should_publish_with_retryability` instead.
    ///
    /// # Arguments
    ///
    /// * `step_succeeded` - Whether the step completed successfully
    ///
    /// # Returns
    ///
    /// `true` if the event should be published based on the condition.
    /// Note: `RetryableFailure` and `PermanentFailure` conditions always
    /// return `false` when called without retryability info - use
    /// `should_publish_with_retryability` for those conditions.
    pub fn should_publish(&self, step_succeeded: bool) -> bool {
        self.should_publish_with_retryability(step_succeeded, None)
    }

    /// Checks if this event should be published for the given step outcome and retryability
    ///
    /// # Arguments
    ///
    /// * `step_succeeded` - Whether the step completed successfully
    /// * `is_retryable` - From DB: whether step can be retried. Should be `Some(true)`
    ///   if step is retryable, `Some(false)` if permanently failed, or `None` if unknown
    ///   (e.g., on success). Determined from `get_step_readiness_status()` SQL function.
    ///
    /// # Returns
    ///
    /// `true` if the event should be published based on the condition and retryability
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get retryability from DB (authoritative source)
    /// let readiness = get_step_readiness_status(&pool, step_uuid).await?;
    /// let is_retryable = readiness.map(|s| s.is_retryable());
    ///
    /// if event_decl.should_publish_with_retryability(step_succeeded, is_retryable) {
    ///     // Publish the event
    /// }
    /// ```
    pub fn should_publish_with_retryability(
        &self,
        step_succeeded: bool,
        is_retryable: Option<bool>,
    ) -> bool {
        match self.condition {
            PublicationCondition::Success => step_succeeded,
            PublicationCondition::Failure => !step_succeeded,
            PublicationCondition::RetryableFailure => !step_succeeded && is_retryable == Some(true),
            PublicationCondition::PermanentFailure => {
                !step_succeeded && is_retryable == Some(false)
            }
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
        assert!(result
            .unwrap_err()
            .contains("must contain at least one dot"));
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
        assert!(result
            .unwrap_err()
            .contains("must not start or end with a dot"));
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
    fn test_publisher_validation_pascal_case() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("OrderEventPublisher".to_string())
            .build();

        assert!(event.validate_publisher().is_ok());
    }

    #[test]
    fn test_publisher_validation_snake_case() {
        // Snake case is valid for Rust/Python publishers
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("order_event_publisher".to_string())
            .build();

        assert!(event.validate_publisher().is_ok());
    }

    #[test]
    fn test_publisher_validation_rust_ruby_namespace() {
        // Rust/Ruby style namespacing with ::
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("MyModule::OrderEventPublisher".to_string())
            .build();

        assert!(event.validate_publisher().is_ok());
    }

    #[test]
    fn test_publisher_validation_rust_module_path() {
        // Rust module path style
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("my_module::order_publisher::OrderEventPublisher".to_string())
            .build();

        assert!(event.validate_publisher().is_ok());
    }

    #[test]
    fn test_publisher_validation_python_namespace() {
        // Python/Java style namespacing with dots
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("my_package.publishers.OrderEventPublisher".to_string())
            .build();

        assert!(event.validate_publisher().is_ok());
    }

    #[test]
    fn test_publisher_validation_underscore_prefix() {
        // Leading underscore is valid (private convention in many languages)
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("_InternalPublisher".to_string())
            .build();

        assert!(event.validate_publisher().is_ok());
    }

    #[test]
    fn test_publisher_validation_invalid_starts_with_number() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("123Publisher".to_string())
            .build();

        let result = event.validate_publisher();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be a valid identifier"));
    }

    #[test]
    fn test_publisher_validation_invalid_special_chars() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("Order-Event-Publisher".to_string())
            .build();

        let result = event.validate_publisher();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be a valid identifier"));
    }

    #[test]
    fn test_publisher_validation_invalid_trailing_separator() {
        let event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .publisher("MyModule::".to_string())
            .build();

        let result = event.validate_publisher();
        assert!(result.is_err());
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

        let broadcast = EventDeliveryMode::Broadcast;
        let json = serde_json::to_string(&broadcast).unwrap();
        assert_eq!(json, "\"broadcast\"");
    }

    #[test]
    fn test_delivery_mode_deserialization() {
        let durable: EventDeliveryMode = serde_json::from_str("\"durable\"").unwrap();
        assert_eq!(durable, EventDeliveryMode::Durable);

        let fast: EventDeliveryMode = serde_json::from_str("\"fast\"").unwrap();
        assert_eq!(fast, EventDeliveryMode::Fast);

        let broadcast: EventDeliveryMode = serde_json::from_str("\"broadcast\"").unwrap();
        assert_eq!(broadcast, EventDeliveryMode::Broadcast);
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
        assert_eq!(serde_json::to_string(&success).unwrap(), "\"success\"");

        let failure = PublicationCondition::Failure;
        assert_eq!(serde_json::to_string(&failure).unwrap(), "\"failure\"");

        let always = PublicationCondition::Always;
        assert_eq!(serde_json::to_string(&always).unwrap(), "\"always\"");

        let retryable = PublicationCondition::RetryableFailure;
        assert_eq!(
            serde_json::to_string(&retryable).unwrap(),
            "\"retryable_failure\""
        );

        let permanent = PublicationCondition::PermanentFailure;
        assert_eq!(
            serde_json::to_string(&permanent).unwrap(),
            "\"permanent_failure\""
        );
    }

    #[test]
    fn test_publication_condition_deserialization() {
        let retryable: PublicationCondition =
            serde_json::from_str("\"retryable_failure\"").unwrap();
        assert_eq!(retryable, PublicationCondition::RetryableFailure);

        let permanent: PublicationCondition =
            serde_json::from_str("\"permanent_failure\"").unwrap();
        assert_eq!(permanent, PublicationCondition::PermanentFailure);
    }

    #[test]
    fn test_should_publish_retryable_failure_condition() {
        let event = EventDeclaration::builder()
            .name("payment.failed.retryable".to_string())
            .description("Retryable payment failure".to_string())
            .condition(PublicationCondition::RetryableFailure)
            .schema(json!({"type": "object"}))
            .build();

        // Success: never publish
        assert!(!event.should_publish_with_retryability(true, Some(true)));
        assert!(!event.should_publish_with_retryability(true, Some(false)));
        assert!(!event.should_publish_with_retryability(true, None));

        // Failure + retryable: publish
        assert!(event.should_publish_with_retryability(false, Some(true)));

        // Failure + not retryable: don't publish
        assert!(!event.should_publish_with_retryability(false, Some(false)));

        // Failure + unknown retryability: don't publish
        assert!(!event.should_publish_with_retryability(false, None));
    }

    #[test]
    fn test_should_publish_permanent_failure_condition() {
        let event = EventDeclaration::builder()
            .name("payment.failed.permanent".to_string())
            .description("Permanent payment failure".to_string())
            .condition(PublicationCondition::PermanentFailure)
            .schema(json!({"type": "object"}))
            .build();

        // Success: never publish
        assert!(!event.should_publish_with_retryability(true, Some(true)));
        assert!(!event.should_publish_with_retryability(true, Some(false)));
        assert!(!event.should_publish_with_retryability(true, None));

        // Failure + retryable: don't publish
        assert!(!event.should_publish_with_retryability(false, Some(true)));

        // Failure + not retryable: publish
        assert!(event.should_publish_with_retryability(false, Some(false)));

        // Failure + unknown retryability: don't publish
        assert!(!event.should_publish_with_retryability(false, None));
    }

    #[test]
    fn test_should_publish_legacy_method_backward_compatible() {
        // Success condition works with legacy method
        let success_event = EventDeclaration::builder()
            .name("order.created".to_string())
            .description("Test".to_string())
            .condition(PublicationCondition::Success)
            .schema(json!({"type": "object"}))
            .build();
        assert!(success_event.should_publish(true));
        assert!(!success_event.should_publish(false));

        // Failure condition works with legacy method
        let failure_event = EventDeclaration::builder()
            .name("order.failed".to_string())
            .description("Test".to_string())
            .condition(PublicationCondition::Failure)
            .schema(json!({"type": "object"}))
            .build();
        assert!(!failure_event.should_publish(true));
        assert!(failure_event.should_publish(false));

        // Always condition works with legacy method
        let always_event = EventDeclaration::builder()
            .name("order.attempted".to_string())
            .description("Test".to_string())
            .condition(PublicationCondition::Always)
            .schema(json!({"type": "object"}))
            .build();
        assert!(always_event.should_publish(true));
        assert!(always_event.should_publish(false));

        // RetryableFailure returns false with legacy method (no retryability info)
        let retryable_event = EventDeclaration::builder()
            .name("payment.failed.retryable".to_string())
            .description("Test".to_string())
            .condition(PublicationCondition::RetryableFailure)
            .schema(json!({"type": "object"}))
            .build();
        assert!(!retryable_event.should_publish(true));
        assert!(!retryable_event.should_publish(false)); // Returns false without retryability

        // PermanentFailure returns false with legacy method (no retryability info)
        let permanent_event = EventDeclaration::builder()
            .name("payment.failed.permanent".to_string())
            .description("Test".to_string())
            .condition(PublicationCondition::PermanentFailure)
            .schema(json!({"type": "object"}))
            .build();
        assert!(!permanent_event.should_publish(true));
        assert!(!permanent_event.should_publish(false)); // Returns false without retryability
    }

    #[test]
    fn test_broadcast_delivery_mode_in_event_declaration() {
        let event = EventDeclaration::builder()
            .name("order.completed".to_string())
            .description("Order completed - needs both external and internal delivery".to_string())
            .condition(PublicationCondition::Success)
            .delivery_mode(EventDeliveryMode::Broadcast)
            .schema(json!({"type": "object"}))
            .build();

        assert_eq!(event.delivery_mode, EventDeliveryMode::Broadcast);
    }
}
