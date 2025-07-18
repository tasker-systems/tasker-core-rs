//! Rails-compatible event payload structures for FFI integration
//!
//! This module provides payload structures that match Rails dry-events expectations,
//! enabling seamless integration between Rust events and Rails subscribers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Rails-compatible event payload structure
///
/// This structure matches the payload format expected by Rails BaseSubscriber
/// and TelemetrySubscriber, ensuring that Rust events can be processed by
/// existing Rails event handlers without modification.
///
/// ## Payload Structure
///
/// The payload follows Rails conventions with these core fields:
/// - `task_id`: Task identifier (string for Rails compatibility)
/// - `step_id`: Step identifier (optional)
/// - `timestamp`: ISO 8601 timestamp (Rails expects Time.current format)
/// - `task_name`: Human-readable task name
/// - `step_name`: Human-readable step name
/// - Additional event-specific fields as needed
///
/// ## Usage
///
/// ```rust
/// use tasker_core::events::rails_payload::RailsCompatiblePayload;
///
/// let payload = RailsCompatiblePayload::new()
///     .with_task_id("123")
///     .with_task_name("order_processor")
///     .with_step_id("456")
///     .with_step_name("validate_payment")
///     .with_field("amount", 99.99)
///     .with_field("currency", "USD");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailsCompatiblePayload {
    /// Task identifier (string for Rails compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,

    /// Step identifier (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,

    /// Event timestamp in ISO 8601 format
    pub timestamp: DateTime<Utc>,

    /// Human-readable task name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_name: Option<String>,

    /// Human-readable step name  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_name: Option<String>,

    /// Additional event-specific fields
    #[serde(flatten)]
    pub additional_fields: HashMap<String, serde_json::Value>,
}

impl RailsCompatiblePayload {
    /// Create a new payload with current timestamp
    pub fn new() -> Self {
        Self {
            task_id: None,
            step_id: None,
            timestamp: Utc::now(),
            task_name: None,
            step_name: None,
            additional_fields: HashMap::new(),
        }
    }

    /// Set task ID (converts to string for Rails compatibility)
    pub fn with_task_id<T: ToString>(mut self, task_id: T) -> Self {
        self.task_id = Some(task_id.to_string());
        self
    }

    /// Set step ID (converts to string for Rails compatibility)
    pub fn with_step_id<T: ToString>(mut self, step_id: T) -> Self {
        self.step_id = Some(step_id.to_string());
        self
    }

    /// Set task name
    pub fn with_task_name<T: ToString>(mut self, task_name: T) -> Self {
        self.task_name = Some(task_name.to_string());
        self
    }

    /// Set step name
    pub fn with_step_name<T: ToString>(mut self, step_name: T) -> Self {
        self.step_name = Some(step_name.to_string());
        self
    }

    /// Add custom field to payload
    pub fn with_field<K: ToString, V: Into<serde_json::Value>>(mut self, key: K, value: V) -> Self {
        self.additional_fields.insert(key.to_string(), value.into());
        self
    }

    /// Convert to JSON for FFI transmission
    pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// Create from existing JSON payload (for compatibility)
    pub fn from_json(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }
}

impl Default for RailsCompatiblePayload {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper for creating task-related payloads
impl RailsCompatiblePayload {
    /// Create payload for task events
    pub fn for_task<T: ToString>(task_id: T, task_name: Option<String>) -> Self {
        let mut payload = Self::new().with_task_id(task_id);
        if let Some(name) = task_name {
            payload = payload.with_task_name(name);
        }
        payload
    }

    /// Create payload for step events
    pub fn for_step<T: ToString, S: ToString>(
        task_id: T,
        step_id: S,
        task_name: Option<String>,
        step_name: Option<String>,
    ) -> Self {
        let mut payload = Self::new().with_task_id(task_id).with_step_id(step_id);

        if let Some(name) = task_name {
            payload = payload.with_task_name(name);
        }
        if let Some(name) = step_name {
            payload = payload.with_step_name(name);
        }
        payload
    }
}

/// Timing metrics payload builder
impl RailsCompatiblePayload {
    /// Add timing metrics to payload (for completion events)
    pub fn with_timing_metrics(
        mut self,
        execution_duration: Option<f64>,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
    ) -> Self {
        if let Some(duration) = execution_duration {
            self.additional_fields
                .insert("execution_duration".to_string(), duration.into());
        }
        if let Some(start) = started_at {
            self.additional_fields
                .insert("started_at".to_string(), start.to_rfc3339().into());
        }
        if let Some(end) = completed_at {
            self.additional_fields
                .insert("completed_at".to_string(), end.to_rfc3339().into());
        }
        self
    }

    /// Add error information to payload (for failure events)
    pub fn with_error_info(
        mut self,
        error_message: Option<String>,
        exception_class: Option<String>,
        retryable: Option<bool>,
    ) -> Self {
        if let Some(msg) = error_message {
            self.additional_fields
                .insert("error_message".to_string(), msg.into());
        }
        if let Some(class) = exception_class {
            self.additional_fields
                .insert("exception_class".to_string(), class.into());
        }
        if let Some(retry) = retryable {
            self.additional_fields
                .insert("retryable".to_string(), retry.into());
        }
        self
    }

