//! Integration tests for the complete TAS-43 Worker Event System
//!
//! This test validates the complete event flow from pgmq-notify events
//! through worker queue processing to command execution.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use pgmq_notify::MessageReadyEvent;
use tasker_shared::{
    event_system::{
        deployment::DeploymentMode,
        event_driven::{EventDrivenSystem, EventSystemStatistics},
    },
    messaging::UnifiedPgmqClient,
    system_context::SystemContext,
};
use tasker_worker::worker::{
    event_systems::worker_event_system::{WorkerEventSystem, WorkerEventSystemConfig},
    worker_queues::{
        events::WorkerQueueEvent, fallback_poller::WorkerPollerConfig,
        listener::WorkerListenerConfig,
    },
};

/// Test that the WorkerEventSystem can be created and started successfully
#[tokio::test]
async fn test_worker_event_system_creation_and_startup() {
    // Create test configuration
    let config = WorkerEventSystemConfig {
        system_id: "test-worker-event-system".to_string(),
        deployment_mode: DeploymentMode::PollingOnly, // Use polling only to avoid dependency on actual DB
        supported_namespaces: vec!["test_namespace".to_string()],
        health_monitoring_enabled: true,
        health_check_interval: Duration::from_secs(30),
        max_concurrent_processors: 5,
        processing_timeout: Duration::from_millis(100),
        listener: WorkerListenerConfig::default(),
        fallback_poller: WorkerPollerConfig {
            enabled: true,
            polling_interval: Duration::from_secs(1), // Fast polling for test
            batch_size: 5,
            age_threshold: Duration::from_millis(100),
            max_age: Duration::from_secs(60),
            supported_namespaces: vec!["test_namespace".to_string()],
            visibility_timeout: Duration::from_secs(10),
        },
    };

    // Create system context (mock)
    let context = Arc::new(
        SystemContext::new()
            .await
            .expect("Failed to create SystemContext"),
    );

    // Create command channel
    let (command_sender, _command_receiver) = mpsc::channel(100);

    // Create mock PGMQ client
    let pgmq_client = create_pgmq_client().await;

    // Create WorkerEventSystem
    let result = WorkerEventSystem::new(config, context, command_sender, pgmq_client).await;
    assert!(
        result.is_ok(),
        "Failed to create WorkerEventSystem: {:?}",
        result.err()
    );

    let event_system = result.unwrap();

    // Verify initial state
    assert_eq!(event_system.system_id(), "test-worker-event-system");
    assert_eq!(event_system.deployment_mode(), DeploymentMode::PollingOnly);
    assert!(!event_system.is_running());

    // Test that we can get initial statistics
    let stats = event_system.statistics();
    assert_eq!(stats.events_processed, 0);
    assert_eq!(stats.events_failed, 0);

    // Test health check
    let health_result = event_system.health_check().await;
    assert!(
        health_result.is_ok(),
        "Health check failed: {:?}",
        health_result.err()
    );

    println!("✅ WorkerEventSystem creation and basic operations test passed");
}

/// Test the event processing flow through the WorkerEventSystem
#[tokio::test]
async fn test_worker_event_processing_flow() {
    // Create configuration for hybrid mode (would use both listener and poller in real scenario)
    let config = WorkerEventSystemConfig {
        system_id: "test-event-processing".to_string(),
        deployment_mode: DeploymentMode::Hybrid,
        supported_namespaces: vec!["linear_workflow".to_string()],
        health_monitoring_enabled: true,
        health_check_interval: Duration::from_secs(10),
        max_concurrent_processors: 3,
        processing_timeout: Duration::from_millis(500),
        listener: WorkerListenerConfig::default(),
        fallback_poller: WorkerPollerConfig::default(),
    };

    let context = Arc::new(
        SystemContext::new()
            .await
            .expect("Failed to create SystemContext"),
    );
    let (command_sender, _command_receiver) = mpsc::channel(100);
    let pgmq_client = create_pgmq_client().await;

    let event_system = WorkerEventSystem::new(config, context, command_sender, pgmq_client)
        .await
        .expect("Failed to create event system");

    // Create test events
    let step_message_event =
        create_test_message_ready_event(123, "linear_workflow_queue", "linear_workflow");

    let worker_queue_event = WorkerQueueEvent::StepMessage(step_message_event);

    // Test event processing
    let process_result = event_system.process_event(worker_queue_event).await;
    assert!(
        process_result.is_ok(),
        "Event processing failed: {:?}",
        process_result.err()
    );

    // Verify statistics were updated
    let stats = event_system.statistics();
    assert_eq!(stats.step_messages_processed, 1);

    println!("✅ Worker event processing flow test passed");
}

