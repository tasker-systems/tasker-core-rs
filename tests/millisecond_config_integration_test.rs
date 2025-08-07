//! Integration test for millisecond polling configuration
//!
//! This test validates that our millisecond refactor is working correctly
//! by loading configuration and ensuring timing values are properly converted.

use std::time::Duration;
use tasker_core::orchestration::config::ConfigurationManager;

#[tokio::test]
async fn test_millisecond_configuration_loading() {
    // Test that our base configuration loads with correct millisecond defaults
    let config_manager = ConfigurationManager::load_from_file("config/tasker-config.yaml")
        .await
        .expect("Should load base configuration");

    let system_config = config_manager.system_config();

    // Verify millisecond fields are set correctly
    assert_eq!(
        system_config.orchestration.cycle_interval_ms, 250,
        "Default cycle interval should be 250ms"
    );
    assert_eq!(
        system_config.orchestration.task_request_polling_interval_ms, 250,
        "Default polling interval should be 250ms"
    );
    assert_eq!(
        system_config.orchestration.heartbeat_interval_ms, 5000,
        "Default heartbeat interval should be 5000ms"
    );

    // Verify business logic fields remain in seconds
    assert_eq!(
        system_config
            .orchestration
            .task_request_visibility_timeout_seconds,
        300,
        "Visibility timeout should remain in seconds"
    );
    assert_eq!(
        system_config.orchestration.default_claim_timeout_seconds, 300,
        "Claim timeout should remain in seconds"
    );
}

#[tokio::test]
async fn test_orchestration_system_config_conversion() {
    // Test that OrchestrationConfig converts to OrchestrationSystemConfig correctly
    let config_manager = ConfigurationManager::load_from_file("config/tasker-config.yaml")
        .await
        .expect("Should load base configuration");

    let system_config = config_manager.system_config();
    let orchestration_system_config = system_config.orchestration.to_orchestration_system_config();

    // Verify polling interval is converted correctly
    assert_eq!(
        orchestration_system_config.task_request_polling_interval_ms, 250,
        "Polling interval should be 250ms"
    );

    // Verify orchestration loop config has correct Duration
    let cycle_interval = orchestration_system_config
        .orchestration_loop_config
        .cycle_interval;
    assert_eq!(
        cycle_interval,
        Duration::from_millis(250),
        "Cycle interval Duration should be 250ms"
    );

    // Verify heartbeat interval is converted correctly
    let heartbeat_interval = orchestration_system_config
        .orchestration_loop_config
        .task_claimer_config
        .heartbeat_interval;
    assert_eq!(
        heartbeat_interval,
        Duration::from_millis(5000),
        "Heartbeat interval Duration should be 5000ms"
    );
}

#[tokio::test]
async fn test_environment_specific_millisecond_values() {
    // Test development config (should be 500ms = 2x/sec)
    let dev_config_manager =
        ConfigurationManager::load_from_file("config/tasker-config-development.yaml")
            .await
            .expect("Should load development configuration");

    let dev_system_config = dev_config_manager.system_config();
    assert_eq!(
        dev_system_config.orchestration.cycle_interval_ms, 500,
        "Development cycle interval should be 500ms"
    );
    assert_eq!(
        dev_system_config
            .orchestration
            .task_request_polling_interval_ms,
        500,
        "Development polling interval should be 500ms"
    );

    // Test production config (should be 200ms = 5x/sec)
    let prod_config_manager =
        ConfigurationManager::load_from_file("config/tasker-config-production.yaml")
            .await
            .expect("Should load production configuration");

    let prod_system_config = prod_config_manager.system_config();
    assert_eq!(
        prod_system_config.orchestration.cycle_interval_ms, 200,
        "Production cycle interval should be 200ms"
    );
    assert_eq!(
        prod_system_config
            .orchestration
            .task_request_polling_interval_ms,
        200,
        "Production polling interval should be 200ms"
    );
    assert_eq!(
        prod_system_config.orchestration.heartbeat_interval_ms, 10000,
        "Production heartbeat interval should be 10000ms"
    );

    // Test test config (should be 100ms = 10x/sec)
    let test_config_manager =
        ConfigurationManager::load_from_file("config/tasker-config-test.yaml")
            .await
            .expect("Should load test configuration");

    let test_system_config = test_config_manager.system_config();
    assert_eq!(
        test_system_config.orchestration.cycle_interval_ms, 100,
        "Test cycle interval should be 100ms"
    );
    assert_eq!(
        test_system_config
            .orchestration
            .task_request_polling_interval_ms,
        100,
        "Test polling interval should be 100ms"
    );
    assert_eq!(
        test_system_config.orchestration.heartbeat_interval_ms, 2000,
        "Test heartbeat interval should be 2000ms"
    );
}

#[test]
fn test_orchestration_system_config_defaults() {
    // Test that our OrchestrationSystemConfig defaults match our millisecond refactor
    use tasker_core::orchestration::OrchestrationSystemConfig;

    let config = OrchestrationSystemConfig::default();

    // Verify default millisecond values
    assert_eq!(
        config.task_request_polling_interval_ms, 250,
        "Default polling interval should be 250ms"
    );

    // Verify business logic timeouts remain in seconds
    assert_eq!(
        config.task_request_visibility_timeout_seconds, 300,
        "Visibility timeout should be 300 seconds"
    );
}

#[test]
fn test_duration_conversions() {
    // Test that our Duration conversions work correctly
    use tasker_core::orchestration::config::OrchestrationConfig;

    let config = OrchestrationConfig {
        cycle_interval_ms: 123,
        task_request_polling_interval_ms: 456,
        heartbeat_interval_ms: 789,
        ..OrchestrationConfig::default()
    };

    let system_config = config.to_orchestration_system_config();

    // Verify Duration conversions
    assert_eq!(
        system_config.orchestration_loop_config.cycle_interval,
        Duration::from_millis(123)
    );
    assert_eq!(system_config.task_request_polling_interval_ms, 456);
    assert_eq!(
        system_config
            .orchestration_loop_config
            .task_claimer_config
            .heartbeat_interval,
        Duration::from_millis(789)
    );
}
