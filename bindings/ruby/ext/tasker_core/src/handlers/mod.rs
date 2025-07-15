//! # Handlers Module - Ruby FFI
//!
//! Proper FFI bridges that delegate to core Rust orchestration systems
//! instead of reimplementing the logic.

pub mod base_step_handler;
pub mod base_task_handler;

use magnus::{Error, RModule, Ruby};

/// Register all handler classes with Ruby
pub fn register_handler_classes(ruby: &Ruby, module: &RModule) -> Result<(), Error> {
    base_step_handler::register_ruby_step_handler_functions(*module)?;
    base_task_handler::register_base_task_handler(ruby, module)?;
    Ok(())
}
