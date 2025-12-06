use magnus::{Error as MagnusError, Module, Ruby};

mod bootstrap;
mod bridge;
mod conversions;
mod event_publisher_ffi; // TAS-65 Phase 2.4a: Domain event publishing FFI
mod ffi_logging;
mod global_event_system;
mod in_process_event_ffi; // TAS-65 Phase 4.1: In-process event polling FFI

// TAS-67: DomainEventCallback is now provided by tasker-worker (shared implementation)

// TAS-67: event_handler module removed - replaced by FfiDispatchChannel in bridge.rs

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), MagnusError> {
    // Initialize logging
    ffi_logging::init_ffi_logger().map_err(|err| {
        MagnusError::new(
            magnus::exception::runtime_error(),
            format!("Failed to initialize logging, {err}"),
        )
    })?;

    let module = ruby.define_module("TaskerCore")?;
    let ffi_module = module.define_module("FFI")?;

    // Initialize bridge with all lifecycle methods
    bridge::init_bridge(&ffi_module)?;

    Ok(())
}
