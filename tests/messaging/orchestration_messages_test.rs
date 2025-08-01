//! # Orchestration Messages Tests
//!
//! Tests for the complete message structures used in the pgmq orchestration workflow.
//! Validates serialization, deserialization, and message creation patterns.

use chrono::Utc;
use serde_json::json;
use tasker_core::messaging::{
    BatchExecutionStatus, BatchMessage, BatchMetadata, BatchResultMessage, BatchRetryPolicy,
    BatchStep, StepExecutionStatus, StepResult, TaskPriority, TaskProcessingMessage,
    TaskRequestMessage,
};

#[test]
fn test_task_request_message_creation_and_serialization() {
    let request = TaskRequestMessage::new(
        "fulfillment".to_string(),
        "process_order".to_string(),
        "1.0.0".to_string(),
        json!({"order_id": 12345, "customer_id": 67890}),
        "api_gateway".to_string(),
    )
    .with_priority(TaskPriority::High)
    .with_custom_metadata("trace_id".to_string(), json!("abc-123-def"));

    // Validate basic properties
    assert_eq!(request.namespace, "fulfillment");
    assert_eq!(request.task_name, "process_order");
    assert_eq!(request.task_version, "1.0.0");
    assert_eq!(request.metadata.requester, "api_gateway");
    assert!(matches!(request.metadata.priority, TaskPriority::High));
    assert!(request.metadata.custom.contains_key("trace_id"));

    // Test serialization round-trip
    let serialized = serde_json::to_string(&request).expect("Should serialize");
    let deserialized: TaskRequestMessage =
        serde_json::from_str(&serialized).expect("Should deserialize");

    assert_eq!(request.request_id, deserialized.request_id);
    assert_eq!(request.namespace, deserialized.namespace);
    assert_eq!(request.task_name, deserialized.task_name);
    assert_eq!(request.input_data, deserialized.input_data);
}

#[test]
fn test_task_processing_message_creation() {
    let mut processing_msg = TaskProcessingMessage::new(
        12345,
        "inventory".to_string(),
        "stock_check".to_string(),
        "2.1.0".to_string(),
        "req-456".to_string(),
        TaskPriority::Normal,
    );

    assert_eq!(processing_msg.task_id, 12345);
    assert_eq!(processing_msg.namespace, "inventory");
    assert_eq!(processing_msg.task_name, "stock_check");
    assert_eq!(processing_msg.metadata.request_id, "req-456");
    assert_eq!(processing_msg.metadata.processing_attempts, 0);

    // Test attempt increment
    processing_msg.increment_attempts();
    assert_eq!(processing_msg.metadata.processing_attempts, 1);

    // Test retry time setting
    let retry_time = Utc::now();
    processing_msg.set_retry_after(retry_time);
    assert_eq!(processing_msg.metadata.retry_after, Some(retry_time));
}

#[test]
fn test_batch_message_with_multiple_steps() {
    let steps = vec![
        BatchStep::new(
            101,
            1,
            "validate_order".to_string(),
            json!({"order_id": 12345, "validation_rules": ["inventory", "payment"]}),
        )
        .with_metadata("timeout_ms".to_string(), json!(30000)),
        BatchStep::new(
            102,
            2,
            "reserve_inventory".to_string(),
            json!({"items": [{"sku": "ABC123", "quantity": 2}]}),
        ),
        BatchStep::new(
            103,
            3,
            "charge_payment".to_string(),
            json!({"amount": 99.99, "currency": "USD"}),
        ),
    ];

    let batch = BatchMessage::new(
        500,
        12345,
        "fulfillment".to_string(),
        "process_order".to_string(),
        "1.0.0".to_string(),
        steps,
    )
    .with_timeout(600)
    .with_retry_policy(BatchRetryPolicy::Linear { delay_seconds: 60 });

    assert_eq!(batch.batch_id, 500);
    assert_eq!(batch.task_id, 12345);
    assert_eq!(batch.namespace, "fulfillment");
    assert_eq!(batch.steps.len(), 3);
    assert_eq!(batch.metadata.timeout_seconds, 600);
    assert!(matches!(
        batch.metadata.retry_policy,
        BatchRetryPolicy::Linear { delay_seconds: 60 }
    ));

    // Verify step ordering and content
    assert_eq!(batch.steps[0].sequence, 1);
    assert_eq!(batch.steps[0].step_name, "validate_order");
    assert!(batch.steps[0].step_metadata.contains_key("timeout_ms"));

    assert_eq!(batch.steps[1].sequence, 2);
    assert_eq!(batch.steps[1].step_name, "reserve_inventory");

    assert_eq!(batch.steps[2].sequence, 3);
    assert_eq!(batch.steps[2].step_name, "charge_payment");
}