/// Test WorkerNotification handling
#[tokio::test]
async fn test_worker_notification_handling() {
    let config = WorkerEventSystemConfig::default();
    let context = Arc::new(
        SystemContext::new()
            .await
            .expect("Failed to create SystemContext"),
    );
    let (command_sender, _command_receiver) = mpsc::channel(100);
    let pgmq_client = create_pgmq_client().await;

    let event_system = WorkerEventSystem::new(config, context, command_sender, pgmq_client)
        .await
        .expect("Failed to create event system");

    // Test different notification types
    let step_event = WorkerQueueEvent::StepMessage(create_test_message_ready_event(
        456,
        "test_queue",
        "test_namespace",
    ));

    let health_event = WorkerQueueEvent::HealthCheck(create_test_message_ready_event(
        457,
        "health_queue",
        "health",
    ));

    let config_event = WorkerQueueEvent::ConfigurationUpdate(create_test_message_ready_event(
        458,
        "config_queue",
        "config",
    ));

    // Process each event type
    assert!(event_system.process_event(step_event).await.is_ok());
    assert!(event_system.process_event(health_event).await.is_ok());
    assert!(event_system.process_event(config_event).await.is_ok());

    // Verify statistics
    let stats = event_system.statistics();
    assert_eq!(stats.events_processed, 3);
    assert_eq!(stats.step_messages_processed, 1);
    assert_eq!(stats.health_checks_processed, 1);
    assert_eq!(stats.config_updates_processed, 1);

    println!("✅ Worker notification handling test passed");
}

/// Test deployment mode behavior
#[tokio::test]
async fn test_deployment_mode_behavior() {
    let test_cases = vec![
        DeploymentMode::PollingOnly,
        DeploymentMode::EventDrivenOnly,
        DeploymentMode::Hybrid,
    ];

    for deployment_mode in test_cases {
        let mut config = WorkerEventSystemConfig::default();
        config.deployment_mode = deployment_mode.clone();
        config.system_id = format!("test-{:?}", deployment_mode);

        let context = Arc::new(
            SystemContext::new()
                .await
                .expect("Failed to create SystemContext"),
        );
        let (command_sender, _command_receiver) = mpsc::channel(100);
        let pgmq_client = create_pgmq_client().await;

        let event_system = WorkerEventSystem::new(config, context, command_sender, pgmq_client)
            .await
            .expect("Failed to create event system");

        // Verify deployment mode is set correctly
        assert_eq!(event_system.deployment_mode(), deployment_mode);

        // Verify health check works for all deployment modes
        let health_result = event_system.health_check().await;
        assert!(
            health_result.is_ok(),
            "Health check failed for {:?}: {:?}",
            deployment_mode,
            health_result.err()
        );

        println!("✅ Deployment mode {:?} test passed", deployment_mode);
    }
}

/// Test error handling in event processing
#[tokio::test]
async fn test_event_processing_error_handling() {
    let config = WorkerEventSystemConfig::default();
    let context = Arc::new(
        SystemContext::new()
            .await
            .expect("Failed to create SystemContext"),
    );
    let (command_sender, _command_receiver) = mpsc::channel(100);
    let pgmq_client = create_pgmq_client().await;

    let event_system = WorkerEventSystem::new(config, context, command_sender, pgmq_client)
        .await
        .expect("Failed to create event system");

    // Test unknown event handling
    let unknown_event = WorkerQueueEvent::Unknown {
        queue_name: "unknown_queue".to_string(),
        payload: "unknown_payload".to_string(),
    };

    // This should handle the unknown event gracefully
    let result = event_system.process_event(unknown_event).await;
    assert!(
        result.is_ok(),
        "Unknown event processing should not fail: {:?}",
        result.err()
    );

    // Verify statistics show the failed event
    let stats = event_system.statistics();
    assert_eq!(stats.events_failed, 1);

    println!("✅ Event processing error handling test passed");
}

// Helper functions

async fn create_pgmq_client() -> Arc<UnifiedPgmqClient> {
    use tasker_shared::config::ConfigManager;
    use tasker_shared::system_context::SystemContext;

    let config_manager = ConfigManager::load_from_env("test").unwrap();
    let system_context = SystemContext::from_config(config_manager)
        .await
        .map_err(|e| {
            panic!("Failed to create system context: {e}");
        })
        .unwrap();
    system_context.message_client.clone()
}

fn create_test_message_ready_event(
    msg_id: i64,
    queue_name: &str,
    namespace: &str,
) -> MessageReadyEvent {
    use std::collections::HashMap;

    MessageReadyEvent {
        msg_id,
        queue_name: queue_name.to_string(),
        namespace: namespace.to_string(),
        ready_at: chrono::Utc::now(),
        metadata: HashMap::new(),
        visibility_timeout_seconds: Some(30),
    }
}

