//! WorkflowCoordinator Integration Tests with Configuration and Events
//!
//! Tests the integration of ConfigurationManager and SystemEventsManager
//! with the WorkflowCoordinator.

use std::sync::Arc;
use tasker_core::orchestration::{
    system_events::{EventMetadata, SchemaField, StateMachineMappings, SystemEventsConfig},
    ConfigurationManager, SystemEventsManager, WorkflowCoordinatorConfig,
};

#[tokio::test]
async fn test_workflow_coordinator_configuration_integration() {
    // Test that WorkflowCoordinator uses ConfigurationManager for settings
    let config_manager = Arc::new(ConfigurationManager::new());
    let config = WorkflowCoordinatorConfig::from_config_manager(&config_manager);

    // Verify configuration values are pulled from system config
    let system_config = config_manager.system_config();

    // Test that timeout configuration is used
    assert_eq!(
        config.max_workflow_duration.as_secs(),
        system_config.execution.default_timeout_seconds
    );

    // Test that telemetry configuration is used
    assert_eq!(config.enable_metrics, system_config.telemetry.enabled);

    // Test that execution limits are used
    assert_eq!(
        config.max_steps_per_run,
        system_config.execution.max_concurrent_steps
    );

    // Test that backoff configuration is used
    assert_eq!(
        config.discovery_retry_delay.as_secs(),
        system_config.backoff.default_reenqueue_delay as u64
    );

    println!("✅ WorkflowCoordinator configuration integration working correctly!");
}

#[tokio::test]
async fn test_workflow_coordinator_default_values() {
    // Test that default configuration matches expected values
    let config_manager = ConfigurationManager::new();
    let config = WorkflowCoordinatorConfig::from_config_manager(&config_manager);

    // Verify default values are sensible
    assert_eq!(config.max_discovery_attempts, 3);
    assert_eq!(config.max_workflow_duration.as_secs(), 3600); // 1 hour
    assert!(!config.enable_metrics); // Telemetry disabled by default
    assert_eq!(config.max_steps_per_run, 1000); // From default config
    assert_eq!(config.discovery_retry_delay.as_secs(), 30); // From default config

    println!("✅ WorkflowCoordinator default configuration values are correct!");
}

#[test]
fn test_workflow_coordinator_config_builder() {
    // Test that we can build configuration from ConfigurationManager
    let config_manager = ConfigurationManager::new();
    let config = WorkflowCoordinatorConfig::from_config_manager(&config_manager);

    // Verify we can access all configuration values
    assert!(config.max_discovery_attempts > 0);
    assert!(config.discovery_retry_delay > std::time::Duration::from_secs(0));
    assert!(config.max_workflow_duration > std::time::Duration::from_secs(0));
    assert!(config.max_steps_per_run > 0);
    assert!(config.step_batch_size > 0);

    println!("✅ WorkflowCoordinator config builder working correctly!");
}

#[test]
fn test_workflow_coordinator_system_events_validation() {
    // Test that we can validate system events payloads
    let events_config = SystemEventsConfig {
        event_metadata: {
            let mut event_metadata = std::collections::HashMap::new();
            let mut task_events = std::collections::HashMap::new();

            // Add task start event metadata
            task_events.insert(
                "start_requested".to_string(),
                EventMetadata {
                    description: "Fired when a task processing begins".to_string(),
                    constant_ref: "TaskEvents::START_REQUESTED".to_string(),
                    payload_schema: {
                        let mut schema = std::collections::HashMap::new();
                        schema.insert(
                            "task_id".to_string(),
                            SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema.insert(
                            "task_name".to_string(),
                            SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema.insert(
                            "timestamp".to_string(),
                            SchemaField {
                                field_type: "Time".to_string(),
                                required: true,
                            },
                        );
                        schema
                    },
                    fired_by: vec!["WorkflowCoordinator".to_string()],
                },
            );

            event_metadata.insert("task".to_string(), task_events);
            event_metadata
        },
        state_machine_mappings: StateMachineMappings {
            task_transitions: vec![],
            step_transitions: vec![],
        },
    };

    let events_manager = SystemEventsManager::new(events_config);

    // Test task start event payload validation
    let task_start_payload = serde_json::json!({
        "task_id": "123",
        "task_name": "test_task",
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    let validation_result = events_manager.config().validate_event_payload(
        "task",
        "start_requested",
        &task_start_payload,
    );
    assert!(validation_result.is_ok());

    // Test missing required field
    let invalid_payload = serde_json::json!({
        "task_id": "123",
        // Missing task_name and timestamp
    });

    let validation_result =
        events_manager
            .config()
            .validate_event_payload("task", "start_requested", &invalid_payload);
    assert!(validation_result.is_err());

    println!("✅ WorkflowCoordinator system events validation working correctly!");
}

#[test]
fn test_workflow_coordinator_event_payload_creation() {
    // Test that SystemEventsManager can create proper event payloads
    let events_config = SystemEventsConfig {
        event_metadata: std::collections::HashMap::new(),
        state_machine_mappings: StateMachineMappings {
            task_transitions: vec![],
            step_transitions: vec![],
        },
    };

    let events_manager = SystemEventsManager::new(events_config);

    // Test task completed payload
    let task_completed_payload =
        events_manager.create_task_completed_payload(123, "test_task", 5, 5, Some(10.5));

    assert_eq!(task_completed_payload["task_id"], "123");
    assert_eq!(task_completed_payload["task_name"], "test_task");
    assert_eq!(task_completed_payload["total_steps"], 5);
    assert_eq!(task_completed_payload["completed_steps"], 5);
    assert_eq!(task_completed_payload["total_duration"], 10.5);
    assert!(task_completed_payload["timestamp"].is_string());

    // Test viable steps discovered payload
    let step_ids = vec![100, 200, 300];
    let viable_steps_payload =
        events_manager.create_viable_steps_discovered_payload(456, &step_ids, "parallel");

    assert_eq!(viable_steps_payload["task_id"], "456");
    assert_eq!(viable_steps_payload["step_count"], 3);
    assert_eq!(viable_steps_payload["processing_mode"], "parallel");
    assert!(viable_steps_payload["step_ids"].is_array());

    println!("✅ WorkflowCoordinator event payload creation working correctly!");
}
