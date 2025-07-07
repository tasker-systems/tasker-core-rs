//! Configuration Module Tests
//!
//! Tests for the TaskerConfig implementation and environment variable handling.

use tasker_core::{BackoffConfig, ExecutionConfig, ReenqueueDelays, TaskerConfig};

#[test]
fn config_loads_successfully() {
    let config = TaskerConfig::default();
    assert_eq!(config.execution.max_concurrent_steps_limit, 12);
    assert_eq!(config.execution.min_concurrent_steps, 3);
}

#[test]
fn config_has_expected_defaults() {
    let config = TaskerConfig::default();

    // Database config
    assert_eq!(
        config.database.url,
        "postgresql://localhost/tasker_rust_development"
    );
    assert_eq!(config.database.max_connections, 10);
    assert_eq!(config.database.connection_timeout_seconds, 30);

    // Execution config
    assert_eq!(config.execution.min_concurrent_steps, 3);
    assert_eq!(config.execution.max_concurrent_steps_limit, 12);
    assert_eq!(config.execution.concurrency_cache_duration, 30);
    assert_eq!(config.execution.batch_timeout_base_seconds, 30);
    assert_eq!(config.execution.max_batch_timeout_seconds, 120);

    // Backoff config
    assert_eq!(
        config.backoff.default_backoff_seconds,
        vec![1, 2, 4, 8, 16, 32]
    );
    assert_eq!(config.backoff.max_backoff_seconds, 300);
    assert!(config.backoff.jitter_enabled);
    assert_eq!(config.backoff.backoff_multiplier, 2.0);

    // Reenqueue delays
    assert_eq!(config.reenqueue.has_ready_steps, 0);
    assert_eq!(config.reenqueue.waiting_for_dependencies, 45);
    assert_eq!(config.reenqueue.processing, 10);

    // Events config
    assert_eq!(config.events.batch_size, 100);
    assert!(config.events.enabled);

    // Telemetry config
    assert!(config.telemetry.enabled);
    assert_eq!(config.telemetry.sampling_rate, 1.0);

    assert!(config.custom_settings.is_empty());
}

#[test]
fn config_from_env_with_defaults() {
    // Test that from_env() works when no environment variables are set
    let config = TaskerConfig::from_env().expect("from_env should succeed");

    // Should have same values as default
    let default_config = TaskerConfig::default();
    assert_eq!(
        config.execution.max_concurrent_steps_limit,
        default_config.execution.max_concurrent_steps_limit
    );
    assert_eq!(
        config.backoff.max_backoff_seconds,
        default_config.backoff.max_backoff_seconds
    );
}

#[test]
fn config_component_defaults() {
    // Test individual config components
    let execution_config = ExecutionConfig::default();
    assert_eq!(execution_config.min_concurrent_steps, 3);
    assert_eq!(execution_config.max_concurrent_steps_limit, 12);

    let backoff_config = BackoffConfig::default();
    assert_eq!(
        backoff_config.default_backoff_seconds,
        vec![1, 2, 4, 8, 16, 32]
    );
    assert_eq!(backoff_config.max_backoff_seconds, 300);
    assert!(backoff_config.jitter_enabled);

    let reenqueue_delays = ReenqueueDelays::default();
    assert_eq!(reenqueue_delays.has_ready_steps, 0);
    assert_eq!(reenqueue_delays.waiting_for_dependencies, 45);
    assert_eq!(reenqueue_delays.processing, 10);
}

#[test]
fn config_validation() {
    let mut config = TaskerConfig::default();

    // Valid config should pass validation
    assert!(config.validate().is_ok());

    // Invalid: min > max concurrent steps
    config.execution.min_concurrent_steps = 20;
    config.execution.max_concurrent_steps_limit = 10;
    assert!(config.validate().is_err());

    // Reset and test backoff multiplier
    config.execution.min_concurrent_steps = 3;
    config.execution.max_concurrent_steps_limit = 12;
    config.backoff.backoff_multiplier = 0.5; // Invalid: must be > 1.0
    assert!(config.validate().is_err());

    // Reset and test telemetry sampling rate
    config.backoff.backoff_multiplier = 2.0;
    config.telemetry.sampling_rate = 1.5; // Invalid: must be 0.0-1.0
    assert!(config.validate().is_err());
}