#[test]
fn test_batch_result_message_with_mixed_outcomes() {
    let step_results = vec![
        StepResult::success(
            101,
            json!({"validation_status": "passed", "checks": 5}),
            150,
        ),
        StepResult::success(
            102,
            json!({"reserved_items": [{"sku": "ABC123", "reserved_quantity": 2}]}),
            300,
        ),
        StepResult::failed(
            103,
            "Insufficient funds".to_string(),
            Some("PAYMENT_DECLINED".to_string()),
            75,
        ),
    ];

    let result = BatchResultMessage::new(
        500,
        12345,
        "fulfillment".to_string(),
        BatchExecutionStatus::PartialSuccess,
        step_results,
        "fulfillment-worker-01".to_string(),
        525, // Total execution time
    )
    .with_hostname("worker-node-5".to_string())
    .with_custom_metadata("execution_context".to_string(), json!("batch_v2"));

    assert_eq!(result.batch_id, 500);
    assert_eq!(result.task_id, 12345);
    assert!(matches!(
        result.batch_status,
        BatchExecutionStatus::PartialSuccess
    ));
    assert_eq!(result.step_results.len(), 3);
    assert_eq!(result.metadata.worker_id, "fulfillment-worker-01");
    assert_eq!(result.metadata.successful_steps, 2);
    assert_eq!(result.metadata.failed_steps, 1);
    assert_eq!(result.metadata.total_execution_time_ms, 525);
    assert_eq!(
        result.metadata.worker_hostname,
        Some("worker-node-5".to_string())
    );

    // Verify individual step results
    assert!(matches!(
        result.step_results[0].status,
        StepExecutionStatus::Success
    ));
    assert!(result.step_results[0].output.is_some());
    assert!(result.step_results[0].error.is_none());

    assert!(matches!(
        result.step_results[2].status,
        StepExecutionStatus::Failed
    ));
    assert!(result.step_results[2].output.is_none());
    assert_eq!(
        result.step_results[2].error,
        Some("Insufficient funds".to_string())
    );
    assert_eq!(
        result.step_results[2].error_code,
        Some("PAYMENT_DECLINED".to_string())
    );
}

#[test]
fn test_retry_policy_serialization() {
    let policies = vec![
        BatchRetryPolicy::None,
        BatchRetryPolicy::Linear { delay_seconds: 30 },
        BatchRetryPolicy::ExponentialBackoff {
            base_delay_seconds: 10,
            max_delay_seconds: 300,
        },
        BatchRetryPolicy::Custom {
            delays: vec![5, 10, 30, 60, 120],
        },
    ];

    for policy in policies {
        let serialized = serde_json::to_string(&policy).expect("Should serialize policy");
        let deserialized: BatchRetryPolicy =
            serde_json::from_str(&serialized).expect("Should deserialize policy");

        // Use Debug comparison since PartialEq might not be implemented
        assert_eq!(format!("{:?}", policy), format!("{:?}", deserialized));
    }
}

#[test]
fn test_task_priority_serialization() {
    let priorities = vec![
        TaskPriority::Low,
        TaskPriority::Normal,
        TaskPriority::High,
        TaskPriority::Urgent,
    ];

    for priority in priorities {
        let serialized = serde_json::to_string(&priority).expect("Should serialize priority");
        let deserialized: TaskPriority =
            serde_json::from_str(&serialized).expect("Should deserialize priority");

        assert_eq!(format!("{:?}", priority), format!("{:?}", deserialized));
    }
}

#[test]
fn test_batch_metadata_defaults() {
    let metadata = BatchMetadata {
        batch_created_at: Utc::now(),
        timeout_seconds: 300,
        retry_policy: BatchRetryPolicy::default(),
        max_retries: 3,
        retry_count: 0,
    };

    assert!(matches!(
        metadata.retry_policy,
        BatchRetryPolicy::ExponentialBackoff { .. }
    ));
}

