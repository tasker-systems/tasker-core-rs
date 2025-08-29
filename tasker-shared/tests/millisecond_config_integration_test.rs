//! Integration test for millisecond polling configuration
//!
//! This test validates that our millisecond refactor is working correctly
//! by loading configuration and ensuring timing values are properly converted.

use tasker_shared::config::ConfigManager;

#[tokio::test]
async fn test_millisecond_configuration_loading() {
    // Test that our base configuration loads with correct millisecond defaults
    let config_manager = ConfigManager::load().unwrap();

    let config = config_manager.config();

    let expected_heartbeat_interval = match config.execution.environment.as_str() {
        "test" => 1000,
        _ => 3000,
    };
    assert_eq!(
        config.orchestration.heartbeat_interval_ms, expected_heartbeat_interval,
        "Heartbeat interval should match environment-specific config"
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
    assert_eq!(
        config.orchestration.heartbeat_interval_ms, 3000,
        "Development config heartbeat interval should use base default 3000ms"
    );
}
