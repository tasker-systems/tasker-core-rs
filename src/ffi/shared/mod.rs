//! # Shared FFI Components
//!
//! Language-agnostic FFI components that can be reused across Ruby, Python,
//! Node.js, WASM, and JNI bindings while preserving the handle-based
//! architecture that eliminates connection pool exhaustion.

pub mod analytics;
pub mod database_cleanup;
pub mod errors;
pub mod event_bridge;
pub mod handles;
pub mod orchestration_system;
pub mod testing;
pub mod types;

// Re-export main types for convenience
pub use errors::*;
pub use handles::*;
pub use orchestration_system::*;
pub use types::*;
