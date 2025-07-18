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
mod ffi_converters;  // 🎯 NEW: Magnus optimized FFI converters
mod globals;
mod handles;  // 🎯 NEW: Handle-based FFI architecture
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

    // 🎯 NEW: Magnus wrapped classes for FFI optimization
    // These classes eliminate JSON serialization overhead
    // The #[magnus::wrap] attribute with free_immediately handles Ruby class registration
    ffi_converters::TaskMetadata::define(ruby, &module)?;
    
    // Register additional Magnus wrapped classes
    let ruby = magnus::Ruby::get().unwrap();
    let workflow_step_input_class = module.define_class("WorkflowStepInput", ruby.class_object())?;
    let complex_workflow_input_class = module.define_class("ComplexWorkflowInput", ruby.class_object())?;
    let factory_result_class = module.define_class("FactoryResult", ruby.class_object())?;

    // Register globals and registry functions
    globals::register_registry_functions(module)?;

    // Register event system FFI bridge
    events::register_event_functions(module)?;

    // Register RubyStepHandler wrapper class
    handlers::ruby_step_handler::register_ruby_step_handler_class(&ruby, &module)?;

    // Register task handler bridge functions
    handlers::base_task_handler::register_base_task_handler(&ruby, &module)?;

    // Register test helpers under TestHelpers module (always available, Rails gem controls exposure)
    test_helpers::register_test_helper_functions(module)?;

    // 🎯 NEW: Register handle-based FFI functions
    handles::register_handle_functions(module)?;

    // 🎯 NEW: Register TestHelpers factory functions that maintain existing Ruby API
    handles::register_test_helpers_factory_functions(module)?;

    // Register root-level performance functions that OrchestrationManager expects
    performance::register_root_performance_functions(module)?;
    
    // Register performance monitoring module
    let performance_module = module.define_module("Performance")?;
    performance::register_performance_functions(performance_module)?;

    // Register event bridge module
    let events_module = module.define_module("Events")?;
    events::event_bridge::register_event_functions(events_module)?;

    Ok(())
}
