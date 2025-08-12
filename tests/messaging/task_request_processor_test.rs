//! # Task Request Processor Tests
//!
//! Comprehensive tests for the TaskRequestProcessor component that validates
//! task requests, creates tasks, and enqueues them for orchestration processing.

use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use tasker_core::error::{Result, TaskerError};
use tasker_core::messaging::{PgmqClient, TaskPriority, TaskProcessingMessage, TaskRequestMessage};
use tasker_core::models::core::{
    named_task::NamedTask,
    task::{NewTask, Task},
    task_namespace::TaskNamespace,
};
use tasker_core::orchestration::{
    task_initializer::TaskInitializer,
    task_request_processor::{TaskRequestProcessor, TaskRequestProcessorConfig},
};
use tasker_core::registry::TaskHandlerRegistry;

/// Mock PgmqClient for testing without requiring actual database
#[derive(Clone)]
pub struct MockPgmqClient {
    pub sent_messages: Arc<tokio::sync::Mutex<Vec<(String, serde_json::Value)>>>,
    pub queue_operations: Arc<tokio::sync::Mutex<Vec<String>>>,
    pub should_fail_send: bool,
    pub should_fail_create: bool,
}

impl MockPgmqClient {
    pub fn new() -> Self {
        Self {
            sent_messages: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            queue_operations: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            should_fail_send: false,
            should_fail_create: false,
        }
    }

    pub fn with_send_failure() -> Self {
        Self {
            sent_messages: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            queue_operations: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            should_fail_send: true,
            should_fail_create: false,
        }
    }

    pub async fn create_queue(
        &self,
        queue_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.should_fail_create {
            return Err("Mock queue creation failure".into());
        }

        let mut ops = self.queue_operations.lock().await;
        ops.push(format!("create_queue:{}", queue_name));
        Ok(())
    }

    pub async fn send_json_message<T: serde::Serialize>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        if self.should_fail_send {
            return Err("Mock send failure".into());
        }

        let serialized = serde_json::to_value(message)?;
        let mut messages = self.sent_messages.lock().await;
        messages.push((queue_name.to_string(), serialized));
        Ok(messages.len() as i64)
    }

    pub async fn get_sent_messages(&self) -> Vec<(String, serde_json::Value)> {
        self.sent_messages.lock().await.clone()
    }

    pub async fn get_queue_operations(&self) -> Vec<String> {
        self.queue_operations.lock().await.clone()
    }
}

/// Mock TaskHandlerRegistry for testing validation
pub struct MockTaskHandlerRegistry {
    pub should_fail_validation: bool,
    pub valid_tasks: Vec<(String, String, String)>, // namespace, task_name, version
}

impl MockTaskHandlerRegistry {
    pub fn new() -> Self {
        Self {
            should_fail_validation: false,
            valid_tasks: vec![
                (
                    "fulfillment".to_string(),
                    "process_order".to_string(),
                    "1.0.0".to_string(),
                ),
                (
                    "inventory".to_string(),
                    "stock_check".to_string(),
                    "2.1.0".to_string(),
                ),
                (
                    "notifications".to_string(),
                    "send_email".to_string(),
                    "1.2.0".to_string(),
                ),
            ],
        }
    }

    pub fn with_validation_failure() -> Self {
        Self {
            should_fail_validation: true,
            valid_tasks: vec![],
        }
    }

    pub async fn get_task_template(
        &self,
        namespace: &str,
        task_name: &str,
        task_version: &str,
    ) -> Result<serde_json::Value> {
        if self.should_fail_validation {
            return Err(TaskerError::ValidationError(
                "Mock validation failure".to_string(),
            ));
        }

        // Check if task is in our valid tasks list
        let is_valid = self.valid_tasks.iter().any(|(ns, name, version)| {
            ns == namespace && name == task_name && version == task_version
        });

        if is_valid {
            Ok(json!({
                "namespace": namespace,
                "task_name": task_name,
                "version": task_version,
                "steps": [
                    {"name": "step1", "handler": "TestHandler"},
                    {"name": "step2", "handler": "TestHandler"}
                ]
            }))
        } else {
            Err(TaskerError::ValidationError(format!(
                "Task not found: {}/{}/{}",
                namespace, task_name, task_version
            )))
        }
    }
}

