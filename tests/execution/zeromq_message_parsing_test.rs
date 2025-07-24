// ZeroMQ Message Parsing Unit Tests
// Tests the message parsing logic in isolation

use serde_json::json;
use std::collections::HashMap;
use tasker_core::execution::message_protocols::{
    StepBatchRequest, StepBatchResponse, StepExecutionRequest, StepExecutionResult,
};

/// Test message protocol format for ZeroMQ communication
#[test]
fn test_zeromq_topic_message_format() {
    // Create a step batch response
    let result = StepExecutionResult::success(
        123,
        json!({"completed": true, "order_id": "TEST-123"}),
        1500,
    );
    
    let response = StepBatchResponse::new("batch_abc123".to_string(), vec![result]);
    let json_data = serde_json::to_string(&response).expect("Should serialize");
    
    // Test the expected ZeroMQ topic message format: "topic json_data"
    let topic_message = format!("results {}", json_data);
    
    // Verify the format can be split correctly
    let parts: Vec<&str> = topic_message.splitn(2, ' ').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], "results");
    
    // Verify the JSON part can be parsed back
    let parsed_response: StepBatchResponse = serde_json::from_str(parts[1]).expect("Should parse JSON");
    assert_eq!(parsed_response.batch_id, "batch_abc123");
    assert_eq!(parsed_response.results.len(), 1);
    assert_eq!(parsed_response.results[0].step_id, 123);
}

/// Test step batch request serialization/deserialization
#[test]
fn test_step_batch_request_serialization() {
    let mut config = HashMap::new();
    config.insert("timeout".to_string(), json!(30));
    config.insert("retries".to_string(), json!(3));
    
    let mut previous_results = HashMap::new();
    previous_results.insert("validate_order".to_string(), json!({"status": "approved"}));
    previous_results.insert("calculate_tax".to_string(), json!({"tax_amount": 15.50}));
    
    let step_request = StepExecutionRequest::new(
        456,
        789,
        "process_payment".to_string(),
        "PaymentHandler".to_string(),
        config.clone(),
        json!({"order_id": "ORDER-789", "amount": 125.50}),
        previous_results.clone(),
        3,
        30000,
    );
    
    let batch = StepBatchRequest::new("test_batch_456".to_string(), vec![step_request]);
    
    // Test serialization
    let json_str = serde_json::to_string(&batch).expect("Should serialize");
    assert!(json_str.contains("test_batch_456"));
    assert!(json_str.contains("process_payment"));
    assert!(json_str.contains("PaymentHandler"));
    assert!(json_str.contains("ORDER-789"));
    assert!(json_str.contains("validate_order"));
    
    // Test deserialization
    let deserialized: StepBatchRequest = serde_json::from_str(&json_str).expect("Should deserialize");
    assert_eq!(deserialized.batch_id, "test_batch_456");
    assert_eq!(deserialized.steps.len(), 1);
    
    let step = &deserialized.steps[0];
    assert_eq!(step.step_id, 456);
    assert_eq!(step.task_id, 789);
    assert_eq!(step.step_name, "process_payment");
    assert_eq!(step.handler_class, "PaymentHandler");
    assert_eq!(step.retry_limit, 3);
    assert_eq!(step.timeout_ms, 30000);
    
    // Verify complex data structures
    assert!(step.task_context.get("order_id").is_some());
    assert!(step.previous_results.contains_key("validate_order"));
    assert!(step.previous_results.contains_key("calculate_tax"));
}

