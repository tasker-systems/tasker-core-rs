//! # Tasker Core Ruby Bindings - Rails Integration
//!
//! This module provides Ruby FFI bindings for the Tasker Core Rust orchestration engine,
//! focusing on high-performance operations that complement the existing Rails engine.
//!
//! ## Architecture
//!
//! The Ruby bindings follow a delegation pattern where:
//! - Ruby provides FFI bridges to core Rust logic
//! - Singleton patterns are used for shared resources (database connections, event publishers)
//! - Core Rust logic is the single source of truth for orchestration
//! - Performance targets are met through proper resource reuse

// Allow dead code and unused variables in FFI bindings - this is expected
#![allow(dead_code)]
#![allow(unused_variables)]

use magnus::{Error, Module, Ruby};

mod context;
mod error_translation;
mod globals;
mod handlers;
mod events;
mod models;
mod performance;
mod test_helpers;

/// Initialize the Ruby extension focused on Rails integration
#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    // Define TaskerCore module
    let module = ruby.define_module("TaskerCore")?;

    // Define version constants
    module.const_set("RUST_VERSION", env!("CARGO_PKG_VERSION"))?;
    module.const_set("STATUS", "ffi_bridges_restored")?;
    module.const_set("FEATURES", "singleton_resources,core_delegation,performance_optimization")?;

    // Define error hierarchy
    let base_error = module.define_error("Error", ruby.exception_standard_error())?;
    module.define_error("OrchestrationError", base_error)?;
    module.define_error("DatabaseError", base_error)?;
    module.define_error("StateTransitionError", base_error)?;
    module.define_error("ValidationError", base_error)?;
    module.define_error("TimeoutError", base_error)?;
    module.define_error("FFIError", base_error)?;

    // Register Ruby wrapper classes for structured return types
    performance::RubyTaskExecutionContext::define(ruby, &module)?;
    performance::RubyViableStep::define(ruby, &module)?;
    performance::RubySystemHealth::define(ruby, &module)?;
    performance::RubyAnalyticsMetrics::define(ruby, &module)?;
    performance::RubySlowestStepAnalysis::define(ruby, &module)?;
    performance::RubySlowestTaskAnalysis::define(ruby, &module)?;
    performance::RubyDependencyAnalysis::define(ruby, &module)?;
    performance::RubyDependencyLevel::define(ruby, &module)?;

    // Note: Magnus wrapped classes are automatically registered when used
    // The #[magnus::wrap] attribute handles Ruby class registration
    // We don't need to manually register them here

    // Register globals and registry functions
    globals::register_registry_functions(module)?;

    // Register event system FFI bridge
    events::register_event_functions(module)?;

    // Register step handler bridge functions
    handlers::base_step_handler::register_ruby_step_handler_functions(module)?;
    
    // Register RubyStepHandler wrapper class
    handlers::ruby_step_handler::register_ruby_step_handler_class(ruby, &module)?;
    
    // Register task handler bridge functions
    handlers::base_task_handler::register_base_task_handler(ruby, &module)?;

    // Register test helpers under TestHelpers module (always available, Rails gem controls exposure)
    let test_helpers_module = module.define_module("TestHelpers")?;
    test_helpers::register_test_helper_functions(test_helpers_module)?;

    // Define performance function wrappers - simplified approach
    module.define_module_function(
        "get_task_execution_context",
        magnus::function!(performance::get_task_execution_context_sync, 2),
    )?;
    module.define_module_function(
        "discover_viable_steps",
        magnus::function!(performance::discover_viable_steps_sync, 2),
    )?;
    module.define_module_function(
        "get_system_health",
        magnus::function!(performance::get_system_health_sync, 1),
    )?;
    module.define_module_function(
        "get_analytics_metrics",
        magnus::function!(performance::get_analytics_metrics_sync, 2),
    )?;
    module.define_module_function(
        "batch_update_step_states",
        magnus::function!(performance::batch_update_step_states_sync, 2),
    )?;
    module.define_module_function(
        "analyze_dependencies",
        magnus::function!(performance::analyze_dependencies_sync, 2),
    )?;

    Ok(())
}