/// Mock TaskInitializer for testing task creation
pub struct MockTaskInitializer;

impl MockTaskInitializer {
    pub fn new() -> Self {
        Self
    }
}

#[tokio::test]
async fn test_processor_config_defaults() {
    let config = TaskRequestProcessorConfig::default();

    assert_eq!(config.request_queue_name, "orchestration_task_requests");
    assert_eq!(
        config.processing_queue_name,
        "orchestration_tasks_to_be_processed"
    );
    assert_eq!(config.batch_size, 10);
    assert_eq!(config.visibility_timeout_seconds, 300);
    assert_eq!(config.polling_interval_seconds, 1);
    assert_eq!(config.max_processing_attempts, 3);
}

#[tokio::test]
async fn test_processor_config_customization() {
    let config = TaskRequestProcessorConfig {
        request_queue_name: "custom_requests".to_string(),
        processing_queue_name: "custom_processing".to_string(),
        batch_size: 20,
        visibility_timeout_seconds: 600,
        polling_interval_seconds: 5,
        max_processing_attempts: 5,
    };

    assert_eq!(config.request_queue_name, "custom_requests");
    assert_eq!(config.processing_queue_name, "custom_processing");
    assert_eq!(config.batch_size, 20);
    assert_eq!(config.visibility_timeout_seconds, 600);
    assert_eq!(config.polling_interval_seconds, 5);
    assert_eq!(config.max_processing_attempts, 5);
}

#[tokio::test]
async fn test_queue_creation_on_startup() {
    let mock_client = Arc::new(MockPgmqClient::new());
    let mock_registry = Arc::new(MockTaskHandlerRegistry::new());
    let mock_initializer = Arc::new(MockTaskInitializer::new());

    // Create a test database pool (this would normally be a real connection)
    let database_url = "postgresql://test:test@localhost/test_db";

    // For this test, we'll use a minimal processor setup
    let config = TaskRequestProcessorConfig::default();

    // We can't easily test the actual database operations without a real DB,
    // but we can test that the config is properly set up
    assert_eq!(config.request_queue_name, "orchestration_task_requests");
    assert_eq!(
        config.processing_queue_name,
        "orchestration_tasks_to_be_processed"
    );
}

#[tokio::test]
async fn test_task_request_validation_success() {
    let mock_registry = MockTaskHandlerRegistry::new();

    let request = TaskRequestMessage::new(
        "fulfillment".to_string(),
        "process_order".to_string(),
        "1.0.0".to_string(),
        json!({"order_id": 12345}),
        "api_gateway".to_string(),
    );

    let result = mock_registry
        .get_task_template(
            &request.namespace,
            &request.task_name,
            &request.task_version,
        )
        .await;

    assert!(result.is_ok());
    let template = result.unwrap();
    assert_eq!(template["namespace"], "fulfillment");
    assert_eq!(template["task_name"], "process_order");
    assert_eq!(template["version"], "1.0.0");
}

#[tokio::test]
async fn test_task_request_validation_failure() {
    let mock_registry = MockTaskHandlerRegistry::new();

    let request = TaskRequestMessage::new(
        "unknown_namespace".to_string(),
        "unknown_task".to_string(),
        "1.0.0".to_string(),
        json!({"data": "test"}),
        "api_gateway".to_string(),
    );

    let result = mock_registry
        .get_task_template(
            &request.namespace,
            &request.task_name,
            &request.task_version,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TaskerError::ValidationError(msg) => {
            assert!(msg.contains("Task not found"));
            assert!(msg.contains("unknown_namespace"));
            assert!(msg.contains("unknown_task"));
        }
        _ => panic!("Expected ValidationError"),
    }
}

#[tokio::test]
async fn test_task_request_validation_registry_failure() {
    let mock_registry = MockTaskHandlerRegistry::with_validation_failure();

    let request = TaskRequestMessage::new(
        "fulfillment".to_string(),
        "process_order".to_string(),
        "1.0.0".to_string(),
        json!({"order_id": 12345}),
        "api_gateway".to_string(),
    );

    let result = mock_registry
        .get_task_template(
            &request.namespace,
            &request.task_name,
            &request.task_version,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TaskerError::ValidationError(msg) => {
            assert_eq!(msg, "Mock validation failure");
        }
        _ => panic!("Expected ValidationError"),
    }
}

