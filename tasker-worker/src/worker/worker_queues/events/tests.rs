//! Unit tests for worker queue events

use super::*;

#[test]
fn test_worker_queue_event_creation() {
    // TAS-133: Use provider-agnostic MessageEvent
    let message_event = MessageEvent::new("test_queue", "test_namespace", "123");

    let step_event = WorkerQueueEvent::StepMessage(message_event.clone());
    let health_event = WorkerQueueEvent::HealthCheck(message_event.clone());
    let config_event = WorkerQueueEvent::ConfigurationUpdate(message_event);

    // Test event creation
    match step_event {
        WorkerQueueEvent::StepMessage(event) => {
            assert_eq!(event.queue_name, "test_queue");
            assert_eq!(event.namespace, "test_namespace");
            assert_eq!(event.message_id.as_str(), "123");
        }
        _ => panic!("Expected StepMessage"),
    }

    match health_event {
        WorkerQueueEvent::HealthCheck(_) => {}
        _ => panic!("Expected HealthCheck"),
    }

    match config_event {
        WorkerQueueEvent::ConfigurationUpdate(_) => {}
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
fn test_worker_notification_creation() {
    // TAS-133: Use provider-agnostic MessageEvent
    let message_event = MessageEvent::new("test_queue", "test_namespace", "123");

    let event_notification =
        WorkerNotification::Event(WorkerQueueEvent::StepMessage(message_event));

    let health_notification = WorkerNotification::Health(WorkerHealthUpdate {
        worker_id: "worker-1".to_string(),
        status: WorkerHealthStatus::Healthy,
        supported_namespaces: vec!["test_namespace".to_string()],
        last_activity: Some(chrono::Utc::now().to_rfc3339()),
        details: Some("All systems operational".to_string()),
    });

    let config_notification = WorkerNotification::Configuration(WorkerConfigUpdate {
        worker_id: "worker-1".to_string(),
        update_type: ConfigUpdateType::ProcessingParametersUpdated,
        namespaces: vec!["test_namespace".to_string()],
        timestamp: chrono::Utc::now().to_rfc3339(),
        details: Some("Batch size updated".to_string()),
    });

    // Test notification creation
    match event_notification {
        WorkerNotification::Event(WorkerQueueEvent::StepMessage(event)) => {
            assert_eq!(event.queue_name, "test_queue");
        }
        _ => panic!("Expected Event notification with StepMessage"),
    }

    match health_notification {
        WorkerNotification::Health(update) => {
            assert_eq!(update.worker_id, "worker-1");
            assert!(matches!(update.status, WorkerHealthStatus::Healthy));
        }
        _ => panic!("Expected Health notification"),
    }

    match config_notification {
        WorkerNotification::Configuration(update) => {
            assert_eq!(update.worker_id, "worker-1");
            assert!(matches!(
                update.update_type,
                ConfigUpdateType::ProcessingParametersUpdated
            ));
        }
        _ => panic!("Expected Configuration notification"),
    }
}

#[test]
fn test_worker_queue_event_methods() {
    // TAS-133: Use provider-agnostic MessageEvent
    let message_event = MessageEvent::new("test_queue", "test_namespace", "123");

    let step_event = WorkerQueueEvent::StepMessage(message_event.clone());

    // Test namespace method
    assert_eq!(step_event.namespace(), Some("test_namespace"));

    // Test queue_name method
    assert_eq!(step_event.queue_name(), "test_queue");

    // Test is_step_message method
    assert!(step_event.is_step_message());

    let health_event = WorkerQueueEvent::HealthCheck(message_event);
    assert!(!health_event.is_step_message());
}

#[test]
fn test_worker_notification_methods() {
    // TAS-133: Use provider-agnostic MessageEvent
    let message_event = MessageEvent::new("test_queue", "test_namespace", "123");

    let event_notification =
        WorkerNotification::Event(WorkerQueueEvent::StepMessage(message_event));

    // Test is_step_message method
    assert!(event_notification.is_step_message());

    // Test as_queue_event method
    assert!(event_notification.as_queue_event().is_some());

    let health_notification = WorkerNotification::Health(WorkerHealthUpdate {
        worker_id: "worker-1".to_string(),
        status: WorkerHealthStatus::Healthy,
        supported_namespaces: vec!["test_namespace".to_string()],
        last_activity: Some(chrono::Utc::now().to_rfc3339()),
        details: Some("All systems operational".to_string()),
    });

    assert!(!health_notification.is_step_message());
    assert!(health_notification.as_queue_event().is_none());
}
