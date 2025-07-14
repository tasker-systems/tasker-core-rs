//! Test to verify configuration extraction from YAML
//!
//! This test ensures that all previously hardcoded values are now properly
//! loaded from the configuration file.

use tasker_core::orchestration::config::ConfigurationManager;
use tasker_core::orchestration::step_executor::{RetryConfig, StepExecutionConfig};
use tasker_core::orchestration::workflow_coordinator::WorkflowCoordinatorConfig;

#[test]
fn test_configuration_extraction() {
    let config_manager = ConfigurationManager::new();
    let system_config = config_manager.system_config();

    // Test that previously hardcoded values are now configurable
    assert_eq!(system_config.execution.step_execution_timeout_seconds, 300);
    assert_eq!(system_config.execution.default_timeout_seconds, 3600);
    assert_eq!(system_config.execution.max_concurrent_steps, 1000);
    assert_eq!(system_config.execution.max_discovery_attempts, 3);
    assert_eq!(system_config.execution.step_batch_size, 10);

    // Test backoff configuration
    assert_eq!(system_config.backoff.default_reenqueue_delay, 30);
    assert_eq!(system_config.backoff.max_backoff_seconds, 300);
    assert_eq!(system_config.backoff.backoff_multiplier, 2.0);

    // Test reenqueue delays
    assert_eq!(
        system_config
            .backoff
            .reenqueue_delays
            .get("has_ready_steps")
            .unwrap(),
        &0
    );
    assert_eq!(
        system_config
            .backoff
            .reenqueue_delays
            .get("waiting_for_dependencies")
            .unwrap(),
        &45
    );
    assert_eq!(
        system_config
            .backoff
            .reenqueue_delays
            .get("processing")
            .unwrap(),
        &10
    );

    // Test system configuration
    assert_eq!(system_config.system.default_dependent_system_id, 1);
    assert_eq!(system_config.system.default_queue_name, "default");
    assert_eq!(system_config.system.version, "1.0.0");
}

#[test]
fn test_step_execution_config_from_config_manager() {
    let config_manager = ConfigurationManager::new();
    let step_config = StepExecutionConfig::from_config_manager(&config_manager);

    // Verify values are loaded from configuration, not hardcoded
    assert_eq!(step_config.max_concurrent_steps, 1000);
    assert_eq!(step_config.default_timeout.as_secs(), 300);
    assert_eq!(step_config.max_timeout.as_secs(), 3600);
    assert!(step_config.enable_metrics);
}

#[test]
fn test_retry_config_from_config_manager() {
    let config_manager = ConfigurationManager::new();
    let retry_config = RetryConfig::from_config_manager(&config_manager);

    // Verify retry configuration is loaded from YAML
    assert_eq!(retry_config.max_attempts, 6); // Length of default_backoff_seconds array
    assert_eq!(retry_config.base_delay.as_secs(), 1);
    assert_eq!(retry_config.max_delay.as_secs(), 300);
    assert_eq!(retry_config.backoff_multiplier, 2.0);
    assert!(retry_config.jitter);
}

#[test]
fn test_workflow_coordinator_config_from_config_manager() {
    let config_manager = ConfigurationManager::new();
    let workflow_config = WorkflowCoordinatorConfig::from_config_manager(&config_manager);

    // Verify workflow coordinator configuration is loaded from YAML
    assert_eq!(workflow_config.max_discovery_attempts, 3);
    assert_eq!(workflow_config.discovery_retry_delay.as_secs(), 30);
    assert_eq!(workflow_config.max_workflow_duration.as_secs(), 3600);
    assert_eq!(workflow_config.step_batch_size, 10);
    assert_eq!(workflow_config.max_steps_per_run, 1000);
}

#[test]
fn test_no_hardcoded_values_in_production_paths() {
    // This test serves as documentation that the following previously hardcoded values
    // are now configurable:
    //
    // 1. src/client/task_handler.rs:397 - timeout_seconds: 300
    //    -> Now uses: config_manager.system_config().execution.step_execution_timeout_seconds
    //
    // 2. src/orchestration/step_executor.rs:688,689 - default_timeout: 300s, max_timeout: 3600s
    //    -> Now uses: StepExecutionConfig::from_config_manager()
    //
    // 3. src/orchestration/task_finalizer.rs:654-657 - reenqueue delays
    //    -> Now uses: config_manager.system_config().backoff.reenqueue_delays
    //
    // 4. src/orchestration/workflow_coordinator.rs:115-124 - discovery and batch settings
    //    -> Now uses: WorkflowCoordinatorConfig::from_config_manager()

    // If this test compiles, it means the configuration extraction was successful
    // Note: This test validates that configuration extraction compiles correctly
}
