// Temporary test file to verify deprecation warnings
// This file will be deleted after testing

use tasker_shared::config::manager::ConfigManager;
use tasker_shared::system_context::SystemContext;

#[tokio::main]
async fn main() {
    // Set environment to test and config root
    std::env::set_var("TASKER_ENV", "test");
    std::env::set_var("TASKER_CONFIG_ROOT", "/Users/petetaylor/projects/tasker-systems/tasker-core/config");

    println!("=== Testing Deprecation Warnings ===\n");

    // Test 1: Legacy ConfigManager::load_from_env() should warn
    println!("Test 1: Loading config via legacy ConfigManager::load_from_env()");
    match ConfigManager::load_from_env() {
        Ok(config) => println!("✓ Legacy config loaded successfully (should see deprecation warning above)\n"),
        Err(e) => println!("✗ Error loading legacy config: {}\n", e),
    }

    // Test 2: V2 ConfigManager::load_from_v2() should NOT warn
    println!("Test 2: Loading config via v2 ConfigManager::load_from_v2()");
    match ConfigManager::load_from_v2("test") {
        Ok(config) => println!("✓ V2 config loaded successfully (no deprecation warning)\n"),
        Err(e) => println!("✗ Error loading v2 config: {}\n", e),
    }

    // Test 3: Legacy SystemContext::new_for_orchestration() should warn
    println!("Test 3: Creating orchestration context via legacy method");
    match SystemContext::new_for_orchestration().await {
        Ok(_) => println!("✓ Legacy orchestration context created (should see deprecation warning above)\n"),
        Err(e) => println!("✗ Error creating legacy orchestration context: {}\n", e),
    }

    // Test 4: V2 SystemContext::new_for_orchestration_v2() should NOT warn
    println!("Test 4: Creating orchestration context via v2 method");
    match SystemContext::new_for_orchestration_v2().await {
        Ok(_) => println!("✓ V2 orchestration context created (no deprecation warning)\n"),
        Err(e) => println!("✗ Error creating v2 orchestration context: {}\n", e),
    }

    println!("=== All Tests Complete ===");
}
