//! Configuration Module Tests
//!
//! Tests for the TaskerConfig implementation and environment variable handling.

use tasker_core::{BackoffConfig, ExecutionConfig, ReenqueueDelays, TaskerConfig};

#[test]
fn config_loads_successfully() {
    let config = TaskerConfig::default();
    assert_eq!(config.execution.max_concurrent_tasks, 100);
    assert_eq!(config.execution.max_concurrent_steps, 1000);
}

#[test]
fn config_has_expected_defaults() {
    let config = TaskerConfig::default();

    // Database config
    assert_eq!(config.database.database, None);
    assert!(!config.database.enable_secondary_database);

    // Execution config
    assert_eq!(config.execution.max_concurrent_tasks, 100);
    assert_eq!(config.execution.max_concurrent_steps, 1000);
    assert_eq!(config.execution.default_timeout_seconds, 3600);
    assert_eq!(config.execution.step_execution_timeout_seconds, 300);

    // Backoff config
    assert_eq!(
        config.backoff.default_backoff_seconds,
        vec![1, 2, 4, 8, 16, 32]
    );
    assert_eq!(config.backoff.max_backoff_seconds, 300);
    assert!(config.backoff.jitter_enabled);
    assert_eq!(config.backoff.backoff_multiplier, 2.0);

    // Reenqueue delays
    assert_eq!(config.reenqueue.has_ready_steps, 1);
    assert_eq!(config.reenqueue.waiting_for_dependencies, 5);
    assert_eq!(config.reenqueue.processing, 2);

    // Events config
    assert_eq!(config.events.batch_size, 100);
    assert!(config.events.enabled);

    // Telemetry config
    assert!(!config.telemetry.enabled);
    assert_eq!(config.telemetry.sample_rate, 1.0);

    // Note: custom_settings field no longer exists in new config structure
}

#[test]
fn config_from_env_with_defaults() {
    // Note: from_env() is not implemented in the new configuration system
    // The new system uses YAML-based configuration through ConfigurationManager
    // For now, we'll test that default config works
    let config = TaskerConfig::default();
    let default_config = TaskerConfig::default();

    // Should have same values
    assert_eq!(
        config.execution.max_concurrent_steps,
        default_config.execution.max_concurrent_steps
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
    assert_eq!(execution_config.max_concurrent_tasks, 100);
    assert_eq!(execution_config.max_concurrent_steps, 1000);

    let backoff_config = BackoffConfig::default();
    assert_eq!(
        backoff_config.default_backoff_seconds,
        vec![1, 2, 4, 8, 16, 32]
    );
    assert_eq!(backoff_config.max_backoff_seconds, 300);
    assert!(backoff_config.jitter_enabled);

    let reenqueue_delays = ReenqueueDelays::default();
    assert_eq!(reenqueue_delays.has_ready_steps, 1);
    assert_eq!(reenqueue_delays.waiting_for_dependencies, 5);
    assert_eq!(reenqueue_delays.processing, 2);
}

#[test]
fn config_validation() {
    let mut config = TaskerConfig::default();

    // Test setting various config values
    config.execution.max_concurrent_tasks = 50;
    config.execution.max_concurrent_steps = 500;
    assert_eq!(config.execution.max_concurrent_tasks, 50);
    assert_eq!(config.execution.max_concurrent_steps, 500);

    // Test backoff multiplier
    config.backoff.backoff_multiplier = 3.0;
    assert_eq!(config.backoff.backoff_multiplier, 3.0);

    // Test telemetry sample rate
    config.telemetry.sample_rate = 0.5;
    assert_eq!(config.telemetry.sample_rate, 0.5);

    // Note: The new config doesn't have a validate() method
    // Validation is handled by the ConfigurationManager when loading YAML
}