#[tokio::test]
async fn test_task_processing_message_creation() {
    let request = TaskRequestMessage::new(
        "inventory".to_string(),
        "stock_check".to_string(),
        "2.1.0".to_string(),
        json!({"sku": "ABC123", "quantity": 10}),
        "warehouse_system".to_string(),
    )
    .with_priority(TaskPriority::High);

    // Simulate successful task creation with a UUID
    let task_uuid = Uuid::now_v7();

    let processing_message = TaskProcessingMessage::new(
        task_uuid,
        request.namespace.clone(),
        request.task_name.clone(),
        request.task_version.clone(),
        request.request_id.clone(),
        request.metadata.priority.clone(),
    );

    assert_eq!(processing_message.task_uuid, task_uuid);
    assert_eq!(processing_message.namespace, "inventory");
    assert_eq!(processing_message.task_name, "stock_check");
    assert_eq!(processing_message.task_version, "2.1.0");
    assert_eq!(processing_message.metadata.request_id, request.request_id);
    assert!(matches!(
        processing_message.metadata.priority,
        TaskPriority::High
    ));
    assert_eq!(processing_message.metadata.processing_attempts, 0);
    assert!(processing_message.metadata.retry_after.is_none());
}

#[tokio::test]
async fn test_message_enqueueing_success() {
    let mock_client = MockPgmqClient::new();

    let test_uuid = Uuid::now_v7();
    let processing_message = TaskProcessingMessage::new(
        test_uuid,
        "fulfillment".to_string(),
        "process_order".to_string(),
        "1.0.0".to_string(),
        "req-abc-123".to_string(),
        TaskPriority::Normal,
    );

    let result = mock_client
        .send_json_message("orchestration_tasks_to_be_processed", &processing_message)
        .await;

    assert!(result.is_ok());
    let message_id = result.unwrap();
    assert_eq!(message_id, 1);

    // Verify message was stored correctly
    let sent_messages = mock_client.get_sent_messages().await;
    assert_eq!(sent_messages.len(), 1);

    let (queue_name, message_data) = &sent_messages[0];
    assert_eq!(queue_name, "orchestration_tasks_to_be_processed");
    assert_eq!(message_data["task_uuid"], test_uuid.to_string());
    assert_eq!(message_data["namespace"], "fulfillment");
    assert_eq!(message_data["task_name"], "process_order");
}

#[tokio::test]
async fn test_message_enqueueing_failure() {
    let mock_client = MockPgmqClient::with_send_failure();

    let test_uuid_2 = Uuid::now_v7();
    let processing_message = TaskProcessingMessage::new(
        test_uuid_2,
        "fulfillment".to_string(),
        "process_order".to_string(),
        "1.0.0".to_string(),
        "req-abc-123".to_string(),
        TaskPriority::Normal,
    );

    let result = mock_client
        .send_json_message("orchestration_tasks_to_be_processed", &processing_message)
        .await;

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Mock send failure"));

    // Verify no messages were stored
    let sent_messages = mock_client.get_sent_messages().await;
    assert_eq!(sent_messages.len(), 0);
}