#[test]
fn test_message_json_compatibility() {
    // Test that our messages can be parsed from typical JSON payloads
    // that might come from external systems

    let json_payload = json!({
        "request_id": "ext-req-123",
        "namespace": "external_integration",
        "task_name": "sync_data",
        "task_version": "1.2.3",
        "input_data": {
            "source_system": "crm",
            "entity_type": "customer",
            "sync_mode": "incremental"
        },
        "metadata": {
            "requested_at": "2025-08-01T12:00:00Z",
            "requester": "scheduled_job",
            "priority": "Normal",
            "custom": {
                "job_id": "job-456",
                "retry_policy": "exponential"
            }
        }
    });

    let parsed: TaskRequestMessage =
        serde_json::from_value(json_payload).expect("Should parse from JSON");

    assert_eq!(parsed.request_id, "ext-req-123");
    assert_eq!(parsed.namespace, "external_integration");
    assert_eq!(parsed.task_name, "sync_data");
    assert_eq!(parsed.metadata.requester, "scheduled_job");
    assert!(parsed.metadata.custom.contains_key("job_id"));
}

#[test]
fn test_step_result_creation_helpers() {
    // Test success result
    let success_result = StepResult::success(
        42,
        json!({"processed_records": 150, "status": "complete"}),
        1250,
    );

    assert_eq!(success_result.step_id, 42);
    assert!(matches!(
        success_result.status,
        StepExecutionStatus::Success
    ));
    assert!(success_result.output.is_some());
    assert!(success_result.error.is_none());
    assert_eq!(success_result.execution_duration_ms, 1250);

    // Test failure result
    let failure_result = StepResult::failed(
        43,
        "Database connection timeout".to_string(),
        Some("DB_TIMEOUT".to_string()),
        500,
    );

    assert_eq!(failure_result.step_id, 43);
    assert!(matches!(
        failure_result.status,
        StepExecutionStatus::Failed
    ));
    assert!(failure_result.output.is_none());
    assert_eq!(
        failure_result.error,
        Some("Database connection timeout".to_string())
    );
    assert_eq!(
        failure_result.error_code,
        Some("DB_TIMEOUT".to_string())
    );
    assert_eq!(failure_result.execution_duration_ms, 500);
}

#[test]
fn test_large_message_serialization() {
    // Test with larger payloads to ensure our message structures can handle
    // realistic workflow data sizes

    let large_input_data = json!({
        "order_items": (0..100).map(|i| json!({
            "sku": format!("ITEM-{:04}", i),
            "quantity": i % 10 + 1,
            "price": (i as f64) * 9.99,
            "metadata": {
                "weight": i as f64 * 0.5,
                "dimensions": [10.0, 5.0, 2.0],
                "fragile": i % 7 == 0
            }
        })).collect::<Vec<_>>(),
        "shipping_address": {
            "street": "123 Main Street, Apt 4B",
            "city": "San Francisco",
            "state": "CA",
            "zip_code": "94105",
            "country": "USA"
        },
        "customer_preferences": {
            "delivery_speed": "standard",
            "packaging": "eco_friendly",
            "notifications": ["email", "sms"],
            "special_instructions": "Leave with doorman. Ring apartment 4B."
        }
    });

    let request = TaskRequestMessage::new(
        "fulfillment".to_string(),
        "complex_order_processing".to_string(),
        "2.0.0".to_string(),
        large_input_data,
        "e_commerce_platform".to_string(),
    );

    // Should serialize and deserialize without issues
    let serialized = serde_json::to_string(&request).expect("Should serialize large message");
    let deserialized: TaskRequestMessage =
        serde_json::from_str(&serialized).expect("Should deserialize large message");

    assert_eq!(request.request_id, deserialized.request_id);
    assert_eq!(request.task_name, deserialized.task_name);

    // Verify the large data structure is preserved
    let order_items = deserialized.input_data["order_items"]
        .as_array()
        .expect("Should have order_items array");
    assert_eq!(order_items.len(), 100);
    assert_eq!(order_items[0]["sku"], "ITEM-0000");
    assert_eq!(order_items[99]["sku"], "ITEM-0099");
}