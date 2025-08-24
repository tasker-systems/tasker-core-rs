//! Integration test for circuit breakers enabled via configuration

use tracing::{info, Level};

#[tokio::test]
async fn test_circuit_breakers_enabled_from_config() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .try_init();

    info!("🧪 Testing circuit breakers enabled via configuration");

    // Setup test environment (respects existing DATABASE_URL in CI)
    tasker_shared::test_utils::setup_test_environment();

    // Load configuration manager from the test config
    let config_manager = tasker_shared::config::ConfigManager::load_from_env("test")?;

    info!("🔧 Creating OrchestrationCore with config (circuit breakers enabled)");
    let core = tasker_orchestration::orchestration::OrchestrationCore::from_config(config_manager).await?;

    info!("✅ OrchestrationCore initialized successfully from config");
    info!(
        "🛡️ Circuit breakers enabled: {}",
        core.circuit_breakers_enabled()
    );

    // Verify circuit breakers are enabled
    assert!(
        core.circuit_breakers_enabled(),
        "Circuit breakers should be enabled from config"
    );

    // Verify circuit breaker manager is present
    let cb_manager = core.circuit_breaker_manager();
    assert!(
        cb_manager.is_some(),
        "Circuit breaker manager should be present when enabled"
    );

    info!(
        "✅ Circuit breaker manager present: {}",
        cb_manager.is_some()
    );

    // Test basic functionality with circuit breakers enabled
    let namespaces = &["circuit_breaker_test"];
    core.initialize_queues(namespaces).await?;
    info!("✅ Queue initialization successful with circuit breakers enabled");

    info!("🎉 Circuit breaker enabled configuration test completed successfully!");
    Ok(())
}

#[tokio::test]
async fn test_auto_vs_explicit_config_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .try_init();

    info!("🧪 Testing auto-config vs explicit config manager comparison");

    // Setup test environment (respects existing DATABASE_URL in CI)
    tasker_shared::test_utils::setup_test_environment();

    // Test 1: Auto-configuration mode
    info!("🔧 Test 1: Auto-configuration mode initialization");
    let auto_core = tasker_orchestration::orchestration::OrchestrationCore::new().await?;

    let auto_cb_enabled = auto_core.circuit_breakers_enabled();
    info!(
        "🔧 Auto-config circuit breakers enabled: {}",
        auto_cb_enabled
    );

    // Test 2: Explicit config manager mode (should be identical)
    info!("🔧 Test 2: Explicit config manager mode initialization");
    let config_manager = tasker_shared::config::ConfigManager::load_from_env("test")?;

    let explicit_core =
        tasker_orchestration::orchestration::OrchestrationCore::from_config(config_manager).await?;
    let explicit_cb_enabled = explicit_core.circuit_breakers_enabled();
    info!(
        "🛡️ Explicit config circuit breakers enabled: {}",
        explicit_cb_enabled
    );

    // Verify they are identical (both methods should load same config)
    assert_eq!(
        auto_cb_enabled, explicit_cb_enabled,
        "Auto and explicit config modes should have identical circuit breaker settings"
    );
    assert!(
        auto_cb_enabled,
        "Test environment should have circuit breakers enabled"
    );
    assert!(
        explicit_cb_enabled,
        "Test environment should have circuit breakers enabled"
    );

    info!("✅ Auto vs Explicit config comparison test passed!");
    Ok(())
}