/// Test step batch response with multiple results
#[test]
fn test_step_batch_response_multiple_results() {
    let results = vec![
        StepExecutionResult::success(
            111,
            json!({"validated": true}),
            800,
        ),
        StepExecutionResult::failure(
            222,
            "Payment gateway timeout".to_string(),
            "GATEWAY_TIMEOUT".to_string(),
            2500,
        ),
        StepExecutionResult::error(
            333,
            "Database connection failed".to_string(),
            1200,
        ),
    ];
    
    let response = StepBatchResponse::new("multi_step_batch".to_string(), results);
    
    // Test serialization
    let json_str = serde_json::to_string(&response).expect("Should serialize");
    assert!(json_str.contains("multi_step_batch"));
    assert!(json_str.contains("validated"));
    assert!(json_str.contains("GATEWAY_TIMEOUT"));
    assert!(json_str.contains("Database connection failed"));
    
    // Test deserialization
    let deserialized: StepBatchResponse = serde_json::from_str(&json_str).expect("Should deserialize");
    assert_eq!(deserialized.batch_id, "multi_step_batch");
    assert_eq!(deserialized.results.len(), 3);
    
    // Check success result
    let success_result = &deserialized.results[0];
    assert_eq!(success_result.step_id, 111);
    assert_eq!(success_result.status, "completed");
    assert!(success_result.output.is_some());
    assert!(success_result.error.is_none());
    
    // Check failure result
    let failure_result = &deserialized.results[1];
    assert_eq!(failure_result.step_id, 222);
    assert_eq!(failure_result.status, "failed");
    assert!(failure_result.error.is_some());
    
    // Check error result
    let error_result = &deserialized.results[2];
    assert_eq!(error_result.step_id, 333);
    assert_eq!(error_result.status, "error");
    assert!(error_result.error.is_some());
}

/// Test protocol version compatibility
#[test]
fn test_protocol_version_compatibility() {
    let step_request = StepExecutionRequest::new(
        100,
        200,
        "test_step".to_string(),
        "TestHandler".to_string(),
        HashMap::new(),
        json!({}),
        HashMap::new(),
        1,
        10000,
    );
    
    let batch = StepBatchRequest::new("version_test".to_string(), vec![step_request]);
    
    // Verify protocol version is included
    let json_str = serde_json::to_string(&batch).expect("Should serialize");
    assert!(json_str.contains("protocol_version"));
    
    let deserialized: StepBatchRequest = serde_json::from_str(&json_str).expect("Should deserialize");
    assert_eq!(deserialized.protocol_version, "1.0");
}

/// Test edge cases and boundary conditions
#[test]
fn test_edge_cases() {
    // Empty step batch
    let empty_batch = StepBatchRequest::new("empty".to_string(), vec![]);
    let json_str = serde_json::to_string(&empty_batch).expect("Should serialize");
    let deserialized: StepBatchRequest = serde_json::from_str(&json_str).expect("Should deserialize");
    assert_eq!(deserialized.steps.len(), 0);
    
    // Very long batch ID
    let long_id = "a".repeat(1000);
    let step = StepExecutionRequest::new(
        1, 1, "test".to_string(), "Handler".to_string(),
        HashMap::new(), json!({}), HashMap::new(), 1, 1000,
    );
    let long_batch = StepBatchRequest::new(long_id.clone(), vec![step]);
    let json_str = serde_json::to_string(&long_batch).expect("Should serialize");
    let deserialized: StepBatchRequest = serde_json::from_str(&json_str).expect("Should deserialize");
    assert_eq!(deserialized.batch_id, long_id);
    
    // Large previous results
    let mut large_results = HashMap::new();
    for i in 0..100 {
        large_results.insert(format!("step_{}", i), json!({"result": i, "data": "x".repeat(100)}));
    }
    
    let large_step = StepExecutionRequest::new(
        999, 888, "large_test".to_string(), "LargeHandler".to_string(),
        HashMap::new(), json!({}), large_results, 1, 5000,
    );
    
    let large_batch = StepBatchRequest::new("large_test".to_string(), vec![large_step]);
    let json_str = serde_json::to_string(&large_batch).expect("Should serialize");
    let deserialized: StepBatchRequest = serde_json::from_str(&json_str).expect("Should deserialize");
    assert_eq!(deserialized.steps[0].previous_results.len(), 100);
}

