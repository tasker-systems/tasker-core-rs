//! # Handlers Module - Ruby FFI
//!
//! Proper FFI bridges that delegate to core Rust orchestration systems
//! instead of reimplementing the logic.

pub mod step_handler_bridge;
pub mod task_handler_bridge;
pub mod registry;

// Re-export for convenience
pub use step_handler_bridge::register_step_handler_functions;
pub use task_handler_bridge::register_task_handler_functions;
pub use registry::register_registry_functions;

use magnus::{Error, RModule, Ruby};

/// Register all handler classes and functions
pub fn register_handler_classes(_ruby: &Ruby, module: RModule) -> Result<(), Error> {
    // Register TaskHandlerRegistry functions
    register_registry_functions(module)?;

    // Register proper FFI bridges that delegate to core logic
    register_task_handler_functions(module)?;
    register_step_handler_functions(module)?;

    Ok(())
}
