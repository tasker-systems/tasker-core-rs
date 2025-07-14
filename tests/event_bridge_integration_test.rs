//! Integration test for Ruby event bridge
//!
//! This test verifies that events published by the Rust EventPublisher
//! can be forwarded to external callback systems like Ruby bindings.

use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tasker_core::events::publisher::{
    clear_external_event_callbacks, register_external_event_callback, EventPublisher,
    EventPublisherConfig,
};
use tasker_core::events::types::{Event, OrchestrationEvent};

#[tokio::test]
async fn test_external_event_callback_registration() {
    // Clear any existing callbacks for test isolation
    clear_external_event_callbacks();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Create a test callback that collects events
    let received_events = Arc::new(Mutex::new(HashMap::new()));
    let received_events_clone = received_events.clone();

    register_external_event_callback(move |event_name: &str, payload: &serde_json::Value| {
        println!("📥 Received event: {event_name} with payload: {payload}");
        let mut events = received_events_clone.lock().unwrap();
        events.insert(event_name.to_string(), payload.clone());
        Ok(())
    });

    // Create event publisher with FFI enabled, sync processing for test isolation
    let config = EventPublisherConfig {
        ffi_enabled: true,
        async_processing: false, // Disable async processing for deterministic tests
        ..Default::default()
    };
    let publisher = EventPublisher::with_config(config);

    // Test simple event
    publisher
        .publish("test.simple", json!({"message": "hello"}))
        .await
        .unwrap();

    // Test orchestration event
    let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationStarted {
        task_id: 123,
        framework: "test".to_string(),
        started_at: Utc::now(),
    });
    publisher.publish_event(event).await.unwrap();

    // Give FFI callbacks time to complete (sync processing)
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    // Verify events were received
    let events = received_events.lock().unwrap();
    println!(
        "📊 All received events: {:?}",
        events.keys().collect::<Vec<_>>()
    );
    for (name, payload) in events.iter() {
        println!("   {name}: {payload}");
    }

    // Check simple event
    assert!(
        events.contains_key("test.simple"),
        "Simple event not received"
    );
    let simple_event = &events["test.simple"];
    // Simple events are wrapped in Generic type
    assert_eq!(simple_event["Generic"]["context"]["message"], "hello");

    // Check orchestration event
    assert!(
        events.contains_key("task_orchestration_started"),
        "Orchestration event not received"
    );
    let orch_event = &events["task_orchestration_started"];
    // Orchestration events are wrapped in Orchestration type
    let orch_data = &orch_event["Orchestration"]["TaskOrchestrationStarted"];
    assert_eq!(orch_data["task_id"], 123);
    assert_eq!(orch_data["framework"], "test");

    println!("✅ External event callback integration test passed");

    // Cleanup for test isolation
    clear_external_event_callbacks();
}

#[tokio::test]
#[serial_test::serial]
async fn test_multiple_external_callbacks() {
    // Ensure test isolation with better synchronization
    clear_external_event_callbacks();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Register multiple callbacks
    let counter1 = Arc::new(Mutex::new(0));
    let counter1_clone = counter1.clone();
    register_external_event_callback(move |_event_name, _payload| {
        let mut count = counter1_clone.lock().unwrap();
        *count += 1;
        Ok(())
    });

    let counter2 = Arc::new(Mutex::new(0));
    let counter2_clone = counter2.clone();
    register_external_event_callback(move |_event_name, _payload| {
        let mut count = counter2_clone.lock().unwrap();
        *count += 1;
        Ok(())
    });

    // Publish an event with FFI enabled, sync processing for test isolation
    let config = EventPublisherConfig {
        ffi_enabled: true,
        async_processing: false, // Disable async processing for deterministic tests
        ..Default::default()
    };
    let publisher = EventPublisher::with_config(config);
    // Use unique event name to avoid cross-test interference
    let unique_event = format!(
        "test.multi.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    publisher
        .publish(&unique_event, json!({"test": true}))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Both callbacks should have been called
    assert_eq!(*counter1.lock().unwrap(), 1, "First callback not called");
    assert_eq!(*counter2.lock().unwrap(), 1, "Second callback not called");

    println!("✅ Multiple external callbacks test passed");

    // Cleanup for test isolation
    clear_external_event_callbacks();
}

#[tokio::test]
#[serial_test::serial]
async fn test_callback_error_handling() {
    // Ensure test isolation with better synchronization
    clear_external_event_callbacks();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Register a callback that fails
    register_external_event_callback(|_event_name, _payload| {
        Err("Intentional test error".to_string())
    });

    // Register a callback that succeeds
    let success_counter = Arc::new(Mutex::new(0));
    let success_counter_clone = success_counter.clone();
    register_external_event_callback(move |_event_name, _payload| {
        let mut count = success_counter_clone.lock().unwrap();
        *count += 1;
        Ok(())
    });

    // Publish an event with FFI enabled, sync processing for test isolation
    let config = EventPublisherConfig {
        ffi_enabled: true,
        async_processing: false, // Disable async processing for deterministic tests
        ..Default::default()
    };
    let publisher = EventPublisher::with_config(config);
    // Use unique event name to avoid cross-test interference
    let unique_event = format!(
        "test.error.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    publisher
        .publish(&unique_event, json!({"test": true}))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Success callback should still have been called despite the error
    assert_eq!(
        *success_counter.lock().unwrap(),
        1,
        "Success callback not called despite other callback error"
    );

    println!("✅ Callback error handling test passed");

    // Cleanup for test isolation
    clear_external_event_callbacks();
}
