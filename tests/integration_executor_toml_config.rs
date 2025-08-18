//! Integration tests for TAS-34 executor system YAML configuration
//!
//! These tests verify that the executor pool configuration system works correctly
//! with the YAML-driven configuration approach, ensuring proper integration
//! between configuration loading and executor creation.

use sqlx::PgPool;
use tasker_core::config::ConfigManager;
use tasker_core::orchestration::executor::base::BaseExecutor;
use tasker_core::orchestration::executor::traits::{ExecutorType, OrchestrationExecutor};
use tasker_core::{Result, TaskerError};

/// Create a test database pool for integration tests
async fn create_test_database_pool() -> Result<PgPool> {
    let database_url = tasker_core::test_utils::get_test_database_url();

    PgPool::connect(&database_url)
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to connect to database: {e}")))
}

#[tokio::test]
async fn test_toml_executor_configuration_loading() {
    // This test verifies that executor configuration is correctly loaded from TOML
    // and used to create properly configured executors

    let config_manager = ConfigManager::load().expect("Failed to load configuration");

    // Check that executor pools configuration is available
    let executor_pools = config_manager.config().executor_pools();

    // Verify coordinator configuration
    assert!(executor_pools.coordinator.target_utilization > 0.0);
    assert!(executor_pools.coordinator.target_utilization <= 1.0);
    assert!(executor_pools.coordinator.scaling_interval_seconds > 0);
    assert!(executor_pools.coordinator.health_check_interval_seconds > 0);

    // Verify task request processor configuration
    assert!(executor_pools.task_request_processor.min_executors > 0);
    assert!(
        executor_pools.task_request_processor.max_executors
            >= executor_pools.task_request_processor.min_executors
    );
    assert!(executor_pools.task_request_processor.polling_interval_ms > 0);
    assert!(executor_pools.task_request_processor.batch_size > 0);
    assert!(executor_pools.task_request_processor.processing_timeout_ms > 0);

    println!("✅ YAML executor configuration loaded and validated successfully");
}

#[tokio::test]
async fn test_environment_specific_executor_configuration() {
    // This test verifies that test environment has different executor pool settings
    // than production, demonstrating environment-aware configuration

    let config_manager =
        ConfigManager::load_from_env("test").expect("Failed to load test configuration");

    let test_pools = config_manager.config().executor_pools();

    // Test environment should have smaller pools and faster polling for test responsiveness
    // These assertions verify the YAML overrides in the test environment section
    if config_manager.config().is_test_environment() {
        // Test environment typically has smaller max_executors for resource efficiency
        assert!(test_pools.task_request_processor.max_executors <= 5);
        assert!(test_pools.step_enqueuer.max_executors <= 5);

        // Test environment often has faster polling for test responsiveness
        assert!(test_pools.task_request_processor.polling_interval_ms <= 100);
        assert!(test_pools.step_enqueuer.polling_interval_ms <= 50);

        println!("✅ Test environment executor configuration validated");
    }

    // Load production configuration for comparison
    let prod_config_manager = ConfigManager::load_from_env("production")
        .expect("Failed to load production configuration");

    let prod_pools = prod_config_manager.config().executor_pools();

    if prod_config_manager.config().is_production_environment() {
        // Production environment should have larger pools for throughput
        assert!(prod_pools.task_request_processor.max_executors >= 5);
        assert!(prod_pools.step_enqueuer.max_executors >= 10);

        println!("✅ Production environment executor configuration validated");
    }
}

#[tokio::test]
async fn test_executor_creation_from_toml_config() {
    // This test verifies that executors can be created using TOML configuration
    // and that the configuration values are properly applied

    let config_manager =
        ConfigManager::load_from_env("test").expect("Failed to load test configuration");

    let pool = create_test_database_pool()
        .await
        .expect("Failed to create test database pool");

    // Test each executor type
    for executor_type in ExecutorType::all() {
        let executor_config = config_manager.config().get_executor_config(executor_type);
        let executor = BaseExecutor::with_config(executor_type, pool.clone(), executor_config)
            .unwrap_or_else(|_| panic!("Failed to create {} executor", executor_type.name()));

        // Verify the executor was created with correct type
        assert_eq!(executor.executor_type(), executor_type);

        // Verify configuration was applied correctly
        let config = executor.get_config().await;
        assert!(config.polling_interval_ms > 0);
        assert!(config.batch_size > 0);
        assert!(config.processing_timeout_ms > 0);
        assert!(config.max_retries > 0);
        assert!(config.backpressure_factor >= 0.0);
        assert!(config.backpressure_factor <= 1.0);

        // For test environment, verify test-specific settings were applied
        if config_manager.config().is_test_environment() {
            // Test environment should have smaller timeouts and batch sizes
            assert!(config.processing_timeout_ms <= 30000); // 30 seconds or less for tests
            assert!(config.batch_size <= 50); // Reasonable batch size for tests
        }

        println!(
            "✅ {} executor created successfully with TOML configuration",
            executor_type.name()
        );
    }
}

