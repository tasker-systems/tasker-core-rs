//! Test for unified event systems configuration
//!
//! This test validates that the new unified event systems configuration
//! can be loaded successfully and provides the expected structure.

use std::path::PathBuf;
use tasker_shared::config::EventSystemsConfig;

/// Test loading and parsing the unified event systems TOML configuration
#[test]
fn test_load_unified_event_systems_toml() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");

    // Test loading the TOML file directly
    let toml_content =
        std::fs::read_to_string(&config_path).expect("Should be able to read event_systems.toml");

    assert!(!toml_content.is_empty(), "TOML file should not be empty");

    // Test parsing the TOML into our unified structure
    let event_systems_config: EventSystemsConfig = toml::from_str(&toml_content)
        .expect("Should be able to parse TOML into EventSystemsConfig");

    // Basic structure validation
    assert!(!event_systems_config.orchestration.system_id.is_empty());
    assert!(!event_systems_config.task_readiness.system_id.is_empty());
    assert!(!event_systems_config.worker.system_id.is_empty());
}

/// Test orchestration event system configuration
#[test]
fn test_orchestration_event_system_config() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");
    let toml_content = std::fs::read_to_string(&config_path).unwrap();
    let config: EventSystemsConfig = toml::from_str(&toml_content).unwrap();

    let orchestration = &config.orchestration;

    // Validate basic properties
    assert_eq!(orchestration.system_id, "orchestration-event-system");
    assert!(!orchestration.namespaces.is_empty());
    assert!(orchestration.timing.health_check_interval_seconds > 0);
    assert!(orchestration.processing.max_concurrent_operations > 0);
    assert!(orchestration.processing.batch_size > 0);

    // Validate metadata
    assert!(orchestration.metadata.queues_populated_at_runtime);
}

/// Test task readiness event system configuration
#[test]
fn test_task_readiness_event_system_config() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");
    let toml_content = std::fs::read_to_string(&config_path).unwrap();
    let config: EventSystemsConfig = toml::from_str(&toml_content).unwrap();

    let task_readiness = &config.task_readiness;

    // Validate basic properties
    assert_eq!(task_readiness.system_id, "task-readiness-event-system");
    assert!(!task_readiness.namespaces.is_empty());
    assert!(task_readiness.timing.health_check_interval_seconds > 0);
    assert!(task_readiness.processing.max_concurrent_operations > 0);

    // Validate system-specific metadata
    assert!(
        task_readiness
            .metadata
            .enhanced_settings
            .rollback_threshold_percent
            > 0.0
    );
    assert!(!task_readiness
        .metadata
        .notification
        .global_channels
        .is_empty());
    assert_eq!(
        task_readiness.metadata.notification.max_payload_size_bytes,
        8000
    );
}

/// Test worker event system configuration
#[test]
fn test_worker_event_system_config() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");
    let toml_content = std::fs::read_to_string(&config_path).unwrap();
    let config: EventSystemsConfig = toml::from_str(&toml_content).unwrap();

    let worker = &config.worker;

    // Validate basic properties
    assert_eq!(worker.system_id, "worker-event-system");
    assert!(!worker.namespaces.is_empty());
    assert!(worker.timing.health_check_interval_seconds > 0);
    assert!(worker.processing.max_concurrent_operations > 0);

    // Validate worker-specific metadata
    assert!(worker.metadata.in_process_events.ffi_integration_enabled);
    assert!(worker.metadata.in_process_events.broadcast_buffer_size > 0);
    assert!(!worker.metadata.in_process_events.queue_names.is_empty());
    assert!(worker.metadata.resource_limits.max_memory_mb > 0);

    // Validate queue names include expected namespaces
    let queue_names = &worker.metadata.in_process_events.queue_names;
    assert!(queue_names.contains(&"fulfillment_queue".to_string()));
    assert!(queue_names.contains(&"inventory_queue".to_string()));
    assert!(queue_names.contains(&"notifications_queue".to_string()));
}

/// Test serialization round-trip consistency
#[test]
fn test_serialization_round_trip() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");
    let toml_content = std::fs::read_to_string(&config_path).unwrap();
    let original_config: EventSystemsConfig = toml::from_str(&toml_content).unwrap();

    // Serialize back to TOML
    let serialized = toml::to_string_pretty(&original_config)
        .expect("Should be able to serialize config back to TOML");

    // Deserialize again
    let round_trip_config: EventSystemsConfig =
        toml::from_str(&serialized).expect("Should be able to deserialize round-trip TOML");

    // Validate key properties are preserved
    assert_eq!(
        original_config.orchestration.system_id,
        round_trip_config.orchestration.system_id
    );
    assert_eq!(
        original_config.task_readiness.system_id,
        round_trip_config.task_readiness.system_id
    );
    assert_eq!(
        original_config.worker.system_id,
        round_trip_config.worker.system_id
    );

    assert_eq!(
        original_config.orchestration.deployment_mode,
        round_trip_config.orchestration.deployment_mode
    );
    assert_eq!(
        original_config.task_readiness.deployment_mode,
        round_trip_config.task_readiness.deployment_mode
    );
    assert_eq!(
        original_config.worker.deployment_mode,
        round_trip_config.worker.deployment_mode
    );
}

