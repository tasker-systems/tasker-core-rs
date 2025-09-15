mod common;

use common::{create_test_message, TestDb};
use serde_json::json;
use std::collections::HashMap;

/// Test basic PGMQ wrapper functions with notifications
/// Converted from test_pgmq_wrapper.sql
#[tokio::test]
async fn test_pgmq_wrapper_functions() {
    let mut test_db = TestDb::new()
        .await
        .expect("Failed to connect to test database");

    // Test 1: Single message with pgmq_send_with_notify
    let test_queue = test_db
        .create_test_queue("test_queue")
        .await
        .expect("Failed to create test queue");

    let test_message = create_test_message("test", json!("hello world"));
    let msg_id = test_db
        .send_message(&test_queue, test_message.clone(), 0)
        .await
        .expect("Failed to send single message");

    assert!(msg_id > 0, "Message ID should be positive");

    // Test 2: Batch messages with pgmq_send_batch_with_notify
    let batch_messages = vec![
        create_test_message("batch_test", json!("message 1")),
        create_test_message("batch_test", json!("message 2")),
        create_test_message("batch_test", json!("message 3")),
    ];

    let batch_msg_ids = test_db
        .send_batch_messages(&test_queue, batch_messages, 0)
        .await
        .expect("Failed to send batch messages");

    assert_eq!(batch_msg_ids.len(), 3, "Should return 3 message IDs");
    for id in &batch_msg_ids {
        assert!(*id > 0, "Each batch message ID should be positive");
    }

    // Test 3: Different queue patterns with notifications
    let worker_rust_queue = test_db
        .create_test_queue("worker_rust_queue")
        .await
        .expect("Failed to create worker rust queue");

    let worker_message = create_test_message("worker_test", json!("rust worker task"));
    let worker_msg_id = test_db
        .send_message(&worker_rust_queue, worker_message, 0)
        .await
        .expect("Failed to send worker message");

    assert!(worker_msg_id > 0, "Worker message ID should be positive");

    let orchestration_queue = test_db
        .create_test_queue("orchestration_priority")
        .await
        .expect("Failed to create orchestration queue");

    let orchestration_message = create_test_message(
        "orchestration_test",
        json!("high priority orchestration task"),
    );
    let orchestration_msg_id = test_db
        .send_message(&orchestration_queue, orchestration_message, 0)
        .await
        .expect("Failed to send orchestration message");

    assert!(
        orchestration_msg_id > 0,
        "Orchestration message ID should be positive"
    );

    // Test 4: Check messages were actually sent
    let test_queue_count = test_db
        .get_message_count(&test_queue)
        .await
        .expect("Failed to get test queue message count");
    assert_eq!(
        test_queue_count, 4,
        "Test queue should have 4 messages (1 single + 3 batch)"
    );

    let worker_queue_count = test_db
        .get_message_count(&worker_rust_queue)
        .await
        .expect("Failed to get worker queue message count");
    assert_eq!(worker_queue_count, 1, "Worker queue should have 1 message");

    let orchestration_queue_count = test_db
        .get_message_count(&orchestration_queue)
        .await
        .expect("Failed to get orchestration queue message count");
    assert_eq!(
        orchestration_queue_count, 1,
        "Orchestration queue should have 1 message"
    );

    // Test 5: Test namespace extraction for different patterns
    let queue_patterns = vec![
        "test_queue",
        "worker_rust_queue",
        "worker_python_queue",
        "orchestration",
        "orchestration_priority",
        "analytics_queue",
        "some_unknown_format",
    ];

    let namespace_results = test_db
        .test_namespace_extraction(queue_patterns)
        .await
        .expect("Failed to test namespace extraction");

    // Verify expected namespace mappings
    let expected_mappings: HashMap<&str, &str> = [
        ("test_queue", "test"),
        ("worker_rust_queue", "rust"),
        ("worker_python_queue", "python"),
        ("orchestration", "orchestration"),
        ("orchestration_priority", "orchestration"),
        ("analytics_queue", "analytics"),
        ("some_unknown_format", "default"),
    ]
    .iter()
    .cloned()
    .collect();

    for (queue_name, expected_namespace) in expected_mappings {
        let actual_namespace = namespace_results
            .get(queue_name)
            .unwrap_or_else(|| panic!("Missing namespace result for {}", queue_name));
        assert_eq!(
            actual_namespace, expected_namespace,
            "Queue '{}' should extract namespace '{}'",
            queue_name, expected_namespace
        );
    }

    // Test notification channel construction
    assert_eq!(
        test_db.get_notification_channel("worker_rust_queue"),
        "pgmq_message_ready.rust"
    );
    assert_eq!(
        test_db.get_notification_channel("orchestration"),
        "pgmq_message_ready.orchestration"
    );
    assert_eq!(
        test_db.get_notification_channel("analytics_queue"),
        "pgmq_message_ready.analytics"
    );

    println!("✅ All PGMQ wrapper function tests passed!");
    println!("   - Single message sending with notifications");
    println!("   - Batch message sending with notifications");
    println!("   - Multiple queue patterns");
    println!("   - Namespace extraction and channel mapping");
}

/// Test message content integrity through the wrapper functions
#[tokio::test]
async fn test_message_content_integrity() {
    let mut test_db = TestDb::new()
        .await
        .expect("Failed to connect to test database");
    let test_queue = test_db
        .create_test_queue("content_test")
        .await
        .expect("Failed to create test queue");

    // Send a message with complex JSON content
    let complex_message = json!({
        "type": "complex_test",
        "nested": {
            "array": [1, 2, 3],
            "object": {
                "key": "value",
                "number": 42
            }
        },
        "unicode": "Hello 世界 🌍",
        "boolean": true,
        "null_value": null
    });

    let msg_id = test_db
        .send_message(&test_queue, complex_message.clone(), 0)
        .await
        .expect("Failed to send complex message");

    // Read the message back and verify content integrity
    let messages = test_db
        .read_messages(&test_queue, 30, 1)
        .await
        .expect("Failed to read messages");

    assert_eq!(messages.len(), 1, "Should have exactly 1 message");
    let (returned_msg_id, returned_message) = &messages[0];
    assert_eq!(*returned_msg_id, msg_id, "Message ID should match");
    assert_eq!(
        *returned_message, complex_message,
        "Message content should be identical"
    );

    println!("✅ Message content integrity test passed!");
}
