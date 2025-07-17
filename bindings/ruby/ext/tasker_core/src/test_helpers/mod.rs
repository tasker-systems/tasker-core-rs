//! # Test Helpers - Development Only
//!
//! **WARNING**: This module should only be available in development and test environments.
//! These functions should NEVER be compiled into production builds.
//!
//! ## Purpose
//!
//! This module provides Ruby FFI wrappers around the existing Rust factory system
//! to enable Ruby specs to create realistic test data using the same patterns
//! as Rust integration tests.
//!
//! ## Design Principles
//!
//! 1. **Reuse Existing Factories**: All functions delegate to the established
//!    factory system in `tests/factories/` instead of reimplementing database operations
//!
//! 2. **Development Only**: This entire module should be feature-gated and only
//!    available when compiled with test/development features
//!
//! 3. **Thin Wrappers**: Functions should be minimal FFI wrappers that convert
//!    Ruby arguments to Rust and call the existing factory methods

pub mod testing_factory;
pub mod database_cleanup;
pub mod testing_framework;

use magnus::{Error, Module, RModule};

/// Register test helper functions - always available, Rails gem controls exposure
pub fn register_test_helper_functions(module: RModule) -> Result<(), Error> {
    let test_helpers_module = module.define_module("TestHelpers")?;
    // Register factory wrapper functions
    testing_factory::register_factory_functions(test_helpers_module)?;


    // Register database cleanup functions
    database_cleanup::register_cleanup_functions(test_helpers_module)?;

    let testing_framework_module = module.define_module("TestingFramework")?;
    // Register testing framework functions
    testing_framework::register_testing_framework_functions(testing_framework_module)?;

    Ok(())
}
