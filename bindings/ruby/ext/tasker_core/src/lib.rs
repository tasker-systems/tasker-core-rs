//! # Tasker Core Ruby Bindings - Rails Integration
//!
//! This module provides Ruby FFI bindings for the Tasker Core Rust orchestration engine,
//! focusing on high-performance operations that complement the existing Rails engine.

use magnus::{Error, Module, Ruby};

mod context;
mod error_translation;
mod handlers;
mod performance;

/// Initialize the Ruby extension focused on Rails integration
#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    // Define TaskerCore module
    let module = ruby.define_module("TaskerCore")?;

    // Define version constants
    module.const_set("RUST_VERSION", env!("CARGO_PKG_VERSION"))?;
    module.const_set("STATUS", "rails_integration")?;
    module.const_set("FEATURES", "handler_foundation,performance_functions")?;

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

    // Register handler foundation classes - these are the base classes Rails handlers inherit from
    handlers::register_handler_classes(ruby, module)?;

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