    /// Add step metrics to payload
    pub fn with_step_metrics(
        mut self,
        attempt_number: Option<u32>,
        retry_limit: Option<u32>,
        total_steps: Option<u32>,
        completed_steps: Option<u32>,
    ) -> Self {
        if let Some(attempt) = attempt_number {
            self.additional_fields
                .insert("attempt_number".to_string(), attempt.into());
        }
        if let Some(limit) = retry_limit {
            self.additional_fields
                .insert("retry_limit".to_string(), limit.into());
        }
        if let Some(total) = total_steps {
            self.additional_fields
                .insert("total_steps".to_string(), total.into());
        }
        if let Some(completed) = completed_steps {
            self.additional_fields
                .insert("completed_steps".to_string(), completed.into());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_payload_creation() {
        let payload = RailsCompatiblePayload::new()
            .with_task_id("123")
            .with_task_name("test_task")
            .with_step_id("456")
            .with_step_name("test_step");

        assert_eq!(payload.task_id, Some("123".to_string()));
        assert_eq!(payload.task_name, Some("test_task".to_string()));
        assert_eq!(payload.step_id, Some("456".to_string()));
        assert_eq!(payload.step_name, Some("test_step".to_string()));
    }

    #[test]
    fn test_payload_with_custom_fields() {
        let payload = RailsCompatiblePayload::new()
            .with_task_id("123")
            .with_field("amount", 99.99)
            .with_field("currency", "USD")
            .with_field("user_id", 42);

        assert_eq!(
            payload.additional_fields["amount"],
            serde_json::Value::from(99.99)
        );
        assert_eq!(
            payload.additional_fields["currency"],
            serde_json::Value::from("USD")
        );
        assert_eq!(
            payload.additional_fields["user_id"],
            serde_json::Value::from(42)
        );
    }

    #[test]
    fn test_task_payload_factory() {
        let payload = RailsCompatiblePayload::for_task("456", Some("order_processor".to_string()));

        assert_eq!(payload.task_id, Some("456".to_string()));
        assert_eq!(payload.task_name, Some("order_processor".to_string()));
        assert!(payload.step_id.is_none());
    }

    #[test]
    fn test_step_payload_factory() {
        let payload = RailsCompatiblePayload::for_step(
            "789",
            "101",
            Some("payment_processor".to_string()),
            Some("validate_card".to_string()),
        );

        assert_eq!(payload.task_id, Some("789".to_string()));
        assert_eq!(payload.step_id, Some("101".to_string()));
        assert_eq!(payload.task_name, Some("payment_processor".to_string()));
        assert_eq!(payload.step_name, Some("validate_card".to_string()));
    }

    #[test]
    fn test_timing_metrics() {
        let now = Utc::now();
        let later = now + chrono::Duration::seconds(5);

        let payload = RailsCompatiblePayload::new()
            .with_task_id("123")
            .with_timing_metrics(Some(5.0), Some(now), Some(later));

        assert_eq!(
            payload.additional_fields["execution_duration"],
            serde_json::Value::from(5.0)
        );
        assert!(payload.additional_fields.contains_key("started_at"));
        assert!(payload.additional_fields.contains_key("completed_at"));
    }

    #[test]
    fn test_error_info() {
        let payload = RailsCompatiblePayload::new()
            .with_task_id("123")
            .with_error_info(
                Some("Payment failed".to_string()),
                Some("PaymentError".to_string()),
                Some(true),
            );

        assert_eq!(
            payload.additional_fields["error_message"],
            serde_json::Value::from("Payment failed")
        );
        assert_eq!(
            payload.additional_fields["exception_class"],
            serde_json::Value::from("PaymentError")
        );
        assert_eq!(
            payload.additional_fields["retryable"],
            serde_json::Value::from(true)
        );
    }

    #[test]
    fn test_json_serialization() {
        let payload = RailsCompatiblePayload::new()
            .with_task_id("123")
            .with_task_name("test")
            .with_field("status", "completed");

        let json = payload.to_json().unwrap();

        assert_eq!(json["task_id"], serde_json::Value::from("123"));
        assert_eq!(json["task_name"], serde_json::Value::from("test"));
        assert_eq!(json["status"], serde_json::Value::from("completed"));
        assert!(json["timestamp"].is_string());
    }

    #[test]
    fn test_json_deserialization() {
        let json = serde_json::json!({
            "task_id": "456",
            "task_name": "test_task",
            "timestamp": "2025-01-14T10:00:00Z",
            "custom_field": "value"
        });

        let payload = RailsCompatiblePayload::from_json(json).unwrap();

        assert_eq!(payload.task_id, Some("456".to_string()));
        assert_eq!(payload.task_name, Some("test_task".to_string()));
        assert_eq!(
            payload.additional_fields["custom_field"],
            serde_json::Value::from("value")
        );
    }
}
