//! Unit tests for worker queue event types and notifications
//!
//! These tests validate the event type creation, classification,
//! and method implementations without requiring external dependencies.

use super::*;
use pgmq_notify::MessageReadyEvent;
use std::collections::HashMap;

#[test]
fn test_worker_queue_event_creation() {
    let message_event = MessageReadyEvent {
        msg_id: 123,
        queue_name: "test_queue".to_string(),
        namespace: "test_namespace".to_string(),
        ready_at: chrono::Utc::now(),
        metadata: HashMap::new(),
        visibility_timeout_seconds: Some(30),
    };

    let step_event = WorkerQueueEvent::StepMessage(message_event.clone());
    let health_event = WorkerQueueEvent::HealthCheck(message_event.clone());
    let config_event = WorkerQueueEvent::ConfigurationUpdate(message_event.clone());

    // Test event creation
    match step_event {
        WorkerQueueEvent::StepMessage(event) => {
            assert_eq!(event.queue_name, "test_queue");
            assert_eq!(event.namespace, "test_namespace");
            assert_eq!(event.msg_id, 123);
        }
        _ => panic!("Expected StepMessage"),
    }

    match health_event {
        WorkerQueueEvent::HealthCheck(event) => {
            assert_eq!(event.queue_name, "test_queue");
            assert_eq!(event.namespace, "test_namespace");
        }
        _ => panic!("Expected HealthCheck"),
    }

    match config_event {
        WorkerQueueEvent::ConfigurationUpdate(event) => {
            assert_eq!(event.queue_name, "test_queue");
            assert_eq!(event.namespace, "test_namespace");
        }
        _ => panic!("Expected ConfigurationUpdate"),
    }
}

#[test]
fn test_worker_queue_event_unknown() {
    let unknown_event = WorkerQueueEvent::Unknown {
        queue_name: "unknown_queue".to_string(),
        payload: "unknown payload".to_string(),
    };

    match unknown_event {
        WorkerQueueEvent::Unknown {
            queue_name,
            payload,
        } => {
            assert_eq!(queue_name, "unknown_queue");
            assert_eq!(payload, "unknown payload");
        }
        _ => panic!("Expected Unknown event"),
    }
}

#[test]
fn test_worker_queue_event_methods() {
    let message_event = MessageReadyEvent {
        msg_id: 123,
        queue_name: "linear_workflow_queue".to_string(),
        namespace: "linear_workflow".to_string(),
        ready_at: chrono::Utc::now(),
        metadata: HashMap::new(),
        visibility_timeout_seconds: Some(30),
    };

    let step_event = WorkerQueueEvent::StepMessage(message_event.clone());
    let health_event = WorkerQueueEvent::HealthCheck(message_event.clone());
    let config_event = WorkerQueueEvent::ConfigurationUpdate(message_event);
    let unknown_event = WorkerQueueEvent::Unknown {
        queue_name: "mystery_queue".to_string(),
        payload: "mystery_payload".to_string(),
    };

    // Test namespace method
    assert_eq!(step_event.namespace(), Some("linear_workflow"));
    assert_eq!(health_event.namespace(), Some("linear_workflow"));
    assert_eq!(config_event.namespace(), Some("linear_workflow"));
    assert_eq!(unknown_event.namespace(), None);

    // Test queue_name method
    assert_eq!(step_event.queue_name(), "linear_workflow_queue");
    assert_eq!(health_event.queue_name(), "linear_workflow_queue");
    assert_eq!(config_event.queue_name(), "linear_workflow_queue");
    assert_eq!(unknown_event.queue_name(), "mystery_queue");

    // Test is_step_message method
    assert!(step_event.is_step_message());
    assert!(!health_event.is_step_message());
    assert!(!config_event.is_step_message());
    assert!(!unknown_event.is_step_message());
}

