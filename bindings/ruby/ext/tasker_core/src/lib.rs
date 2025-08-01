//! # Tasker Core Ruby Bindings - Rails Integration (FFI Migration Complete)
//!
//! MIGRATION STATUS: ✅ COMPLETED - All major components migrated to shared FFI architecture
//! This module provides Ruby FFI bindings for the Tasker Core Rust orchestration engine,
//! focusing on high-performance operations that complement the existing Rails engine.
//!
//! ## Migration Achievements (TAS-21)
//!
//! ✅ **1,850+ lines of duplicate code eliminated** across 5 major Ruby FFI files:
//! - globals.rs: 700+ lines → thin wrapper (orchestration system migration)
//! - handles.rs: 600+ lines → enhanced wrapper (auto-refresh handle validation)
//! - performance.rs: 400+ lines → validated wrapper (validate_or_refresh pattern)
//! - testing_factory.rs: 1,100+ lines → thin wrapper (shared testing factory)
//! - event_bridge.rs: 54+ lines → thin wrapper (shared event bridge)
//! - ruby_step_handler.rs: Global pool access → shared handle access
//!
//! ## Architecture (Post-Migration)
//!
//! The Ruby bindings now follow a **shared component delegation pattern**:
//! - ✅ **Shared Components**: All core logic in `src/ffi/shared/` for multi-language support
//! - ✅ **Thin Ruby Wrappers**: Magnus-specific type conversion and method registration only
//! - ✅ **Handle-Based Architecture**: Persistent Arc<> references eliminate global lookups
//! - ✅ **Production Resilience**: Automatic handle refresh for long-running systems
//! - ✅ **Multi-Language Ready**: Foundation prepared for Python, Node.js, WASM, JNI bindings

// Allow dead code and unused variables in FFI bindings - this is expected
#![allow(dead_code)]
#![allow(unused_variables)]

use magnus::{Error, Module, Ruby};

mod context;
mod error_translation;
mod ffi_logging;
mod types;

// Direct model imports removed - pgmq architecture uses dry-struct data classes

/// Initialize the Ruby extension focused on Rails integration
#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    // Initialize FFI logger for debugging
    if let Err(e) = crate::ffi_logging::init_ffi_logger() {
        eprintln!("Warning: Failed to initialize FFI logger: {e}");
    }

    // Define TaskerCore module
    let module = ruby.define_module("TaskerCore")?;

    // Define version constants
    module.const_set("RUST_VERSION", env!("CARGO_PKG_VERSION"))?;
    module.const_set("STATUS", "ffi_bridges_restored")?;
    module.const_set(
        "FEATURES",
        "singleton_resources,core_delegation,performance_optimization",
    )?;

    // Define error hierarchy
    let base_error = module.define_error("Error", ruby.exception_standard_error())?;
    module.define_error("OrchestrationError", base_error)?;
    module.define_error("DatabaseError", base_error)?;
    module.define_error("StateTransitionError", base_error)?;
    module.define_error("ValidationError", base_error)?;
    module.define_error("TimeoutError", base_error)?;
    module.define_error("FFIError", base_error)?;

    // Ruby wrapper classes will be registered under their proper namespaces below

    // Note: Magnus-wrapped structs are automatically registered when used
    // All type classes are properly namespaced (e.g., TaskerCore::Types::WorkflowStepInput)

    // FFI handle and performance functions removed for pgmq architecture

    // 🎯 TYPES: Register types under Types:: namespace to avoid conflicts
    let types_module = module.define_module("Types")?;

    // FFI orchestration handle removed for pgmq architecture
    // BaseTaskHandler simplified for step execution only

    // TCP command architecture removed in favor of pgmq message queues

    Ok(())
}
