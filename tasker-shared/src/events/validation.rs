//! Event schema validation for TAS-65 distributed event system
//!
//! Provides JSON Schema-based validation for event payloads to ensure type safety
//! and contract compliance across the distributed system.

use jsonschema::{Draft, JSONSchema};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during event validation
#[derive(Debug, Clone, Error)]
pub enum EventValidationError {
    /// Invalid JSON Schema definition
    #[error("Invalid JSON Schema for event '{event_name}': {reason}")]
    InvalidSchema { event_name: String, reason: String },

    /// Event not registered in validator
    #[error("Unknown event '{event_name}' - not registered in schema validator")]
    UnknownEvent { event_name: String },

    /// Payload failed validation against schema
    #[error("Event '{event_name}' payload validation failed: {}", errors.join("; "))]
    ValidationFailed {
        event_name: String,
        errors: Vec<String>,
    },
}

/// Event schema validator using JSON Schema
///
/// Compiles and caches JSON Schemas for event validation. Validates event payloads
/// at runtime to ensure they conform to their declared schemas.
///
/// ## Usage
///
/// ```rust,ignore
/// use tasker_shared::events::validation::EventSchemaValidator;
/// use serde_json::json;
///
/// let mut validator = EventSchemaValidator::new();
///
/// // Register an event schema
/// let schema = json!({
///     "type": "object",
///     "properties": {
///         "order_id": {"type": "string"},
///         "amount": {"type": "number"}
///     },
///     "required": ["order_id", "amount"]
/// });
/// validator.register_event_schema("order.created", schema)?;
///
/// // Validate a payload (success)
/// let valid_payload = json!({
///     "order_id": "12345",
///     "amount": 99.99
/// });
/// validator.validate_payload("order.created", &valid_payload)?; // ✓ Success
///
/// // Validate an invalid payload (failure)
/// let invalid_payload = json!({"order_id": "12345"}); // Missing 'amount'
/// let result = validator.validate_payload("order.created", &invalid_payload);
/// assert!(result.is_err()); // ✗ Validation failed
/// ```
#[derive(Debug)]
pub struct EventSchemaValidator {
    schemas: HashMap<String, JSONSchema>,
}

impl EventSchemaValidator {
    /// Create a new event schema validator
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Register an event schema for validation
    ///
    /// Compiles the JSON Schema and stores it for future validation calls.
    /// Schema compilation happens once at registration time for performance.
    ///
    /// # Arguments
    ///
    /// * `event_name` - The event name (e.g., "order.created")
    /// * `schema` - The JSON Schema definition (draft-07 compatible)
    ///
    /// # Returns
    ///
    /// `Ok(())` if schema compiled successfully, `Err` if schema is invalid
    ///
    /// # Errors
    ///
    /// Returns `EventValidationError::InvalidSchema` if the schema is malformed
    /// or fails to compile.
    pub fn register_event_schema(
        &mut self,
        event_name: &str,
        schema: Value,
    ) -> Result<(), EventValidationError> {
        // Compile JSON Schema with draft-07
        let compiled = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema)
            .map_err(|e| EventValidationError::InvalidSchema {
                event_name: event_name.to_string(),
                reason: e.to_string(),
            })?;

        self.schemas.insert(event_name.to_string(), compiled);
        Ok(())
    }

    /// Validate an event payload against its registered schema
    ///
    /// # Arguments
    ///
    /// * `event_name` - The event name to validate against
    /// * `payload` - The event payload to validate
    ///
    /// # Returns
    ///
    /// `Ok(())` if validation succeeds, `Err` with detailed error information if:
    /// - Event schema not registered
    /// - Payload fails validation
    ///
    /// # Errors
    ///
    /// Returns `EventValidationError::UnknownEvent` if the event is not registered,
    /// or `EventValidationError::ValidationFailed` with details if validation fails.
    pub fn validate_payload(
        &self,
        event_name: &str,
        payload: &Value,
    ) -> Result<(), EventValidationError> {
        // Get the compiled schema
        let schema =
            self.schemas
                .get(event_name)
                .ok_or_else(|| EventValidationError::UnknownEvent {
                    event_name: event_name.to_string(),
                })?;

        // Validate the payload
        let validation_result = schema.validate(payload);

        match validation_result {
            Ok(()) => Ok(()),
            Err(errors) => {
                // Collect all validation errors into a vector
                let error_messages: Vec<String> = errors
                    .map(|e| {
                        format!(
                            "Property '{}': {}",
                            e.instance_path,
                            e.to_string().replace('\n', " ")
                        )
                    })
                    .collect();

                Err(EventValidationError::ValidationFailed {
                    event_name: event_name.to_string(),
                    errors: error_messages,
                })
            }
        }
    }

    /// Check if an event schema is registered
    pub fn has_event(&self, event_name: &str) -> bool {
        self.schemas.contains_key(event_name)
    }

    /// Get the count of registered event schemas
    pub fn schema_count(&self) -> usize {
        self.schemas.len()
    }

    /// Clear all registered schemas
    pub fn clear(&mut self) {
        self.schemas.clear();
    }
}

impl Default for EventSchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_register_valid_schema() {
        let mut validator = EventSchemaValidator::new();

        let schema = json!({
            "type": "object",
            "properties": {
                "order_id": {"type": "string"},
                "amount": {"type": "number"}
            },
            "required": ["order_id", "amount"]
        });

