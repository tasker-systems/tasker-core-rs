//! Integration test for Rust→Rails event publishing payload compatibility
//!
//! This test verifies that Rust can create Rails-compatible event payloads
//! that work seamlessly with existing Rails subscribers without modifications.

use serde_json::json;
use std::sync::{Arc, Mutex};

use tasker_core::events::RailsCompatiblePayload;

/// Mock Rails Publisher for testing
/// Simulates the behavior of Tasker::Events::Publisher.instance
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MockRailsPublisher {
    published_events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
}

#[allow(dead_code)]
impl MockRailsPublisher {
    fn new() -> Self {
        Self {
            published_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn publish(&self, event_name: &str, payload: serde_json::Value) {
        let mut events = self.published_events.lock().unwrap();
        events.push((event_name.to_string(), payload));
    }

    fn get_published_events(&self) -> Vec<(String, serde_json::Value)> {
        self.published_events.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.published_events.lock().unwrap().clear();
    }
}

/// Test that RailsCompatiblePayload creates proper Rails-compatible structure
#[test]
fn test_rails_compatible_payload_structure() {
    let payload = RailsCompatiblePayload::new()
        .with_task_id("123")
        .with_task_name("order_processor")
        .with_step_id("456")
        .with_step_name("validate_payment")
        .with_field("amount", 99.99)
        .with_field("currency", "USD");

    let json = payload.to_json().unwrap();

    // Verify core Rails fields
    assert_eq!(json["task_id"], "123");
    assert_eq!(json["task_name"], "order_processor");
    assert_eq!(json["step_id"], "456");
    assert_eq!(json["step_name"], "validate_payment");

    // Verify additional fields
    assert_eq!(json["amount"], 99.99);
    assert_eq!(json["currency"], "USD");

    // Verify timestamp is present and properly formatted
    assert!(json["timestamp"].is_string());
    let timestamp_str = json["timestamp"].as_str().unwrap();
    assert!(timestamp_str.contains("T")); // ISO 8601 format
    assert!(timestamp_str.contains("Z") || timestamp_str.contains("+")); // Timezone info
}

/// Test that payload conversion works for different input formats
#[test]
fn test_payload_conversion_compatibility() {
    // Test 1: Basic task completion payload
    let task_payload =
        RailsCompatiblePayload::for_task("789", Some("payment_processor".to_string()))
            .with_timing_metrics(Some(2.5), None, None)
            .with_field("status", "completed");

    let json = task_payload.to_json().unwrap();
    assert_eq!(json["task_id"], "789");
    assert_eq!(json["task_name"], "payment_processor");
    assert_eq!(json["execution_duration"], 2.5);
    assert_eq!(json["status"], "completed");

    // Test 2: Step failure payload
    let step_payload = RailsCompatiblePayload::for_step(
        "101",
        "202",
        Some("inventory_processor".to_string()),
        Some("check_stock".to_string()),
    )
    .with_error_info(
        Some("Insufficient stock".to_string()),
        Some("StockError".to_string()),
        Some(true),
    )
    .with_step_metrics(Some(2), Some(3), None, None);

    let json = step_payload.to_json().unwrap();
    assert_eq!(json["task_id"], "101");
    assert_eq!(json["step_id"], "202");
    assert_eq!(json["task_name"], "inventory_processor");
    assert_eq!(json["step_name"], "check_stock");
    assert_eq!(json["error_message"], "Insufficient stock");
    assert_eq!(json["exception_class"], "StockError");
    assert_eq!(json["retryable"], true);
    assert_eq!(json["attempt_number"], 2);
    assert_eq!(json["retry_limit"], 3);
}

/// Test payload serialization and deserialization round-trip
#[test]
fn test_payload_serialization_round_trip() {
    let original = RailsCompatiblePayload::new()
        .with_task_id("999")
        .with_task_name("test_task")
        .with_field(
            "complex_data",
            json!({
                "nested": {
                    "values": [1, 2, 3],
                    "metadata": {
                        "source": "test",
                        "version": "1.0"
                    }
                }
            }),
        );

    // Serialize to JSON
    let json = original.to_json().unwrap();

    // Deserialize back
    let recovered = RailsCompatiblePayload::from_json(json).unwrap();

    // Verify core fields match
    assert_eq!(recovered.task_id, Some("999".to_string()));
    assert_eq!(recovered.task_name, Some("test_task".to_string()));

    // Verify complex data preservation
    let complex_data = &recovered.additional_fields["complex_data"];
    assert_eq!(complex_data["nested"]["values"], json!([1, 2, 3]));
    assert_eq!(complex_data["nested"]["metadata"]["source"], "test");
}

/// Test Rails payload compatibility with various event types
#[test]
fn test_rails_event_type_compatibility() {
    // Test task events
    let task_completed =
        RailsCompatiblePayload::for_task("task_123", Some("OrderProcessor".to_string()))
            .with_timing_metrics(Some(5.2), None, None)
            .with_field("total_steps", 4)
            .with_field("completed_steps", 4);

    let json = task_completed.to_json().unwrap();

    // Should match Rails TelemetrySubscriber expectations
    assert!(json.get("task_id").is_some());
    assert!(json.get("task_name").is_some());
    assert!(json.get("execution_duration").is_some());
    assert!(json.get("total_steps").is_some());
    assert!(json.get("completed_steps").is_some());
    assert!(json.get("timestamp").is_some());

    // Test step events
    let step_failed = RailsCompatiblePayload::for_step(
        "task_456",
        "step_789",
        Some("PaymentProcessor".to_string()),
        Some("validate_card".to_string()),
    )
    .with_error_info(
        Some("Invalid card number".to_string()),
        Some("ValidationError".to_string()),
        Some(true),
    )
    .with_step_metrics(Some(1), Some(3), None, None);

    let json = step_failed.to_json().unwrap();

    // Should match Rails BaseSubscriber expectations
    assert!(json.get("task_id").is_some());
    assert!(json.get("step_id").is_some());
    assert!(json.get("task_name").is_some());
    assert!(json.get("step_name").is_some());
    assert!(json.get("error_message").is_some());
    assert!(json.get("exception_class").is_some());
    assert!(json.get("retryable").is_some());
    assert!(json.get("attempt_number").is_some());
    assert!(json.get("retry_limit").is_some());
}

/// Test metrics extraction helper compatibility
#[test]
fn test_rails_metrics_extraction_compatibility() {
    // Create payload that matches Rails BaseSubscriber::extract_timing_metrics expectations
    let timing_payload = RailsCompatiblePayload::new()
        .with_task_id("metrics_test")
        .with_timing_metrics(
            Some(3.7),
            Some(chrono::Utc::now() - chrono::Duration::seconds(5)),
            Some(chrono::Utc::now()),
        )
        .with_step_metrics(Some(1), Some(2), Some(10), Some(8));

    let json = timing_payload.to_json().unwrap();

    // Fields that Rails BaseSubscriber::extract_timing_metrics expects
    assert_eq!(json["execution_duration"], 3.7);
    assert!(json.get("started_at").is_some());
    assert!(json.get("completed_at").is_some());

    // Fields that Rails BaseSubscriber::extract_step_metrics expects
    assert_eq!(json["attempt_number"], 1);
    assert_eq!(json["retry_limit"], 2);
    assert_eq!(json["total_steps"], 10);
    assert_eq!(json["completed_steps"], 8);

    // Create payload that matches Rails BaseSubscriber::extract_error_metrics expectations
    let error_payload = RailsCompatiblePayload::new()
        .with_task_id("error_test")
        .with_error_info(
            Some("Connection timeout".to_string()),
            Some("Net::TimeoutError".to_string()),
            Some(true),
        )
        .with_step_metrics(Some(3), Some(5), None, None);

    let json = error_payload.to_json().unwrap();

    // Fields that Rails BaseSubscriber::extract_error_metrics expects
    assert_eq!(json["error_message"], "Connection timeout");
    assert_eq!(json["exception_class"], "Net::TimeoutError");
    assert_eq!(json["retryable"], true);
    assert_eq!(json["attempt_number"], 3);
    assert_eq!(json["retry_limit"], 5);
}

/// Test that timestamps are properly formatted for Rails
#[test]
fn test_timestamp_formatting() {
    let payload = RailsCompatiblePayload::new();
    let json = payload.to_json().unwrap();

    let timestamp_str = json["timestamp"].as_str().unwrap();

    // Should be ISO 8601 format that Rails can parse
    assert!(chrono::DateTime::parse_from_rfc3339(timestamp_str).is_ok());

    // Test with explicit timing
    let start_time = chrono::Utc::now() - chrono::Duration::seconds(10);
    let end_time = chrono::Utc::now();

    let timed_payload = RailsCompatiblePayload::new().with_timing_metrics(
        Some(10.0),
        Some(start_time),
        Some(end_time),
    );

    let json = timed_payload.to_json().unwrap();

    // Verify all timestamps are properly formatted
    let started_at_str = json["started_at"].as_str().unwrap();
    let completed_at_str = json["completed_at"].as_str().unwrap();

    assert!(chrono::DateTime::parse_from_rfc3339(started_at_str).is_ok());
    assert!(chrono::DateTime::parse_from_rfc3339(completed_at_str).is_ok());
}

/// Test payload size and performance for large event payloads
#[test]
fn test_payload_performance_characteristics() {
    // Create a large payload to test serialization performance
    let mut large_payload = RailsCompatiblePayload::new()
        .with_task_id("perf_test")
        .with_task_name("large_data_processor");

    // Add many fields to test performance
    for i in 0..1000 {
        large_payload = large_payload.with_field(format!("field_{i}"), format!("value_{i}"));
    }

    // Test serialization time (should be reasonable)
    let start = std::time::Instant::now();
    let json = large_payload.to_json().unwrap();
    let serialization_time = start.elapsed();

    // Should serialize in under 10ms for 1000 fields
    assert!(serialization_time.as_millis() < 10);

    // Test deserialization time
    let start = std::time::Instant::now();
    let _recovered = RailsCompatiblePayload::from_json(json).unwrap();
    let deserialization_time = start.elapsed();

    // Should deserialize in under 10ms for 1000 fields
    assert!(deserialization_time.as_millis() < 10);
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Integration test that would verify the complete FFI flow
    /// Note: This is a conceptual test - actual FFI testing requires Ruby runtime
    #[test]
    fn test_ffi_publishing_flow_concept() {
        // This test demonstrates the expected flow:

        // 1. Create Rails-compatible payload in Rust
        let payload = RailsCompatiblePayload::new()
            .with_task_id("ffi_test")
            .with_task_name("test_processor")
            .with_field("status", "completed")
            .with_timing_metrics(Some(1.5), None, None);

        let json = payload.to_json().unwrap();

        // 2. Verify payload structure matches Rails expectations
        assert_eq!(json["task_id"], "ffi_test");
        assert_eq!(json["task_name"], "test_processor");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["execution_duration"], 1.5);
        assert!(json.get("timestamp").is_some());

        // 3. In actual FFI flow, this would be:
        //    - rust_publish_to_rails("task.completed", payload)
        //    - FFI calls ensure_rails_compatible_payload(payload)
        //    - FFI calls Ruby bridge.handle_rust_event_publication
        //    - Ruby publishes to Tasker::Events::Publisher.instance
        //    - Existing Rails subscribers receive the event

        // Verify the payload would work with Rails event patterns
        assert!(is_rails_compatible_payload(&json));
    }

    fn is_rails_compatible_payload(json: &serde_json::Value) -> bool {
        // Check that payload has expected structure for Rails subscribers
        json.get("timestamp").is_some()
            && (json.get("task_id").is_some() || json.get("step_id").is_some())
    }
}
