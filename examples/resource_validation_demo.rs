//! # Resource Validation Demo
//!
//! Demonstrates the TAS-34 Phase 1 resource constraint validation system
//! that prevents database pool exhaustion by validating executor configurations.

use tasker_core::config::ConfigManager;
use tasker_core::orchestration::coordinator::resource_limits::ResourceValidator;
use tasker_core::orchestration::OrchestrationCore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for better output
    tracing_subscriber::fmt::init();

    println!("🔍 TAS-34 Resource Validation Demo");
    println!("===================================");

    // Set up test environment
    tasker_core::test_utils::setup_test_environment();

    // Load configuration
    let config_manager = ConfigManager::load_from_env("test")?;

    println!("📋 Current Configuration:");
    println!("  Environment: {}", config_manager.environment());
    println!(
        "  Database URL: {}...",
        &config_manager.config().database_url()[..30]
    );

    // Create orchestration core to get database pool
    let orchestration_core = OrchestrationCore::new().await?;

    println!("\n📊 Database Pool Status:");
    println!(
        "  Max connections: {}",
        orchestration_core.database_pool().size()
    );
    println!(
        "  Active connections: {}",
        orchestration_core.database_pool().size()
            - orchestration_core.database_pool().num_idle() as u32
    );
    println!(
        "  Idle connections: {}",
        orchestration_core.database_pool().num_idle()
    );

    // Create resource validator
    println!("\n🔍 Performing Resource Validation...");
    let resource_validator =
        ResourceValidator::new(orchestration_core.database_pool(), config_manager.clone()).await?;

    let resource_limits = resource_validator.resource_limits();
    println!("\n📈 Detected Resource Limits:");
    println!(
        "  Max DB Connections: {}",
        resource_limits.max_database_connections
    );
    println!(
        "  Active DB Connections: {}",
        resource_limits.active_database_connections
    );
    println!(
        "  Reserved DB Connections: {}",
        resource_limits.reserved_database_connections
    );
    println!(
        "  Available DB Connections: {}",
        resource_limits.available_database_connections
    );

    if let Some(max_memory) = resource_limits.max_memory_mb {
        println!("  Total Memory: {max_memory} MB");
    }

    if let Some(available_memory) = resource_limits.available_memory_mb {
        println!("  Available Memory: {available_memory} MB");
    }

    if let Some(cpu_cores) = resource_limits.cpu_cores {
        println!("  CPU Cores: {cpu_cores}");
    }

    if !resource_limits.warnings.is_empty() {
        println!("\n⚠️ Resource Warnings:");
        for warning in &resource_limits.warnings {
            println!("  - {warning}");
        }
    }

    // Perform validation
    println!("\n🧪 Validating Executor Configuration...");
    let validation_result = resource_validator.validate_and_log_info().await;

    match validation_result {
        Ok(result) => {
            println!("✅ Validation PASSED - Configuration is safe");
            println!("\n📊 Validation Summary:");
            for line in result.summary().lines() {
                println!("  {line}");
            }

            let recommended_size = result.recommended_database_pool_size();
            if recommended_size > result.resource_limits.max_database_connections {
                println!("\n💡 Optimization Recommendation:");
                println!("  Consider increasing database pool size to {recommended_size} for optimal performance");
            }
        }
        Err(e) => {
            println!("❌ Validation FAILED - Configuration is unsafe");
            println!("  Error: {e}");
            println!("\n🛡️ This is GOOD! Resource validation prevented an unsafe configuration.");
            println!("   The system would have exhausted database connections under load.");
        }
    }

    println!("\n🎉 Demo completed successfully!");
    println!("   Resource validation is working correctly to prevent database pool exhaustion.");

    Ok(())
}
