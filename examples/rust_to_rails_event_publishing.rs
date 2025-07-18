//! Example: Rust→Rails Event Publishing Integration
//!
//! This example demonstrates how Rust can publish events that appear natively
//! in Rails, enabling existing Rails subscribers to receive Rust events.

use serde_json::json;
use tasker_core::events::RailsCompatiblePayload;

fn main() {
    println!("=== Rust→Rails Event Publishing Example ===\n");

    // Example 1: Task completion event
    println!("1. Task Completion Event:");
    let task_completed =
        RailsCompatiblePayload::for_task("123", Some("OrderProcessor".to_string()))
            .with_timing_metrics(Some(5.2), None, None)
            .with_field("total_steps", 4)
            .with_field("completed_steps", 4)
            .with_field("status", "completed");

    let json = task_completed.to_json().unwrap();
    println!("   Event Name: task.completed");
    println!(
        "   Payload: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
    println!();

    // Example 2: Step failure event
    println!("2. Step Failure Event:");
    let step_failed = RailsCompatiblePayload::for_step(
        "456",
        "789",
        Some("PaymentProcessor".to_string()),
        Some("validate_card".to_string()),
    )
    .with_error_info(
        Some("Invalid card number".to_string()),
        Some("ValidationError".to_string()),
        Some(true),
    )
    .with_step_metrics(Some(2), Some(3), None, None);

    let json = step_failed.to_json().unwrap();
    println!("   Event Name: step.failed");
    println!(
        "   Payload: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
    println!();

    // Example 3: Custom business event
    println!("3. Custom Business Event:");
    let order_shipped = RailsCompatiblePayload::new()
        .with_task_id("order_123")
        .with_task_name("ShippingProcessor")
        .with_field("order_id", "ORD-456789")
        .with_field("tracking_number", "1Z999AA1234567890")
        .with_field("carrier", "UPS")
        .with_field("estimated_delivery", "2025-01-16T10:00:00Z");

    let json = order_shipped.to_json().unwrap();
    println!("   Event Name: order.shipped");
    println!(
        "   Payload: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
    println!();

    // Example 4: Performance monitoring event
    println!("4. Performance Monitoring Event:");
    let bottleneck_detected = RailsCompatiblePayload::new()
        .with_field("bottleneck_type", "dependency_timeout")
        .with_field(
            "affected_steps",
            json!(["validate_payment", "process_order", "update_inventory"]),
        )
        .with_field("average_delay", 15.7)
        .with_field("threshold_exceeded", true);

    let json = bottleneck_detected.to_json().unwrap();
    println!("   Event Name: performance.bottleneck_detected");
    println!(
        "   Payload: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
    println!();

    // Demonstrate Rails compatibility
    println!("=== Rails Integration Compatibility ===");
    println!();

    println!("These events can be published to Rails using:");
    println!("```ruby");
    println!("# In Rails application");
    println!("TaskerCore.rust_publish_to_rails('task.completed', payload_hash)");
    println!("```");
    println!();

    println!("Existing Rails subscribers will automatically receive these events:");
    println!("- TelemetrySubscriber → Creates OpenTelemetry spans");
    println!("- MetricsSubscriber → Records performance metrics");
    println!("- Custom subscribers → Execute business logic");
    println!("- Slack notifications → Send alerts");
    println!("- PagerDuty integration → Trigger incidents");
    println!();

    println!("Rails BaseSubscriber methods work unchanged:");
    println!("- safe_get(event, :task_id) → Returns task ID");
    println!("- extract_timing_metrics(event) → Extracts duration, timestamps");
    println!("- extract_error_metrics(event) → Extracts error information");
    println!("- extract_metric_tags(event) → Builds StatsD tags");
    println!();

    println!("✅ Complete compatibility with existing Rails event system!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_examples_create_valid_payloads() {
        // Test that all examples create valid JSON
        let examples = [
            ("task.completed", create_task_completed_example()),
            ("step.failed", create_step_failed_example()),
            ("order.shipped", create_order_shipped_example()),
            (
                "performance.bottleneck_detected",
                create_bottleneck_example(),
            ),
        ];

        for (event_name, payload) in examples {
            let json = payload.to_json().unwrap();

            // All events should have a timestamp
            assert!(
                json.get("timestamp").is_some(),
                "Missing timestamp for {}",
                event_name
            );

            // All events should be serializable back to payload
            let _recovered = RailsCompatiblePayload::from_json(json).unwrap();
        }
    }

    #[test]
    fn test_rails_subscriber_compatibility() {
        // Test that payloads have fields expected by Rails BaseSubscriber methods

        // Test timing metrics compatibility
        let task_payload = create_task_completed_example();
        let json = task_payload.to_json().unwrap();

        // extract_timing_metrics expects these fields
        assert!(json.get("execution_duration").is_some());
        assert!(json.get("total_steps").is_some());
        assert!(json.get("completed_steps").is_some());

        // Test error metrics compatibility
        let step_payload = create_step_failed_example();
        let json = step_payload.to_json().unwrap();

        // extract_error_metrics expects these fields
        assert!(json.get("error_message").is_some());
        assert!(json.get("exception_class").is_some());
        assert!(json.get("retryable").is_some());
        assert!(json.get("attempt_number").is_some());
    }

    fn create_task_completed_example() -> RailsCompatiblePayload {
        RailsCompatiblePayload::for_task("123", Some("OrderProcessor".to_string()))
            .with_timing_metrics(Some(5.2), None, None)
            .with_field("total_steps", 4)
            .with_field("completed_steps", 4)
            .with_field("status", "completed")
    }

    fn create_step_failed_example() -> RailsCompatiblePayload {
        RailsCompatiblePayload::for_step(
            "456",
            "789",
            Some("PaymentProcessor".to_string()),
            Some("validate_card".to_string()),
        )
        .with_error_info(
            Some("Invalid card number".to_string()),
            Some("ValidationError".to_string()),
            Some(true),
        )
        .with_step_metrics(Some(2), Some(3), None, None)
    }

    fn create_order_shipped_example() -> RailsCompatiblePayload {
        RailsCompatiblePayload::new()
            .with_task_id("order_123")
            .with_task_name("ShippingProcessor")
            .with_field("order_id", "ORD-456789")
            .with_field("tracking_number", "1Z999AA1234567890")
            .with_field("carrier", "UPS")
            .with_field("estimated_delivery", "2025-01-16T10:00:00Z")
    }

    fn create_bottleneck_example() -> RailsCompatiblePayload {
        RailsCompatiblePayload::new()
            .with_field("bottleneck_type", "dependency_timeout")
            .with_field(
                "affected_steps",
                json!(["validate_payment", "process_order", "update_inventory"]),
            )
            .with_field("average_delay", 15.7)
            .with_field("threshold_exceeded", true)
    }
}