#[test]
fn test_worker_notification_creation() {
    let message_event = MessageReadyEvent {
        msg_id: 456,
        queue_name: "order_fulfillment_queue".to_string(),
        namespace: "order_fulfillment".to_string(),
        ready_at: chrono::Utc::now(),
        metadata: HashMap::new(),
        visibility_timeout_seconds: Some(60),
    };

    let event_notification =
        WorkerNotification::Event(WorkerQueueEvent::StepMessage(message_event));

    let health_notification = WorkerNotification::Health(WorkerHealthUpdate {
        worker_id: "worker-production-1".to_string(),
        status: WorkerHealthStatus::Healthy,
        supported_namespaces: vec![
            "order_fulfillment".to_string(),
            "linear_workflow".to_string(),
        ],
        last_activity: Some(chrono::Utc::now().to_rfc3339()),
        details: Some("All systems operational - processing 50 msgs/sec".to_string()),
    });

    let config_notification = WorkerNotification::Configuration(WorkerConfigUpdate {
        worker_id: "worker-production-1".to_string(),
        update_type: ConfigUpdateType::ProcessingParametersUpdated,
        namespaces: vec!["order_fulfillment".to_string()],
        timestamp: chrono::Utc::now().to_rfc3339(),
        details: Some("Batch size increased from 10 to 25".to_string()),
    });

    // Test notification creation
    match event_notification {
        WorkerNotification::Event(WorkerQueueEvent::StepMessage(event)) => {
            assert_eq!(event.queue_name, "order_fulfillment_queue");
            assert_eq!(event.namespace, "order_fulfillment");
            assert_eq!(event.msg_id, 456);
        }
        _ => panic!("Expected Event notification with StepMessage"),
    }

    match health_notification {
        WorkerNotification::Health(update) => {
            assert_eq!(update.worker_id, "worker-production-1");
            assert!(matches!(update.status, WorkerHealthStatus::Healthy));
            assert_eq!(update.supported_namespaces.len(), 2);
            assert!(update.last_activity.is_some());
        }
        _ => panic!("Expected Health notification"),
    }

    match config_notification {
        WorkerNotification::Configuration(update) => {
            assert_eq!(update.worker_id, "worker-production-1");
            assert!(matches!(
                update.update_type,
                ConfigUpdateType::ProcessingParametersUpdated
            ));
            assert_eq!(update.namespaces, vec!["order_fulfillment"]);
        }
        _ => panic!("Expected Configuration notification"),
    }
}

#[test]
fn test_worker_notification_methods() {
    let message_event = MessageReadyEvent {
        msg_id: 789,
        queue_name: "inventory_queue".to_string(),
        namespace: "inventory".to_string(),
        ready_at: chrono::Utc::now(),
        metadata: HashMap::new(),
        visibility_timeout_seconds: Some(45),
    };

    let event_notification =
        WorkerNotification::Event(WorkerQueueEvent::StepMessage(message_event));

    let health_notification = WorkerNotification::Health(WorkerHealthUpdate {
        worker_id: "worker-staging-2".to_string(),
        status: WorkerHealthStatus::Warning,
        supported_namespaces: vec!["inventory".to_string()],
        last_activity: Some(chrono::Utc::now().to_rfc3339()),
        details: Some("High memory usage detected".to_string()),
    });

    let config_notification = WorkerNotification::Configuration(WorkerConfigUpdate {
        worker_id: "worker-staging-2".to_string(),
        update_type: ConfigUpdateType::NamespaceAdded,
        namespaces: vec!["inventory".to_string(), "notifications".to_string()],
        timestamp: chrono::Utc::now().to_rfc3339(),
        details: Some("Added support for notifications namespace".to_string()),
    });

    // Test is_step_message method
    assert!(event_notification.is_step_message());
    assert!(!health_notification.is_step_message());
    assert!(!config_notification.is_step_message());

    // Test as_queue_event method
    assert!(event_notification.as_queue_event().is_some());
    assert!(health_notification.as_queue_event().is_none());
    assert!(config_notification.as_queue_event().is_none());

    // Verify the returned queue event
    if let Some(queue_event) = event_notification.as_queue_event() {
        assert!(queue_event.is_step_message());
        assert_eq!(queue_event.namespace(), Some("inventory"));
        assert_eq!(queue_event.queue_name(), "inventory_queue");
    }
}

