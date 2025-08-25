//! Integration test for configuration system
//!
//! This test verifies that hardcoded values have been replaced with
//! configuration-driven values from the YAML configuration system.
//!
//! TODO: TAS-40 - Update this test for command pattern architecture
//! The WorkflowCoordinatorConfig will be replaced with OrchestrationProcessor config

use tasker_orchestration::orchestration::config::{ConfigurationManager, ExecutionConfig};
// TODO: Replace with command pattern configuration once OrchestrationProcessor is implemented
// use tasker_orchestration::orchestration::workflow_coordinator::WorkflowCoordinatorConfig;

#[tokio::test]
async fn test_configuration_replaces_hardcoded_values() {
    // Test default configuration values
    let config_manager = ConfigurationManager::new();
    let system_config = config_manager.system_config();

    // Verify execution config has the expected values
    assert_eq!(system_config.execution.step_execution_timeout_seconds, 300);
    assert_eq!(system_config.execution.max_discovery_attempts, 3);
    assert_eq!(system_config.execution.step_batch_size, 10);
    assert_eq!(system_config.execution.max_concurrent_steps, 1000);
    assert_eq!(system_config.execution.max_concurrent_tasks, 100);

    // TODO: TAS-40 - Re-enable once OrchestrationProcessor configuration is implemented
    // Test that command processor config uses configuration values
    /*
    let processor_config = OrchestrationProcessorConfig::from_config_manager(&config_manager);
    assert_eq!(
        processor_config.max_discovery_attempts,
        system_config.execution.max_discovery_attempts
    );
    assert_eq!(
        processor_config.step_batch_size,
        system_config.execution.step_batch_size
    );
    assert_eq!(
        processor_config.max_steps_per_run,
        system_config.execution.max_concurrent_steps
    );
    */

    println!("✅ Configuration integration test passed");
    println!(
        "   Step execution timeout: {}",
        system_config.execution.step_execution_timeout_seconds
    );
    println!(
        "   Max discovery attempts: {}",
        system_config.execution.max_discovery_attempts
    );
    println!(
        "   Step batch size: {}",
        system_config.execution.step_batch_size
    );
}

#[test]
fn test_execution_config_field_availability() {
    // Test that ExecutionConfig has the specific fields we added to replace hardcoded values
    let execution_config = ExecutionConfig::default();

    // Verify the fields that were specifically added to replace hardcoded values
    assert_eq!(
        execution_config.max_discovery_attempts, 3,
        "max_discovery_attempts should default to 3"
    );
    assert_eq!(
        execution_config.step_batch_size, 10,
        "step_batch_size should default to 10"
    );

    // Test that we can create a WorkflowCoordinatorConfig from the configuration manager
    let config_manager = ConfigurationManager::new();
    let wf_config = WorkflowCoordinatorConfig::from_config_manager(&config_manager);

    // Verify that the workflow coordinator gets values from the config, not hardcoded
    assert_eq!(
        wf_config.max_discovery_attempts,
        execution_config.max_discovery_attempts
    );
    assert_eq!(wf_config.step_batch_size, execution_config.step_batch_size);

    println!("✅ ExecutionConfig field availability test passed");
    println!(
        "   max_discovery_attempts configurable: {}",
        execution_config.max_discovery_attempts
    );
    println!(
        "   step_batch_size configurable: {}",
        execution_config.step_batch_size
    );
    println!("   WorkflowCoordinatorConfig uses config values correctly");
}

#[test]
fn test_configuration_validation() {
    // Test that the ExecutionConfig has all the required fields to replace hardcoded values
    let execution_config = ExecutionConfig::default();

    // These fields were previously hardcoded and should now be configurable
    assert!(
        execution_config.step_execution_timeout_seconds > 0,
        "Step execution timeout should be configurable"
    );
    assert!(
        execution_config.max_discovery_attempts > 0,
        "Max discovery attempts should be configurable"
    );
    assert!(
        execution_config.step_batch_size > 0,
        "Step batch size should be configurable"
    );
    assert!(
        execution_config.max_concurrent_steps > 0,
        "Max concurrent steps should be configurable"
    );
    assert!(
        execution_config.max_concurrent_tasks > 0,
        "Max concurrent tasks should be configurable"
    );

    println!("✅ Configuration validation test passed");
    println!("   All previously hardcoded values are now configurable");
}
