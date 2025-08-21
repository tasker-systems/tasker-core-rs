//! Integration test for millisecond polling configuration
//!
//! This test validates that our millisecond refactor is working correctly
//! by loading configuration and ensuring timing values are properly converted.

use std::time::Duration;
use tasker_shared::config::ConfigManager;

#[tokio::test]
async fn test_millisecond_configuration_loading() {
    // Test that our base configuration loads with correct millisecond defaults
    let config_manager = ConfigManager::load().unwrap();

    let config = config_manager.config();

    // Verify millisecond fields are set correctly (values depend on environment)
    // In test environment: cycle_interval_ms=50, heartbeat_interval_ms=1000
    // In base config: cycle_interval_ms=250, heartbeat_interval_ms=5000
    let expected_cycle_interval = match config.execution.environment.as_str() {
        "test" => 50,
        _ => 200,
    };
    let expected_heartbeat_interval = match config.execution.environment.as_str() {
        "test" => 1000,
        _ => 3000,
    };

    assert_eq!(
        config.orchestration.cycle_interval_ms, expected_cycle_interval,
        "Cycle interval should match environment-specific config"
    );
    assert_eq!(
        config.orchestration.task_request_polling_interval_ms, expected_cycle_interval,
        "Polling interval should match environment-specific config"
    );
    assert_eq!(
        config.orchestration.heartbeat_interval_ms, expected_heartbeat_interval,
        "Heartbeat interval should match environment-specific config"
    );

    // Verify business logic fields remain in seconds (also environment-dependent)
    let expected_visibility_timeout = match config.execution.environment.as_str() {
        "test" => 10,
        _ => 60,
    };
    let expected_claim_timeout = match config.execution.environment.as_str() {
        "test" => 30,
        _ => 60,
    };

    assert_eq!(
        config.orchestration.task_request_visibility_timeout_seconds, expected_visibility_timeout,
        "Visibility timeout should match environment-specific config"
    );
    assert_eq!(
        config.orchestration.default_claim_timeout_seconds, expected_claim_timeout,
        "Claim timeout should match environment-specific config"
    );
}

#[tokio::test]
async fn test_orchestration_system_config_conversion() {
    // Test that OrchestrationConfig converts to OrchestrationSystemConfig correctly
    let config_manager = ConfigManager::load().unwrap();

    let config = config_manager.config();
    let orchestration_system_config = config.orchestration.to_orchestration_system_config();

    // Verify polling interval is converted correctly (environment-dependent)
    let expected_cycle_interval = match config.execution.environment.as_str() {
        "test" => 50,
        _ => 200,
    };
    let expected_heartbeat_interval = match config.execution.environment.as_str() {
        "test" => 1000,
        _ => 3000,
    };

    assert_eq!(
        orchestration_system_config.task_request_polling_interval_ms, expected_cycle_interval,
        "Polling interval should match environment config"
    );

    // Verify orchestration loop config has correct Duration
    let cycle_interval = orchestration_system_config
        .orchestration_loop_config
        .cycle_interval;
    assert_eq!(
        cycle_interval,
        Duration::from_millis(expected_cycle_interval),
        "Cycle interval Duration should match environment config"
    );

    // Verify heartbeat interval is converted correctly
    let heartbeat_interval = orchestration_system_config
        .orchestration_loop_config
        .task_claimer_config
        .heartbeat_interval;
    assert_eq!(
        heartbeat_interval,
        Duration::from_millis(expected_heartbeat_interval),
        "Heartbeat interval Duration should match environment config"
    );
}

#[tokio::test]
async fn test_environment_specific_millisecond_values() {
    // Skip test if component-based config directory doesn't exist
    // Our component-based configuration approach uses config/tasker/ directory structure
    if !std::path::Path::new("config/tasker").exists() {
        eprintln!("Skipping test: config/tasker/ component config directory not found");
        eprintln!("Note: Using component-based configuration at config/tasker/");
        return;
    }

    // Test development config - should use base defaults
    let config_manager = ConfigManager::load_from_env("development").unwrap();
    let config = config_manager.config();

    // Use environment-specific defaults since this is component-based config
    assert_eq!(
        config.orchestration.cycle_interval_ms, 200,
        "Development config cycle interval should use base default 200ms"
    );
    assert_eq!(
        config.orchestration.task_request_polling_interval_ms, 200,
        "Development config polling interval should use base default 200ms"
    );
    assert_eq!(
        config.orchestration.heartbeat_interval_ms, 3000,
        "Development config heartbeat interval should use base default 3000ms"
    );
}

#[test]
fn test_orchestration_system_config_defaults() {
    // Test that our OrchestrationSystemConfig defaults match our millisecond refactor
    use tasker_shared::config::orchestration::OrchestrationSystemConfig;

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
    use tasker_shared::config::OrchestrationConfig;

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
