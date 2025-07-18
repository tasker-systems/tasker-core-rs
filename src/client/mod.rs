//! # Rust Client Foundation
//!
//! This module provides the foundation for Rust-native task and step handlers
//! that can be used directly within Rust applications or as a base for FFI
//! integrations with other frameworks.
//!
//! ## Architecture
//!
//! The client foundation follows the delegation pattern established in the orchestration core:
//! - **BaseTaskHandler**: Main entry point for task execution
//! - **BaseStepHandler**: Step execution foundation with framework hooks
//! - **TaskContext**: Execution context for task-level operations
//! - **StepContext**: Execution context for step-level operations
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::client::{BaseTaskHandler, BaseStepHandler, RustStepHandler, StepContext};
//! use tasker_core::orchestration::handler_config::HandlerConfiguration;
//! use std::sync::Arc;
//!
//! // For step handlers
//! #[derive(Clone)]
//! struct MyStepHandler;
//!
//! #[async_trait::async_trait]
//! impl RustStepHandler for MyStepHandler {
//!     async fn process(&self, context: &StepContext) -> tasker_core::Result<serde_json::Value> {
//!         // Your business logic here
//!         Ok(serde_json::json!({"status": "processed"}))
//!     }
//! }
//!
//! // For task handlers
//! #[derive(Clone)]
//! struct MyTaskHandler {
//!     step_handlers: std::collections::HashMap<String, Arc<dyn RustStepHandler>>,
//! }
//!
//! impl MyTaskHandler {
//!     fn new() -> Self {
//!         let mut step_handlers = std::collections::HashMap::new();
//!         step_handlers.insert("my_step".to_string(), Arc::new(MyStepHandler) as Arc<dyn RustStepHandler>);
//!         Self { step_handlers }
//!     }
//! }
//! ```

pub mod context;
pub mod step_handler;
pub mod task_handler;
pub mod traits;

// Re-export main types for easy access
pub use context::{ExecutionMetadata, StepContext, TaskContext};
pub use step_handler::BaseStepHandler;
pub use task_handler::BaseTaskHandler;
pub use traits::{RustStepHandler, RustTaskHandler};