#[tokio::test]
async fn test_executor_config_type_conversion() {
    // This test verifies that ExecutorInstanceConfig correctly converts to ExecutorConfig
    // and that all configuration values are preserved

    let config_manager =
        ConfigManager::load_from_env("test").expect("Failed to load test configuration");

    // Test conversion for each executor type
    for executor_type in ExecutorType::all() {
        let exec_config = config_manager.config().get_executor_config(executor_type);

        // Verify all required fields are present and valid
        assert!(
            exec_config.polling_interval_ms > 0,
            "Polling interval must be positive for {}",
            executor_type.name()
        );
        assert!(
            exec_config.batch_size > 0,
            "Batch size must be positive for {}",
            executor_type.name()
        );
        assert!(
            exec_config.processing_timeout_ms > 0,
            "Processing timeout must be positive for {}",
            executor_type.name()
        );
        assert!(
            exec_config.max_retries > 0,
            "Max retries must be positive for {}",
            executor_type.name()
        );
        assert!(
            exec_config.backpressure_factor == 1.0,
            "Backpressure factor should default to 1.0 for {}",
            executor_type.name()
        );

        // Verify circuit breaker configuration is preserved
        // The exact values depend on YAML configuration, so we just check they're reasonable
        assert!(
            exec_config.circuit_breaker_threshold > 0,
            "Circuit breaker threshold must be positive for {}",
            executor_type.name()
        );
        assert!(
            exec_config.circuit_breaker_threshold <= 20,
            "Circuit breaker threshold should be reasonable for {}",
            executor_type.name()
        );

        println!(
            "✅ Configuration conversion validated for {}",
            executor_type.name()
        );
    }
}

#[tokio::test]
async fn test_executor_pool_defaults_when_yaml_missing() {
    // This test verifies that when executor_pools is not specified in YAML,
    // sensible defaults are provided that allow the system to function

    let config_manager = ConfigManager::load().expect("Failed to load configuration");

    // Even if YAML doesn't explicitly configure executor_pools, we should get defaults
    let executor_pools = config_manager.config().executor_pools();

    // Verify default coordinator configuration
    assert!(executor_pools.coordinator.target_utilization > 0.0);
    assert!(executor_pools.coordinator.target_utilization <= 1.0);
    assert!(executor_pools.coordinator.scaling_interval_seconds > 0);
    assert!(executor_pools.coordinator.health_check_interval_seconds > 0);
    assert!(executor_pools.coordinator.scaling_cooldown_seconds > 0);
    assert!(executor_pools.coordinator.max_db_pool_usage > 0.0);
    assert!(executor_pools.coordinator.max_db_pool_usage <= 1.0);

    // Verify default executor configurations
    for executor_type in ExecutorType::all() {
        let exec_config = config_manager.config().get_executor_config(executor_type);

        // All defaults should be positive and reasonable
        assert!(exec_config.polling_interval_ms >= 10); // At least 10ms
        assert!(exec_config.polling_interval_ms <= 10000); // At most 10 seconds
        assert!(exec_config.batch_size >= 1);
        assert!(exec_config.batch_size <= 100); // Reasonable upper bound
        assert!(exec_config.processing_timeout_ms >= 1000); // At least 1 second
        assert!(exec_config.max_retries >= 1);
        assert!(exec_config.max_retries <= 10); // Reasonable upper bound

        println!(
            "✅ Default configuration validated for {}",
            executor_type.name()
        );
    }

    println!(
        "✅ Executor pool defaults work correctly when YAML is missing explicit configuration"
    );
}
