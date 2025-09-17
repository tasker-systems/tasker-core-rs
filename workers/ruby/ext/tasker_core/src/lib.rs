use magnus::{Error as MagnusError, Module, Ruby};

mod bootstrap;
mod bridge;
mod conversions;
mod event_handler;
mod ffi_logging;
mod global_event_system;

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