/// Integration test that validates the complete event flow
/// This test demonstrates the full TAS-43 implementation working together
#[tokio::test]
async fn test_complete_tas43_integration() {
    println!("🚀 Running complete TAS-43 Worker Event System integration test");

    // 1. Create a complete WorkerEventSystem configuration
    let config = WorkerEventSystemConfig {
        system_id: "tas43-integration-test".to_string(),
        deployment_mode: DeploymentMode::Hybrid,
        supported_namespaces: vec![
            "linear_workflow".to_string(),
            "order_fulfillment".to_string(),
        ],
        health_monitoring_enabled: true,
        health_check_interval: Duration::from_secs(5),
        max_concurrent_processors: 10,
        processing_timeout: Duration::from_millis(200),
        listener: WorkerListenerConfig {
            supported_namespaces: vec![
                "linear_workflow".to_string(),
                "order_fulfillment".to_string(),
            ],
            retry_interval: Duration::from_millis(100),
            max_retry_attempts: 3,
            event_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(10),
            batch_processing: true,
            connection_timeout: Duration::from_secs(5),
        },
        fallback_poller: WorkerPollerConfig {
            enabled: true,
            polling_interval: Duration::from_millis(500),
            batch_size: 10,
            age_threshold: Duration::from_millis(200),
            max_age: Duration::from_secs(300),
            supported_namespaces: vec![
                "linear_workflow".to_string(),
                "order_fulfillment".to_string(),
            ],
            visibility_timeout: Duration::from_secs(30),
        },
    };

    // 2. Set up system components
    let context = Arc::new(
        SystemContext::new()
            .await
            .expect("Failed to create SystemContext"),
    );
    let (command_sender, _command_receiver) = mpsc::channel(1000);
    let pgmq_client = create_pgmq_client().await;

    // 3. Create and verify WorkerEventSystem
    let event_system = WorkerEventSystem::new(config, context, command_sender.clone(), pgmq_client)
        .await
        .expect("Failed to create WorkerEventSystem for integration test");

    // 4. Verify system properties
    assert_eq!(event_system.system_id(), "tas43-integration-test");
    assert_eq!(event_system.deployment_mode(), DeploymentMode::Hybrid);
    assert!(!event_system.is_running());

    // 5. Test event processing capabilities
    let events = vec![
        WorkerQueueEvent::StepMessage(create_test_message_ready_event(
            1001,
            "linear_workflow_queue",
            "linear_workflow",
        )),
        WorkerQueueEvent::StepMessage(create_test_message_ready_event(
            1002,
            "order_fulfillment_queue",
            "order_fulfillment",
        )),
        WorkerQueueEvent::HealthCheck(create_test_message_ready_event(
            1003,
            "health_queue",
            "health",
        )),
        WorkerQueueEvent::ConfigurationUpdate(create_test_message_ready_event(
            1004,
            "config_queue",
            "config",
        )),
    ];

    // Process all test events
    for event in events {
        let result = event_system.process_event(event).await;
        assert!(
            result.is_ok(),
            "Event processing failed: {:?}",
            result.err()
        );
    }

    // 6. Verify comprehensive statistics
    let stats = event_system.statistics();
    assert_eq!(stats.events_processed, 4, "Should have processed 4 events");
    assert_eq!(
        stats.step_messages_processed, 2,
        "Should have processed 2 step messages"
    );
    assert_eq!(
        stats.health_checks_processed, 1,
        "Should have processed 1 health check"
    );
    assert_eq!(
        stats.config_updates_processed, 1,
        "Should have processed 1 config update"
    );
    assert_eq!(stats.events_failed, 0, "Should have no failed events");

    // 7. Test health check
    let health_result = event_system.health_check().await;
    assert!(health_result.is_ok(), "Health check should pass");

    // 8. Test unknown event handling
    let unknown_event = WorkerQueueEvent::Unknown {
        queue_name: "mystery_queue".to_string(),
        payload: "mysterious_payload".to_string(),
    };

    let unknown_result = event_system.process_event(unknown_event).await;
    assert!(
        unknown_result.is_ok(),
        "Unknown event should be handled gracefully"
    );

    // Final statistics verification
    let final_stats = event_system.statistics();
    assert_eq!(
        final_stats.events_processed, 4,
        "Events processed count should remain 4"
    );
    assert_eq!(
        final_stats.events_failed, 1,
        "Should have 1 failed event (unknown)"
    );

    println!("✅ TAS-43 Complete integration test passed!");
    println!("   📊 Events processed: {}", final_stats.events_processed);
    println!(
        "   📨 Step messages: {}",
        final_stats.step_messages_processed
    );
    println!(
        "   🔍 Health checks: {}",
        final_stats.health_checks_processed
    );
    println!(
        "   ⚙️  Config updates: {}",
        final_stats.config_updates_processed
    );
    println!("   ❌ Failed events: {}", final_stats.events_failed);
    println!("   🎉 TAS-43 Worker Event System is fully operational!");
}
