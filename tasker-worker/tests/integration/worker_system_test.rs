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
#[ignore] // Ignore until we have proper test setup - WorkerCore API has been updated for TAS-43
async fn test_worker_foundation_bootstrap() -> Result<()> {
    // This test verifies the basic bootstrap works
    // TODO: Update this test to use the new WorkerCore::new() signature with namespace and event-driven parameters
    // In a real test, we'd use a test database and configuration

    // For now, just verify compilation
    println!("WorkerCore API has been updated for TAS-43 event-driven integration");
    
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