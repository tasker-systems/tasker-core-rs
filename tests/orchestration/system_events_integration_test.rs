//! System Events Integration Tests
//!
//! Tests for the complete system events integration with BaseStepHandler.

use std::collections::HashMap;
use std::sync::Arc;
use tasker_core::orchestration::{
    constants, BaseStepHandler, ConfigurationManager, OrchestrationResult, StepExecutionContext,
    StepHandler, SystemEventsConfig, SystemEventsManager,
};

/// Test step handler for system events integration
struct SystemEventsTestHandler {
    should_succeed: bool,
    execution_duration_ms: u64,
}

#[async_trait::async_trait]
impl StepHandler for SystemEventsTestHandler {
    async fn process(
        &self,
        context: &StepExecutionContext,
    ) -> OrchestrationResult<serde_json::Value> {
        // Simulate processing delay
        if self.execution_duration_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.execution_duration_ms)).await;
        }

        if self.should_succeed {
            Ok(serde_json::json!({
                "processing_result": "success",
                "processed_data": context.input_data.clone(),
                "step_name": context.step_name
            }))
        } else {
            Err(
                tasker_core::orchestration::OrchestrationError::StepExecutionFailed {
                    step_id: context.step_id,
                    task_id: context.task_id,
                    reason: "Test failure for system events".to_string(),
                    error_code: Some("SYSTEM_EVENTS_TEST_ERROR".to_string()),
                    retry_after: Some(std::time::Duration::from_secs(1)),
                },
            )
        }
    }
}

#[tokio::test]
async fn test_system_events_configuration_loading() {
    // Test loading system events configuration from file
    let config_path = "config/system_events.yaml";

    // Check if file exists (skip test if not)
    if !std::path::Path::new(config_path).exists() {
        eprintln!("Skipping test: {config_path} not found");
        return;
    }

    let events_config = SystemEventsConfig::load_from_file(config_path).await;

    match events_config {
        Ok(config) => {
            // Test event metadata access
            let step_completed_metadata = config.get_event_metadata("step", "completed");
            assert!(step_completed_metadata.is_ok());

            let metadata = step_completed_metadata.unwrap();
            assert_eq!(
                metadata.description,
                "Fired when a step completes successfully"
            );
            assert_eq!(metadata.constant_ref, "StepEvents::COMPLETED");
            assert!(metadata.payload_schema.contains_key("task_id"));
            assert!(metadata.payload_schema.contains_key("step_id"));
            assert!(metadata.payload_schema.contains_key("execution_duration"));

            // Test state transitions
            let step_transitions = config.get_step_transitions();
            assert!(!step_transitions.is_empty());

            // Find a specific transition
            let pending_to_progress = step_transitions.iter().find(|t| {
                t.from_state == Some("pending".to_string()) && t.to_state == "in_progress"
            });
            assert!(pending_to_progress.is_some());

            println!("✅ System events configuration loaded and validated successfully!");
        }
        Err(e) => {
            eprintln!("❌ Failed to load system events configuration: {e}");
            panic!("System events configuration loading failed");
        }
    }
}

