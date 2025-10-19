// TAS-50 Phase 2: Context-specific TOML configuration loading tests
//
// These tests verify that the Phase 2 context-specific configuration loading
// works correctly with actual TOML files.

use tasker_shared::config::{
    contexts::{ConfigContext, ConfigurationContext},
    unified_loader::UnifiedConfigLoader,
    ConfigManager,
};

#[test]
fn test_load_common_config_from_toml() {
    // Test loading CommonConfig directly from contexts/common.toml
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let common_config = loader
        .load_common_config()
        .expect("Failed to load CommonConfig");

    // Verify basic structure
    assert_eq!(common_config.environment, "test");
    assert!(!common_config.database.host.is_empty());
    // username field removed from DatabaseConfig - not functional, handled by connection string
    assert!(common_config.queues.default_batch_size > 0);

    // Verify validation passes
    assert!(common_config.validate().is_ok());
}

#[test]
fn test_load_orchestration_config_from_toml() {
    // Test loading OrchestrationConfig directly from contexts/orchestration.toml
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let orch_config = loader
        .load_orchestration_config()
        .expect("Failed to load OrchestrationConfig");

    // Verify basic structure
    assert_eq!(orch_config.environment, "test");
    assert!(orch_config.backoff.max_backoff_seconds > 0);
    assert!(!orch_config.backoff.default_backoff_seconds.is_empty());
    assert!(!orch_config.orchestration_system.mode.is_empty());

    // Verify validation passes
    assert!(orch_config.validate().is_ok());
}

#[test]
fn test_load_worker_config_from_toml() {
    // Test loading WorkerConfig directly from contexts/worker.toml
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let worker_config = loader
        .load_worker_config()
        .expect("Failed to load WorkerConfig");

    // Verify basic structure
    assert_eq!(worker_config.environment, "test");
    assert!(!worker_config.worker_system.worker_id.is_empty());
    assert!(worker_config.worker_system.web.enabled);

    // Verify validation passes
    assert!(worker_config.validate().is_ok());
}

#[test]
fn test_environment_overrides_for_common_config() {
    // Test that test environment overrides are applied to common config
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let common_config = loader
        .load_common_config()
        .expect("Failed to load CommonConfig");

    // Test environment should have smaller pool sizes
    assert!(
        common_config.database.pool.max_connections <= 10,
        "Test environment should have max_connections <= 10, got {}",
        common_config.database.pool.max_connections
    );

    // Test environment should have smaller batch sizes
    assert!(
        common_config.queues.default_batch_size <= 10,
        "Test environment should have batch_size <= 10, got {}",
        common_config.queues.default_batch_size
    );
}

#[test]
fn test_environment_overrides_for_orchestration_config() {
    // Test that test environment overrides are applied to orchestration config
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let orch_config = loader
        .load_orchestration_config()
        .expect("Failed to load OrchestrationConfig");

    // Test environment should have shorter backoff times
    assert!(
        orch_config.backoff.max_backoff_seconds <= 10,
        "Test environment should have max_backoff <= 10, got {}",
        orch_config.backoff.max_backoff_seconds
    );

    // Test environment should have smaller MPSC channel buffers
    assert!(
        orch_config
            .mpsc_channels
            .command_processor
            .command_buffer_size
            <= 500,
        "Test environment should have small command buffer, got {}",
        orch_config
            .mpsc_channels
            .command_processor
            .command_buffer_size
    );
}

#[test]
fn test_environment_overrides_for_worker_config() {
    // Test that test environment overrides are applied to worker config
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let worker_config = loader
        .load_worker_config()
        .expect("Failed to load WorkerConfig");

    // Test environment should have limited concurrent steps
    assert!(
        worker_config
            .worker_system
            .step_processing
            .max_concurrent_steps
            <= 100,
        "Test environment should have limited concurrent steps, got {}",
        worker_config
            .worker_system
            .step_processing
            .max_concurrent_steps
    );

    // Test environment should have smaller MPSC channel buffers
    assert!(
        worker_config
            .mpsc_channels
            .command_processor
            .command_buffer_size
            <= 500,
        "Test environment should have small command buffer, got {}",
        worker_config
            .mpsc_channels
            .command_processor
            .command_buffer_size
    );
}

#[test]
fn test_config_manager_load_context_direct_orchestration() {
    // Test ConfigManager.load_context_direct() for orchestration context
    let manager = ConfigManager::load_context_direct(ConfigContext::Orchestration)
        .expect("Failed to load orchestration context");

    // Verify context
    assert_eq!(manager.context(), ConfigContext::Orchestration);
    assert_eq!(manager.environment(), "test");

    // Verify we can access common config
    let common = manager.common().expect("Should have common config");
    assert!(!common.database.host.is_empty());

    // Verify we can access orchestration config
    let orch = manager
        .orchestration()
        .expect("Should have orchestration config");
    assert!(orch.backoff.max_backoff_seconds > 0);

    // Verify we cannot access worker config (type safety)
    assert!(manager.worker().is_none());
}

