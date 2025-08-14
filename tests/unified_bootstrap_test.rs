//! Integration test for unified bootstrap architecture with circuit breakers

use tasker_core::messaging::PgmqClientTrait;
use tracing::{info, Level};

#[tokio::test]
async fn test_unified_bootstrap_architecture() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging for this test
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .try_init();

    info!("🧪 Testing unified bootstrap architecture with circuit breakers");

    // Setup test environment (respects existing DATABASE_URL in CI)
    tasker_core::test_utils::setup_test_environment();

    // Test: Initialize OrchestrationCore with configuration
    info!("🔧 Test: Initialize with configuration");
    let core = tasker_core::orchestration::OrchestrationCore::new().await?;

    info!("✅ OrchestrationCore initialized successfully with configuration");
    info!(
        "🛡️ Circuit breakers enabled: {}",
        core.circuit_breakers_enabled()
    );
    info!(
        "📊 Database pool active: {}",
        !core.database_pool().is_closed()
    );

    // Test basic functionality - queue initialization
    let namespaces = &["test_unified_bootstrap"];
    core.initialize_queues(namespaces).await?;
    info!("✅ Queue initialization successful with configuration");

    // Test unified client functionality
    info!("🔧 Testing unified PGMQ client functionality");
    let client = core.pgmq_client();
    match client.initialize_namespace_queues(&["client_test"]).await {
        Ok(()) => info!("✅ Unified client queue initialization successful"),
        Err(e) => {
            info!("❌ Unified client failed: {}", e);
            return Err(format!("Unified client failed: {e}").into());
        }
    }

    info!("🎉 Unified bootstrap test completed successfully!");
    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker_manager_access() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .try_init();

    info!("🔧 Testing circuit breaker manager access");

    // Setup test environment (respects existing DATABASE_URL in CI)
    tasker_core::test_utils::setup_test_environment();

    let core = tasker_core::orchestration::OrchestrationCore::new().await?;

    // Check circuit breaker manager is accessible
    let cb_manager = core.circuit_breaker_manager();
    info!(
        "🛡️ Circuit breaker manager present: {}",
        cb_manager.is_some()
    );

    // With configuration, circuit breakers should be enabled in test environment
    info!(
        "🛡️ Circuit breakers enabled: {}",
        core.circuit_breakers_enabled()
    );

    info!("✅ Circuit breaker manager access test passed");
    Ok(())
}