#[test]
fn test_worker_health_status_display() {
    assert_eq!(WorkerHealthStatus::Healthy.to_string(), "healthy");
    assert_eq!(WorkerHealthStatus::Degraded.to_string(), "degraded");
    assert_eq!(WorkerHealthStatus::Warning.to_string(), "warning");
    assert_eq!(WorkerHealthStatus::Critical.to_string(), "critical");
    assert_eq!(WorkerHealthStatus::Offline.to_string(), "offline");
}

#[test]
fn test_config_update_type_display() {
    assert_eq!(
        ConfigUpdateType::NamespaceAdded.to_string(),
        "namespace_added"
    );
    assert_eq!(
        ConfigUpdateType::NamespaceRemoved.to_string(),
        "namespace_removed"
    );
    assert_eq!(
        ConfigUpdateType::ProcessingParametersUpdated.to_string(),
        "processing_parameters_updated"
    );
    assert_eq!(
        ConfigUpdateType::TemplateCacheUpdated.to_string(),
        "template_cache_updated"
    );
    assert_eq!(
        ConfigUpdateType::ConnectionConfigUpdated.to_string(),
        "connection_config_updated"
    );
}

#[test]
fn test_worker_queue_event_serialization() {
    // Test health update serialization
    let health_update = WorkerHealthUpdate {
        worker_id: "test-worker".to_string(),
        status: WorkerHealthStatus::Healthy,
        supported_namespaces: vec!["test".to_string()],
        last_activity: Some("2025-08-29T10:00:00Z".to_string()),
        details: Some("Test details".to_string()),
    };

    let json = serde_json::to_string(&health_update).expect("Should serialize");
    let deserialized: WorkerHealthUpdate = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(deserialized.worker_id, "test-worker");
    assert!(matches!(deserialized.status, WorkerHealthStatus::Healthy));

    // Test config update serialization
    let config_update = WorkerConfigUpdate {
        worker_id: "test-worker".to_string(),
        update_type: ConfigUpdateType::ProcessingParametersUpdated,
        namespaces: vec!["test".to_string()],
        timestamp: "2025-08-29T10:00:00Z".to_string(),
        details: Some("Updated batch size".to_string()),
    };

    let json = serde_json::to_string(&config_update).expect("Should serialize");
    let deserialized: WorkerConfigUpdate = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(deserialized.worker_id, "test-worker");
    assert!(matches!(
        deserialized.update_type,
        ConfigUpdateType::ProcessingParametersUpdated
    ));
}

#[test]
fn test_queue_event_classification_patterns() {
    // Test different queue naming patterns for event classification
    let test_cases = vec![
        ("linear_workflow_queue", "linear_workflow", true),
        ("order_fulfillment_queue", "order_fulfillment", true),
        ("inventory_health", "inventory", false),
        ("notifications_config", "notifications", false),
        ("mystery_system", "mystery", false),
        ("payment_processing_queue", "payment_processing", true),
    ];

    for (queue_name, namespace, should_be_step) in test_cases {
        let message_event = MessageReadyEvent {
            msg_id: 100,
            queue_name: queue_name.to_string(),
            namespace: namespace.to_string(),
            ready_at: chrono::Utc::now(),
            metadata: HashMap::new(),
            visibility_timeout_seconds: Some(30),
        };

        // Simulate event classification logic
        let event = if queue_name.ends_with("_queue") {
            WorkerQueueEvent::StepMessage(message_event)
        } else if queue_name.contains("_health") {
            WorkerQueueEvent::HealthCheck(message_event)
        } else if queue_name.contains("_config") {
            WorkerQueueEvent::ConfigurationUpdate(message_event)
        } else {
            WorkerQueueEvent::Unknown {
                queue_name: queue_name.to_string(),
                payload: "Unclassified worker queue event".to_string(),
            }
        };

        assert_eq!(
            event.is_step_message(),
            should_be_step,
            "Queue '{}' step message classification failed",
            queue_name
        );

        // For Unknown events, namespace() returns None, otherwise it should match
        let expected_namespace = if matches!(event, WorkerQueueEvent::Unknown { .. }) {
            None
        } else {
            Some(namespace)
        };
        assert_eq!(
            event.namespace(),
            expected_namespace,
            "Queue '{}' namespace extraction failed",
            queue_name
        );
    }
}