#[test]
fn test_config_manager_load_context_direct_worker() {
    // Test ConfigManager.load_context_direct() for worker context
    let manager = ConfigManager::load_context_direct(ConfigContext::Worker)
        .expect("Failed to load worker context");

    // Verify context
    assert_eq!(manager.context(), ConfigContext::Worker);
    assert_eq!(manager.environment(), "test");

    // Verify we can access common config
    let common = manager.common().expect("Should have common config");
    assert!(common.queues.default_batch_size > 0);

    // Verify we can access worker config
    let worker = manager.worker().expect("Should have worker config");
    assert!(!worker.worker_system.worker_id.is_empty());

    // Verify we cannot access orchestration config (type safety)
    assert!(manager.orchestration().is_none());
}

#[test]
fn test_config_manager_load_context_direct_combined() {
    // Test ConfigManager.load_context_direct() for combined context
    let manager = ConfigManager::load_context_direct(ConfigContext::Combined)
        .expect("Failed to load combined context");

    // Verify context
    assert_eq!(manager.context(), ConfigContext::Combined);
    assert_eq!(manager.environment(), "test");

    // Verify we can access all configs
    assert!(manager.common().is_some());
    assert!(manager.orchestration().is_some());
    assert!(manager.worker().is_some());

    // Verify legacy is not available
    assert!(manager.legacy().is_none());
}

// TAS-50 Phase 2-3: Removed test_config_manager_load_context_direct_legacy
// Legacy context loading is no longer supported after TOML migration

#[test]
fn test_production_environment_has_larger_values() {
    // Verify that production environment has appropriately larger configuration values
    let mut test_loader = UnifiedConfigLoader::new("test").expect("Failed to create test loader");
    let mut prod_loader =
        UnifiedConfigLoader::new("production").expect("Failed to create prod loader");

    let test_common = test_loader
        .load_common_config()
        .expect("Failed to load test common");
    let prod_common = prod_loader
        .load_common_config()
        .expect("Failed to load prod common");

    // Production should have larger database pool
    assert!(
        prod_common.database.pool.max_connections > test_common.database.pool.max_connections,
        "Production max_connections ({}) should be > test ({})",
        prod_common.database.pool.max_connections,
        test_common.database.pool.max_connections
    );

    // Production should have larger batch sizes
    assert!(
        prod_common.queues.default_batch_size >= test_common.queues.default_batch_size,
        "Production batch_size ({}) should be >= test ({})",
        prod_common.queues.default_batch_size,
        test_common.queues.default_batch_size
    );
}

#[test]
fn test_context_config_summary_contains_key_info() {
    // Test that summary() methods provide useful information
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let common = loader.load_common_config().expect("Failed to load common");
    let summary = common.summary();
    assert!(summary.contains("CommonConfig"));
    assert!(summary.contains("test"));
    assert!(summary.contains(&common.database.host));

    let orch = loader
        .load_orchestration_config()
        .expect("Failed to load orchestration");
    let summary = orch.summary();
    assert!(summary.contains("OrchestrationConfig"));
    assert!(summary.contains("test"));

    let worker = loader.load_worker_config().expect("Failed to load worker");
    let summary = worker.summary();
    assert!(summary.contains("WorkerConfig"));
    assert!(summary.contains("test"));
}

#[test]
fn test_common_config_database_url_method() {
    // Test that CommonConfig.database_url() works correctly
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");
    let common = loader.load_common_config().expect("Failed to load common");

    let db_url = common.database_url();

    // Should return a valid PostgreSQL URL
    assert!(
        db_url.starts_with("postgresql://"),
        "Database URL should start with postgresql://, got: {}",
        db_url
    );
}

#[test]
fn test_worker_config_effective_template_path() {
    // Test that WorkerConfig.effective_template_path() handles ENV override correctly
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");
    let worker = loader.load_worker_config().expect("Failed to load worker");

    // In test environment without TASKER_TEMPLATE_PATH set, should return None or TOML value
    let template_path = worker.effective_template_path();

    // This is fine - either None or Some path from TOML
    // The ENV override is tested in worker.rs with serial_test
    match template_path {
        Some(path) => {
            assert!(
                !path.as_os_str().is_empty(),
                "Template path should not be empty if set"
            );
        }
        None => {
            // No template path configured, which is valid
        }
    }
}

