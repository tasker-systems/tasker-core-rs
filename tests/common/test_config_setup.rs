//! Test configuration setup helpers
//!
//! Provides utilities for configuring the test environment to use unified config files.

use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

/// Setup test environment to use complete unified config file
///
/// This sets TASKER_CONFIG_PATH to point to the complete-test.toml file
/// which contains all contexts (common + orchestration + worker) merged together.
///
/// Call this at the beginning of integration tests that need full system context.
pub fn setup_complete_test_config() {
    INIT.call_once(|| {
        // Get the project root (assuming we're in tests/common/)
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let complete_config_path = project_root.join("config/tasker/complete-test.toml");

        // Set TASKER_CONFIG_PATH to point to complete config
        std::env::set_var("TASKER_CONFIG_PATH", complete_config_path.to_str().unwrap());

        println!("✓ Test config setup: TASKER_CONFIG_PATH={}", complete_config_path.display());
    });
}

/// Setup test environment to use context-specific config file
///
/// This sets TASKER_CONFIG_ROOT to the config directory, allowing SystemContext
/// to load context-specific files like worker-test.toml or orchestration-test.toml.
///
/// Call this if you need context-specific configuration for targeted testing.
pub fn setup_context_specific_test_config() {
    INIT.call_once(|| {
        // Get the project root
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_root = project_root.join("config/tasker");

        // Set TASKER_CONFIG_ROOT for convention-based loading
        std::env::set_var("TASKER_CONFIG_ROOT", config_root.to_str().unwrap());

        println!("✓ Test config setup: TASKER_CONFIG_ROOT={}", config_root.display());
    });
}