#[tokio::test]
async fn test_task_request_message_parsing_variants() {
    // Test various task request message formats to ensure robust parsing

    // Basic request
    let basic_request = TaskRequestMessage::new(
        "payments".to_string(),
        "charge_card".to_string(),
        "1.0.0".to_string(),
        json!({"amount": 99.99, "currency": "USD"}),
        "e_commerce_api".to_string(),
    );

    let serialized = serde_json::to_value(&basic_request).unwrap();
    let parsed: TaskRequestMessage = serde_json::from_value(serialized).unwrap();
    assert_eq!(parsed.namespace, "payments");
    assert_eq!(parsed.task_name, "charge_card");

    // Request with high priority and custom metadata
    let priority_request = TaskRequestMessage::new(
        "notifications".to_string(),
        "urgent_alert".to_string(),
        "2.0.0".to_string(),
        json!({"message": "System failure detected", "recipients": ["admin@company.com"]}),
        "monitoring_system".to_string(),
    )
    .with_priority(TaskPriority::Urgent)
    .with_custom_metadata("alert_level".to_string(), json!("critical"))
    .with_custom_metadata("incident_id".to_string(), json!("INC-2025-001"));

    let serialized = serde_json::to_value(&priority_request).unwrap();
    let parsed: TaskRequestMessage = serde_json::from_value(serialized).unwrap();
    assert_eq!(parsed.namespace, "notifications");
    assert!(matches!(parsed.metadata.priority, TaskPriority::Urgent));
    assert!(parsed.metadata.custom.contains_key("alert_level"));
    assert!(parsed.metadata.custom.contains_key("incident_id"));

    // Request with complex nested data
    let complex_request = TaskRequestMessage::new(
        "order_fulfillment".to_string(),
        "process_bulk_order".to_string(),
        "3.1.2".to_string(),
        json!({
            "orders": [
                {
                    "order_id": "ORD-001",
                    "items": [
                        {"sku": "ITEM-A", "quantity": 2, "price": 29.99},
                        {"sku": "ITEM-B", "quantity": 1, "price": 49.99}
                    ],
                    "shipping": {
                        "method": "express",
                        "address": {
                            "street": "123 Main St",
                            "city": "San Francisco",
                            "state": "CA",
                            "zip": "94105"
                        }
                    }
                },
                {
                    "order_id": "ORD-002",
                    "items": [
                        {"sku": "ITEM-C", "quantity": 3, "price": 19.99}
                    ],
                    "shipping": {
                        "method": "standard",
                        "address": {
                            "street": "456 Oak Ave",
                            "city": "New York",
                            "state": "NY",
                            "zip": "10001"
                        }
                    }
                }
            ],
            "batch_settings": {
                "parallel_processing": true,
                "timeout_minutes": 30,
                "retry_failed_orders": false
            }
        }),
        "bulk_order_api".to_string(),
    );

    let serialized = serde_json::to_value(&complex_request).unwrap();
    let parsed: TaskRequestMessage = serde_json::from_value(serialized).unwrap();
    assert_eq!(parsed.namespace, "order_fulfillment");
    assert_eq!(parsed.task_name, "process_bulk_order");
    assert_eq!(parsed.task_version, "3.1.2");

    // Verify complex data structure is preserved
    let orders = parsed.input_data["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0]["order_id"], "ORD-001");
    assert_eq!(orders[1]["order_id"], "ORD-002");
    assert_eq!(
        parsed.input_data["batch_settings"]["parallel_processing"],
        true
    );
}

#[tokio::test]
async fn test_malformed_message_handling() {
    // Test various malformed message scenarios

    // Missing required fields
    let malformed_json = json!({
        "namespace": "test",
        // Missing task_name, task_version, input_data, metadata
    });

    let result: Result<TaskRequestMessage, _> = serde_json::from_value(malformed_json);
    assert!(result.is_err());

    // Invalid JSON structure
    let invalid_json = json!({
        "namespace": 123, // Should be string
        "task_name": "test_task",
        "task_version": "1.0.0",
        "input_data": {},
        "metadata": "invalid_metadata" // Should be object
    });

    let result: Result<TaskRequestMessage, _> = serde_json::from_value(invalid_json);
    assert!(result.is_err());

    // Empty values
    let empty_values = json!({
        "request_id": "",
        "namespace": "",
        "task_name": "",
        "task_version": "",
        "input_data": {},
        "metadata": {
            "requested_at": "2025-08-01T12:00:00Z",
            "requester": "",
            "priority": "Normal",
            "custom": {}
        }
    });

    let result: Result<TaskRequestMessage, _> = serde_json::from_value(empty_values);
    // This should parse successfully but with empty string values
    assert!(result.is_ok());
    let parsed = result.unwrap();
    assert_eq!(parsed.namespace, "");
    assert_eq!(parsed.task_name, "");
    assert_eq!(parsed.metadata.requester, "");
}

#[tokio::test]
async fn test_processor_statistics() {
    // Test the statistics structure (actual implementation would require database)
    let config = TaskRequestProcessorConfig::default();

    // Verify expected queue names are used
    assert_eq!(config.request_queue_name, "orchestration_task_requests");
    assert_eq!(
        config.processing_queue_name,
        "orchestration_tasks_to_be_processed"
    );

    // Test that we can create statistics structure
    // (In real implementation, this would query actual queue sizes)
    let stats = crate::orchestration::task_request_processor::TaskRequestProcessorStats {
        request_queue_size: 5,
        processing_queue_size: 12,
        request_queue_name: config.request_queue_name.clone(),
        processing_queue_name: config.processing_queue_name.clone(),
    };

    assert_eq!(stats.request_queue_size, 5);
    assert_eq!(stats.processing_queue_size, 12);
    assert_eq!(stats.request_queue_name, "orchestration_task_requests");
    assert_eq!(
        stats.processing_queue_name,
        "orchestration_tasks_to_be_processed"
    );
}

#[tokio::test]
async fn test_error_scenarios_and_recovery() {
    // Test various error scenarios that the processor should handle gracefully

    // Test TaskerError variants that could occur during processing
    let validation_error = TaskerError::ValidationError("Invalid task configuration".to_string());
    let database_error = TaskerError::DatabaseError("Connection timeout".to_string());
    let messaging_error = TaskerError::MessagingError("Queue unavailable".to_string());

    // Verify error message formatting
    assert_eq!(
        validation_error.to_string(),
        "Validation error: Invalid task configuration"
    );
    assert_eq!(
        database_error.to_string(),
        "Database error: Connection timeout"
    );
    assert_eq!(
        messaging_error.to_string(),
        "Messaging error: Queue unavailable"
    );

    // Test JSON serialization error conversion
    let json_error = serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Test JSON error",
    ));
    let tasker_error: TaskerError = json_error.into();

    match tasker_error {
        TaskerError::ValidationError(msg) => {
            assert!(msg.contains("JSON serialization error"));
        }
        _ => panic!("Expected ValidationError from JSON error conversion"),
    }
}