#[test]
fn test_all_context_configs_deserialize_successfully() {
    // Comprehensive test that all context configs can be loaded and deserialized
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    // All should deserialize without errors
    let common = loader.load_common_config();
    assert!(
        common.is_ok(),
        "CommonConfig should deserialize: {:?}",
        common.err()
    );

    let orch = loader.load_orchestration_config();
    assert!(
        orch.is_ok(),
        "OrchestrationConfig should deserialize: {:?}",
        orch.err()
    );

    let worker = loader.load_worker_config();
    assert!(
        worker.is_ok(),
        "WorkerConfig should deserialize: {:?}",
        worker.err()
    );
}

#[test]
fn test_all_context_configs_validate_successfully() {
    // Comprehensive test that all loaded configs pass validation
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let common = loader.load_common_config().expect("Failed to load common");
    assert!(common.validate().is_ok(), "CommonConfig should validate");

    let orch = loader
        .load_orchestration_config()
        .expect("Failed to load orchestration");
    assert!(
        orch.validate().is_ok(),
        "OrchestrationConfig should validate"
    );

    let worker = loader.load_worker_config().expect("Failed to load worker");
    assert!(worker.validate().is_ok(), "WorkerConfig should validate");
}

#[test]
fn test_circuit_breakers_loads_in_common_config() {
    // Verify circuit_breakers configuration migrated to CommonConfig successfully
    let mut loader = UnifiedConfigLoader::new("test").expect("Failed to create loader");

    let common = loader.load_common_config().expect("Failed to load common");

    // Verify circuit breakers are enabled
    assert!(
        common.circuit_breakers.enabled,
        "Circuit breakers should be enabled"
    );

    // Verify global settings
    assert_eq!(
        common.circuit_breakers.global_settings.max_circuit_breakers, 20,
        "Test environment should have max_circuit_breakers = 20"
    );
    assert_eq!(
        common
            .circuit_breakers
            .global_settings
            .metrics_collection_interval_seconds,
        10,
        "Test environment should have metrics interval = 10s"
    );

    // Verify default config (test overrides)
    assert_eq!(
        common.circuit_breakers.default_config.failure_threshold, 3,
        "Test environment should have failure_threshold = 3"
    );
    assert_eq!(
        common.circuit_breakers.default_config.timeout_seconds, 5,
        "Test environment should have timeout = 5s"
    );
    assert_eq!(
        common.circuit_breakers.default_config.success_threshold, 1,
        "Test environment should have success_threshold = 1"
    );

    // Verify component configs are present
    assert!(
        common
            .circuit_breakers
            .component_configs
            .contains_key("pgmq"),
        "Circuit breakers should have pgmq component config"
    );
    assert!(
        common
            .circuit_breakers
            .component_configs
            .contains_key("task_readiness"),
        "Circuit breakers should have task_readiness component config"
    );
}

#[test]
fn test_circuit_breakers_production_has_higher_capacity() {
    // Verify production has more resilient circuit breaker settings
    let mut test_loader = UnifiedConfigLoader::new("test").expect("Failed to create test loader");
    let mut prod_loader =
        UnifiedConfigLoader::new("production").expect("Failed to create prod loader");

    let test_common = test_loader
        .load_common_config()
        .expect("Failed to load test common");
    let prod_common = prod_loader
        .load_common_config()
        .expect("Failed to load prod common");

    // Production should have more circuit breakers allowed
    assert!(
        prod_common
            .circuit_breakers
            .global_settings
            .max_circuit_breakers
            > test_common
                .circuit_breakers
                .global_settings
                .max_circuit_breakers,
        "Production max_circuit_breakers ({}) should be > test ({})",
        prod_common
            .circuit_breakers
            .global_settings
            .max_circuit_breakers,
        test_common
            .circuit_breakers
            .global_settings
            .max_circuit_breakers
    );

    // Production should have higher failure threshold (more tolerant)
    assert!(
        prod_common
            .circuit_breakers
            .default_config
            .failure_threshold
            > test_common
                .circuit_breakers
                .default_config
                .failure_threshold,
        "Production failure_threshold ({}) should be > test ({})",
        prod_common
            .circuit_breakers
            .default_config
            .failure_threshold,
        test_common
            .circuit_breakers
            .default_config
            .failure_threshold
    );

    // Production should have longer timeout
    assert!(
        prod_common.circuit_breakers.default_config.timeout_seconds
            > test_common.circuit_breakers.default_config.timeout_seconds,
        "Production timeout ({}) should be > test ({})",
        prod_common.circuit_breakers.default_config.timeout_seconds,
        test_common.circuit_breakers.default_config.timeout_seconds
    );
}
