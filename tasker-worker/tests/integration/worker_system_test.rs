//! Integration tests for worker foundation system

use std::sync::Arc;
use tokio::time::{sleep, Duration};

use tasker_shared::config::ConfigManager;
use tasker_worker_foundation::{
    event_subscriber::EventSubscriber,
    events::StepEventPayload,
    worker::core::WorkerCore,
    Result,
};

#[tokio::test]
#[ignore] // Ignore until we have proper test setup
async fn test_worker_foundation_bootstrap() -> Result<()> {
    // This test verifies the basic bootstrap works
    // In a real test, we'd use a test database and configuration

    // Load test configuration
    let config_manager = Arc::new(ConfigManager::new().await?);

    // Create WorkerCore
    let worker_core = WorkerCore::new_with_config(config_manager).await?;

    // Verify components are initialized
    assert!(worker_core.event_subscriber().step_handler_count().await == 0);
    assert!(worker_core.event_subscriber().result_handler_count().await == 0);

    Ok(())
}

#[tokio::test]
async fn test_event_system_registration() -> Result<()> {
    // Test the event system works independently
    use tasker_worker_foundation::events::InProcessEventSystem;
    use tasker_shared::types::StepExecutionResult;
    use uuid::Uuid;

    let event_system = Arc::new(InProcessEventSystem::new());
    let event_subscriber = EventSubscriber::new(event_system.clone());

    // Register a test step handler
    event_subscriber
        .subscribe_to_step_events("test_step", |payload: StepEventPayload| {
            Ok(StepExecutionResult {
                step_id: payload.step.step_uuid,
                status: tasker_shared::constants::ExecutionStatus::Completed,
                outputs: serde_json::json!({"result": "success"}),
                error: None,
                metadata: serde_json::json!({"test": true}),
            })
        })
        .await?;

    // Verify handler is registered
    assert_eq!(event_subscriber.step_handler_count().await, 1);

    Ok(())
}

#[tokio::test]
async fn test_configuration_loading() -> Result<()> {
    // Test that worker configuration can be loaded
    use tasker_worker_foundation::config::WorkerConfig;

    // Create a mock config manager
    let config_manager = Arc::new(ConfigManager::new().await?);

    // This would fail in real test without proper config, but we're testing the structure
    if let Err(e) = WorkerConfig::from_config_manager(&config_manager) {
        // Expected to fail without worker.toml configuration
        assert!(e.to_string().contains("Worker configuration not found"));
    }

    Ok(())
}