//! # Shared FFI Components
//!
//! Language-agnostic FFI components that can be reused across Ruby, Python,
//! Node.js, WASM, and JNI bindings for pgmq-based workflow orchestration.

// pub mod analytics; // Disabled - depends on old TCP orchestration system
// pub mod database_cleanup; // Disabled - depends on handles which depends on old TCP orchestration system
pub mod errors;
// pub mod event_bridge; // Disabled - depends on old TCP orchestration system
// pub mod handles; // Disabled - depends on old TCP orchestration system
// pub mod orchestration_system; // Disabled - TCP-based orchestration system replaced by orchestration_system_pgmq
pub mod orchestration_system_pgmq;
// pub mod testing; // Disabled - depends on old TCP orchestration system
pub mod types;

// #[cfg(test)]
// mod orchestration_system_pgmq_test; // TODO: Create this test file when needed

// Re-export main types for convenience
pub use errors::*;
// pub use handles::*; // Disabled - depends on old TCP orchestration system
// pub use orchestration_system::{OrchestrationSystem, initialize_unified_orchestration_system, get_global_runtime, get_global_database_pool, get_global_event_publisher, get_global_task_handler_registry};
pub use orchestration_system_pgmq::OrchestrationSystemPgmq;
pub use types::*;
