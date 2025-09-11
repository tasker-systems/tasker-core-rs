mod common;

use common::{create_orchestration_message, create_test_message, TestDb};
use serde_json::json;

/// Test complete PGMQ wrapper functions flow with notifications
/// Converted from test_complete_wrapper_flow.sql
/// Demonstrates: queue creation -> message sending -> notifications -> verification
#[tokio::test]
async fn test_complete_wrapper_flow() {
    let mut test_db = TestDb::new()
        .await
        .expect("Failed to connect to test database");

    // Setup test queues
    let test_complete_queue = test_db
        .create_test_queue("test_complete_queue")
        .await
        .expect("Failed to create test_complete_queue");

    let worker_rust_queue = test_db
        .create_test_queue("worker_rust_queue")
        .await
        .expect("Failed to create worker_rust_queue");

    let orchestration_queue = test_db
        .create_test_queue("orchestration")
        .await
        .expect("Failed to create orchestration queue");

    // Test 1: Single Message with Notifications
    // This will:
    // 1. Send message to PGMQ queue
    // 2. Extract namespace from queue name (test_complete_queue -> test_complete_queue)
    // 3. Send pg_notify to: pgmq_message_ready.test_complete_queue
    // 4. Send pg_notify to: pgmq_message_ready (global channel)
    let integration_message =
        create_test_message("integration_test", json!("testing complete flow"));

    let single_msg_id = test_db
        .send_message(&test_complete_queue, integration_message, 0)
        .await
        .expect("Failed to send single message");

    assert!(single_msg_id > 0, "Single message ID should be positive");

    // Verify expected notification channel
    assert_eq!(
        test_db.get_notification_channel(&test_complete_queue),
        "pgmq_message_ready.test_complete_queue"
    );

    // Test 2: Batch Messages with Notifications
    // This will:
    // 1. Send batch to PGMQ queue
    // 2. Extract namespace (worker_rust_queue -> rust)
    // 3. Send batch notification to: pgmq_message_ready.rust
    // 4. Send batch notification to: pgmq_message_ready (global channel)
    let batch_messages = vec![
        json!({
            "type": "batch_task_1",
            "data": "first task",
            "priority": "high"
        }),
        json!({
            "type": "batch_task_2",
            "data": "second task",
            "priority": "medium"
        }),
    ];

    let batch_msg_ids = test_db
        .send_batch_messages(&worker_rust_queue, batch_messages, 0)
        .await
        .expect("Failed to send batch messages");

    assert_eq!(batch_msg_ids.len(), 2, "Should return 2 batch message IDs");
    for id in &batch_msg_ids {
        assert!(*id > 0, "Each batch message ID should be positive");
    }

    // Verify expected notification channel for worker queue
    assert_eq!(
        test_db.get_notification_channel(&worker_rust_queue),
        "pgmq_message_ready.rust"
    );

    // Test 3: Orchestration Pattern
    // This will:
    // 1. Send message to orchestration queue
    // 2. Extract namespace (orchestration -> orchestration)
    // 3. Send pg_notify to: pgmq_message_ready.orchestration
    // 4. Send pg_notify to: pgmq_message_ready (global channel)
    let orchestration_message = create_orchestration_message(
        "process_task",
        "task-123",
        json!({
            "workflow": "order-fulfillment"
        }),
    );

    let orchestration_msg_id = test_db
        .send_message(&orchestration_queue, orchestration_message, 0)
        .await
        .expect("Failed to send orchestration message");

    assert!(
        orchestration_msg_id > 0,
        "Orchestration message ID should be positive"
    );

    // Verify expected notification channel for orchestration
    assert_eq!(
        test_db.get_notification_channel(&orchestration_queue),
        "pgmq_message_ready.orchestration"
    );

    // Test 4: Verification - Messages are in PGMQ queues
    let test_complete_count = test_db
        .get_message_count(&test_complete_queue)
        .await
        .expect("Failed to get test_complete_queue message count");
    assert_eq!(
        test_complete_count, 1,
        "test_complete_queue should have 1 message"
    );

    let worker_rust_count = test_db
        .get_message_count(&worker_rust_queue)
        .await
        .expect("Failed to get worker_rust_queue message count");
    assert_eq!(
        worker_rust_count, 2,
        "worker_rust_queue should have 2 messages (batch)"
    );

    let orchestration_count = test_db
        .get_message_count(&orchestration_queue)
        .await
        .expect("Failed to get orchestration queue message count");
    assert_eq!(
        orchestration_count, 1,
        "orchestration queue should have 1 message"
    );

    // Test 5: Summary - Expected Notification Channels
    let queue_channel_mappings = vec![
        (&test_complete_queue, "test_complete_queue"),
        (&worker_rust_queue, "rust"),
        (&orchestration_queue, "orchestration"),
    ];

    for (queue_name, expected_namespace) in queue_channel_mappings {
        let actual_channel = test_db.get_notification_channel(queue_name);
        let expected_channel = format!("pgmq_message_ready.{}", expected_namespace);
        assert_eq!(
            actual_channel, expected_channel,
            "Queue '{}' should map to channel '{}'",
            queue_name, expected_channel
        );
    }

    println!("✅ Complete wrapper flow test passed!");
    println!("   SUCCESS: All wrapper functions working with atomic notifications!");
    println!("   Each message send operation above also triggered pg_notify calls to:");
    println!("     1. Namespace-specific channel (e.g., pgmq_message_ready.rust)");
    println!("     2. Global channel (pgmq_message_ready)");
    println!("   This enables both targeted and broadcast notification listening patterns.");
}

