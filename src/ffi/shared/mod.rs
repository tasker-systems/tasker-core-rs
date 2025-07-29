//! # Shared FFI Components
//!
//! Language-agnostic FFI components that can be reused across Ruby, Python,
//! Node.js, WASM, and JNI bindings while preserving the handle-based
//! architecture that eliminates connection pool exhaustion.

pub mod analytics;
pub mod command_client;
pub mod command_listener;
pub mod database_cleanup;
pub mod errors;
pub mod event_bridge;
pub mod handles;
pub mod orchestration_system;
pub mod testing;
pub mod types;
pub mod worker_manager;

// Re-export main types for convenience
pub use command_client::{SharedCommandClient, CommandClientConfig, create_default_command_client, create_command_client};
pub use command_listener::{SharedCommandListener, CommandListenerConfig, create_default_command_listener, create_command_listener};
pub use errors::*;
pub use handles::*;
pub use orchestration_system::{OrchestrationSystem, initialize_unified_orchestration_system, get_global_runtime, get_global_database_pool, get_global_event_publisher, get_global_task_handler_registry};
pub use types::*;
pub use worker_manager::{SharedWorkerManager, WorkerManagerConfig, create_default_worker_manager, create_worker_manager};