        let result = validator.register_event_schema("order.created", schema);
        assert!(result.is_ok());
        assert!(validator.has_event("order.created"));
        assert_eq!(validator.schema_count(), 1);
    }

    #[test]
    fn test_register_invalid_schema() {
        let mut validator = EventSchemaValidator::new();

        // Invalid schema: "type" has invalid value
        let invalid_schema = json!({
            "type": "invalid_type"
        });

        let result = validator.register_event_schema("test.event", invalid_schema);
        assert!(result.is_err());

        match result {
            Err(EventValidationError::InvalidSchema { event_name, .. }) => {
                assert_eq!(event_name, "test.event");
            }
            _ => panic!("Expected InvalidSchema error"),
        }
    }

    #[test]
    fn test_validate_payload_success() {
        let mut validator = EventSchemaValidator::new();

        let schema = json!({
            "type": "object",
            "properties": {
                "order_id": {"type": "string"},
                "amount": {"type": "number"}
            },
            "required": ["order_id", "amount"]
        });

        validator
            .register_event_schema("order.created", schema)
            .unwrap();

        let payload = json!({
            "order_id": "12345",
            "amount": 99.99
        });

        let result = validator.validate_payload("order.created", &payload);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_payload_missing_required_field() {
        let mut validator = EventSchemaValidator::new();

        let schema = json!({
            "type": "object",
            "properties": {
                "order_id": {"type": "string"},
                "amount": {"type": "number"}
            },
            "required": ["order_id", "amount"]
        });

        validator
            .register_event_schema("order.created", schema)
            .unwrap();

        // Missing 'amount' field
        let payload = json!({
            "order_id": "12345"
        });

        let result = validator.validate_payload("order.created", &payload);
        assert!(result.is_err());

        match result {
            Err(EventValidationError::ValidationFailed { event_name, errors }) => {
                assert_eq!(event_name, "order.created");
                assert!(!errors.is_empty());
                // Check that error mentions the missing field
                let error_str = errors.join("; ");
                assert!(
                    error_str.contains("amount") || error_str.contains("required"),
                    "Error should mention missing required field: {}",
                    error_str
                );
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }

    #[test]
    fn test_validate_payload_wrong_type() {
        let mut validator = EventSchemaValidator::new();

        let schema = json!({
            "type": "object",
            "properties": {
                "order_id": {"type": "string"},
                "amount": {"type": "number"}
            },
            "required": ["order_id", "amount"]
        });

        validator
            .register_event_schema("order.created", schema)
            .unwrap();

        // 'amount' should be number, not string
        let payload = json!({
            "order_id": "12345",
            "amount": "not a number"
        });

        let result = validator.validate_payload("order.created", &payload);
        assert!(result.is_err());

        match result {
            Err(EventValidationError::ValidationFailed { event_name, errors }) => {
                assert_eq!(event_name, "order.created");
                assert!(!errors.is_empty());
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }

    #[test]
    fn test_validate_unknown_event() {
        let validator = EventSchemaValidator::new();

        let payload = json!({
            "order_id": "12345",
            "amount": 99.99
        });

        let result = validator.validate_payload("unknown.event", &payload);
        assert!(result.is_err());

        match result {
            Err(EventValidationError::UnknownEvent { event_name }) => {
                assert_eq!(event_name, "unknown.event");
            }
            _ => panic!("Expected UnknownEvent error"),
        }
    }

    #[test]
    fn test_validator_clear() {
        let mut validator = EventSchemaValidator::new();

        let schema = json!({
            "type": "object",
            "properties": {
                "value": {"type": "string"}
            }
        });

        validator
            .register_event_schema("test.event", schema)
            .unwrap();
        assert_eq!(validator.schema_count(), 1);

        validator.clear();
        assert_eq!(validator.schema_count(), 0);
        assert!(!validator.has_event("test.event"));
    }

    #[test]
    fn test_multiple_schemas() {
        let mut validator = EventSchemaValidator::new();

        let order_schema = json!({
            "type": "object",
            "properties": {
                "order_id": {"type": "string"}
            }
        });

        let payment_schema = json!({
            "type": "object",
            "properties": {
                "payment_id": {"type": "string"}
            }
        });

        validator
            .register_event_schema("order.created", order_schema)
            .unwrap();
        validator
            .register_event_schema("payment.authorized", payment_schema)
            .unwrap();

        assert_eq!(validator.schema_count(), 2);
        assert!(validator.has_event("order.created"));
        assert!(validator.has_event("payment.authorized"));
    }

    #[test]
    fn test_additional_properties_allowed() {
        let mut validator = EventSchemaValidator::new();

        let schema = json!({
            "type": "object",
            "properties": {
                "required_field": {"type": "string"}
            },
            "required": ["required_field"]
        });

        validator
            .register_event_schema("test.event", schema)
            .unwrap();

        // Payload with additional property not in schema
        let payload = json!({
            "required_field": "value",
            "extra_field": "allowed"
        });

        // By default, JSON Schema allows additional properties
        let result = validator.validate_payload("test.event", &payload);
        assert!(result.is_ok());
    }

    #[test]
    fn test_additional_properties_forbidden() {
        let mut validator = EventSchemaValidator::new();

        let schema = json!({
            "type": "object",
            "properties": {
                "required_field": {"type": "string"}
            },
            "required": ["required_field"],
            "additionalProperties": false
        });

        validator
            .register_event_schema("test.event", schema)
            .unwrap();

        // Payload with additional property not in schema
        let payload = json!({
            "required_field": "value",
            "extra_field": "not_allowed"
        });

        let result = validator.validate_payload("test.event", &payload);
        assert!(result.is_err());

        match result {
            Err(EventValidationError::ValidationFailed { errors, .. }) => {
                let error_str = errors.join("; ");
                assert!(
                    error_str.contains("additional") || error_str.contains("extra_field"),
                    "Error should mention additional property: {}",
                    error_str
                );
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }
}
