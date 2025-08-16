//! # Shared FFI Components
//!
//! Language-agnostic FFI components that can be reused across Ruby, Python,
//! Node.js, WASM, and JNI bindings for pgmq-based workflow orchestration.

pub mod analytics;
pub mod config;
pub mod database_cleanup;
pub mod errors;
pub mod event_bridge;
pub mod handles;
pub mod task_initialization;
pub mod test_database_management;
pub mod testing;
pub mod types;

// #[cfg(test)]
// mod orchestration_system_test; // TODO: Create this test file when needed

// Re-export main types for convenience
pub use config::*;
pub use errors::*;
pub use handles::*;
pub use task_initialization::*;
pub use test_database_management::*;
pub use types::*;