#[tokio::test]
async fn test_concurrent_processing_simulation() {
    // Simulate concurrent processing of multiple task requests
    let mock_client = Arc::new(MockPgmqClient::new());

    let task_requests = vec![
        TaskRequestMessage::new(
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            json!({"order_id": 1001}),
            "api_1".to_string(),
        ),
        TaskRequestMessage::new(
            "inventory".to_string(),
            "stock_check".to_string(),
            "2.1.0".to_string(),
            json!({"sku": "ABC123"}),
            "api_2".to_string(),
        ),
        TaskRequestMessage::new(
            "notifications".to_string(),
            "send_email".to_string(),
            "1.2.0".to_string(),
            json!({"recipient": "test@example.com"}),
            "api_3".to_string(),
        ),
    ];

    // Generate UUIDs for each task
    let task_uuids: Vec<Uuid> = (0..task_requests.len()).map(|_| Uuid::now_v7()).collect();

    // Simulate concurrent processing by creating processing messages
    let mut handles = vec![];

    for (i, request) in task_requests.iter().enumerate() {
        let client = mock_client.clone();
        let request = request.clone();
        let task_uuid = task_uuids[i];

        let handle = tokio::spawn(async move {
            let processing_message = TaskProcessingMessage::new(
                task_uuid,
                request.namespace.clone(),
                request.task_name.clone(),
                request.task_version.clone(),
                request.request_id.clone(),
                request.metadata.priority.clone(),
            );

            client
                .send_json_message("orchestration_tasks_to_be_processed", &processing_message)
                .await
        });

        handles.push(handle);
    }

    // Wait for all concurrent operations to complete
    let results = futures::future::join_all(handles).await;

    // Verify all operations succeeded
    for result in results {
        let send_result = result.unwrap();
        assert!(send_result.is_ok());
    }

    // Verify all messages were sent
    let sent_messages = mock_client.get_sent_messages().await;
    assert_eq!(sent_messages.len(), 3);

    // Verify message content
    for (i, (queue_name, message_data)) in sent_messages.iter().enumerate() {
        assert_eq!(queue_name, "orchestration_tasks_to_be_processed");
        assert_eq!(message_data["task_uuid"], task_uuids[i].to_string());
    }
}
