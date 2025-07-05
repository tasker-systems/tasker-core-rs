//! Configuration Module Tests
//!
//! Tests for the TaskerConfig implementation and environment variable handling.

use tasker_core::TaskerConfig;

#[test]
fn config_loads_successfully() {
    let config = TaskerConfig::default();
    assert_eq!(config.max_concurrent_steps, 10);
    assert_eq!(config.retry_limit, 3);
}

#[test]
fn config_has_expected_defaults() {
    let config = TaskerConfig::default();

    assert_eq!(
        config.database_url,
        "postgresql://localhost/tasker_rust_development"
    );
    assert_eq!(config.max_concurrent_steps, 10);
    assert_eq!(config.retry_limit, 3);
    assert_eq!(config.backoff_base_ms, 1000);
    assert_eq!(config.backoff_max_ms, 60000);
    assert_eq!(config.event_batch_size, 100);
    assert!(config.telemetry_enabled);
    assert!(config.custom_settings.is_empty());
}

#[test]
fn config_from_env_with_defaults() {
    // Test that from_env() works when no environment variables are set
    let config = TaskerConfig::from_env().expect("from_env should succeed");

    // Should have same values as default
    let default_config = TaskerConfig::default();
    assert_eq!(
        config.max_concurrent_steps,
        default_config.max_concurrent_steps
    );
    assert_eq!(config.retry_limit, default_config.retry_limit);
}