#[tokio::test]
async fn test_system_events_manager_integration() {
    // Test creating and using the SystemEventsManager
    let events_config = SystemEventsConfig {
        event_metadata: {
            let mut event_metadata = HashMap::new();
            let mut step_events = HashMap::new();

            step_events.insert(
                "completed".to_string(),
                tasker_core::orchestration::EventMetadata {
                    description: "Step completed successfully".to_string(),
                    constant_ref: "StepEvents::COMPLETED".to_string(),
                    payload_schema: {
                        let mut schema = HashMap::new();
                        schema.insert(
                            "task_id".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema.insert(
                            "step_id".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema.insert(
                            "execution_duration".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "Float".to_string(),
                                required: true,
                            },
                        );
                        schema
                    },
                    fired_by: vec!["BaseStepHandler".to_string()],
                },
            );

            event_metadata.insert("step".to_string(), step_events);
            event_metadata
        },
        state_machine_mappings: tasker_core::orchestration::system_events::StateMachineMappings {
            task_transitions: vec![],
            step_transitions: vec![],
        },
    };

    let events_manager = SystemEventsManager::new(events_config);

    // Test event payload creation
    let completed_payload =
        events_manager.create_step_completed_payload(123, 456, "test_step", 1.5, 2);

    assert_eq!(completed_payload["task_id"], "123");
    assert_eq!(completed_payload["step_id"], "456");
    assert_eq!(completed_payload["step_name"], "test_step");
    assert_eq!(completed_payload["execution_duration"], 1.5);
    assert_eq!(completed_payload["attempt_number"], 2);
    assert!(completed_payload["timestamp"].is_string());

    // Test payload validation
    let validation_result =
        events_manager
            .config()
            .validate_event_payload("step", "completed", &completed_payload);
    assert!(validation_result.is_ok());

    println!("✅ System events manager integration working correctly!");
}

#[tokio::test]
async fn test_step_handler_with_system_events() {
    // Test BaseStepHandler with SystemEventsManager integration
    let config_manager = Arc::new(ConfigurationManager::new());

    let step_template = tasker_core::orchestration::StepTemplate {
        name: "system_events_integration_step".to_string(),
        description: Some("System events integration test step".to_string()),
        handler_class: "SystemEventsTestHandler".to_string(),
        handler_config: Some({
            let mut config = HashMap::new();
            config.insert("test_mode".to_string(), serde_json::json!(true));
            config
        }),
        depends_on_step: None,
        depends_on_steps: None,
        default_retryable: Some(true),
        default_retry_limit: Some(3),
        timeout_seconds: Some(30),
        retry_backoff: None,
    };

    let mut step_handler =
        BaseStepHandler::with_config_manager(step_template, Arc::clone(&config_manager));

    // Create system events manager
    let events_config = SystemEventsConfig {
        event_metadata: {
            let mut event_metadata = HashMap::new();
            let mut step_events = HashMap::new();

            // Add before_handle event metadata
            step_events.insert(
                "before_handle".to_string(),
                tasker_core::orchestration::EventMetadata {
                    description: "Fired just before a step handler is called".to_string(),
                    constant_ref: "StepEvents::BEFORE_HANDLE".to_string(),
                    payload_schema: {
                        let mut schema = HashMap::new();
                        schema.insert(
                            "task_id".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema.insert(
                            "step_id".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema.insert(
                            "step_name".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema
                    },
                    fired_by: vec!["BaseStepHandler".to_string()],
                },
            );

            // Add completed event metadata
            step_events.insert(
                "completed".to_string(),
                tasker_core::orchestration::EventMetadata {
                    description: "Fired when a step completes successfully".to_string(),
                    constant_ref: "StepEvents::COMPLETED".to_string(),
                    payload_schema: {
                        let mut schema = HashMap::new();
                        schema.insert(
                            "task_id".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema.insert(
                            "step_id".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "String".to_string(),
                                required: true,
                            },
                        );
                        schema.insert(
                            "execution_duration".to_string(),
                            tasker_core::orchestration::system_events::SchemaField {
                                field_type: "Float".to_string(),
                                required: true,
                            },
                        );
                        schema
                    },
                    fired_by: vec!["BaseStepHandler".to_string()],
                },
            );

            event_metadata.insert("step".to_string(), step_events);
            event_metadata
        },
        state_machine_mappings: tasker_core::orchestration::system_events::StateMachineMappings {
            task_transitions: vec![],
            step_transitions: vec![],
        },
    };

    let events_manager = Arc::new(SystemEventsManager::new(events_config));
    step_handler.set_events_manager(Arc::clone(&events_manager));

    // Set custom handler implementation
    let custom_handler = SystemEventsTestHandler {
        should_succeed: true,
        execution_duration_ms: 100,
    };
    step_handler.set_custom_handler(Box::new(custom_handler));

    // Create execution context
    let context = StepExecutionContext {
        step_id: 789,
        task_id: 123,
        step_name: "system_events_integration_step".to_string(),
        input_data: serde_json::json!({
            "test_data": "system_events_integration",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }),
        previous_results: None,
        step_config: HashMap::new(),
        attempt_number: 1,
        max_retry_attempts: 3,
        timeout_seconds: 30,
        is_retryable: true,
        environment: "test".to_string(),
        metadata: HashMap::new(),
    };

    // Execute step
    let result = step_handler.execute_step(context).await.unwrap();

    // Verify successful execution
    assert!(result.success);
    assert!(result.output_data.is_some());
    assert!(result.error.is_none());

    let output = result.output_data.unwrap();
    assert_eq!(output["processing_result"], "success");
    assert_eq!(output["step_name"], "system_events_integration_step");

    println!("✅ Step handler with system events integration working correctly!");
}

#[test]
fn test_event_constants() {
    // Test that event constants are properly defined
    assert_eq!(constants::task_events::COMPLETED, "task.completed");
    assert_eq!(constants::task_events::FAILED, "task.failed");
    assert_eq!(
        constants::task_events::START_REQUESTED,
        "task.start_requested"
    );

    assert_eq!(constants::step_events::BEFORE_HANDLE, "step.before_handle");
    assert_eq!(constants::step_events::COMPLETED, "step.completed");
    assert_eq!(constants::step_events::FAILED, "step.failed");
    assert_eq!(
        constants::step_events::EXECUTION_REQUESTED,
        "step.execution_requested"
    );

    assert_eq!(
        constants::workflow_events::VIABLE_STEPS_DISCOVERED,
        "workflow.viable_steps_discovered"
    );

    assert_eq!(
        constants::registry_events::HANDLER_REGISTERED,
        "handler.registered"
    );
    assert_eq!(
        constants::registry_events::HANDLER_VALIDATION_FAILED,
        "handler.validation_failed"
    );

    println!("✅ Event constants properly defined and accessible!");
}
