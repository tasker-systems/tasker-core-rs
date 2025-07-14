//! # Events Module - Ruby FFI
//!
//! Proper FFI bridge that delegates to core event system instead of
//! reimplementing it.

pub mod event_bridge;

use magnus::{Error, RModule};

/// Register all event system functions
pub fn register_event_functions(module: RModule) -> Result<(), Error> {
    // Register proper FFI bridge that delegates to core event system
    event_bridge::register_event_functions(module)?;

    Ok(())
}