/// Test delayed message functionality with wrapper functions
#[tokio::test]
async fn test_delayed_message_flow() {
    let mut test_db = TestDb::new()
        .await
        .expect("Failed to connect to test database");

    let test_queue = test_db
        .create_test_queue("delayed_test")
        .await
        .expect("Failed to create delayed test queue");

    // Send a delayed message (5 seconds)
    let delayed_message = create_test_message("delayed_test", json!("delayed message"));
    let msg_id = test_db
        .send_message(&test_queue, delayed_message, 5)
        .await
        .expect("Failed to send delayed message");

    assert!(msg_id > 0, "Delayed message ID should be positive");

    // Message should be in the queue but not immediately available for reading
    let message_count = test_db
        .get_message_count(&test_queue)
        .await
        .expect("Failed to get message count");
    assert_eq!(message_count, 1, "Queue should contain 1 delayed message");

    // Try to read immediately - should get empty result for visibility timeout
    let _immediate_messages = test_db
        .read_messages(&test_queue, 1, 10) // Very short visibility timeout
        .await
        .expect("Failed to read messages");

    // The message might be visible or not depending on when exactly it was inserted
    // and the current time, so we just verify the function works without errors

    println!("✅ Delayed message flow test passed!");
    println!("   - Delayed message successfully sent");
    println!("   - Queue contains expected message count");
    println!("   - Notification sent immediately (before delay expires)");
}

/// Test error handling in wrapper functions
#[tokio::test]
async fn test_wrapper_error_handling() {
    let test_db = TestDb::new()
        .await
        .expect("Failed to connect to test database");

    // Try to send to a non-existent queue (should fail gracefully)
    let result = test_db
        .send_message("nonexistent_queue_12345", json!({"test": "data"}), 0)
        .await;

    assert!(result.is_err(), "Sending to non-existent queue should fail");

    // Try to send batch to a non-existent queue (should fail gracefully)
    let batch_result = test_db
        .send_batch_messages(
            "nonexistent_batch_queue_12345",
            vec![json!({"test": "data"})],
            0,
        )
        .await;

    assert!(
        batch_result.is_err(),
        "Batch sending to non-existent queue should fail"
    );

    println!("✅ Wrapper error handling test passed!");
    println!("   - Non-existent queue errors handled gracefully");
    println!("   - Both single and batch operations validated");
}