/// Test naming consistency across all event systems
#[test]
fn test_naming_consistency() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");
    let toml_content = std::fs::read_to_string(&config_path).unwrap();
    let config: EventSystemsConfig = toml::from_str(&toml_content).unwrap();

    // All systems should use consistent field names for common concepts
    assert!(config.orchestration.timing.health_check_interval_seconds > 0);
    assert!(config.task_readiness.timing.health_check_interval_seconds > 0);
    assert!(config.worker.timing.health_check_interval_seconds > 0);

    assert!(config.orchestration.processing.max_concurrent_operations > 0);
    assert!(config.task_readiness.processing.max_concurrent_operations > 0);
    assert!(config.worker.processing.max_concurrent_operations > 0);

    assert!(config.orchestration.processing.batch_size > 0);
    assert!(config.task_readiness.processing.batch_size > 0);
    assert!(config.worker.processing.batch_size > 0);

    // All should use Vec<String> for namespaces
    assert!(!config.orchestration.namespaces.is_empty());
    assert!(!config.task_readiness.namespaces.is_empty());
    assert!(!config.worker.namespaces.is_empty());
}

/// Test system-specific metadata specialization
#[test]
fn test_system_specific_metadata() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");
    let toml_content = std::fs::read_to_string(&config_path).unwrap();
    let config: EventSystemsConfig = toml::from_str(&toml_content).unwrap();

    // Orchestration has minimal metadata (runtime populated)
    assert!(config.orchestration.metadata.queues_populated_at_runtime);

    // Task readiness has complex notification system
    assert!(!config
        .task_readiness
        .metadata
        .notification
        .global_channels
        .is_empty());
    assert!(
        config
            .task_readiness
            .metadata
            .enhanced_settings
            .rollback_threshold_percent
            > 0.0
    );

    // Worker has in-process event configuration (FFI integration)
    assert!(
        config
            .worker
            .metadata
            .in_process_events
            .ffi_integration_enabled
    );
    assert!(!config
        .worker
        .metadata
        .in_process_events
        .queue_names
        .is_empty());
    assert!(config.worker.metadata.resource_limits.max_memory_mb > 0);
}

/// Test deployment mode configuration
#[test]
fn test_deployment_modes() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");
    let toml_content = std::fs::read_to_string(&config_path).unwrap();
    let config: EventSystemsConfig = toml::from_str(&toml_content).unwrap();

    // All deployment modes should be valid (they can be different intentionally)
    use tasker_shared::event_system::DeploymentMode;

    // Validate deployment modes are proper enum values
    match config.orchestration.deployment_mode {
        DeploymentMode::EventDrivenOnly | DeploymentMode::Hybrid | DeploymentMode::PollingOnly => {}
    }

    match config.task_readiness.deployment_mode {
        DeploymentMode::EventDrivenOnly | DeploymentMode::Hybrid | DeploymentMode::PollingOnly => {}
    }

    match config.worker.deployment_mode {
        DeploymentMode::EventDrivenOnly | DeploymentMode::Hybrid | DeploymentMode::PollingOnly => {}
    }
}

/// Test that configuration eliminates the original drift issues
#[test]
fn test_drift_elimination() {
    let config_path = PathBuf::from("config/tasker/base/event_systems.toml");
    let toml_content = std::fs::read_to_string(&config_path).unwrap();
    let config: EventSystemsConfig = toml::from_str(&toml_content).unwrap();

    // Before: each system had slightly different field names and structures
    // After: all use the same EventSystemConfig<T> structure with consistent naming

    // Test that all use the same timing structure
    let orchestration_health_interval = config.orchestration.timing.health_check_interval_seconds;
    let task_readiness_health_interval = config.task_readiness.timing.health_check_interval_seconds;
    let worker_health_interval = config.worker.timing.health_check_interval_seconds;

    // Values can be different, but field names should be consistent
    assert!(orchestration_health_interval > 0);
    assert!(task_readiness_health_interval > 0);
    assert!(worker_health_interval > 0);

    // Test that all use the same processing structure
    assert!(config.orchestration.processing.max_concurrent_operations > 0);
    assert!(config.task_readiness.processing.max_concurrent_operations > 0);
    assert!(config.worker.processing.max_concurrent_operations > 0);

    // Test that all have consistent health configuration structure
    assert!(config.orchestration.health.error_rate_threshold_per_minute >= 1);
    assert!(config.task_readiness.health.error_rate_threshold_per_minute >= 1);
    assert!(config.worker.health.error_rate_threshold_per_minute >= 1);
}
